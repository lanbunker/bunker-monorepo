use bunker_models::{
    GameName, MatchId, MatchRecord, NEMESIS_MIN_LOSSES, PageQuery, Paginated, PlayedMatch,
    PlayerId, Rivalry, TournamentId, TournamentName,
};

use super::db::DbPool;
use super::error::StorageError;
use super::row::{OptionalPlayerRow, PlayerRow, parse_day, parse_uuid};

const TABLE: &str = "player_matches";

/// The read side of the bracket results, through the `player_matches` view: one
/// row per side of every played match. The writes stay in `TournamentStorage`,
/// which owns the bracket.
#[derive(Debug, Clone)]
pub struct MatchStorage {
    pool: DbPool,
}

impl MatchStorage {
    pub const fn new(pool: DbPool) -> Self {
        Self { pool }
    }

    /// Wins and losses over every played match of the player.
    pub async fn record(&self, player: PlayerId) -> Result<MatchRecord, StorageError> {
        let player = player.into_inner().to_string();
        let row = sqlx::query!(
            r#"select count(*) filter (where won = 1) as "wins!: i64",
                      count(*) filter (where won = 0) as "losses!: i64"
               from player_matches where player_id = ?1"#,
            player,
        )
        .fetch_one(&self.pool)
        .await
        .map_err(StorageError::from_query)?;

        Ok(MatchRecord {
            wins: count(row.wins)?,
            losses: count(row.losses)?,
        })
    }

