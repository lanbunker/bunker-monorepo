use bunker_models::{Glyph, GlyphBits, GlyphColor, Handle, Player, PlayerId};
use time::OffsetDateTime;
use uuid::Uuid;

use super::db::DbPool;
use super::error::StorageError;

const TABLE: &str = "players";

/// The unique index that the migration makes on `players.handle`, as SQLite names
/// it in its error message. Storage owns the index, so storage knows its name.
const HANDLE_CONSTRAINT: &str = "players.handle";

/// The maximum number of rows from [`PlayerStorage::list`], which has no window.
const LIST_LIMIT: i64 = 1_000;

#[derive(Debug, Clone)]
pub struct NewPlayer {
    pub id: PlayerId,
    pub handle: Handle,
    pub password_hash: String,
    pub glyph: Glyph,
    pub created_at: OffsetDateTime,
}

/// What an insert did. `HandleTaken` is a result, not a failure. Only a service
/// can say what it means to a client.
#[derive(Debug)]
pub enum Created {
    Player(Player),
    HandleTaken,
}

/// The hash never leaves the auth service.
#[derive(Debug, Clone)]
pub struct Credentials {
    pub id: PlayerId,
    pub password_hash: String,
}

#[derive(Debug, Clone)]
pub struct PlayerStorage {
    pool: DbPool,
}

impl PlayerStorage {
    pub const fn new(pool: DbPool) -> Self {
        Self { pool }
    }

    pub async fn create(&self, player: &NewPlayer) -> Result<Created, StorageError> {
        let id = player.id.into_inner().to_string();
        let handle = player.handle.as_ref();
        let glyph_bits = i64::from(player.glyph.bits.into_inner());
        let glyph_color = player.glyph.color.hex();
        let created_at = to_micros(player.created_at)?;

        let inserted = sqlx::query!(
            "insert into players (id, handle, password_hash, glyph_bits, glyph_color, created_at)
             values (?1, ?2, ?3, ?4, ?5, ?6)",
            id,
            handle,
            player.password_hash,
            glyph_bits,
            glyph_color,
            created_at,
        )
        .execute(&self.pool)
        .await;

        match inserted {
            Ok(_) => Ok(Created::Player(Player {
                id: player.id,
                handle: player.handle.clone(),
                glyph: player.glyph,
                created_at: player.created_at,
            })),
            Err(error) => match StorageError::from_query(error) {
                StorageError::UniqueViolation { constraint, .. }
                    if constraint == HANDLE_CONSTRAINT =>
                {
                    Ok(Created::HandleTaken)
                }
                other => Err(other),
            },
        }
    }

    /// The lookup is case-insensitive, because the unique index is. `Dave` and
    /// `dave` are one player.
    pub async fn get_by_handle(&self, handle: &Handle) -> Result<Option<Player>, StorageError> {
        let handle = handle.as_ref();
        let row = sqlx::query_as!(
            PlayerRow,
            "select id, handle, glyph_bits, glyph_color, created_at
             from players where handle = ?1 collate nocase",
            handle,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(StorageError::from_query)?;

        row.map(TryInto::try_into).transpose()
    }

    pub async fn get_by_id(&self, id: PlayerId) -> Result<Option<Player>, StorageError> {
        let id = id.into_inner().to_string();
        let row = sqlx::query_as!(
            PlayerRow,
            "select id, handle, glyph_bits, glyph_color, created_at
             from players where id = ?1",
            id,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(StorageError::from_query)?;

        row.map(TryInto::try_into).transpose()
    }

    pub async fn credentials_by_handle(
        &self,
        handle: &Handle,
    ) -> Result<Option<Credentials>, StorageError> {
        let handle = handle.as_ref();
        let row = sqlx::query!(
            "select id, password_hash from players where handle = ?1 collate nocase",
            handle,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(StorageError::from_query)?;

        row.map(|row| {
            Ok(Credentials {
                id: parse_id(&row.id)?,
                password_hash: row.password_hash,
            })
        })
        .transpose()
    }

    /// Newest first, so a roster shows who joined last at the top. The id breaks a
    /// tie, so two signups in the same microsecond keep one order.
    pub async fn list(&self) -> Result<Vec<Player>, StorageError> {
        let rows = sqlx::query_as!(
            PlayerRow,
            "select id, handle, glyph_bits, glyph_color, created_at
             from players order by created_at desc, id asc limit ?1",
            LIST_LIMIT,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(StorageError::from_query)?;

        rows.into_iter().map(TryInto::try_into).collect()
    }

    /// Sends a query and waits for the answer, which is the only proof that the
    /// database file is present and readable.
    pub async fn check_reachable(&self) -> Result<(), StorageError> {
        sqlx::query!("select 1 as one")
            .fetch_one(&self.pool)
            .await
            .map_err(StorageError::from_query)?;

        Ok(())
    }
}

/// Separate from [`Player`], because the database holds primitives and the domain
/// holds validated types.
#[derive(Debug)]
struct PlayerRow {
    id: String,
    handle: String,
    glyph_bits: i64,
    glyph_color: String,
    created_at: i64,
}

impl TryFrom<PlayerRow> for Player {
    type Error = StorageError;

    fn try_from(row: PlayerRow) -> Result<Self, Self::Error> {
        let bits = u32::try_from(row.glyph_bits)
            .ok()
            .and_then(|raw| GlyphBits::try_new(raw).ok())
            .ok_or_else(|| StorageError::malformed_row(TABLE, MalformedField("glyph_bits")))?;
        let color = GlyphColor::from_hex(&row.glyph_color)
            .ok_or_else(|| StorageError::malformed_row(TABLE, MalformedField("glyph_color")))?;

        Ok(Self {
            id: parse_id(&row.id)?,
            handle: Handle::try_new(row.handle)
                .map_err(|error| StorageError::malformed_row(TABLE, error))?,
            glyph: Glyph { bits, color },
            created_at: from_micros(row.created_at)?,
        })
    }
}

fn to_micros(at: OffsetDateTime) -> Result<i64, StorageError> {
    let micros = at.unix_timestamp_nanos() / 1_000;
    i64::try_from(micros).map_err(|error| StorageError::malformed_row(TABLE, error))
}

fn from_micros(micros: i64) -> Result<OffsetDateTime, StorageError> {
    OffsetDateTime::from_unix_timestamp_nanos(i128::from(micros) * 1_000)
        .map_err(|error| StorageError::malformed_row(TABLE, error))
}

fn parse_id(raw: &str) -> Result<PlayerId, StorageError> {
    Uuid::parse_str(raw)
        .map(PlayerId::new)
        .map_err(|error| StorageError::malformed_row(TABLE, error))
}

/// A column value that the domain refuses and that has no error type of its own.
#[derive(Debug, thiserror::Error)]
#[error("column `{0}` holds a value outside the domain")]
struct MalformedField(&'static str);
