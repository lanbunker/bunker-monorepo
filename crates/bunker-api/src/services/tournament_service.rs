use std::cmp::Reverse;

use bunker_models::{
    Award, Bracket, Entrant, EntrantId, Match, MatchId, NewTournament, PageQuery, Paginated,
    PlayerId, Registrations, SeedOrder, SkillLevel, StatusChange, Tournament, TournamentDetail,
    TournamentId, TournamentStatus, TournamentUpdate, tournament_awards,
};
use rand::seq::SliceRandom;
use time::OffsetDateTime;

use crate::storage::{
    BracketWrite, Enrolled, EntrantGuard, NewEntrant, NewTournamentRow, PlayerStorage,
    StorageError, TournamentStorage,
};

use super::error::ServiceError;
use super::outcome::{Outcome, Visibility};

/// The rules of a tournament: who may enter, when the status moves, and what a
/// bracket accepts.
///
/// A rule reads the tournament, then writes with the state it read as a guard.
/// Storage checks the guard in the statement that writes, so a concurrent
/// change refuses the write, and the service says which rule it broke.
#[derive(Debug, Clone)]
pub struct TournamentService {
    storage: TournamentStorage,
    players: PlayerStorage,
}

impl TournamentService {
    pub const fn new(storage: TournamentStorage, players: PlayerStorage) -> Self {
        Self { storage, players }
    }

    pub async fn create(&self, new: NewTournament) -> Result<Tournament, ServiceError> {
        let id = TournamentId::generate();
        self.storage
            .create(&NewTournamentRow {
                id,
                name: new.name,
                game: new.game,
                mode: new.mode,
                description: new.description,
                date: new.date,
                registration_closes_at: new.registration_closes_at,
                created_at: OffsetDateTime::now_utc(),
            })
            .await?;

        self.load(id).await
    }

    /// The public list hides drafts. The admin list shows everything.
    pub async fn list(
        &self,
        query: PageQuery,
        visibility: Visibility,
    ) -> Result<Paginated<Tournament>, ServiceError> {
        Ok(self
            .storage
            .list(query, visibility == Visibility::WithDrafts)
            .await?)
    }

    /// A draft is not found for anyone but an admin.
    pub async fn detail(
        &self,
        id: TournamentId,
        visibility: Visibility,
    ) -> Result<TournamentDetail, ServiceError> {
        let tournament = self.load(id).await?;
        if tournament.status == TournamentStatus::Draft && visibility == Visibility::Public {
            return Err(ServiceError::TournamentNotFound(id));
        }
        let entrants = self.storage.entrants(id).await?;
        let bracket = self.bracket(id).await?;

        Ok(TournamentDetail {
            tournament,
            entrants,
            bracket,
        })
    }

    /// Any field, in any status but `concluded`.
    pub async fn update(
        &self,
        id: TournamentId,
        update: TournamentUpdate,
    ) -> Result<Tournament, ServiceError> {
        let tournament = self.load(id).await?;
        require_not_concluded(&tournament)?;
        if !self.storage.update(id, tournament.status, &update).await? {
            return Err(self.stale(id).await);
        }

        self.load(id).await
    }

    /// Idempotent. Entrants and matches go with the tournament.
    pub async fn delete(&self, id: TournamentId) -> Result<(), ServiceError> {
        self.storage.delete(id).await?;

        Ok(())
    }

    /// The state machine. `concluded` is final. The conclusion pays the cycles
    /// of the tournament, in the same write as the status.
    ///
    /// A second click on the same button changes nothing and fails nothing, and
    /// that holds for two clicks at once too: the second write finds the status
    /// moved, and answers the tournament as the first one left it.
    pub async fn change_status(
        &self,
        id: TournamentId,
        change: StatusChange,
    ) -> Result<Tournament, ServiceError> {
        let tournament = self.load(id).await?;
        if tournament.status == change.status && change.winner.is_none() {
            return Ok(tournament);
        }
        let (winner, awards) = self.plan_transition(&tournament, &change).await?;

        let written = self
            .storage
            .set_status(
                id,
                tournament.status,
                change.status,
                winner,
                &awards,
                OffsetDateTime::now_utc(),
            )
            .await;
        match written {
            Ok(true) => self.load(id).await,
            // An entrant or a player that left since the read breaks a foreign
            // key of the winner or of an award.
            Ok(false) | Err(StorageError::ForeignKeyViolation(_)) => {
                let fresh = self.load(id).await?;
                if fresh.status == change.status && change.winner.is_none() {
                    Ok(fresh)
                } else {
                    Err(ServiceError::TournamentChanged)
                }
            }
            Err(error) => Err(error.into()),
        }
    }

