//! SQLite access through `sqlx`. Storage translates between rows and domain
//! values, and does nothing else.

mod db;
mod error;
mod event_storage;
mod match_storage;
mod player_storage;
mod point_storage;
mod row;
mod tournament_storage;

pub use db::{DbPool, connect, run_pending_migrations};
pub use error::StorageError;
pub use event_storage::{CheckedIn, EventStorage, NewEventRow, StoredEvent};
pub use match_storage::MatchStorage;
pub use player_storage::{
    Created, Credentials, ListOrder, NewPlayer, PlayerStorage, Renamed, StoredAccount,
};
pub use point_storage::{NewAdjustment, PointStorage};
pub use tournament_storage::{Enrolled, NewEntrant, NewTournamentRow, TournamentStorage};
