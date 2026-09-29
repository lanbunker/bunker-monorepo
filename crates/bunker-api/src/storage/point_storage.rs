use bunker_models::{
    Amount, Award, EventId, EventName, KindTotal, Note, PageQuery, Paginated, PlayerId, PointEntry,
    PointEntryId, PointKind, TournamentId, TournamentName,
};
use sqlx::SqliteConnection;
use time::OffsetDateTime;

use super::db::DbPool;
use super::error::StorageError;
use super::row::{from_micros, parse_uuid, to_micros};

const TABLE: &str = "point_entries";

#[derive(Debug, Clone)]
pub struct NewAdjustment {
    pub id: PointEntryId,
    pub player: PlayerId,
    pub amount: Amount,
    pub note: Note,
    pub created_by: PlayerId,
    pub created_at: OffsetDateTime,
}

/// What pays a batch of awards, and the column that ties the rows to it, so
/// the rows go when it goes.
#[derive(Debug, Clone, Copy)]
pub(super) enum AwardSource {
    Tournament(TournamentId),
    Event(EventId),
}

/// The ledger. Every write to `point_entries` is in this file: the adjustments
/// through the pool, and the awards through `insert_awards`, which the storage
/// of a tournament or an event calls inside the transaction that pays.
#[derive(Debug, Clone)]
pub struct PointStorage {
    pool: DbPool,
}

impl PointStorage {
    pub const fn new(pool: DbPool) -> Self {
        Self { pool }
    }

    /// The entry comes back with the time as the row holds it, in micros.
    pub async fn add_adjustment(
        &self,
        adjustment: &NewAdjustment,
    ) -> Result<PointEntry, StorageError> {
        let id = adjustment.id.into_inner().to_string();
        let player = adjustment.player.into_inner().to_string();
        let amount = adjustment.amount.into_inner();
        let kind = PointKind::Adjustment.as_str();
        let note = adjustment.note.as_ref();
        let created_by = adjustment.created_by.into_inner().to_string();
        let created_at = to_micros(TABLE, adjustment.created_at)?;

        // An adjustment is its own source: nothing else can be paid twice for it.
        sqlx::query!(
            "insert into point_entries (id, player_id, amount, kind, source_ref, note, created_by, created_at)
             values (?1, ?2, ?3, ?4, ?1, ?5, ?6, ?7)",
            id,
            player,
            amount,
            kind,
            note,
            created_by,
            created_at,
        )
        .execute(&self.pool)
        .await
        .map_err(StorageError::from_query)?;

        Ok(PointEntry {
            id: adjustment.id,
            amount,
            kind: PointKind::Adjustment,
            tournament_id: None,
            tournament_name: None,
            event_id: None,
            event_name: None,
            note: Some(adjustment.note.clone()),
            created_at: from_micros(TABLE, created_at)?,
        })
    }

    /// Newest first. The count and the page run in one transaction.
    pub async fn history(
        &self,
        player: PlayerId,
        query: PageQuery,
    ) -> Result<Paginated<PointEntry>, StorageError> {
        let player = player.into_inner().to_string();
        let limit = query.limit();
        let offset = query.offset();

        let mut tx = self.pool.begin().await.map_err(StorageError::from_query)?;
        let total = sqlx::query_scalar!(
            "select count(*) from point_entries where player_id = ?1",
            player
        )
        .fetch_one(&mut *tx)
        .await
        .map_err(StorageError::from_query)?;
        let rows = sqlx::query_as!(
            EntryRow,
            r#"select e.id, e.amount, e.kind, e.tournament_id, t.name as "tournament_name?",
                      e.event_id, v.name as "event_name?", e.note, e.created_at
               from point_entries e
               left join tournaments t on t.id = e.tournament_id
               left join events v on v.id = e.event_id
               where e.player_id = ?1
               order by e.created_at desc, e.id asc
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

    /// The sum of every kind a player has at least one line of. The order is
    /// the caller's to give.
    pub async fn totals(&self, player: PlayerId) -> Result<Vec<KindTotal>, StorageError> {
        let player = player.into_inner().to_string();
        let rows = sqlx::query!(
            r#"select kind as "kind!", sum(amount) as "cycles!: i64"
               from point_entries where player_id = ?1 group by kind"#,
            player,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(StorageError::from_query)?;

        rows.into_iter()
            .map(|row| {
                Ok(KindTotal {
                    kind: row
                        .kind
                        .parse()
                        .map_err(|error| StorageError::malformed_row(TABLE, error))?,
                    cycles: row.cycles,
                })
            })
            .collect()
    }
}

/// Writes the awards of a source on the caller's transaction, so they land with
/// the write that earned them or not at all. A duplicate stays a failure: the
/// caller guards the write that pays, so the unique index sees a bug only.
pub(super) async fn insert_awards(
    connection: &mut SqliteConnection,
    source: AwardSource,
    awards: &[Award],
    at: OffsetDateTime,
) -> Result<(), StorageError> {
    let (tournament, event) = match source {
        AwardSource::Tournament(id) => (Some(id.into_inner().to_string()), None),
        AwardSource::Event(id) => (None, Some(id.into_inner().to_string())),
    };
    let created_at = to_micros(TABLE, at)?;
    for award in awards {
        let id = PointEntryId::generate().into_inner().to_string();
        let player = award.player.into_inner().to_string();
        let kind = award.kind.as_str();
        sqlx::query!(
            "insert into point_entries (id, player_id, amount, kind, source_ref, tournament_id, event_id, created_at)
             values (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            id,
            player,
            award.amount,
            kind,
            award.source_ref,
            tournament,
            event,
            created_at,
        )
        .execute(&mut *connection)
        .await
        .map_err(StorageError::from_query)?;
    }

    Ok(())
}

#[derive(Debug)]
struct EntryRow {
    id: String,
    amount: i64,
    kind: String,
    tournament_id: Option<String>,
    tournament_name: Option<String>,
    event_id: Option<String>,
    event_name: Option<String>,
    note: Option<String>,
    created_at: i64,
}

impl TryFrom<EntryRow> for PointEntry {
    type Error = StorageError;

    fn try_from(row: EntryRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: PointEntryId::new(parse_uuid(TABLE, &row.id)?),
            amount: row.amount,
            kind: row
                .kind
                .parse()
                .map_err(|error| StorageError::malformed_row(TABLE, error))?,
            tournament_id: row
                .tournament_id
                .map(|raw| parse_uuid(TABLE, &raw).map(TournamentId::new))
                .transpose()?,
            tournament_name: row
                .tournament_name
                .map(TournamentName::try_new)
                .transpose()
                .map_err(|error| StorageError::malformed_row(TABLE, error))?,
            event_id: row
                .event_id
                .map(|raw| parse_uuid(TABLE, &raw).map(EventId::new))
                .transpose()?,
            event_name: row
                .event_name
                .map(EventName::try_new)
                .transpose()
                .map_err(|error| StorageError::malformed_row(TABLE, error))?,
            note: row
                .note
                .map(Note::try_new)
                .transpose()
                .map_err(|error| StorageError::malformed_row(TABLE, error))?,
            created_at: from_micros(TABLE, row.created_at)?,
        })
    }
}