    /// The opponent who beat the player the most, once they reach
    /// [`NEMESIS_MIN_LOSSES`] wins over them. A tie on the count goes to the
    /// opponent whose last win over the player is the most recent, and the
    /// handle breaks a full tie, so two reads answer the same name.
    ///
    /// Recency is one key and not two, because a separate `max` per column
    /// takes each maximum from a different loss: an opponent could win the
    /// comparison with the date of one loss and the instant of another. The day
    /// is a fixed width ISO string, so the pair sorts as text.
    /// [`NEMESIS_MIN_LOSSES`] and the handle are bound, and the day comes from
    /// the column, so no value a caller sent reaches the SQL.
    ///
    /// An opponent who deleted their account leaves no row in `players` to
    /// join, so they never hold the title.
    pub async fn nemesis(&self, player: PlayerId) -> Result<Option<Rivalry>, StorageError> {
        let player = player.into_inner().to_string();
        let threshold = i64::from(NEMESIS_MIN_LOSSES);
        let row = sqlx::query_as!(
            NemesisRow,
            r#"select p.id as "id!", p.handle as "handle!", p.glyph_bits as "glyph_bits!: i64",
                      p.glyph_color as "glyph_color!", p.role as "role!",
                      p.created_at as "created_at!: i64",
                      s.cycles as "cycles!: i64", s.place as "place!: i64",
                      s.players as "players!: i64",
                      sum(v.won = 1) as "wins!: i64",
                      sum(v.won = 0) as "losses!: i64"
               from player_matches v
               join players p on p.id = v.opponent_id
               join player_standings s on s.player_id = p.id
               where v.player_id = ?1
               group by v.opponent_id
               having sum(v.won = 0) >= ?2
               order by sum(v.won = 0) desc,
                        max(case when v.won = 0
                                 then v.played_on || printf('%020d', v.tournament_created_at)
                            end) desc,
                        p.handle collate nocase asc
               limit 1"#,
            player,
            threshold,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(StorageError::from_query)?;

        row.map(TryInto::try_into).transpose()
    }

    /// The played matches of the player, newest first. The count and the page
    /// run in one transaction.
    ///
    /// The order ends on the slot, which no pair of rows reaches today: a
    /// player meets one opponent at most one time per round. It is there
    /// because `(tournament, round, slot)` is the unique key of `matches`, so
    /// the order stays total, and a page stays stable, without the caller
    /// trusting that rule.
    pub async fn history(
        &self,
        player: PlayerId,
        query: PageQuery,
    ) -> Result<Paginated<PlayedMatch>, StorageError> {
        let player = player.into_inner().to_string();
        let limit = query.limit();
        let offset = query.offset();

        let mut tx = self.pool.begin().await.map_err(StorageError::from_query)?;
        let total = sqlx::query_scalar!(
            "select count(*) from player_matches where player_id = ?1",
            player
        )
        .fetch_one(&mut *tx)
        .await
        .map_err(StorageError::from_query)?;
        let rows = sqlx::query_as!(
            MatchLogRow,
            r#"select v.match_id as "match_id!", v.tournament_id as "tournament_id!",
                      t.name as "tournament_name!", t.game as "game!",
                      v.played_on as "played_on!", v.round as "round!: i64",
                      (select max(round) from matches m where m.tournament_id = v.tournament_id)
                          as "rounds!: i64",
                      v.won as "won!: i64",
                      p.id as "opponent_id?", p.handle as "opponent_handle?",
                      p.glyph_bits as "opponent_glyph_bits?: i64",
                      p.glyph_color as "opponent_glyph_color?",
                      p.role as "opponent_role?",
                      p.created_at as "opponent_created_at?: i64",
                      s.cycles as "opponent_cycles?: i64", s.place as "opponent_place?: i64",
                      s.players as "opponent_players?: i64"
               from player_matches v
               join tournaments t on t.id = v.tournament_id
               left join players p on p.id = v.opponent_id
               left join player_standings s on s.player_id = p.id
               where v.player_id = ?1
               order by v.played_on desc, v.tournament_created_at desc,
                        v.round desc, v.slot asc
               limit ?2 offset ?3"#,
            player,
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
}

fn count(raw: i64) -> Result<u32, StorageError> {
    u32::try_from(raw).map_err(|error| StorageError::malformed_row(TABLE, error))
}

#[derive(Debug)]
struct NemesisRow {
    id: String,
    handle: String,
    glyph_bits: i64,
    glyph_color: String,
    role: String,
    created_at: i64,
    cycles: i64,
    place: i64,
    players: i64,
    wins: i64,
    losses: i64,
}

impl TryFrom<NemesisRow> for Rivalry {
    type Error = StorageError;

    fn try_from(row: NemesisRow) -> Result<Self, Self::Error> {
        Ok(Self {
            opponent: PlayerRow {
                id: row.id,
                handle: row.handle,
                glyph_bits: row.glyph_bits,
                glyph_color: row.glyph_color,
                role: row.role,
                created_at: row.created_at,
                cycles: row.cycles,
                place: row.place,
                players: row.players,
            }
            .try_into()?,
            wins: count(row.wins)?,
            losses: count(row.losses)?,
        })
    }
}

#[derive(Debug)]
struct MatchLogRow {
    match_id: String,
    tournament_id: String,
    tournament_name: String,
    game: String,
    played_on: String,
    round: i64,
    rounds: i64,
    won: i64,
    opponent_id: Option<String>,
    opponent_handle: Option<String>,
    opponent_glyph_bits: Option<i64>,
    opponent_glyph_color: Option<String>,
    opponent_role: Option<String>,
    opponent_created_at: Option<i64>,
    opponent_cycles: Option<i64>,
    opponent_place: Option<i64>,
    opponent_players: Option<i64>,
}

impl TryFrom<MatchLogRow> for PlayedMatch {
    type Error = StorageError;

    fn try_from(row: MatchLogRow) -> Result<Self, Self::Error> {
        let opponent = OptionalPlayerRow {
            id: row.opponent_id,
            handle: row.opponent_handle,
            glyph_bits: row.opponent_glyph_bits,
            glyph_color: row.opponent_glyph_color,
            role: row.opponent_role,
            created_at: row.opponent_created_at,
            cycles: row.opponent_cycles,
            place: row.opponent_place,
            players: row.opponent_players,
        }
        .into_player(TABLE)?;

        Ok(Self {
            id: MatchId::new(parse_uuid(TABLE, &row.match_id)?),
            tournament_id: TournamentId::new(parse_uuid(TABLE, &row.tournament_id)?),
            tournament_name: TournamentName::try_new(row.tournament_name)
                .map_err(|error| StorageError::malformed_row(TABLE, error))?,
            game: GameName::try_new(row.game)
                .map_err(|error| StorageError::malformed_row(TABLE, error))?,
            date: parse_day(TABLE, &row.played_on)?,
            round: count(row.round)?,
            rounds: count(row.rounds)?,
            opponent,
            won: row.won != 0,
        })
    }
}
