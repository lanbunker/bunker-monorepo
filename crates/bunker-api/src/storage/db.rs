use std::str::FromStr;
use std::time::Duration;

use sqlx::migrate::Migrator;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};

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
/// WAL lets readers run while a writer runs. `synchronous = NORMAL` is safe in
/// WAL mode and skips one fsync per transaction.
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
        migrations = MIGRATOR.iter().count(),
        "migrations up to date"
    );

    Ok(())
}
