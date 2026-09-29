use std::error::Error as StdError;

use bunker_models::{
    BracketError, EntrantId, EventId, Handle, MatchId, PlayerId, TournamentId, TournamentStatus,
};
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
    /// The caller logged in with a temporary password and must choose a new
    /// one before any route other than `/api/me` and `/api/me/password`.
    PasswordChangeRequired,
    /// The current password given for a change is wrong.
    WrongPassword,
    /// Registration is not open, or the deadline passed.
    RegistrationClosed,
    /// The doors of the event are not open: too early, or the night is over.
    CheckinClosed,
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

/// Why the door of an event refuses a scan. An open door is not a refusal, so
/// it has no variant here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClosedDoor {
    Early,
    Over,
}

/// What the business rules can refuse to do. Each variant holds the values that
/// its message needs.
#[derive(Debug, thiserror::Error)]
pub enum ServiceError {
    #[error("player `{0}` not found")]
    PlayerNotFound(Handle),

    #[error("player with id {0} not found")]
    PlayerIdNotFound(PlayerId),

    #[error("handle `{0}` is already taken")]
    HandleTaken(Handle),

    /// One variant for a wrong handle and a wrong password. A separate answer
    /// would tell an attacker which handles exist.
    #[error("the handle or the password is wrong")]
    InvalidCredentials,

    #[error("the current password is wrong")]
    WrongPassword,

    #[error("an admin cannot act on their own account")]
    SelfAction,

    #[error("the last admin cannot be demoted or deleted")]
    LastAdmin,

    #[error("tournament {0} not found")]
    TournamentNotFound(TournamentId),

    #[error("event {0} not found")]
    EventNotFound(EventId),

    /// One variant for an unknown code and a draft. A leaked link must not say
    /// that an event exists before it is published.
    #[error("no published event answers to this check-in code")]
    UnknownCheckinCode,

    #[error("check-in is {}", match .0 { ClosedDoor::Early => "not open yet", ClosedDoor::Over => "over" })]
    CheckinClosed(ClosedDoor),

