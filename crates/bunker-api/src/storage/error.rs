use std::error::Error as StdError;

/// The failures of the storage layer. A constraint violation has its own
/// variant, so a service can make a domain error from it and does not read
/// driver messages.
#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("could not open the database")]
    Connection(#[source] Box<dyn StdError + Send + Sync>),

    #[error("could not apply migrations")]
    Migration(#[source] sqlx::migrate::MigrateError),

    #[error("unique constraint `{constraint}` is already satisfied by another row")]
    UniqueViolation {
        constraint: String,
        #[source]
        source: sqlx::Error,
    },

    #[error("a row in `{table}` does not satisfy the domain model")]
    MalformedRow {
        table: &'static str,
        #[source]
        source: Box<dyn StdError + Send + Sync>,
    },

    /// The database is locked or the pool gave no connection in time. The client
    /// must retry, not report a bug.
    #[error("the database is busy")]
    Busy(#[source] sqlx::Error),

    #[error("database query failed")]
    Query(#[source] sqlx::Error),
}

impl StorageError {
    /// Classifies a driver error and keeps the original. SQLite reports a unique
    /// violation as `UNIQUE constraint failed: table.column`, and the column list
    /// after the colon is the only name the constraint has.
    pub fn from_query(error: sqlx::Error) -> Self {
        match &error {
            sqlx::Error::Database(db) if db.is_unique_violation() => {
                let constraint = db
                    .message()
                    .rsplit_once(": ")
                    .map_or("unknown", |(_, columns)| columns)
                    .to_owned();

                Self::UniqueViolation {
                    constraint,
                    source: error,
                }
            }
            sqlx::Error::PoolTimedOut | sqlx::Error::PoolClosed => Self::Busy(error),
            sqlx::Error::Database(db) if is_busy(db.code().as_deref()) => Self::Busy(error),
            // The file cannot be opened, or the disk failed. A lazy pool reports
            // this at the first query, so it arrives here and not from `connect`.
            sqlx::Error::Io(_) => Self::Connection(Box::new(error)),
            sqlx::Error::Database(db) if is_cannot_open(db.code().as_deref()) => {
                Self::Connection(Box::new(error))
            }
            _ => Self::Query(error),
        }
    }

    /// Wraps a validation failure from a row that becomes a domain value. This
    /// error means the database holds data that the model refuses.
    pub fn malformed_row<E>(table: &'static str, source: E) -> Self
    where
        E: StdError + Send + Sync + 'static,
    {
        Self::MalformedRow {
            table,
            source: Box::new(source),
        }
    }

    pub fn connection<E>(error: E) -> Self
    where
        E: StdError + Send + Sync + 'static,
    {
        Self::Connection(Box::new(error))
    }
}

/// SQLite extended result codes for `SQLITE_BUSY` and `SQLITE_LOCKED`.
fn is_busy(code: Option<&str>) -> bool {
    matches!(code, Some("5" | "6" | "261" | "262" | "517"))
}

/// `SQLITE_CANTOPEN` and its extended codes: the file or its directory is not
/// there, or is not readable.
fn is_cannot_open(code: Option<&str>) -> bool {
    matches!(
        code,
        Some("14" | "1038" | "1294" | "1550" | "1806" | "2062")
    )
}
