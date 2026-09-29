use std::str::FromStr;
use std::time::Duration;

use sqlx::migrate::Migrator;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use sqlx::{Sqlite, Transaction};

use crate::config::DbConfig;

use super::error::StorageError;

pub type DbPool = sqlx::SqlitePool;

/// How long a writer waits for the lock before SQLite reports `busy`. One API
/// process is the only writer, so a wait this long means a stuck transaction.
const BUSY_TIMEOUT: Duration = Duration::from_secs(5);

const ACQUIRE_TIMEOUT: Duration = Duration::from_secs(5);

/// The `migrations/` directory. The compiler puts it into the binary, so a
/// deployment needs no files next to it.
static MIGRATOR: Migrator = sqlx::migrate!("./migrations");

/// Builds the pool. A connection opens at the first query, so this reports a
/// URL that cannot be parsed and not a file that cannot be opened. The readiness
/// route covers the second case.
///
/// WAL lets readers run while a writer runs. In WAL mode, `synchronous = NORMAL`
/// cannot corrupt the file, but a power loss can undo the last commits. It skips
/// one fsync per transaction.
pub fn connect(config: &DbConfig) -> Result<DbPool, StorageError> {
    let options = SqliteConnectOptions::from_str(&config.url)
        .map_err(StorageError::connection)?
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal)
        .foreign_keys(true)
        .busy_timeout(BUSY_TIMEOUT);

    Ok(SqlitePoolOptions::new()
        .max_connections(config.max_connections.into_inner())
        .acquire_timeout(ACQUIRE_TIMEOUT)
        .connect_lazy_with(options))
}

pub async fn run_pending_migrations(pool: &DbPool) -> Result<(), StorageError> {
    MIGRATOR.run(pool).await.map_err(StorageError::Migration)?;

    tracing::info!(
        known = MIGRATOR.iter().count(),
        "every known migration is applied"
    );

    Ok(())
}

/// A transaction that takes the write lock when it begins. A deferred one that
/// reads first fails with `BUSY_SNAPSHOT` when another writer commits before its
/// first write, and `busy_timeout` does not help it. This one waits instead.
pub(super) async fn begin_write(
    pool: &DbPool,
) -> Result<Transaction<'static, Sqlite>, StorageError> {
    pool.begin_with("BEGIN IMMEDIATE")
        .await
        .map_err(StorageError::from_query)
}

/// Proves that the database opens and holds the migration table.
pub(super) async fn check_reachable(pool: &DbPool) -> Result<(), StorageError> {
    sqlx::query_scalar!("select count(*) from _sqlx_migrations")
        .fetch_one(pool)
        .await
        .map_err(StorageError::from_query)?;

    Ok(())
}
