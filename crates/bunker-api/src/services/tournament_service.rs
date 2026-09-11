use bunker_models::{
    Bracket, Entrant, EntrantId, MatchId, NewTournament, PageQuery, Paginated, PlayerId, SeedOrder,
    StatusChange, Tournament, TournamentDetail, TournamentId, TournamentStatus, TournamentUpdate,
};
use rand::seq::SliceRandom;
use time::OffsetDateTime;

use crate::storage::{Enrolled, NewEntrant, NewTournamentRow, PlayerStorage, TournamentStorage};

use super::error::ServiceError;

/// The rules of a tournament: who may enter, when the status moves, and what a
/// bracket accepts.
///
/// Each rule reads first and writes second, in two statements. One admin at a
/// time, WAL serialisation and the CHECK and foreign key constraints make the
/// gap harmless: the worst case is a refused write, never a broken row.
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
        include_drafts: bool,
    ) -> Result<Paginated<Tournament>, ServiceError> {
        Ok(self.storage.list(query, include_drafts).await?)
    }

    /// A draft is not found for anyone but an admin.
    pub async fn detail(
        &self,
        id: TournamentId,
        include_drafts: bool,
    ) -> Result<TournamentDetail, ServiceError> {
        let tournament = self.load(id).await?;
        if tournament.status == TournamentStatus::Draft && !include_drafts {
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

    pub async fn update(
        &self,
        id: TournamentId,
        update: TournamentUpdate,
    ) -> Result<Tournament, ServiceError> {
        if !self.storage.update(id, &update).await? {
            return Err(ServiceError::TournamentNotFound(id));
        }

        self.load(id).await
    }

    /// Idempotent. Entrants and matches go with the tournament.
    pub async fn delete(&self, id: TournamentId) -> Result<(), ServiceError> {
        self.storage.delete(id).await?;

        Ok(())
    }

    /// The state machine. `concluded` is final. A bracket decides the winner;
    /// without one the admin names the winner, or nobody.
    pub async fn change_status(
        &self,
        id: TournamentId,
        change: StatusChange,
    ) -> Result<Tournament, ServiceError> {
        let tournament = self.load(id).await?;
        let (from, to) = (tournament.status, change.status);
        let transition = ServiceError::InvalidTransition { from, to };

        use TournamentStatus::{Concluded, Draft, Live, Open};
        let winner = match (from, to) {
            (Draft, Open) | (Draft | Open, Live) => {
                if change.winner.is_some() {
                    return Err(ServiceError::UnexpectedWinner);
                }
                None
            }
            (Live, Open) => {
                if tournament.has_bracket {
                    return Err(transition);
                }
                if change.winner.is_some() {
                    return Err(ServiceError::UnexpectedWinner);
                }
                None
            }
            (Draft | Open | Live, Concluded) => {
                if tournament.has_bracket {
                    if change.winner.is_some() {
                        return Err(ServiceError::UnexpectedWinner);
                    }
                    let bracket = self
                        .bracket(id)
                        .await?
                        .ok_or(ServiceError::BracketMissing)?;
                    Some(bracket.champion().ok_or(ServiceError::BracketIncomplete)?)
                } else {
                    match change.winner {
                        Some(winner) => {
                            self.require_entrant(id, winner).await?;
                            Some(winner)
                        }
                        None => None,
                    }
                }
            }
            _ => return Err(transition),
        };

        self.storage.set_status(id, to, winner).await?;

        self.load(id).await
    }

    /// A player enters while registration is open. A second call answers the
    /// same entrant.
    pub async fn register(
        &self,
        id: TournamentId,
        player: PlayerId,
    ) -> Result<Entrant, ServiceError> {
        let tournament = self.load_public(id).await?;
        require_registration_open(&tournament)?;

        self.enrol(id, player).await.map(|(entrant, _)| entrant)
    }

    /// Idempotent, and only while registration is open. After the deadline the
    /// entrants are fixed for the bracket.
    pub async fn retire(&self, id: TournamentId, player: PlayerId) -> Result<(), ServiceError> {
        let tournament = self.load_public(id).await?;
        require_registration_open(&tournament)?;
        self.storage.remove_entrant_of(id, player).await?;

        Ok(())
    }

    /// An admin adds any existing player, in any status, while no bracket exists.
    pub async fn add_entrant(
        &self,
        id: TournamentId,
        player: PlayerId,
    ) -> Result<(Entrant, Enrolled), ServiceError> {
        let tournament = self.load(id).await?;
        if tournament.has_bracket {
            return Err(ServiceError::BracketExists);
        }
        if self.players.get_by_id(player).await?.is_none() {
            return Err(ServiceError::PlayerIdNotFound(player));
        }

        self.enrol(id, player).await
    }

    /// Idempotent. The winner of a concluded tournament stays.
    pub async fn remove_entrant(
        &self,
        id: TournamentId,
        entrant: EntrantId,
    ) -> Result<(), ServiceError> {
        let tournament = self.load(id).await?;
        if tournament.has_bracket {
            return Err(ServiceError::BracketExists);
        }
        if tournament.winner.is_some_and(|w| w.id == entrant) {
            return Err(ServiceError::WinnerCannotLeave);
        }
        self.storage.remove_entrant(id, entrant).await?;

        Ok(())
    }

    /// Builds the bracket from a random order of the entrants. Runs again as
    /// long as no result was entered.
    pub async fn generate_bracket(&self, id: TournamentId) -> Result<Bracket, ServiceError> {
        self.require_bracket_editable(id).await?;
        let mut order: Vec<EntrantId> = self
            .storage
            .entrants(id)
            .await?
            .into_iter()
            .map(|entrant| entrant.id)
            .collect();
        order.shuffle(&mut rand::rng());

        self.build_bracket(id, &order).await
    }

    /// Rebuilds the bracket with the given seed order. Every entrant, one time.
    pub async fn reorder_seeds(
        &self,
        id: TournamentId,
        order: SeedOrder,
    ) -> Result<Bracket, ServiceError> {
        self.require_bracket_editable(id).await?;
        if self.bracket(id).await?.is_none() {
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

        self.build_bracket(id, &order.entrants).await
    }

    /// Idempotent, as long as no result was entered.
    pub async fn delete_bracket(&self, id: TournamentId) -> Result<(), ServiceError> {
        let tournament = self.load(id).await?;
        if tournament.status == TournamentStatus::Concluded {
            return Err(ServiceError::TournamentConcluded);
        }
        if let Some(bracket) = self.bracket(id).await? {
            if bracket.has_results() {
                return Err(ServiceError::BracketLocked);
            }
            self.storage.delete_bracket(id).await?;
        }

        Ok(())
    }

    pub async fn report_result(
        &self,
        id: TournamentId,
        match_id: MatchId,
        winner: EntrantId,
    ) -> Result<Bracket, ServiceError> {
        let mut bracket = self.load_open_bracket(id, match_id).await?;
        let changed = bracket.report(match_id, winner)?;
        self.storage.update_matches(&changed).await?;

        Ok(bracket)
    }

    pub async fn clear_result(
        &self,
        id: TournamentId,
        match_id: MatchId,
    ) -> Result<Bracket, ServiceError> {
        let mut bracket = self.load_open_bracket(id, match_id).await?;
        let changed = bracket.clear(match_id)?;
        self.storage.update_matches(&changed).await?;

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
        Ok((!matches.is_empty()).then(|| Bracket::from_matches(matches)))
    }

    /// The bracket of a live tournament, for a result on one of its matches.
    async fn load_open_bracket(
        &self,
        id: TournamentId,
        match_id: MatchId,
    ) -> Result<Bracket, ServiceError> {
        let tournament = self.load(id).await?;
        if tournament.status == TournamentStatus::Concluded {
            return Err(ServiceError::TournamentConcluded);
        }
        self.bracket(id)
            .await?
            .ok_or(ServiceError::MatchNotFound(match_id))
    }

    /// Generation and reordering need a live tournament and no result yet.
    async fn require_bracket_editable(&self, id: TournamentId) -> Result<(), ServiceError> {
        let tournament = self.load(id).await?;
        match tournament.status {
            TournamentStatus::Live => {}
            TournamentStatus::Concluded => return Err(ServiceError::TournamentConcluded),
            TournamentStatus::Draft | TournamentStatus::Open => return Err(ServiceError::NotLive),
        }
        if self.bracket(id).await?.is_some_and(|b| b.has_results()) {
            return Err(ServiceError::BracketLocked);
        }
        Ok(())
    }

    async fn build_bracket(
        &self,
        id: TournamentId,
        order: &[EntrantId],
    ) -> Result<Bracket, ServiceError> {
        let bracket = Bracket::generate(order)?;
        self.storage.replace_bracket(id, order, &bracket).await?;

        Ok(bracket)
    }

    async fn require_entrant(
        &self,
        id: TournamentId,
        entrant: EntrantId,
    ) -> Result<(), ServiceError> {
        let entrants = self.storage.entrants(id).await?;
        if entrants.iter().any(|e| e.id == entrant) {
            Ok(())
        } else {
            Err(ServiceError::NotAnEntrant(entrant))
        }
    }

    async fn enrol(
        &self,
        id: TournamentId,
        player: PlayerId,
    ) -> Result<(Entrant, Enrolled), ServiceError> {
        let enrolled = self
            .storage
            .add_entrant(&NewEntrant {
                id: EntrantId::generate(),
                tournament: id,
                player,
                registered_at: OffsetDateTime::now_utc(),
            })
            .await?;
        let entrant = self
            .storage
            .entrant_of(id, player)
            .await?
            .ok_or(ServiceError::PlayerIdNotFound(player))?;

        Ok((entrant, enrolled))
    }
}

fn require_registration_open(tournament: &Tournament) -> Result<(), ServiceError> {
    let open = tournament.status == TournamentStatus::Open
        && OffsetDateTime::now_utc() < tournament.registration_closes_at;
    if open {
        Ok(())
    } else {
        Err(ServiceError::RegistrationClosed)
    }
}
