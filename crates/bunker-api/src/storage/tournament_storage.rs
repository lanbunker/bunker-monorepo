use bunker_models::{
    Bracket, Description, Entrant, EntrantId, GameMode, GameName, Match, MatchId, PageQuery,
    Paginated, Player, PlayerId, Tournament, TournamentId, TournamentName, TournamentStatus,
    TournamentUpdate,
};
use time::macros::format_description;
use time::{Date, OffsetDateTime};

use super::db::DbPool;
use super::error::StorageError;
use super::row::{MalformedField, PlayerRow, from_micros, parse_uuid, to_micros};

const TOURNAMENTS: &str = "tournaments";
const ENTRANTS: &str = "tournament_entrants";
const MATCHES: &str = "matches";

/// The unique index on `(tournament_id, player_id)`, as SQLite names it.
const ENTRANT_CONSTRAINT: &str = "tournament_entrants.tournament_id, tournament_entrants.player_id";

/// The date column holds ISO days. This is the one place that spells the format.
const DAY: &[time::format_description::BorrowedFormatItem<'_>] =
    format_description!("[year]-[month]-[day]");

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
    pub registered_at: OffsetDateTime,
}

/// What an insert of an entrant did. A second registration is a result, not a
/// failure, and the service says what it means.
#[derive(Debug)]
pub enum Enrolled {
    New,
    Already,
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
                      w.id as "winner_id?", w.seed as "winner_seed?", w.registered_at as "winner_registered_at?",
                      p.id as "winner_player_id?", p.handle as "winner_handle?",
                      p.glyph_bits as "winner_glyph_bits?", p.glyph_color as "winner_glyph_color?",
                      p.role as "winner_role?", p.created_at as "winner_player_created_at?"
               from tournaments t
               left join tournament_entrants w on w.id = t.winner_entrant_id
               left join players p on p.id = w.player_id
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
                      w.id as "winner_id?", w.seed as "winner_seed?", w.registered_at as "winner_registered_at?",
                      p.id as "winner_player_id?", p.handle as "winner_handle?",
                      p.glyph_bits as "winner_glyph_bits?", p.glyph_color as "winner_glyph_color?",
                      p.role as "winner_role?", p.created_at as "winner_player_created_at?"
               from tournaments t
               left join tournament_entrants w on w.id = t.winner_entrant_id
               left join players p on p.id = w.player_id
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

    /// `false` when no row has the id. An absent field keeps its value.
    pub async fn update(
        &self,
        id: TournamentId,
        update: &TournamentUpdate,
    ) -> Result<bool, StorageError> {
        let id = id.into_inner().to_string();
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
             where id = ?7",
            name,
            game,
            mode,
            description,
            date,
            closes_at,
            id,
        )
        .execute(&self.pool)
        .await
        .map_err(StorageError::from_query)?;

