use axum::extract::State;
use axum::routing::{get, post, put};
use axum::{Json, Router};
use bunker_models::{
    Account, CyclesLog, CyclesRules, Handle, HandleChange, MatchLog, PageQuery, Paginated,
    PasswordChange, Player, RosterQuery, TokenResponse,
};
use serde::Deserialize;

use crate::internal::http::{
    ApiError, ApiErrorBody, Authenticated, ValidJson, ValidPath, ValidQuery,
};
use crate::services::{AuthService, MatchService, PlayerService, PointsService};

use super::AppState;

pub fn player_router() -> Router<AppState> {
    Router::new()
        .route("/api/me", get(me))
        .route("/api/me/password", post(change_password))
        .route("/api/me/handle", put(change_handle))
        .route("/api/players", get(list_players))
        .route("/api/players/{handle}", get(get_player))
        .route("/api/players/{handle}/cycles", get(cycles_history))
        .route("/api/players/{handle}/matches", get(match_log))
        .route("/api/cycles/rules", get(cycles_rules))
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
    put,
    path = "/api/me/handle",
    tag = "players",
    security(("bearer" = [])),
    request_body = HandleChange,
    responses(
        (status = 200, body = Player, description = "The player with the new handle. The glyph does not change"),
        (status = 400, body = ApiErrorBody),
        (status = 401, body = ApiErrorBody),
        (status = 409, body = ApiErrorBody),
        (status = 422, body = ApiErrorBody),
    )
)]
pub(super) async fn change_handle(
    State(players): State<PlayerService>,
    Authenticated(account): Authenticated,
    ValidJson(change): ValidJson<HandleChange>,
) -> Result<Json<Player>, ApiError> {
    Ok(Json(
        players.rename(account.player.id, change.handle).await?,
    ))
}

#[utoipa::path(
    get,
    path = "/api/players",
    tag = "players",
    params(RosterQuery),
    responses(
        (status = 200, body = Paginated<Player>, description = "The leaderboard: first place first"),
        (status = 400, body = ApiErrorBody),
    )
)]
pub(super) async fn list_players(
    State(players): State<PlayerService>,
    ValidQuery(query): ValidQuery<RosterQuery>,
) -> Result<Json<Paginated<Player>>, ApiError> {
    Ok(Json(players.leaderboard(&query).await?))
}

#[utoipa::path(
    get,
    path = "/api/players/{handle}/cycles",
    tag = "players",
    params(PlayerPath, PageQuery),
    responses(
        (status = 200, body = CyclesLog, description = "Newest first, with the totals by kind"),
        (status = 400, body = ApiErrorBody),
        (status = 404, body = ApiErrorBody),
    )
)]
pub(super) async fn cycles_history(
    State(points): State<PointsService>,
    ValidPath(path): ValidPath<PlayerPath>,
    ValidQuery(query): ValidQuery<PageQuery>,
) -> Result<Json<CyclesLog>, ApiError> {
    Ok(Json(points.history(&path.handle, query).await?))
}

#[utoipa::path(
    get,
    path = "/api/players/{handle}/matches",
    tag = "players",
    params(PlayerPath, PageQuery),
    responses(
        (status = 200, body = MatchLog, description = "Newest first, with the record and the nemesis"),
        (status = 400, body = ApiErrorBody),
        (status = 404, body = ApiErrorBody),
    )
)]
pub(super) async fn match_log(
    State(matches): State<MatchService>,
    ValidPath(path): ValidPath<PlayerPath>,
    ValidQuery(query): ValidQuery<PageQuery>,
) -> Result<Json<MatchLog>, ApiError> {
    Ok(Json(matches.log(&path.handle, query).await?))
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

#[utoipa::path(
    get,
    path = "/api/cycles/rules",
    tag = "players",
    responses(
        (status = 200, body = CyclesRules, description = "How cycles are earned, and the ladder. The same values the ledger pays"),
    )
)]
pub(super) async fn cycles_rules() -> Json<CyclesRules> {
    Json(CyclesRules::current())
}
