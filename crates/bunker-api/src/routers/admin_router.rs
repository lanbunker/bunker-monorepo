use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::{get, post, put};
use axum::{Json, Router};
use bunker_models::{
    Adjustment, HandleChange, Paginated, Player, PlayerId, PointEntry, RoleUpdate, RosterQuery,
    TemporaryPassword,
};
use serde::Deserialize;

use crate::internal::http::{AdminOnly, ApiError, ApiErrorBody, ValidJson, ValidPath, ValidQuery};
use crate::services::{AuthService, PlayerService, PointsService};

use super::AppState;

/// Every route here asks for [`AdminOnly`] first, so a user gets `403` before any
/// handler runs.
pub fn admin_router() -> Router<AppState> {
    Router::new()
        .route("/api/admin/players", get(list_players))
        .route(
            "/api/admin/players/{id}",
            axum::routing::patch(set_role).delete(delete_player),
        )
        .route(
            "/api/admin/players/{id}/password-reset",
            post(reset_password),
        )
        .route("/api/admin/players/{id}/cycles", post(adjust_cycles))
        .route("/api/admin/players/{id}/handle", put(rename_player))
}

#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub(super) struct PlayerIdPath {
    id: PlayerId,
}

#[utoipa::path(
    get,
    path = "/api/admin/players",
    tag = "admin",
    security(("bearer" = [])),
    params(RosterQuery),
    responses(
        (status = 200, body = Paginated<Player>, description = "The last signup first"),
        (status = 400, body = ApiErrorBody),
        (status = 401, body = ApiErrorBody),
        (status = 403, body = ApiErrorBody),
    )
)]
pub(super) async fn list_players(
    State(players): State<PlayerService>,
    AdminOnly(_admin): AdminOnly,
    ValidQuery(query): ValidQuery<RosterQuery>,
) -> Result<Json<Paginated<Player>>, ApiError> {
    Ok(Json(players.roster(&query).await?))
}

#[utoipa::path(
    post,
    path = "/api/admin/players/{id}/cycles",
    tag = "admin",
    security(("bearer" = [])),
    params(PlayerIdPath),
    request_body = Adjustment,
    responses(
        (status = 201, body = PointEntry, description = "The new line of the ledger"),
        (status = 400, body = ApiErrorBody),
        (status = 401, body = ApiErrorBody),
        (status = 403, body = ApiErrorBody, description = "Not an admin, or the admin's own account"),
        (status = 404, body = ApiErrorBody),
        (status = 422, body = ApiErrorBody, description = "Zero, out of range, or no note"),
    )
)]
pub(super) async fn adjust_cycles(
    State(points): State<PointsService>,
    AdminOnly(admin): AdminOnly,
    ValidPath(path): ValidPath<PlayerIdPath>,
    ValidJson(adjustment): ValidJson<Adjustment>,
) -> Result<(StatusCode, Json<PointEntry>), ApiError> {
    let entry = points.adjust(admin.player.id, path.id, adjustment).await?;

    Ok((StatusCode::CREATED, Json(entry)))
}

#[utoipa::path(
    patch,
    path = "/api/admin/players/{id}",
    tag = "admin",
    security(("bearer" = [])),
    params(PlayerIdPath),
    request_body = RoleUpdate,
    responses(
        (status = 200, body = Player),
        (status = 400, body = ApiErrorBody),
        (status = 401, body = ApiErrorBody),
        (status = 403, body = ApiErrorBody, description = "Not an admin, or the admin's own account"),
        (status = 404, body = ApiErrorBody),
        (status = 422, body = ApiErrorBody),
    )
)]
async fn set_role(
    State(players): State<PlayerService>,
    AdminOnly(admin): AdminOnly,
    ValidPath(path): ValidPath<PlayerIdPath>,
    ValidJson(update): ValidJson<RoleUpdate>,
) -> Result<Json<Player>, ApiError> {
    Ok(Json(
        players
            .set_role(admin.player.id, path.id, update.role)
            .await?,
    ))
}

#[utoipa::path(
    put,
    path = "/api/admin/players/{id}/handle",
    tag = "admin",
    security(("bearer" = [])),
    params(PlayerIdPath),
    request_body = HandleChange,
    responses(
        (status = 200, body = Player),
        (status = 400, body = ApiErrorBody),
        (status = 401, body = ApiErrorBody),
        (status = 403, body = ApiErrorBody),
        (status = 404, body = ApiErrorBody),
        (status = 409, body = ApiErrorBody),
        (status = 422, body = ApiErrorBody),
    )
)]
pub(super) async fn rename_player(
    State(players): State<PlayerService>,
    AdminOnly(_admin): AdminOnly,
    ValidPath(path): ValidPath<PlayerIdPath>,
    ValidJson(change): ValidJson<HandleChange>,
) -> Result<Json<Player>, ApiError> {
    Ok(Json(players.rename(path.id, change.handle).await?))
}

#[utoipa::path(
    post,
    path = "/api/admin/players/{id}/password-reset",
    tag = "admin",
    security(("bearer" = [])),
    params(PlayerIdPath),
    responses(
        (status = 200, body = TemporaryPassword),
        (status = 400, body = ApiErrorBody),
        (status = 401, body = ApiErrorBody),
        (status = 403, body = ApiErrorBody),
        (status = 404, body = ApiErrorBody),
    )
)]
pub(super) async fn reset_password(
    State(auth): State<AuthService>,
    AdminOnly(_admin): AdminOnly,
    ValidPath(path): ValidPath<PlayerIdPath>,
) -> Result<Json<TemporaryPassword>, ApiError> {
    Ok(Json(auth.reset_password(path.id).await?))
}

#[utoipa::path(
    delete,
    path = "/api/admin/players/{id}",
    tag = "admin",
    security(("bearer" = [])),
    params(PlayerIdPath),
    responses(
        (status = 204),
        (status = 400, body = ApiErrorBody),
        (status = 401, body = ApiErrorBody),
        (status = 403, body = ApiErrorBody, description = "Not an admin, or the admin's own account"),
    )
)]
async fn delete_player(
    State(players): State<PlayerService>,
    AdminOnly(admin): AdminOnly,
    ValidPath(path): ValidPath<PlayerIdPath>,
) -> Result<StatusCode, ApiError> {
    players.delete(admin.player.id, path.id).await?;

    Ok(StatusCode::NO_CONTENT)
}