    pub async fn registrations(&self, player: PlayerId) -> Result<Registrations, ServiceError> {
        Ok(Registrations {
            tournaments: self.storage.tournaments_of(player).await?,
        })
    }

    /// A player enters with their level while registration is open. A second
    /// call answers the same entrant, with the level of the second call.
    pub async fn register(
        &self,
        id: TournamentId,
        player: PlayerId,
        skill: SkillLevel,
    ) -> Result<Entrant, ServiceError> {
        let now = OffsetDateTime::now_utc();
        let tournament = self.load_public(id).await?;
        require_registration_open(&tournament, now)?;
        let guard = EntrantGuard {
            status: TournamentStatus::Open,
            open_at: Some(now),
        };

        match self.enrol(id, player, Some(skill), guard).await? {
            Outcome::Created(entrant) | Outcome::Existing(entrant) => Ok(entrant),
        }
    }

    /// Idempotent, and only while registration is open. After the deadline the
    /// entrants are fixed for the bracket.
    pub async fn retire(&self, id: TournamentId, player: PlayerId) -> Result<(), ServiceError> {
        let now = OffsetDateTime::now_utc();
        let tournament = self.load_public(id).await?;
        require_registration_open(&tournament, now)?;
        let guard = EntrantGuard {
            status: TournamentStatus::Open,
            open_at: Some(now),
        };
        if !self.storage.remove_entrant_of(id, player, guard).await? {
            self.explain_refusal(id, guard).await?;
        }

        Ok(())
    }

    /// An admin adds any existing player while no bracket exists and the
    /// tournament is not concluded. A player who is already in keeps their
    /// entry. A given level replaces theirs, so an admin can correct one; no
    /// level leaves theirs alone.
    pub async fn add_entrant(
        &self,
        id: TournamentId,
        player: PlayerId,
        skill: Option<SkillLevel>,
    ) -> Result<Outcome<Entrant>, ServiceError> {
        let tournament = self.load(id).await?;
        require_entrants_editable(&tournament)?;
        if !self.players.exists(player).await? {
            return Err(ServiceError::PlayerIdNotFound(player));
        }
        let guard = EntrantGuard {
            status: tournament.status,
            open_at: None,
        };

        self.enrol(id, player, skill, guard).await
    }

    /// Idempotent, while no bracket exists and the tournament is not concluded.
    pub async fn remove_entrant(
        &self,
        id: TournamentId,
        entrant: EntrantId,
    ) -> Result<(), ServiceError> {
        let tournament = self.load(id).await?;
        require_entrants_editable(&tournament)?;
        let guard = EntrantGuard {
            status: tournament.status,
            open_at: None,
        };
        if !self.storage.remove_entrant(id, entrant, guard).await? {
            self.explain_refusal(id, guard).await?;
        }

        Ok(())
    }

    /// Builds the bracket seeded by level, the strongest first, so that the
    /// bracket pairs neighbours of the same level in round one. Entrants of one
    /// level are shuffled first, so a regeneration gives a new draw among them.
    /// Runs again as long as no result was entered.
    pub async fn generate_bracket(&self, id: TournamentId) -> Result<Bracket, ServiceError> {
        let tournament = self.load_bracket_editable(id).await?;
        let mut entrants = self.storage.entrants(id).await?;
        entrants.shuffle(&mut rand::rng());
        // A stable sort keeps the shuffled order inside one level.
        entrants.sort_by_key(|entrant| Reverse(seeding_level(entrant)));
        let order: Vec<EntrantId> = entrants.into_iter().map(|entrant| entrant.id).collect();

        self.build_bracket(&tournament, &order).await
    }

    /// Rebuilds the bracket with the given seed order. Every entrant, one time.
    pub async fn reorder_seeds(
        &self,
        id: TournamentId,
        order: SeedOrder,
    ) -> Result<Bracket, ServiceError> {
        let tournament = self.load_bracket_editable(id).await?;
        if !tournament.has_bracket {
            return Err(ServiceError::BracketMissing);
        }
        let mut expected: Vec<EntrantId> = self
            .storage
            .entrants(id)
            .await?
            .into_iter()
            .map(|entrant| entrant.id)
            .collect();
        let mut given = order.entrants.clone();
        expected.sort_by_key(|entrant| entrant.into_inner());
        given.sort_by_key(|entrant| entrant.into_inner());
        if expected != given {
            return Err(ServiceError::SeedOrderMismatch);
        }

        self.build_bracket(&tournament, &order.entrants).await
    }

    /// Idempotent, as long as no result was entered.
    pub async fn delete_bracket(&self, id: TournamentId) -> Result<(), ServiceError> {
        let tournament = self.load(id).await?;
        require_not_concluded(&tournament)?;

        bracket_written(self.storage.delete_bracket(id, tournament.status).await?)
    }

