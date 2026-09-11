//! The HTTP handlers. A router extracts the input, calls a service and returns a
//! typed response. It holds no business rules, no SQL and no error responses.

mod admin_router;
mod admin_tournament_router;
mod auth_router;
mod health_router;
mod openapi;
mod player_router;
mod tournament_router;

pub use admin_router::admin_router;
pub use admin_tournament_router::admin_tournament_router;
pub use auth_router::auth_router;
pub use health_router::health_router;
pub use openapi::{ApiDoc, openapi_router};
pub use player_router::player_router;
pub use tournament_router::tournament_router;

use axum::extract::FromRef;

use crate::services::{AuthService, PlayerService, TournamentService};

/// Given to each handler. It holds services, and never storage or configuration.
/// A handler with direct database access could skip the business rules.
///
/// It is here and not in `server.rs`, so a router never imports the composition
/// root. Such an import would point up through the layers.
#[derive(Debug, Clone)]
pub struct AppState {
    pub auth: AuthService,
    pub players: PlayerService,
    pub tournaments: TournamentService,
}

impl FromRef<AppState> for TournamentService {
    fn from_ref(state: &AppState) -> Self {
        state.tournaments.clone()
    }
}

impl FromRef<AppState> for AuthService {
    fn from_ref(state: &AppState) -> Self {
        state.auth.clone()
    }
}

impl FromRef<AppState> for PlayerService {
    fn from_ref(state: &AppState) -> Self {
        state.players.clone()
    }
}
