//! The business rules. A service owns the rules and the error vocabulary.
//! Storage only translates rows, and a router only moves data.

mod auth_service;
mod error;
mod event_service;
mod match_service;
mod outcome;
mod password;
mod player_service;
mod points_service;
mod token;
mod tournament_service;

pub use auth_service::{AuthService, Caller};
pub use error::{ClosedDoor, ErrorCode, ServiceError};
pub use event_service::EventService;
pub use match_service::MatchService;
pub use outcome::{Outcome, Visibility};
pub use password::PasswordHasher;
pub use player_service::PlayerService;
pub use points_service::PointsService;
pub use token::{TokenIssuer, VerifiedToken};
pub use tournament_service::TournamentService;