    pub async fn report_result(
        &self,
        id: TournamentId,
        match_id: MatchId,
        winner: EntrantId,
    ) -> Result<Bracket, ServiceError> {
        let (tournament, read) = self.load_open_bracket(id, match_id).await?;
        let mut bracket = read.clone();
        let changed = bracket.report(match_id, winner)?;
        self.write_results(&tournament, &read, &changed).await?;

        Ok(bracket)
    }

    pub async fn clear_result(
        &self,
        id: TournamentId,
        match_id: MatchId,
    ) -> Result<Bracket, ServiceError> {
        let (tournament, read) = self.load_open_bracket(id, match_id).await?;
        let mut bracket = read.clone();
        let changed = bracket.clear(match_id)?;
        self.write_results(&tournament, &read, &changed).await?;

        Ok(bracket)
    }

    async fn load(&self, id: TournamentId) -> Result<Tournament, ServiceError> {
        self.storage
            .get(id)
            .await?
            .ok_or(ServiceError::TournamentNotFound(id))
    }

    async fn load_public(&self, id: TournamentId) -> Result<Tournament, ServiceError> {
        let tournament = self.load(id).await?;
        if tournament.status == TournamentStatus::Draft {
            return Err(ServiceError::TournamentNotFound(id));
        }
        Ok(tournament)
    }

    /// `None` when no match exists: the tournament has no bracket.
    async fn bracket(&self, id: TournamentId) -> Result<Option<Bracket>, ServiceError> {
        let matches = self.storage.matches(id).await?;
        if matches.is_empty() {
            return Ok(None);
        }
        Ok(Some(Bracket::from_matches(matches)?))
    }

    /// The winner and the awards of a move, before anything is written.
    async fn plan_transition(
        &self,
        tournament: &Tournament,
        change: &StatusChange,
    ) -> Result<(Option<EntrantId>, Vec<Award>), ServiceError> {
        use TournamentStatus::{Concluded, Draft, Live, Open};

        match (tournament.status, change.status) {
            (Draft, Open) | (Draft | Open, Live) => {
                refuse_winner(change)?;
                Ok((None, Vec::new()))
            }
            (Live, Open) if !tournament.has_bracket => {
                refuse_winner(change)?;
                Ok((None, Vec::new()))
            }
            (Draft | Open | Live, Concluded) => self.conclusion(tournament.id, change.winner).await,
            (from, to) => Err(ServiceError::InvalidTransition { from, to }),
        }
    }

    /// With a bracket the final names the winner. Without one the admin names
    /// the winner among the entrants, or nobody.
    async fn conclusion(
        &self,
        id: TournamentId,
        named: Option<EntrantId>,
    ) -> Result<(Option<EntrantId>, Vec<Award>), ServiceError> {
        let entrants = self.storage.entrants(id).await?;
        match self.bracket(id).await? {
            Some(bracket) => {
                if named.is_some() {
                    return Err(ServiceError::UnexpectedWinner);
                }
                let champion = bracket.champion().ok_or(ServiceError::BracketIncomplete)?;
                Ok((
                    Some(champion),
                    tournament_awards(id, &entrants, Some(&bracket), None),
                ))
            }
            None => {
                if let Some(winner) = named
                    && !entrants.iter().any(|e| e.id == winner)
                {
                    return Err(ServiceError::NotAnEntrant(winner));
                }
                Ok((named, tournament_awards(id, &entrants, None, named)))
            }
        }
    }

    /// The bracket of a tournament that takes results, for a result on one of
    /// its matches.
    async fn load_open_bracket(
        &self,
        id: TournamentId,
        match_id: MatchId,
    ) -> Result<(Tournament, Bracket), ServiceError> {
        let tournament = self.load(id).await?;
        require_not_concluded(&tournament)?;
        let bracket = self
            .bracket(id)
            .await?
            .ok_or(ServiceError::MatchNotFound(match_id))?;

        Ok((tournament, bracket))
    }

    /// Writes the matches a result changed, each one guarded by the value it
    /// held in `read`.
    async fn write_results(
        &self,
        tournament: &Tournament,
        read: &Bracket,
        changed: &[Match],
    ) -> Result<(), ServiceError> {
        let changes: Vec<(&Match, &Match)> = changed
            .iter()
            .filter_map(|after| Some((read.flat().find(|m| m.id == after.id)?, after)))
            .collect();
        let written = self
            .storage
            .update_matches(tournament.id, tournament.status, &changes)
            .await?;
        if !written {
            return Err(ServiceError::TournamentChanged);
        }

        Ok(())
    }

