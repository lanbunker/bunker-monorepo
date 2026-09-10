use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use bunker_models::{Handle, Player};
use serde::Deserialize;

use crate::internal::http::{ApiError, Authenticated, ValidPath};
use crate::services::PlayerService;

use super::AppState;

pub fn player_router() -> Router<AppState> {
    Router::new()
        .route("/api/me", get(me))
        .route("/api/players", get(list_players))
        .route("/api/players/{handle}", get(get_player))
}

#[derive(Debug, Deserialize)]
struct PlayerPath {
    handle: Handle,
}

async fn me(
    State(players): State<PlayerService>,
    Authenticated(player_id): Authenticated,
) -> Result<Json<Player>, ApiError> {
    Ok(Json(players.get_by_id(player_id).await?))
}

async fn list_players(State(players): State<PlayerService>) -> Result<Json<Vec<Player>>, ApiError> {
    Ok(Json(players.list().await?))
}

async fn get_player(
    State(players): State<PlayerService>,
    ValidPath(path): ValidPath<PlayerPath>,
) -> Result<Json<Player>, ApiError> {
    Ok(Json(players.get_by_handle(&path.handle).await?))
}
