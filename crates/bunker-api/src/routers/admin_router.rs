use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::{get, patch, post, put};
use axum::{Json, Router};
use bunker_models::{
    Adjustment, HandleChange, Paginated, Player, PlayerId, PointEntry, RoleUpdate, RosterQuery,
    TemporaryPassword,
};
use serde::Deserialize;

use crate::internal::http::{
    AdminOnly, ApiError, ApiErrorBody, ValidJson, ValidPath, ValidQuery, no_store,
};
use crate::services::{AuthService, PlayerService, PointsService};

use super::AppState;
use super::responses::{AdminErrors, BodyErrors, PathErrors};

pub fn admin_router() -> Router<AppState> {
    Router::new()
        .route("/api/admin/players", get(list_players))
        .route(
            "/api/admin/players/{id}",
            patch(set_role).delete(delete_player),
        )
        .route(
            "/api/admin/players/{id}/password-reset",
            post(reset_password).layer(no_store()),
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
        AdminErrors,
        (status = 200, body = Paginated<Player>, description = "The last signup first"),
        (status = 400, body = ApiErrorBody, description = "A query parameter is malformed"),
    )
)]
pub(super) async fn list_players(
    State(players): State<PlayerService>,
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
        AdminErrors,
        BodyErrors,
        (status = 201, body = PointEntry, description = "The new line of the ledger"),
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
    let entry = points.adjust(admin.id, path.id, adjustment).await?;

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
        AdminErrors,
        BodyErrors,
        (status = 200, body = Player),
        (status = 403, body = ApiErrorBody, description = "Not an admin, or the admin's own account"),
        (status = 404, body = ApiErrorBody),
        (status = 409, body = ApiErrorBody, description = "The last admin cannot be demoted"),
    )
)]
pub(super) async fn set_role(
    State(players): State<PlayerService>,
    AdminOnly(admin): AdminOnly,
    ValidPath(path): ValidPath<PlayerIdPath>,
    ValidJson(update): ValidJson<RoleUpdate>,
) -> Result<Json<Player>, ApiError> {
    Ok(Json(
        players.set_role(admin.id, path.id, update.role).await?,
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
        AdminErrors,
        BodyErrors,
        (status = 200, body = Player),
        (status = 404, body = ApiErrorBody),
        (status = 409, body = ApiErrorBody),
    )
)]
pub(super) async fn rename_player(
    State(players): State<PlayerService>,
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
        AdminErrors,
        PathErrors,
        (status = 200, body = TemporaryPassword, description = "Shown one time. Sent with `Cache-Control: no-store`"),
        (status = 404, body = ApiErrorBody),
    )
)]
pub(super) async fn reset_password(
    State(auth): State<AuthService>,
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
        AdminErrors,
        PathErrors,
        (status = 204),
        (status = 403, body = ApiErrorBody, description = "Not an admin, or the admin's own account"),
        (status = 409, body = ApiErrorBody, description = "The last admin cannot be deleted"),
    )
)]
pub(super) async fn delete_player(
    State(players): State<PlayerService>,
    AdminOnly(admin): AdminOnly,
    ValidPath(path): ValidPath<PlayerIdPath>,
) -> Result<StatusCode, ApiError> {
    players.delete(admin.id, path.id).await?;

    Ok(StatusCode::NO_CONTENT)
}
