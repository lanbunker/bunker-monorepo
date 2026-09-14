use bunker_models::{Account, Glyph, Handle, PageQuery, Paginated, Player, PlayerId, Role};
use sqlx::SqliteConnection;
use time::OffsetDateTime;

use super::db::DbPool;
use super::error::StorageError;
use super::row::{MalformedField, PlayerRow, parse_uuid, to_micros};

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

/// What a handle update did. Two outcomes are results, not failures.
#[derive(Debug)]
pub enum Renamed {
    Player(Player),
    HandleTaken,
    NotFound,
}

/// An account with the fact that tokens need to check.
#[derive(Debug, Clone)]
pub struct StoredAccount {
    pub account: Account,
    /// Unix seconds of the last password change. A token issued before is dead.
    pub credentials_changed_at: i64,
}

/// How a list of players is sorted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListOrder {
    /// The leaderboard: first place first.
    Standing,
    /// The roster of the backoffice: the last signup first.
    Newest,
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
        let created_at = to_micros(TABLE, player.created_at)?;

        let role = Role::User.as_str();
        let mut tx = self.pool.begin().await.map_err(StorageError::from_query)?;
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
        .execute(&mut *tx)
        .await;

        if let Err(error) = inserted {
            return handle_taken(error).map(|()| Created::HandleTaken);
        }
        // The standing comes from the view, so the new row is read back instead
        // of built here. The row was just written on this same transaction.
        let player = public_by_id(&mut tx, player.id)
            .await?
            .ok_or_else(|| StorageError::malformed_row(TABLE, MalformedField("id")))?;
        tx.commit().await.map_err(StorageError::from_query)?;

        Ok(Created::Player(player))
    }

    /// The lookup is case-insensitive, because the unique index is. `Dave` and
    /// `dave` are one player.
    pub async fn get_by_handle(&self, handle: &Handle) -> Result<Option<Player>, StorageError> {
        let handle = handle.as_ref();
        let row = sqlx::query_as!(
            PlayerRow,
            r#"select p.id, p.handle, p.glyph_bits, p.glyph_color, p.role, p.created_at,
                    s.cycles as "cycles!: i64", s.place as "place!: i64", s.players as "players!: i64"
             from players p join player_standings s on s.player_id = p.id
             where p.handle = ?1 collate nocase"#,
            handle,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(StorageError::from_query)?;

        row.map(TryInto::try_into).transpose()
    }

    /// The public player with the id, without the account fields.
    pub async fn get_by_id_public(&self, id: PlayerId) -> Result<Option<Player>, StorageError> {
        let mut connection = self
            .pool
            .acquire()
            .await
            .map_err(StorageError::from_query)?;

        public_by_id(&mut connection, id).await
    }

    pub async fn get_by_id(&self, id: PlayerId) -> Result<Option<StoredAccount>, StorageError> {
        let id = id.into_inner().to_string();
        let row = sqlx::query_as!(
            AccountRow,
            r#"select p.id, p.handle, p.glyph_bits, p.glyph_color, p.role, p.created_at,
                      s.cycles as "cycles!: i64", s.place as "place!: i64", s.players as "players!: i64",
                      p.must_change_password, p.credentials_changed_at
               from players p join player_standings s on s.player_id = p.id
               where p.id = ?1"#,
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
                id: PlayerId::new(parse_uuid(TABLE, &row.id)?),
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
                id: PlayerId::new(parse_uuid(TABLE, &row.id)?),
                password_hash: row.password_hash,
            })
        })
        .transpose()
    }

    /// The count and the page run in one transaction, so both see the same rows.
    /// The id breaks every tie, so two signups in the same microsecond keep one
    /// order.
    pub async fn list(
        &self,
        query: PageQuery,
        order: ListOrder,
    ) -> Result<Paginated<Player>, StorageError> {
        let limit = query.limit();
        let offset = query.offset();
        let by_standing = i64::from(matches!(order, ListOrder::Standing));

        let mut tx = self.pool.begin().await.map_err(StorageError::from_query)?;
        let total = sqlx::query_scalar!("select count(*) from players")
            .fetch_one(&mut *tx)
            .await
            .map_err(StorageError::from_query)?;
        // One query for both orders: the leaderboard sorts by place first, and
        // a shared place falls back to the newest player, like the roster.
        let rows = sqlx::query_as!(
            PlayerRow,
            r#"select p.id, p.handle, p.glyph_bits, p.glyph_color, p.role, p.created_at,
                      s.cycles as "cycles!: i64", s.place as "place!: i64", s.players as "players!: i64"
               from players p join player_standings s on s.player_id = p.id
               order by case when ?1 = 1 then s.place else 0 end asc,
                        p.created_at desc, p.id asc
               limit ?2 offset ?3"#,
            by_standing,
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

    /// `None` when no row has the id. The write and the read back share one
    /// transaction, so a delete in between cannot turn a done update into a
    /// missing player.
    pub async fn set_role(&self, id: PlayerId, role: Role) -> Result<Option<Player>, StorageError> {
        let key = id.into_inner().to_string();
        let role = role.as_str();
        let mut tx = self.pool.begin().await.map_err(StorageError::from_query)?;
        let result = sqlx::query!("update players set role = ?1 where id = ?2", role, key)
            .execute(&mut *tx)
            .await
            .map_err(StorageError::from_query)?;
        if result.rows_affected() == 0 {
            return Ok(None);
        }
        let player = public_by_id(&mut tx, id).await?;
        tx.commit().await.map_err(StorageError::from_query)?;

        Ok(player)
    }

    pub async fn rename(&self, id: PlayerId, handle: &Handle) -> Result<Renamed, StorageError> {
        let key = id.into_inner().to_string();
        let handle = handle.as_ref();
        let mut tx = self.pool.begin().await.map_err(StorageError::from_query)?;
        let updated = sqlx::query!("update players set handle = ?1 where id = ?2", handle, key)
            .execute(&mut *tx)
            .await;
        let result = match updated {
            Ok(result) => result,
            Err(error) => return handle_taken(error).map(|()| Renamed::HandleTaken),
        };
        if result.rows_affected() == 0 {
            return Ok(Renamed::NotFound);
        }
        let player = public_by_id(&mut tx, id).await?;
        tx.commit().await.map_err(StorageError::from_query)?;

        Ok(player.map_or(Renamed::NotFound, Renamed::Player))
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

/// The player columns plus what a token check needs.
#[derive(Debug)]
struct AccountRow {
    id: String,
    handle: String,
    glyph_bits: i64,
    glyph_color: String,
    role: String,
    created_at: i64,
    cycles: i64,
    place: i64,
    players: i64,
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
            cycles: row.cycles,
            place: row.place,
            players: row.players,
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

/// The public columns of one player with the standing, on any connection, so a
/// write and its read back can share a transaction.
async fn public_by_id(
    connection: &mut SqliteConnection,
    id: PlayerId,
) -> Result<Option<Player>, StorageError> {
    let id = id.into_inner().to_string();
    let row = sqlx::query_as!(
        PlayerRow,
        r#"select p.id, p.handle, p.glyph_bits, p.glyph_color, p.role, p.created_at,
                  s.cycles as "cycles!: i64", s.place as "place!: i64", s.players as "players!: i64"
           from players p join player_standings s on s.player_id = p.id
           where p.id = ?1"#,
        id,
    )
    .fetch_optional(&mut *connection)
    .await
    .map_err(StorageError::from_query)?;

    row.map(TryInto::try_into).transpose()
}

/// `Ok` when the write hit the case-insensitive unique index on the handle, so
/// the caller can turn it into a result. Any other failure stays a failure.
fn handle_taken(error: sqlx::Error) -> Result<(), StorageError> {
    match StorageError::from_query(error) {
        StorageError::UniqueViolation { constraint, .. } if constraint == HANDLE_CONSTRAINT => {
            Ok(())
        }
        other => Err(other),
    }
}