    /// Generation and reordering need a live tournament and no result yet.
    /// Storage checks both again when it writes.
    async fn load_bracket_editable(&self, id: TournamentId) -> Result<Tournament, ServiceError> {
        let tournament = self.load(id).await?;
        match tournament.status {
            TournamentStatus::Live => {}
            TournamentStatus::Concluded => return Err(ServiceError::TournamentConcluded),
            TournamentStatus::Draft | TournamentStatus::Open => return Err(ServiceError::NotLive),
        }
        if self.bracket(id).await?.is_some_and(|b| b.has_results()) {
            return Err(ServiceError::BracketLocked);
        }
        Ok(tournament)
    }

    async fn build_bracket(
        &self,
        tournament: &Tournament,
        order: &[EntrantId],
    ) -> Result<Bracket, ServiceError> {
        let bracket = Bracket::generate(order)?;
        let written = self
            .storage
            .replace_bracket(tournament.id, tournament.status, order, &bracket)
            .await?;
        bracket_written(written)?;

        Ok(bracket)
    }

    async fn enrol(
        &self,
        id: TournamentId,
        player: PlayerId,
        skill: Option<SkillLevel>,
        guard: EntrantGuard,
    ) -> Result<Outcome<Entrant>, ServiceError> {
        let written = self
            .storage
            .add_entrant(
                &NewEntrant {
                    id: EntrantId::generate(),
                    tournament: id,
                    player,
                    skill,
                    registered_at: OffsetDateTime::now_utc(),
                },
                guard,
            )
            .await;
        let created = match written {
            Ok(Enrolled::New) => true,
            Ok(Enrolled::Already) => {
                if let Some(skill) = skill
                    && !self.storage.set_skill(id, player, skill, guard).await?
                {
                    self.explain_refusal(id, guard).await?;
                }
                false
            }
            Ok(Enrolled::Refused) => {
                self.explain_refusal(id, guard).await?;
                return Err(ServiceError::TournamentFull);
            }
            // The guard found the tournament, so the missing row is the player.
            Err(StorageError::ForeignKeyViolation(_)) => {
                return Err(ServiceError::PlayerIdNotFound(player));
            }
            Err(error) => return Err(error.into()),
        };
        let entrant = self
            .storage
            .entrant_of(id, player)
            .await?
            .ok_or(ServiceError::PlayerIdNotFound(player))?;

        Ok(if created {
            Outcome::Created(entrant)
        } else {
            Outcome::Existing(entrant)
        })
    }

    /// Says which rule a guarded entrant write broke, from the tournament as it
    /// is now. `Ok` means the state still matches the guard, so the write found
    /// no row to touch.
    async fn explain_refusal(
        &self,
        id: TournamentId,
        guard: EntrantGuard,
    ) -> Result<(), ServiceError> {
        let fresh = self.load(id).await?;
        require_entrants_editable(&fresh)?;
        if let Some(now) = guard.open_at {
            return require_registration_open(&fresh, now);
        }
        if fresh.status != guard.status {
            return Err(ServiceError::TournamentChanged);
        }
        Ok(())
    }

    /// The error for a guarded write that found the tournament changed.
    async fn stale(&self, id: TournamentId) -> ServiceError {
        match self.load(id).await {
            Ok(fresh) if fresh.status == TournamentStatus::Concluded => {
                ServiceError::TournamentConcluded
            }
            Ok(_) => ServiceError::TournamentChanged,
            Err(error) => error,
        }
    }
}

fn seeding_level(entrant: &Entrant) -> u8 {
    entrant.skill.map_or(SkillLevel::UNRATED, SkillLevel::value)
}

fn require_registration_open(
    tournament: &Tournament,
    now: OffsetDateTime,
) -> Result<(), ServiceError> {
    if tournament.status == TournamentStatus::Open && now < tournament.registration_closes_at {
        Ok(())
    } else {
        Err(ServiceError::RegistrationClosed)
    }
}

fn require_not_concluded(tournament: &Tournament) -> Result<(), ServiceError> {
    if tournament.status == TournamentStatus::Concluded {
        return Err(ServiceError::TournamentConcluded);
    }
    Ok(())
}

/// A bracket fixes the field, and a concluded tournament is final.
fn require_entrants_editable(tournament: &Tournament) -> Result<(), ServiceError> {
    require_not_concluded(tournament)?;
    if tournament.has_bracket {
        return Err(ServiceError::BracketExists);
    }
    Ok(())
}

fn refuse_winner(change: &StatusChange) -> Result<(), ServiceError> {
    if change.winner.is_some() {
        return Err(ServiceError::UnexpectedWinner);
    }
    Ok(())
}

fn bracket_written(written: BracketWrite) -> Result<(), ServiceError> {
    match written {
        BracketWrite::Done => Ok(()),
        BracketWrite::Locked => Err(ServiceError::BracketLocked),
        BracketWrite::Changed => Err(ServiceError::TournamentChanged),
    }
}