    /// The generated code did not satisfy its own type. A bug, never a client
    /// mistake.
    #[error("could not generate a check-in code")]
    CodeGeneration(#[source] Box<dyn StdError + Send + Sync>),

    #[error("match {0} not found in this tournament")]
    MatchNotFound(MatchId),

    #[error("registration is closed")]
    RegistrationClosed,

    #[error("a {} tournament cannot become {}", from.as_str(), to.as_str())]
    InvalidTransition {
        from: TournamentStatus,
        to: TournamentStatus,
    },

    /// A concurrent request changed the tournament between the read and the
    /// write, so the write was refused.
    #[error("the tournament changed while the request ran")]
    TournamentChanged,

    #[error("the tournament is concluded and takes no more changes")]
    TournamentConcluded,

    #[error("the tournament has no room for another entrant")]
    TournamentFull,

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

    #[error("entrant {0} is not part of this tournament or this match")]
    NotAnEntrant(EntrantId),

    #[error("the seed order must list each entrant exactly once")]
    SeedOrderMismatch,

    /// The stored matches do not form a bracket. A bug, never a client mistake.
    #[error("the stored matches do not form a bracket")]
    CorruptBracket,

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
    /// The code a client selects on, and the message it reads. The internal
    /// `Display` quotes ids and driver text, so it goes to the log and never to
    /// the client.
    pub const fn public(&self) -> (ErrorCode, &'static str) {
        match self {
            Self::PlayerNotFound(_) | Self::PlayerIdNotFound(_) => {
                (ErrorCode::ItemNotFound, "The player was not found")
            }
            Self::TournamentNotFound(_) => {
                (ErrorCode::ItemNotFound, "The tournament was not found")
            }
            Self::EventNotFound(_) => (ErrorCode::ItemNotFound, "The event was not found"),
            Self::MatchNotFound(_) => (
                ErrorCode::ItemNotFound,
                "The match was not found in this tournament",
            ),
            Self::UnknownCheckinCode => {
                (ErrorCode::ItemNotFound, "This check-in link is not valid")
            }
            Self::HandleTaken(_) => (ErrorCode::HandleTaken, "That handle is already taken"),
            Self::InvalidCredentials => (
                ErrorCode::InvalidCredentials,
                "The handle or the password is wrong",
            ),
            Self::WrongPassword => (ErrorCode::WrongPassword, "The current password is wrong"),
            Self::SelfAction => (
                ErrorCode::Forbidden,
                "An admin cannot do this to their own account",
            ),
            Self::LastAdmin => (ErrorCode::InvalidState, "The crew needs at least one admin"),
            Self::RegistrationClosed => (ErrorCode::RegistrationClosed, "Registration is closed"),
            Self::CheckinClosed(ClosedDoor::Early) => (
                ErrorCode::CheckinClosed,
                "Check-in opens when the doors open",
            ),
            Self::CheckinClosed(ClosedDoor::Over) => (
                ErrorCode::CheckinClosed,
                "Check-in is over. The night is done",
            ),
            Self::InvalidTransition { .. } => (
                ErrorCode::InvalidState,
                "The tournament cannot move to that status from here",
            ),
            Self::TournamentChanged => (
                ErrorCode::InvalidState,
                "The tournament changed in the meantime. Load it again and retry",
            ),
            Self::TournamentConcluded => (
                ErrorCode::InvalidState,
                "The tournament is concluded and takes no more changes",
            ),
            Self::TournamentFull => (
                ErrorCode::InvalidState,
                "The tournament has no room for another entrant",
            ),
            Self::NotLive => (
                ErrorCode::InvalidState,
                "Go live before you generate the bracket",
            ),
            Self::BracketExists => (
                ErrorCode::InvalidState,
                "Remove the bracket before you change the entrants",
            ),
            Self::BracketMissing => (ErrorCode::InvalidState, "Generate the bracket first"),
            Self::BracketLocked => (
                ErrorCode::InvalidState,
                "Clear every result before you change the bracket",
            ),
            Self::BracketIncomplete => (ErrorCode::InvalidState, "The final has no winner yet"),
            Self::TooFewEntrants(_) => (
                ErrorCode::InvalidState,
                "A bracket needs at least two entrants",
            ),
            Self::MatchNotReady => (
                ErrorCode::InvalidState,
                "The match does not have both entrants yet",
            ),
            Self::NextMatchDecided => (
                ErrorCode::InvalidState,
                "The next match already has a result. Clear that one first",
            ),
            Self::UnexpectedWinner => (
                ErrorCode::InvalidState,
                "A winner is named only when a tournament without a bracket concludes",
            ),
            Self::NotAnEntrant(_) => (
                ErrorCode::NotAnEntrant,
                "That player is not an entrant of this tournament or this match",
            ),
            Self::SeedOrderMismatch => (
                ErrorCode::UnprocessableRequest,
                "The seed order must list each entrant exactly once",
            ),
            Self::InvalidToken(_) => (
                ErrorCode::Unauthorized,
                "The token is missing, expired or invalid",
            ),
            Self::Unavailable(_) => (
                ErrorCode::ServiceUnavailable,
                "The service is temporarily unavailable",
            ),
            Self::Storage(_) | Self::Crypto(_) | Self::CodeGeneration(_) | Self::CorruptBracket => {
                (ErrorCode::GenericError, "An unexpected error occurred")
            }
        }
    }

    pub fn crypto<E>(error: E) -> Self
    where
        E: StdError + Send + Sync + 'static,
    {
        Self::Crypto(Box::new(error))
    }

    pub fn code_generation<E>(error: E) -> Self
    where
        E: StdError + Send + Sync + 'static,
    {
        Self::CodeGeneration(Box::new(error))
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
            BracketError::TooManyEntrants(_) => Self::TournamentFull,
            BracketError::UnknownMatch(id) => Self::MatchNotFound(id),
            BracketError::NotReady(_) => Self::MatchNotReady,
            BracketError::NotAParticipant(entrant, _) => Self::NotAnEntrant(entrant),
            BracketError::NextDecided(_) => Self::NextMatchDecided,
            BracketError::Malformed => Self::CorruptBracket,
        }
    }
}

/// Separates a lost database, which is temporary, from a bad query or a bad row,
/// which is a bug. It is written by hand, and not derived with `#[from]`, so `?`
/// cannot report one as the other.
///
/// A foreign key failure is a bug here. A service that writes a row a
/// concurrent delete can orphan maps it to the not found error first.
impl From<StorageError> for ServiceError {
    fn from(error: StorageError) -> Self {
        match &error {
            StorageError::Connection(_) | StorageError::Busy(_) => Self::Unavailable(error),
            StorageError::Migration(_)
            | StorageError::UniqueViolation { .. }
            | StorageError::ForeignKeyViolation(_)
            | StorageError::MalformedRow { .. }
            | StorageError::Query(_) => Self::Storage(error),
        }
    }
}
