use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use serde::Serialize;
use utoipa::ToSchema;

use crate::internal::http::{ApiError, ApiErrorBody};
use crate::services::PlayerService;

use super::AppState;

/// `/health/live` tells that the process runs. `/health/ready` tells that the
/// process can query the database. The commit lets a deploy script confirm that
/// the new binary is the one that answers: the version only changes with a
/// release.
pub fn health_router() -> Router<AppState> {
    Router::new()
        .route("/health/live", get(live))
        .route("/health/ready", get(ready))
}

#[derive(Debug, Serialize, ToSchema)]
pub(super) struct Health {
    status: &'static str,
    version: &'static str,
    /// The git commit the binary was built from, `dev` for a local build.
    commit: &'static str,
}

const HEALTHY: Health = Health {
    status: "ok",
    version: env!("CARGO_PKG_VERSION"),
    commit: match option_env!("BUNKER_GIT_SHA") {
        Some(sha) => sha,
        None => "dev",
    },
};

#[utoipa::path(get, path = "/health/live", tag = "health", responses((status = 200, body = Health)))]
pub(super) async fn live() -> Json<Health> {
    Json(HEALTHY)
}

#[utoipa::path(
    get,
    path = "/health/ready",
    tag = "health",
    responses((status = 200, body = Health), (status = 503, body = ApiErrorBody))
)]
pub(super) async fn ready(State(players): State<PlayerService>) -> Result<Json<Health>, ApiError> {
    players.check_ready().await?;

    Ok(Json(HEALTHY))
}
