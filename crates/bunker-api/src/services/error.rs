use std::error::Error as StdError;

use bunker_models::{BracketError, EntrantId, Handle, MatchId, TournamentId, TournamentStatus};
use serde::Serialize;
use utoipa::ToSchema;

use crate::storage::StorageError;

/// Each error code the API can answer with. A client selects on these names, so
/// they are part of the contract.
///
/// Two steps make a response from an error: `ServiceError::public` below selects
/// the code, and `From<ErrorCode> for StatusCode` in `server.rs` selects the
/// status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, ToSchema)]
pub enum ErrorCode {
    GenericError,
    ServiceUnavailable,
    ItemNotFound,
    HandleTaken,
    InvalidCredentials,
    /// The request had no valid bearer token.
    Unauthorized,
    /// The caller is logged in and may not do this.
    Forbidden,
    /// The current password given for a change is wrong.
    WrongPassword,
    /// Registration is not open, or the deadline passed.
    RegistrationClosed,
    /// The tournament or its bracket is in a state that refuses this action.
    InvalidState,
    /// An id in the body is not an entrant of this tournament, or not a side of
    /// this match.
    NotAnEntrant,
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

    #[error("the current password is wrong")]
    WrongPassword,

    #[error("an admin cannot change their own role or delete themself")]
    SelfAction,

    #[error("tournament {0} not found")]
    TournamentNotFound(TournamentId),

    #[error("match {0} not found in this tournament")]
    MatchNotFound(MatchId),

    #[error("registration is closed")]
    RegistrationClosed,

    #[error("a {} tournament cannot become {}", from.as_str(), to.as_str())]
    InvalidTransition {
        from: TournamentStatus,
        to: TournamentStatus,
    },

    #[error("the tournament is concluded and takes no more changes")]
    TournamentConcluded,

    #[error("the bracket must be generated after the tournament goes live")]
    NotLive,

    #[error("remove the bracket before you change the entrants")]
    BracketExists,

    #[error("the tournament has no bracket")]
    BracketMissing,

    #[error("results were entered, remove them before you change the bracket")]
    BracketLocked,

    #[error("the final has no winner yet")]
    BracketIncomplete,

    #[error("a bracket needs at least two entrants, there are {0}")]
    TooFewEntrants(usize),

    #[error("the match does not have both entrants yet")]
    MatchNotReady,

    #[error("the next match already has a result")]
    NextMatchDecided,

    #[error("a winner is accepted only when a tournament without a bracket concludes")]
    UnexpectedWinner,

    #[error("the winner cannot leave the tournament")]
    WinnerCannotLeave,

    #[error("entrant {0} is not part of this tournament or this match")]
    NotAnEntrant(EntrantId),

    #[error("the seed order must list each entrant exactly once")]
    SeedOrderMismatch,

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
            Self::PlayerNotFound(_)
            | Self::PlayerIdNotFound(_)
            | Self::TournamentNotFound(_)
            | Self::MatchNotFound(_) => (ErrorCode::ItemNotFound, None),
            Self::RegistrationClosed => (
                ErrorCode::RegistrationClosed,
                Some("Registration is closed"),
            ),
            Self::InvalidTransition { .. }
            | Self::TournamentConcluded
            | Self::NotLive
            | Self::BracketExists
            | Self::BracketMissing
            | Self::BracketLocked
            | Self::BracketIncomplete
            | Self::TooFewEntrants(_)
            | Self::MatchNotReady
            | Self::NextMatchDecided
            | Self::UnexpectedWinner
            | Self::WinnerCannotLeave => (ErrorCode::InvalidState, None),
            Self::NotAnEntrant(_) => (ErrorCode::NotAnEntrant, None),
            Self::SeedOrderMismatch => (ErrorCode::UnprocessableRequest, None),
            Self::HandleTaken(_) => (ErrorCode::HandleTaken, None),
            Self::InvalidCredentials => (
                ErrorCode::InvalidCredentials,
                Some("The handle or the password is wrong"),
            ),
            Self::SelfAction => (ErrorCode::Forbidden, None),
            Self::WrongPassword => (
                ErrorCode::WrongPassword,
                Some("The current password is wrong"),
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

impl From<BracketError> for ServiceError {
    fn from(error: BracketError) -> Self {
        match error {
            BracketError::TooFewEntrants(count) => Self::TooFewEntrants(count),
            BracketError::UnknownMatch(id) => Self::MatchNotFound(id),
            BracketError::NotReady(_) => Self::MatchNotReady,
            BracketError::NotAParticipant(entrant, _) => Self::NotAnEntrant(entrant),
            BracketError::NextDecided(_) => Self::NextMatchDecided,
        }
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
