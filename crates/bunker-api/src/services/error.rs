use std::error::Error as StdError;

use bunker_models::Handle;
use serde::Serialize;

use crate::storage::StorageError;

/// Each error code the API can answer with. A client selects on these names, so
/// they are part of the contract.
///
/// Two steps make a response from an error: `ServiceError::public` below selects
/// the code, and `From<ErrorCode> for StatusCode` in `server.rs` selects the
/// status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub enum ErrorCode {
    GenericError,
    ServiceUnavailable,
    ItemNotFound,
    HandleTaken,
    InvalidCredentials,
    /// The request had no valid bearer token.
    Unauthorized,
    /// The request did not reach a handler. The HTTP layer raises this code, but
    /// the code is here with the rest of the vocabulary.
    InvalidRequest,
    UnprocessableRequest,
    UnsupportedMediaType,
    PayloadTooLarge,
    RouteNotFound,
    MethodNotAllowed,
}

/// What the business rules can refuse to do. Each variant holds the values that
/// its message needs.
#[derive(Debug, thiserror::Error)]
pub enum ServiceError {
    #[error("player `{0}` not found")]
    PlayerNotFound(Handle),

    #[error("player with id {0} not found")]
    PlayerIdNotFound(bunker_models::PlayerId),

    #[error("handle `{0}` is already taken")]
    HandleTaken(Handle),

    /// One variant for a wrong handle and a wrong password. A separate answer
    /// would tell an attacker which handles exist.
    #[error("the handle or the password is wrong")]
    InvalidCredentials,

    #[error("the token is missing, expired or not signed by this server")]
    InvalidToken(#[source] Box<dyn StdError + Send + Sync>),

    #[error("the service is temporarily unable to reach its database")]
    Unavailable(#[source] StorageError),

    #[error("an operation on the database failed")]
    Storage(#[source] StorageError),

    #[error("a cryptographic operation failed")]
    Crypto(#[source] Box<dyn StdError + Send + Sync>),
}

impl ServiceError {
    /// The code a client selects on, and the message that replaces this error's
    /// own `Display`. `None` keeps the `Display`.
    ///
    /// A storage message comes from the driver, which quotes row values. A lost
    /// database is safe to name, but the client can only try again.
    pub const fn public(&self) -> (ErrorCode, Option<&'static str>) {
        match self {
            Self::PlayerNotFound(_) | Self::PlayerIdNotFound(_) => (ErrorCode::ItemNotFound, None),
            Self::HandleTaken(_) => (ErrorCode::HandleTaken, None),
            Self::InvalidCredentials => (
                ErrorCode::InvalidCredentials,
                Some("The handle or the password is wrong"),
            ),
            Self::InvalidToken(_) => (
                ErrorCode::Unauthorized,
                Some("The token is missing, expired or invalid"),
            ),
            Self::Unavailable(_) => (
                ErrorCode::ServiceUnavailable,
                Some("The service is temporarily unavailable"),
            ),
            Self::Storage(_) | Self::Crypto(_) => (
                ErrorCode::GenericError,
                Some("An unexpected error occurred"),
            ),
        }
    }

    pub fn crypto<E>(error: E) -> Self
    where
        E: StdError + Send + Sync + 'static,
    {
        Self::Crypto(Box::new(error))
    }

    pub fn invalid_token<E>(error: E) -> Self
    where
        E: StdError + Send + Sync + 'static,
    {
        Self::InvalidToken(Box::new(error))
    }
}

/// Separates a lost database, which is temporary, from a bad query or a bad row,
/// which is a bug. It is written by hand, and not derived with `#[from]`, so `?`
/// cannot report one as the other.
impl From<StorageError> for ServiceError {
    fn from(error: StorageError) -> Self {
        match &error {
            StorageError::Connection(_) | StorageError::Busy(_) => Self::Unavailable(error),
            StorageError::Migration(_)
            | StorageError::UniqueViolation { .. }
            | StorageError::MalformedRow { .. }
            | StorageError::Query(_) => Self::Storage(error),
        }
    }
}
