use bunker_models::{
    Award, Bracket, Description, Entrant, EntrantId, GameMode, GameName, MAX_ENTRANTS, Match,
    MatchId, PageQuery, Paginated, PlayerId, SkillLevel, Tournament, TournamentId, TournamentName,
    TournamentStatus, TournamentUpdate,
};
use time::{Date, OffsetDateTime};

use super::db::{DbPool, begin_write};
use super::error::StorageError;
use super::point_storage::{AwardSource, insert_awards};
use super::row::{
    DAY, MalformedField, OptionalPlayerRow, from_micros, parse_day, parse_uuid, to_micros,
};

const TOURNAMENTS: &str = "tournaments";
const ENTRANTS: &str = "tournament_entrants";
const MATCHES: &str = "matches";

/// The unique index on `(tournament_id, player_id)`, as SQLite names it.
const ENTRANT_CONSTRAINT: &str = "tournament_entrants.tournament_id, tournament_entrants.player_id";

/// The most tournaments one read of a player's registrations returns. Years of
/// nights fit, and the newest come first, so a cut drops the oldest.
const REGISTRATIONS_MAX: i64 = 1000;

#[derive(Debug, Clone)]
pub struct NewTournamentRow {
    pub id: TournamentId,
    pub name: TournamentName,
    pub game: GameName,
    pub mode: GameMode,
    pub description: Description,
    pub date: Date,
    pub registration_closes_at: OffsetDateTime,
    pub created_at: OffsetDateTime,
}

#[derive(Debug, Clone)]
pub struct NewEntrant {
    pub id: EntrantId,
    pub tournament: TournamentId,
    pub player: PlayerId,
    pub skill: Option<SkillLevel>,
    pub registered_at: OffsetDateTime,
}

/// What an insert of an entrant did. A second registration and a refused guard
/// are results, not failures, and the service says what they mean.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Enrolled {
    New,
    Already,
    Refused,
}

/// The state of a tournament that an entrant write expects. The write checks it
/// in the statement that writes, so a status change, a new bracket or a passed
/// deadline between the read and the write refuses the write.
///
/// Every entrant write also needs the tournament to have no bracket, and an
/// insert needs a free place under `MAX_ENTRANTS`: a bracket fixes the field,
/// and the field has a size.
#[derive(Debug, Clone, Copy)]
pub struct EntrantGuard {
    pub status: TournamentStatus,
    /// The deadline must still be after this instant. `None` ignores it.
    pub open_at: Option<OffsetDateTime>,
}

/// What a write of the whole bracket did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BracketWrite {
    Done,
    /// A match holds a result, so the bracket stays.
    Locked,
    /// The status or the entrants changed since the service read them.
    Changed,
}

#[derive(Debug, Clone)]
pub struct TournamentStorage {
    pool: DbPool,
}

impl TournamentStorage {
    pub const fn new(pool: DbPool) -> Self {
        Self { pool }
    }

