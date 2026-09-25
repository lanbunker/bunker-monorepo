use std::error::Error as StdError;

use sqlx::error::ErrorKind;

/// SQLite primary result codes, the low byte of an extended code.
const SQLITE_BUSY: i32 = 5;
const SQLITE_LOCKED: i32 = 6;
const SQLITE_READONLY: i32 = 8;
const SQLITE_IOERR: i32 = 10;
const SQLITE_FULL: i32 = 13;
const SQLITE_CANTOPEN: i32 = 14;

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

    /// A row points at a row that is not there, most often one that a
    /// concurrent request deleted.
    #[error("a foreign key points at a missing row")]
    ForeignKeyViolation(#[source] sqlx::Error),

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
        let Some(db) = error.as_database_error() else {
            return match error {
                sqlx::Error::PoolTimedOut | sqlx::Error::PoolClosed => Self::Busy(error),
                // A lazy pool opens the file at the first query, so a missing file
                // arrives here and not from `connect`.
                sqlx::Error::Io(_) => Self::Connection(Box::new(error)),
                _ => Self::Query(error),
            };
        };

        let kind = db.kind();
        let code = primary_code(db.code().as_deref());
        let constraint = db
            .message()
            .rsplit_once(": ")
            .map_or("unknown", |(_, columns)| columns)
            .to_owned();

        match (kind, code) {
            (ErrorKind::UniqueViolation, _) => Self::UniqueViolation {
                constraint,
                source: error,
            },
            (ErrorKind::ForeignKeyViolation, _) => Self::ForeignKeyViolation(error),
            (_, Some(SQLITE_BUSY | SQLITE_LOCKED)) => Self::Busy(error),
            (_, Some(SQLITE_READONLY | SQLITE_IOERR | SQLITE_FULL | SQLITE_CANTOPEN)) => {
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

/// The driver gives the extended code as text. Its low byte is the primary code,
/// which names the family of the failure.
fn primary_code(extended: Option<&str>) -> Option<i32> {
    extended?.parse::<i32>().ok().map(|code| code & 0xff)
}
