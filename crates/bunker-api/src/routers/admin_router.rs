use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use bunker_models::{PageQuery, Paginated, Player, PlayerId, RoleUpdate, TemporaryPassword};
use serde::Deserialize;

use crate::internal::http::{AdminOnly, ApiError, ApiErrorBody, ValidJson, ValidPath, ValidQuery};
use crate::services::{AuthService, PlayerService};

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
    params(PageQuery),
    responses(
        (status = 200, body = Paginated<Player>),
        (status = 400, body = ApiErrorBody),
        (status = 401, body = ApiErrorBody),
        (status = 403, body = ApiErrorBody),
    )
)]
pub(super) async fn list_players(
    State(players): State<PlayerService>,
    AdminOnly(_admin): AdminOnly,
    ValidQuery(query): ValidQuery<PageQuery>,
) -> Result<Json<Paginated<Player>>, ApiError> {
    Ok(Json(players.list(query).await?))
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
