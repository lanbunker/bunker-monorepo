use axum::extract::State;
use axum::routing::{get, post};
use axum::{Json, Router};
use bunker_models::{Account, Handle, PageQuery, Paginated, PasswordChange, Player, TokenResponse};
use serde::Deserialize;

use crate::internal::http::{
    ApiError, ApiErrorBody, Authenticated, ValidJson, ValidPath, ValidQuery,
};
use crate::services::{AuthService, PlayerService};

use super::AppState;

pub fn player_router() -> Router<AppState> {
    Router::new()
        .route("/api/me", get(me))
        .route("/api/me/password", post(change_password))
        .route("/api/players", get(list_players))
        .route("/api/players/{handle}", get(get_player))
}

#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub(super) struct PlayerPath {
    handle: Handle,
}

#[utoipa::path(
    get,
    path = "/api/me",
    tag = "players",
    security(("bearer" = [])),
    responses((status = 200, body = Account), (status = 401, body = ApiErrorBody))
)]
pub(super) async fn me(Authenticated(account): Authenticated) -> Result<Json<Account>, ApiError> {
    Ok(Json(account))
}

#[utoipa::path(
    post,
    path = "/api/me/password",
    tag = "players",
    security(("bearer" = [])),
    request_body = PasswordChange,
    responses(
        (status = 200, body = TokenResponse, description = "A fresh token. Older tokens are refused from now on"),
        (status = 400, body = ApiErrorBody),
        (status = 401, body = ApiErrorBody),
        (status = 422, body = ApiErrorBody),
    )
)]
pub(super) async fn change_password(
    State(auth): State<AuthService>,
    Authenticated(account): Authenticated,
    ValidJson(change): ValidJson<PasswordChange>,
) -> Result<Json<TokenResponse>, ApiError> {
    Ok(Json(auth.change_password(account.player.id, change).await?))
}

#[utoipa::path(
    get,
    path = "/api/players",
    tag = "players",
    params(PageQuery),
    responses((status = 200, body = Paginated<Player>), (status = 400, body = ApiErrorBody))
)]
pub(super) async fn list_players(
    State(players): State<PlayerService>,
    ValidQuery(query): ValidQuery<PageQuery>,
) -> Result<Json<Paginated<Player>>, ApiError> {
    Ok(Json(players.list(query).await?))
}

#[utoipa::path(
    get,
    path = "/api/players/{handle}",
    tag = "players",
    params(PlayerPath),
    responses((status = 200, body = Player), (status = 404, body = ApiErrorBody))
)]
pub(super) async fn get_player(
    State(players): State<PlayerService>,
    ValidPath(path): ValidPath<PlayerPath>,
) -> Result<Json<Player>, ApiError> {
    Ok(Json(players.get_by_handle(&path.handle).await?))
}
