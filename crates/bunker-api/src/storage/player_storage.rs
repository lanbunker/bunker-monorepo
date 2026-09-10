use bunker_models::{
    Account, Glyph, GlyphBits, GlyphColor, Handle, PageQuery, Paginated, Player, PlayerId, Role,
};
use time::OffsetDateTime;
use uuid::Uuid;

use super::db::DbPool;
use super::error::StorageError;

const TABLE: &str = "players";

/// The unique index that the migration makes on `players.handle`, as SQLite names
/// it in its error message. Storage owns the index, so storage knows its name.
const HANDLE_CONSTRAINT: &str = "players.handle";

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

/// An account with the fact that tokens need to check.
#[derive(Debug, Clone)]
pub struct StoredAccount {
    pub account: Account,
    /// Unix seconds of the last password change. A token issued before is dead.
    pub credentials_changed_at: i64,
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

        let role = Role::User.as_str();
        let inserted = sqlx::query!(
            "insert into players (id, handle, password_hash, glyph_bits, glyph_color, role, created_at)
             values (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            id,
            handle,
            player.password_hash,
            glyph_bits,
            glyph_color,
            role,
            created_at,
        )
        .execute(&self.pool)
        .await;

        match inserted {
            Ok(_) => Ok(Created::Player(Player {
                id: player.id,
                handle: player.handle.clone(),
                glyph: player.glyph,
                role: Role::User,
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
            "select id, handle, glyph_bits, glyph_color, role, created_at
             from players where handle = ?1 collate nocase",
            handle,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(StorageError::from_query)?;

        row.map(TryInto::try_into).transpose()
    }

    pub async fn get_by_id(&self, id: PlayerId) -> Result<Option<StoredAccount>, StorageError> {
        let id = id.into_inner().to_string();
        let row = sqlx::query_as!(
            AccountRow,
            "select id, handle, glyph_bits, glyph_color, role, created_at,
                    must_change_password, credentials_changed_at
             from players where id = ?1",
            id,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(StorageError::from_query)?;

        row.map(TryInto::try_into).transpose()
    }

    pub async fn credentials_by_id(
        &self,
        id: PlayerId,
    ) -> Result<Option<Credentials>, StorageError> {
        let id = id.into_inner().to_string();
        let row = sqlx::query!("select id, password_hash from players where id = ?1", id)
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

    /// `true` when a row was updated. `must_change` marks a temporary password.
    /// The change moment is stored in seconds, the unit of a token `iat`, so a
    /// token from the same second as the change stays valid.
    pub async fn set_password(
        &self,
        id: PlayerId,
        password_hash: &str,
        must_change: bool,
        changed_at: OffsetDateTime,
    ) -> Result<bool, StorageError> {
        let id = id.into_inner().to_string();
        let flag = i64::from(must_change);
        let changed_at = changed_at.unix_timestamp();
        let result = sqlx::query!(
            "update players
             set password_hash = ?1, must_change_password = ?2, credentials_changed_at = ?3
             where id = ?4",
            password_hash,
            flag,
            changed_at,
            id,
        )
        .execute(&self.pool)
        .await
        .map_err(StorageError::from_query)?;

        Ok(result.rows_affected() > 0)
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
    ///
    /// The count and the page run in one transaction, so both see the same rows.
    pub async fn list(&self, query: PageQuery) -> Result<Paginated<Player>, StorageError> {
        let limit = query.limit();
        let offset = query.offset();

        let mut tx = self.pool.begin().await.map_err(StorageError::from_query)?;
        let total = sqlx::query_scalar!("select count(*) from players")
            .fetch_one(&mut *tx)
            .await
            .map_err(StorageError::from_query)?;
        let rows = sqlx::query_as!(
            PlayerRow,
            "select id, handle, glyph_bits, glyph_color, role, created_at
             from players order by created_at desc, id asc limit ?1 offset ?2",
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

    /// `None` when no row has the id.
    pub async fn set_role(&self, id: PlayerId, role: Role) -> Result<Option<Player>, StorageError> {
        let id = id.into_inner().to_string();
        let role = role.as_str();
        let row = sqlx::query_as!(
            PlayerRow,
            "update players set role = ?1 where id = ?2
             returning id, handle, glyph_bits, glyph_color, role, created_at",
            role,
            id,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(StorageError::from_query)?;

        row.map(TryInto::try_into).transpose()
    }

    /// `true` when a row was removed.
    pub async fn delete(&self, id: PlayerId) -> Result<bool, StorageError> {
        let id = id.into_inner().to_string();
        let result = sqlx::query!("delete from players where id = ?1", id)
            .execute(&self.pool)
            .await
            .map_err(StorageError::from_query)?;

        Ok(result.rows_affected() > 0)
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
    role: String,
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
            role: row
                .role
                .parse()
                .map_err(|error| StorageError::malformed_row(TABLE, error))?,
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

#[derive(Debug)]
struct AccountRow {
    id: String,
    handle: String,
    glyph_bits: i64,
    glyph_color: String,
    role: String,
    created_at: i64,
    must_change_password: i64,
    credentials_changed_at: i64,
}

impl TryFrom<AccountRow> for StoredAccount {
    type Error = StorageError;

    fn try_from(row: AccountRow) -> Result<Self, Self::Error> {
        let player = PlayerRow {
            id: row.id,
            handle: row.handle,
            glyph_bits: row.glyph_bits,
            glyph_color: row.glyph_color,
            role: row.role,
            created_at: row.created_at,
        }
        .try_into()?;

        Ok(Self {
            account: Account {
                player,
                must_change_password: row.must_change_password != 0,
            },
            credentials_changed_at: row.credentials_changed_at,
        })
    }
}

/// A column value that the domain refuses and that has no error type of its own.
#[derive(Debug, thiserror::Error)]
#[error("column `{0}` holds a value outside the domain")]
struct MalformedField(&'static str);
