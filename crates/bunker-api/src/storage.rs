//! SQLite access through `sqlx`. Storage translates between rows and domain
//! values, and does nothing else.

mod db;
mod error;
mod player_storage;
mod row;
mod tournament_storage;

pub use db::{DbPool, connect, run_pending_migrations};
pub use error::StorageError;
pub use player_storage::{Created, Credentials, NewPlayer, PlayerStorage, Renamed, StoredAccount};
pub use tournament_storage::{Enrolled, NewEntrant, NewTournamentRow, TournamentStorage};