        Ok(result.rows_affected() > 0)
    }

    pub async fn set_status(
        &self,
        id: TournamentId,
        status: TournamentStatus,
        winner: Option<EntrantId>,
    ) -> Result<bool, StorageError> {
        let id = id.into_inner().to_string();
        let status = status.as_str();
        let winner = winner.map(|w| w.into_inner().to_string());

        let result = sqlx::query!(
            "update tournaments set status = ?1, winner_entrant_id = ?2 where id = ?3",
            status,
            winner,
            id,
        )
        .execute(&self.pool)
        .await
        .map_err(StorageError::from_query)?;

        Ok(result.rows_affected() > 0)
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
        let rows = sqlx::query_as!(
            EntrantRow,
            r#"select e.id, e.seed, e.registered_at,
                      p.id as "player_id?", p.handle as "handle?", p.glyph_bits as "glyph_bits?",
                      p.glyph_color as "glyph_color?", p.role as "role?", p.created_at as "player_created_at?"
               from tournament_entrants e
               left join players p on p.id = e.player_id
               where e.tournament_id = ?1
               order by e.seed is null, e.seed asc, e.registered_at asc, e.id asc"#,
            tournament,
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
            r#"select e.id, e.seed, e.registered_at,
                      p.id as "player_id?", p.handle as "handle?", p.glyph_bits as "glyph_bits?",
                      p.glyph_color as "glyph_color?", p.role as "role?", p.created_at as "player_created_at?"
               from tournament_entrants e
               left join players p on p.id = e.player_id
               where e.tournament_id = ?1 and e.player_id = ?2"#,
            tournament,
            player,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(StorageError::from_query)?;

        row.map(TryInto::try_into).transpose()
    }

    /// Every tournament the player entered, in any status. The index on
    /// `player_id` makes this one lookup.
    pub async fn tournaments_of(
        &self,
        player: PlayerId,
    ) -> Result<Vec<TournamentId>, StorageError> {
        let player = player.into_inner().to_string();
        let rows = sqlx::query_scalar!(
            "select tournament_id from tournament_entrants where player_id = ?1 order by registered_at asc",
            player,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(StorageError::from_query)?;

        rows.into_iter()
            .map(|raw| parse_uuid(ENTRANTS, &raw).map(TournamentId::new))
            .collect()
    }

    pub async fn add_entrant(&self, entrant: &NewEntrant) -> Result<Enrolled, StorageError> {
        let id = entrant.id.into_inner().to_string();
        let tournament = entrant.tournament.into_inner().to_string();
        let player = entrant.player.into_inner().to_string();
        let registered_at = to_micros(ENTRANTS, entrant.registered_at)?;

        let inserted = sqlx::query!(
            "insert into tournament_entrants (id, tournament_id, player_id, registered_at)
             values (?1, ?2, ?3, ?4)",
            id,
            tournament,
            player,
            registered_at,
        )
        .execute(&self.pool)
        .await;

        match inserted {
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

    /// `true` when a row was removed.
    pub async fn remove_entrant(
        &self,
        tournament: TournamentId,
        entrant: EntrantId,
    ) -> Result<bool, StorageError> {
        let tournament = tournament.into_inner().to_string();
        let entrant = entrant.into_inner().to_string();
        let result = sqlx::query!(
            "delete from tournament_entrants where tournament_id = ?1 and id = ?2",
            tournament,
            entrant,
        )
        .execute(&self.pool)
        .await
        .map_err(StorageError::from_query)?;

        Ok(result.rows_affected() > 0)
    }

    pub async fn remove_entrant_of(
        &self,
        tournament: TournamentId,
        player: PlayerId,
    ) -> Result<bool, StorageError> {
        let tournament = tournament.into_inner().to_string();
        let player = player.into_inner().to_string();
        let result = sqlx::query!(
            "delete from tournament_entrants where tournament_id = ?1 and player_id = ?2",
            tournament,
            player,
        )
        .execute(&self.pool)
        .await
        .map_err(StorageError::from_query)?;

        Ok(result.rows_affected() > 0)
    }

    /// Every match, in round and slot order. Empty when there is no bracket.
    pub async fn matches(&self, tournament: TournamentId) -> Result<Vec<Match>, StorageError> {
        let tournament = tournament.into_inner().to_string();
        let rows = sqlx::query_as!(
            MatchRow,
            "select id, round, slot, entrant_a, entrant_b, winner
             from matches where tournament_id = ?1 order by round asc, slot asc",
            tournament,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(StorageError::from_query)?;

        rows.into_iter().map(TryInto::try_into).collect()
    }

    /// Replaces the whole bracket: the seeds follow `order`, and the old matches
    /// go away. One transaction, so a reader never sees half a bracket.
    pub async fn replace_bracket(
        &self,
        tournament: TournamentId,
        order: &[EntrantId],
        bracket: &Bracket,
    ) -> Result<(), StorageError> {
        let tournament = tournament.into_inner().to_string();

        let mut tx = self.pool.begin().await.map_err(StorageError::from_query)?;
        sqlx::query!(
            "update tournament_entrants set seed = null where tournament_id = ?1",
            tournament
        )
        .execute(&mut *tx)
        .await
        .map_err(StorageError::from_query)?;
        for (index, entrant) in order.iter().enumerate() {
            let seed = i64::try_from(index + 1).unwrap_or(i64::MAX);
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
            // The service checked the order. A miss here means a match would point
            // at an entrant without a seed, so the whole write is dropped.
            if updated.rows_affected() != 1 {
                return Err(StorageError::malformed_row(
                    ENTRANTS,
                    MalformedField("seed"),
                ));
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

        Ok(())
    }

    /// Removes the matches and the seeds. A tournament with no bracket is left as
    /// it was.
    pub async fn delete_bracket(&self, tournament: TournamentId) -> Result<(), StorageError> {
        let tournament = tournament.into_inner().to_string();

        let mut tx = self.pool.begin().await.map_err(StorageError::from_query)?;
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

        Ok(())
    }

    /// Writes the sides and the winner of each given match. A result touches two
    /// matches, and both land or neither does.
    pub async fn update_matches(&self, matches: &[Match]) -> Result<(), StorageError> {
        let mut tx = self.pool.begin().await.map_err(StorageError::from_query)?;
        for m in matches {
            let id = m.id.into_inner().to_string();
            let a = m.entrant_a.map(|e| e.into_inner().to_string());
            let b = m.entrant_b.map(|e| e.into_inner().to_string());
            let winner = m.winner.map(|e| e.into_inner().to_string());
            sqlx::query!(
                "update matches set entrant_a = ?1, entrant_b = ?2, winner = ?3 where id = ?4",
                a,
                b,
                winner,
                id,
            )
            .execute(&mut *tx)
            .await
            .map_err(StorageError::from_query)?;
        }
        tx.commit().await.map_err(StorageError::from_query)?;

        Ok(())
    }
}

fn format_day(date: Date) -> Result<String, StorageError> {
    date.format(DAY)
        .map_err(|error| StorageError::malformed_row(TOURNAMENTS, error))
}

fn parse_day(raw: &str) -> Result<Date, StorageError> {
    Date::parse(raw, DAY).map_err(|error| StorageError::malformed_row(TOURNAMENTS, error))
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
    winner_registered_at: Option<i64>,
    winner_player_id: Option<String>,
    winner_handle: Option<String>,
    winner_glyph_bits: Option<i64>,
    winner_glyph_color: Option<String>,
    winner_role: Option<String>,
    winner_player_created_at: Option<i64>,
}

impl TryFrom<TournamentRow> for Tournament {
    type Error = StorageError;

    fn try_from(row: TournamentRow) -> Result<Self, Self::Error> {
        let winner = match (row.winner_id, row.winner_registered_at) {
            (Some(id), Some(registered_at)) => Some(
                EntrantRow {
                    id,
                    seed: row.winner_seed,
                    registered_at,
                    player_id: row.winner_player_id,
                    handle: row.winner_handle,
                    glyph_bits: row.winner_glyph_bits,
                    glyph_color: row.winner_glyph_color,
                    role: row.winner_role,
                    player_created_at: row.winner_player_created_at,
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
            date: parse_day(&row.date)?,
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
    registered_at: i64,
    player_id: Option<String>,
    handle: Option<String>,
    glyph_bits: Option<i64>,
    glyph_color: Option<String>,
    role: Option<String>,
    player_created_at: Option<i64>,
}

impl TryFrom<EntrantRow> for Entrant {
    type Error = StorageError;

    fn try_from(row: EntrantRow) -> Result<Self, Self::Error> {
        // The player columns are all present or all absent: they come from one
        // left join. A mix means the join broke, and that is a malformed row.
        let player: Option<Player> = match (
            row.player_id,
            row.handle,
            row.glyph_bits,
            row.glyph_color,
            row.role,
            row.player_created_at,
        ) {
            (
                Some(id),
                Some(handle),
                Some(glyph_bits),
                Some(glyph_color),
                Some(role),
                Some(created_at),
            ) => Some(
                PlayerRow {
                    id,
                    handle,
                    glyph_bits,
                    glyph_color,
                    role,
                    created_at,
                }
                .try_into()?,
            ),
            (None, None, None, None, None, None) => None,
            _ => {
                return Err(StorageError::malformed_row(
                    ENTRANTS,
                    MalformedField("player"),
                ));
            }
        };
        let seed = row
            .seed
            .map(u32::try_from)
            .transpose()
            .map_err(|error| StorageError::malformed_row(ENTRANTS, error))?;

        Ok(Self {
            id: EntrantId::new(parse_uuid(ENTRANTS, &row.id)?),
            player,
            seed,
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