    pub async fn create(&self, row: &NewTournamentRow) -> Result<(), StorageError> {
        let id = row.id.into_inner().to_string();
        let name = row.name.as_ref();
        let game = row.game.as_ref();
        let mode = row.mode.as_ref();
        let description = row.description.as_ref();
        let date = format_day(row.date)?;
        let closes_at = to_micros(TOURNAMENTS, row.registration_closes_at)?;
        let created_at = to_micros(TOURNAMENTS, row.created_at)?;
        let status = TournamentStatus::Draft.as_str();

        sqlx::query!(
            "insert into tournaments (id, name, game, mode, description, date, registration_closes_at, status, created_at)
             values (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            id,
            name,
            game,
            mode,
            description,
            date,
            closes_at,
            status,
            created_at,
        )
        .execute(&self.pool)
        .await
        .map_err(StorageError::from_query)?;

        Ok(())
    }

    /// `None` when no row has the id.
    pub async fn get(&self, id: TournamentId) -> Result<Option<Tournament>, StorageError> {
        let id = id.into_inner().to_string();
        let row = sqlx::query_as!(
            TournamentRow,
            r#"select t.id, t.name, t.game, t.mode, t.description, t.date,
                      t.registration_closes_at, t.status, t.created_at,
                      (select count(*) from tournament_entrants e where e.tournament_id = t.id) as "entrant_count!: i64",
                      exists(select 1 from matches m where m.tournament_id = t.id) as "has_bracket!: i64",
                      w.id as "winner_id?", w.seed as "winner_seed?", w.skill as "winner_skill?",
                      w.registered_at as "winner_registered_at?",
                      p.id as "winner_player_id?", p.handle as "winner_handle?",
                      p.glyph_bits as "winner_glyph_bits?", p.glyph_color as "winner_glyph_color?",
                      p.role as "winner_role?", p.created_at as "winner_player_created_at?",
                      s.cycles as "winner_cycles?: i64", s.place as "winner_place?: i64", s.players as "winner_players?: i64"
               from tournaments t
               left join tournament_entrants w on w.id = t.winner_entrant_id
               left join players p on p.id = w.player_id
               left join player_standings s on s.player_id = p.id
               where t.id = ?1"#,
            id,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(StorageError::from_query)?;

        row.map(TryInto::try_into).transpose()
    }

    /// Newest event first. `include_drafts` is the admin view; the public list
    /// never shows a draft.
    pub async fn list(
        &self,
        query: PageQuery,
        include_drafts: bool,
    ) -> Result<Paginated<Tournament>, StorageError> {
        let limit = query.limit();
        let offset = query.offset();
        let drafts = i64::from(include_drafts);
        let draft = TournamentStatus::Draft.as_str();

        let mut tx = self.pool.begin().await.map_err(StorageError::from_query)?;
        let total = sqlx::query_scalar!(
            "select count(*) from tournaments where ?1 = 1 or status != ?2",
            drafts,
            draft,
        )
        .fetch_one(&mut *tx)
        .await
        .map_err(StorageError::from_query)?;
        let rows = sqlx::query_as!(
            TournamentRow,
            r#"select t.id, t.name, t.game, t.mode, t.description, t.date,
                      t.registration_closes_at, t.status, t.created_at,
                      (select count(*) from tournament_entrants e where e.tournament_id = t.id) as "entrant_count!: i64",
                      exists(select 1 from matches m where m.tournament_id = t.id) as "has_bracket!: i64",
                      w.id as "winner_id?", w.seed as "winner_seed?", w.skill as "winner_skill?",
                      w.registered_at as "winner_registered_at?",
                      p.id as "winner_player_id?", p.handle as "winner_handle?",
                      p.glyph_bits as "winner_glyph_bits?", p.glyph_color as "winner_glyph_color?",
                      p.role as "winner_role?", p.created_at as "winner_player_created_at?",
                      s.cycles as "winner_cycles?: i64", s.place as "winner_place?: i64", s.players as "winner_players?: i64"
               from tournaments t
               left join tournament_entrants w on w.id = t.winner_entrant_id
               left join players p on p.id = w.player_id
               left join player_standings s on s.player_id = p.id
               where ?1 = 1 or t.status != ?2
               order by t.date desc, t.created_at desc, t.id asc
               limit ?3 offset ?4"#,
            drafts,
            draft,
            limit,
            offset,
        )
        .fetch_all(&mut *tx)
        .await
        .map_err(StorageError::from_query)?;
        tx.commit().await.map_err(StorageError::from_query)?;

        let items = rows
            .into_iter()
            .map(TryInto::try_into)
            .collect::<Result<Vec<_>, _>>()?;

        Ok(Paginated::new(
            items,
            u64::try_from(total).unwrap_or_default(),
            query,
        ))
    }

    /// `false` when no row has the id and `expected` status. An absent field
    /// keeps its value.
    pub async fn update(
        &self,
        id: TournamentId,
        expected: TournamentStatus,
        update: &TournamentUpdate,
    ) -> Result<bool, StorageError> {
        let id = id.into_inner().to_string();
        let expected = expected.as_str();
        let name = update.name.as_ref().map(AsRef::<str>::as_ref);
        let game = update.game.as_ref().map(AsRef::<str>::as_ref);
        let mode = update.mode.as_ref().map(AsRef::<str>::as_ref);
        let description = update.description.as_ref().map(AsRef::<str>::as_ref);
        let date = update.date.map(format_day).transpose()?;
        let closes_at = update
            .registration_closes_at
            .map(|at| to_micros(TOURNAMENTS, at))
            .transpose()?;

        let result = sqlx::query!(
            "update tournaments
             set name = coalesce(?1, name),
                 game = coalesce(?2, game),
                 mode = coalesce(?3, mode),
                 description = coalesce(?4, description),
                 date = coalesce(?5, date),
                 registration_closes_at = coalesce(?6, registration_closes_at)
             where id = ?7 and status = ?8",
            name,
            game,
            mode,
            description,
            date,
            closes_at,
            id,
            expected,
        )
        .execute(&self.pool)
        .await
        .map_err(StorageError::from_query)?;

        Ok(result.rows_affected() > 0)
    }

    /// Moves the status from `from` to `to`, with the winner and the cycles the
    /// move pays, in one transaction: a concluded tournament is never half paid.
    /// `false` when the status is no longer `from`, and then nothing is written,
    /// so two concurrent conclusions pay one time.
    pub async fn set_status(
        &self,
        id: TournamentId,
        from: TournamentStatus,
        to: TournamentStatus,
        winner: Option<EntrantId>,
        awards: &[Award],
        at: OffsetDateTime,
    ) -> Result<bool, StorageError> {
        let tournament = id.into_inner().to_string();
        let from = from.as_str();
        let to = to.as_str();
        let winner = winner.map(|w| w.into_inner().to_string());

        let mut tx = begin_write(&self.pool).await?;
        let result = sqlx::query!(
            "update tournaments set status = ?1, winner_entrant_id = ?2 where id = ?3 and status = ?4",
            to,
            winner,
            tournament,
            from,
        )
        .execute(&mut *tx)
        .await
        .map_err(StorageError::from_query)?;
        if result.rows_affected() == 0 {
            return Ok(false);
        }
        insert_awards(&mut tx, AwardSource::Tournament(id), awards, at).await?;
        tx.commit().await.map_err(StorageError::from_query)?;

        Ok(true)
    }

    /// Entrants and matches go with it. `true` when a row was removed.
    pub async fn delete(&self, id: TournamentId) -> Result<bool, StorageError> {
        let id = id.into_inner().to_string();
        let result = sqlx::query!("delete from tournaments where id = ?1", id)
            .execute(&self.pool)
            .await
            .map_err(StorageError::from_query)?;

        Ok(result.rows_affected() > 0)
    }

    /// Seeded entrants first in seed order, then the rest by registration.
    pub async fn entrants(&self, tournament: TournamentId) -> Result<Vec<Entrant>, StorageError> {
        let tournament = tournament.into_inner().to_string();
        let limit = i64::from(MAX_ENTRANTS);
        let rows = sqlx::query_as!(
            EntrantRow,
            r#"select e.id, e.seed, e.skill, e.registered_at,
                      p.id as "player_id?", p.handle as "handle?", p.glyph_bits as "glyph_bits?",
                      p.glyph_color as "glyph_color?", p.role as "role?", p.created_at as "player_created_at?",
                      s.cycles as "cycles?: i64", s.place as "place?: i64", s.players as "players?: i64"
               from tournament_entrants e
               left join players p on p.id = e.player_id
               left join player_standings s on s.player_id = p.id
               where e.tournament_id = ?1
               order by e.seed is null, e.seed asc, e.registered_at asc, e.id asc
               limit ?2"#,
            tournament,
            limit,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(StorageError::from_query)?;

        rows.into_iter().map(TryInto::try_into).collect()
    }

    pub async fn entrant_of(
        &self,
        tournament: TournamentId,
        player: PlayerId,
    ) -> Result<Option<Entrant>, StorageError> {
        let tournament = tournament.into_inner().to_string();
        let player = player.into_inner().to_string();
        let row = sqlx::query_as!(
            EntrantRow,
            r#"select e.id, e.seed, e.skill, e.registered_at,
                      p.id as "player_id?", p.handle as "handle?", p.glyph_bits as "glyph_bits?",
                      p.glyph_color as "glyph_color?", p.role as "role?", p.created_at as "player_created_at?",
                      s.cycles as "cycles?: i64", s.place as "place?: i64", s.players as "players?: i64"
               from tournament_entrants e
               left join players p on p.id = e.player_id
               left join player_standings s on s.player_id = p.id
               where e.tournament_id = ?1 and e.player_id = ?2"#,
            tournament,
            player,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(StorageError::from_query)?;

        row.map(TryInto::try_into).transpose()
    }

    /// The tournaments the player entered, in any status, the newest entry
    /// first, up to [`REGISTRATIONS_MAX`].
    pub async fn tournaments_of(
        &self,
        player: PlayerId,
    ) -> Result<Vec<TournamentId>, StorageError> {
        let player = player.into_inner().to_string();
        let rows = sqlx::query_scalar!(
            "select tournament_id from tournament_entrants where player_id = ?1
             order by registered_at desc limit ?2",
            player,
            REGISTRATIONS_MAX,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(StorageError::from_query)?;

        rows.into_iter()
            .map(|raw| parse_uuid(ENTRANTS, &raw).map(TournamentId::new))
            .collect()
    }

    pub async fn add_entrant(
        &self,
        entrant: &NewEntrant,
        guard: EntrantGuard,
    ) -> Result<Enrolled, StorageError> {
        let id = entrant.id.into_inner().to_string();
        let tournament = entrant.tournament.into_inner().to_string();
        let player = entrant.player.into_inner().to_string();
        let skill = entrant.skill.map(skill_column);
        let registered_at = to_micros(ENTRANTS, entrant.registered_at)?;
        let status = guard.status.as_str();
        let open_at = guard
            .open_at
            .map(|at| to_micros(TOURNAMENTS, at))
            .transpose()?;
        let max = i64::from(MAX_ENTRANTS);

        // A player already in keeps the place, so a full field still lets them
        // reach the unique index and answer `Already`.
        let inserted = sqlx::query!(
            "insert into tournament_entrants (id, tournament_id, player_id, skill, registered_at)
             select ?1, ?2, ?3, ?4, ?5
             where exists (
                 select 1 from tournaments t
                 where t.id = ?2 and t.status = ?6
                   and (?7 is null or t.registration_closes_at > ?7)
                   and not exists (select 1 from matches m where m.tournament_id = t.id)
                   and ((select count(*) from tournament_entrants e where e.tournament_id = t.id) < ?8
                        or exists (select 1 from tournament_entrants e
                                   where e.tournament_id = t.id and e.player_id = ?3)))",
            id,
            tournament,
            player,
            skill,
            registered_at,
            status,
            open_at,
            max,
        )
        .execute(&self.pool)
        .await;

        match inserted {
            Ok(result) if result.rows_affected() == 0 => Ok(Enrolled::Refused),
            Ok(_) => Ok(Enrolled::New),
            Err(error) => match StorageError::from_query(error) {
                StorageError::UniqueViolation { constraint, .. }
                    if constraint == ENTRANT_CONSTRAINT =>
                {
                    Ok(Enrolled::Already)
                }
                other => Err(other),
            },
        }
    }

    /// The level of the entry of `player`. `false` when the guard refuses, or
    /// when the player has no entry.
    pub async fn set_skill(
        &self,
        tournament: TournamentId,
        player: PlayerId,
        skill: SkillLevel,
        guard: EntrantGuard,
    ) -> Result<bool, StorageError> {
        let tournament = tournament.into_inner().to_string();
        let player = player.into_inner().to_string();
        let skill = skill_column(skill);
        let status = guard.status.as_str();
        let open_at = guard
            .open_at
            .map(|at| to_micros(TOURNAMENTS, at))
            .transpose()?;
        let result = sqlx::query!(
            "update tournament_entrants set skill = ?1
             where tournament_id = ?2 and player_id = ?3
               and exists (
                   select 1 from tournaments t
                   where t.id = ?2 and t.status = ?4
                     and (?5 is null or t.registration_closes_at > ?5)
                     and not exists (select 1 from matches m where m.tournament_id = t.id))",
            skill,
            tournament,
            player,
            status,
            open_at,
        )
        .execute(&self.pool)
        .await
        .map_err(StorageError::from_query)?;

        Ok(result.rows_affected() > 0)
    }

    /// `true` when a row was removed. `false` when the guard refuses, or when
    /// there was no such entrant.
    pub async fn remove_entrant(
        &self,
        tournament: TournamentId,
        entrant: EntrantId,
        guard: EntrantGuard,
    ) -> Result<bool, StorageError> {
        let tournament = tournament.into_inner().to_string();
        let entrant = entrant.into_inner().to_string();
        let status = guard.status.as_str();
        let open_at = guard
            .open_at
            .map(|at| to_micros(TOURNAMENTS, at))
            .transpose()?;
        let result = sqlx::query!(
            "delete from tournament_entrants
             where tournament_id = ?1 and id = ?2
               and exists (
                   select 1 from tournaments t
                   where t.id = ?1 and t.status = ?3
                     and (?4 is null or t.registration_closes_at > ?4)
                     and not exists (select 1 from matches m where m.tournament_id = t.id))",
            tournament,
            entrant,
            status,
            open_at,
        )
        .execute(&self.pool)
        .await
        .map_err(StorageError::from_query)?;

        Ok(result.rows_affected() > 0)
    }

    /// The same as [`Self::remove_entrant`], by the player of the entry.
    pub async fn remove_entrant_of(
        &self,
        tournament: TournamentId,
        player: PlayerId,
        guard: EntrantGuard,
    ) -> Result<bool, StorageError> {
        let tournament = tournament.into_inner().to_string();
        let player = player.into_inner().to_string();
        let status = guard.status.as_str();
        let open_at = guard
            .open_at
            .map(|at| to_micros(TOURNAMENTS, at))
            .transpose()?;
        let result = sqlx::query!(
            "delete from tournament_entrants
             where tournament_id = ?1 and player_id = ?2
               and exists (
                   select 1 from tournaments t
                   where t.id = ?1 and t.status = ?3
                     and (?4 is null or t.registration_closes_at > ?4)
                     and not exists (select 1 from matches m where m.tournament_id = t.id))",
            tournament,
            player,
            status,
            open_at,
        )
        .execute(&self.pool)
        .await
        .map_err(StorageError::from_query)?;

        Ok(result.rows_affected() > 0)
    }

    /// Every match, in round and slot order. Empty when there is no bracket. A
    /// bracket of `MAX_ENTRANTS` has one match less than that.
    pub async fn matches(&self, tournament: TournamentId) -> Result<Vec<Match>, StorageError> {
        let tournament = tournament.into_inner().to_string();
        let limit = i64::from(MAX_ENTRANTS);
        let rows = sqlx::query_as!(
            MatchRow,
            "select id, round, slot, entrant_a, entrant_b, winner
             from matches where tournament_id = ?1 order by round asc, slot asc limit ?2",
            tournament,
            limit,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(StorageError::from_query)?;

        rows.into_iter().map(TryInto::try_into).collect()
    }

    /// Replaces the whole bracket: the seeds follow `order`, and the old matches
    /// go away. One transaction, so a reader never sees half a bracket. The
    /// status and the results are read inside it, so a result entered or a
    /// status moved since the service looked refuses the write.
    pub async fn replace_bracket(
        &self,
        tournament: TournamentId,
        expected: TournamentStatus,
        order: &[EntrantId],
        bracket: &Bracket,
    ) -> Result<BracketWrite, StorageError> {
        let id = tournament;
        let tournament = tournament.into_inner().to_string();

        let mut tx = begin_write(&self.pool).await?;
        let blocked = bracket_blocked(&mut tx, id, expected).await?;
        if blocked != BracketWrite::Done {
            return Ok(blocked);
        }
        sqlx::query!(
            "update tournament_entrants set seed = null where tournament_id = ?1",
            tournament
        )
        .execute(&mut *tx)
        .await
        .map_err(StorageError::from_query)?;
        for (seed, entrant) in (1_i64..).zip(order) {
            let entrant = entrant.into_inner().to_string();
            let updated = sqlx::query!(
                "update tournament_entrants set seed = ?1 where tournament_id = ?2 and id = ?3",
                seed,
                tournament,
                entrant,
            )
            .execute(&mut *tx)
            .await
            .map_err(StorageError::from_query)?;
            // The service checked the order against the entrants it read. A miss
            // means one left since, so the whole write is dropped.
            if updated.rows_affected() != 1 {
                return Ok(BracketWrite::Changed);
            }
        }
        sqlx::query!("delete from matches where tournament_id = ?1", tournament)
            .execute(&mut *tx)
            .await
            .map_err(StorageError::from_query)?;
        for m in bracket.flat() {
            let id = m.id.into_inner().to_string();
            let round = i64::from(m.round);
            let slot = i64::from(m.slot);
            let a = m.entrant_a.map(|e| e.into_inner().to_string());
            let b = m.entrant_b.map(|e| e.into_inner().to_string());
            let winner = m.winner.map(|e| e.into_inner().to_string());
            sqlx::query!(
                "insert into matches (id, tournament_id, round, slot, entrant_a, entrant_b, winner)
                 values (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                id,
                tournament,
                round,
                slot,
                a,
                b,
                winner,
            )
            .execute(&mut *tx)
            .await
            .map_err(StorageError::from_query)?;
        }
        tx.commit().await.map_err(StorageError::from_query)?;

        Ok(BracketWrite::Done)
    }

    /// Removes the matches and the seeds, under the same guard as
    /// [`Self::replace_bracket`]. A tournament with no bracket is left as it was.
    pub async fn delete_bracket(
        &self,
        tournament: TournamentId,
        expected: TournamentStatus,
    ) -> Result<BracketWrite, StorageError> {
        let id = tournament;
        let tournament = tournament.into_inner().to_string();

        let mut tx = begin_write(&self.pool).await?;
        let blocked = bracket_blocked(&mut tx, id, expected).await?;
        if blocked != BracketWrite::Done {
            return Ok(blocked);
        }
        sqlx::query!("delete from matches where tournament_id = ?1", tournament)
            .execute(&mut *tx)
            .await
            .map_err(StorageError::from_query)?;
        sqlx::query!(
            "update tournament_entrants set seed = null where tournament_id = ?1",
            tournament
        )
        .execute(&mut *tx)
        .await
        .map_err(StorageError::from_query)?;
        tx.commit().await.map_err(StorageError::from_query)?;

        Ok(BracketWrite::Done)
    }

    /// Writes each change of a result: the match as the service read it, and the
    /// match as it must become. A row that no longer holds the value read, or
    /// that is not in this tournament, refuses the whole write: `false`. So a
    /// result entered while the bracket is rebuilt, or two results that feed
    /// one match at once, never lose a write in silence.
    pub async fn update_matches(
        &self,
        tournament: TournamentId,
        expected: TournamentStatus,
        changes: &[(&Match, &Match)],
    ) -> Result<bool, StorageError> {
        let tournament_key = tournament.into_inner().to_string();
        let expected_status = expected.as_str();

        let mut tx = begin_write(&self.pool).await?;
        let status_holds = sqlx::query_scalar!(
            r#"select exists(select 1 from tournaments where id = ?1 and status = ?2) as "holds!: bool""#,
            tournament_key,
            expected_status,
        )
        .fetch_one(&mut *tx)
        .await
        .map_err(StorageError::from_query)?;
        if !status_holds {
            return Ok(false);
        }
        for (before, after) in changes {
            let id = after.id.into_inner().to_string();
            let side = |entrant: Option<EntrantId>| entrant.map(|e| e.into_inner().to_string());
            let (a, b, winner) = (
                side(after.entrant_a),
                side(after.entrant_b),
                side(after.winner),
            );
            let (old_a, old_b, old_winner) = (
                side(before.entrant_a),
                side(before.entrant_b),
                side(before.winner),
            );
            let updated = sqlx::query!(
                "update matches set entrant_a = ?1, entrant_b = ?2, winner = ?3
                 where id = ?4 and tournament_id = ?5
                   and entrant_a is ?6 and entrant_b is ?7 and winner is ?8",
                a,
                b,
                winner,
                id,
                tournament_key,
                old_a,
                old_b,
                old_winner,
            )
            .execute(&mut *tx)
            .await
            .map_err(StorageError::from_query)?;
            if updated.rows_affected() != 1 {
                return Ok(false);
            }
        }
        tx.commit().await.map_err(StorageError::from_query)?;

        Ok(true)
    }
}

/// `Done` when the tournament still has the `expected` status and no match has
/// a result, which is what a rebuild and a removal of the bracket need.
async fn bracket_blocked(
    tx: &mut sqlx::SqliteConnection,
    tournament: TournamentId,
    expected: TournamentStatus,
) -> Result<BracketWrite, StorageError> {
    let tournament = tournament.into_inner().to_string();
    let expected = expected.as_str();
    let row = sqlx::query!(
        r#"select exists(select 1 from tournaments where id = ?1 and status = ?2) as "status_holds!: bool",
                  exists(select 1 from matches
                         where tournament_id = ?1 and winner is not null
                           and entrant_a is not null and entrant_b is not null) as "played!: bool""#,
        tournament,
        expected,
    )
    .fetch_one(&mut *tx)
    .await
    .map_err(StorageError::from_query)?;

    Ok(if !row.status_holds {
        BracketWrite::Changed
    } else if row.played {
        BracketWrite::Locked
    } else {
        BracketWrite::Done
    })
}

fn skill_column(skill: SkillLevel) -> i64 {
    i64::from(skill.value())
}

fn format_day(date: Date) -> Result<String, StorageError> {
    date.format(DAY)
        .map_err(|error| StorageError::malformed_row(TOURNAMENTS, error))
}

fn parse_skill(raw: i64) -> Result<SkillLevel, StorageError> {
    u8::try_from(raw)
        .ok()
        .and_then(|level| SkillLevel::try_new(level).ok())
        .ok_or_else(|| StorageError::malformed_row(ENTRANTS, MalformedField("skill")))
}

#[derive(Debug)]
struct TournamentRow {
    id: String,
    name: String,
    game: String,
    mode: String,
    description: String,
    date: String,
    registration_closes_at: i64,
    status: String,
    created_at: i64,
    entrant_count: i64,
    has_bracket: i64,
    winner_id: Option<String>,
    winner_seed: Option<i64>,
    winner_skill: Option<i64>,
    winner_registered_at: Option<i64>,
    winner_player_id: Option<String>,
    winner_handle: Option<String>,
    winner_glyph_bits: Option<i64>,
    winner_glyph_color: Option<String>,
    winner_role: Option<String>,
    winner_player_created_at: Option<i64>,
    winner_cycles: Option<i64>,
    winner_place: Option<i64>,
    winner_players: Option<i64>,
}

impl TryFrom<TournamentRow> for Tournament {
    type Error = StorageError;

    fn try_from(row: TournamentRow) -> Result<Self, Self::Error> {
        let winner = match (row.winner_id, row.winner_registered_at) {
            (Some(id), Some(registered_at)) => Some(
                EntrantRow {
                    id,
                    seed: row.winner_seed,
                    skill: row.winner_skill,
                    registered_at,
                    player_id: row.winner_player_id,
                    handle: row.winner_handle,
                    glyph_bits: row.winner_glyph_bits,
                    glyph_color: row.winner_glyph_color,
                    role: row.winner_role,
                    player_created_at: row.winner_player_created_at,
                    cycles: row.winner_cycles,
                    place: row.winner_place,
                    players: row.winner_players,
                }
                .try_into()?,
            ),
            _ => None,
        };

        Ok(Self {
            id: TournamentId::new(parse_uuid(TOURNAMENTS, &row.id)?),
            name: TournamentName::try_new(row.name)
                .map_err(|e| StorageError::malformed_row(TOURNAMENTS, e))?,
            game: GameName::try_new(row.game)
                .map_err(|e| StorageError::malformed_row(TOURNAMENTS, e))?,
            mode: GameMode::try_new(row.mode)
                .map_err(|e| StorageError::malformed_row(TOURNAMENTS, e))?,
            description: Description::try_new(row.description)
                .map_err(|e| StorageError::malformed_row(TOURNAMENTS, e))?,
            date: parse_day(TOURNAMENTS, &row.date)?,
            registration_closes_at: from_micros(TOURNAMENTS, row.registration_closes_at)?,
            status: row
                .status
                .parse()
                .map_err(|e| StorageError::malformed_row(TOURNAMENTS, e))?,
            entrant_count: u32::try_from(row.entrant_count)
                .map_err(|e| StorageError::malformed_row(TOURNAMENTS, e))?,
            has_bracket: row.has_bracket != 0,
            winner,
            created_at: from_micros(TOURNAMENTS, row.created_at)?,
        })
    }
}

#[derive(Debug)]
struct EntrantRow {
    id: String,
    seed: Option<i64>,
    skill: Option<i64>,
    registered_at: i64,
    player_id: Option<String>,
    handle: Option<String>,
    glyph_bits: Option<i64>,
    glyph_color: Option<String>,
    role: Option<String>,
    player_created_at: Option<i64>,
    cycles: Option<i64>,
    place: Option<i64>,
    players: Option<i64>,
}

impl TryFrom<EntrantRow> for Entrant {
    type Error = StorageError;

    fn try_from(row: EntrantRow) -> Result<Self, Self::Error> {
        let player = OptionalPlayerRow {
            id: row.player_id,
            handle: row.handle,
            glyph_bits: row.glyph_bits,
            glyph_color: row.glyph_color,
            role: row.role,
            created_at: row.player_created_at,
            cycles: row.cycles,
            place: row.place,
            players: row.players,
        }
        .into_player(ENTRANTS)?;
        let seed = row
            .seed
            .map(u32::try_from)
            .transpose()
            .map_err(|error| StorageError::malformed_row(ENTRANTS, error))?;
        let skill = row.skill.map(parse_skill).transpose()?;

        Ok(Self {
            id: EntrantId::new(parse_uuid(ENTRANTS, &row.id)?),
            player,
            seed,
            skill,
            registered_at: from_micros(ENTRANTS, row.registered_at)?,
        })
    }
}

#[derive(Debug)]
struct MatchRow {
    id: String,
    round: i64,
    slot: i64,
    entrant_a: Option<String>,
    entrant_b: Option<String>,
    winner: Option<String>,
}

impl TryFrom<MatchRow> for Match {
    type Error = StorageError;

    fn try_from(row: MatchRow) -> Result<Self, Self::Error> {
        let entrant = |raw: Option<String>| {
            raw.map(|raw| parse_uuid(MATCHES, &raw).map(EntrantId::new))
                .transpose()
        };

        Ok(Self {
            id: MatchId::new(parse_uuid(MATCHES, &row.id)?),
            round: u32::try_from(row.round).map_err(|e| StorageError::malformed_row(MATCHES, e))?,
            slot: u32::try_from(row.slot).map_err(|e| StorageError::malformed_row(MATCHES, e))?,
            entrant_a: entrant(row.entrant_a)?,
            entrant_b: entrant(row.entrant_b)?,
            winner: entrant(row.winner)?,
        })
    }
}
