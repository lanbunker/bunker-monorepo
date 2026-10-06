use axum::extract::State;
use axum::routing::{get, post, put};
use axum::{Json, Router};
use bunker_models::{
    Account, CyclesLog, CyclesRules, HandleChange, MatchLog, PageQuery, Paginated, PasswordChange,
    Player, RosterQuery, TokenResponse,
};

use crate::internal::http::{
    ApiError, ApiErrorBody, Authenticated, PendingPassword, ValidJson, ValidPath, ValidQuery,
    no_store,
};
use crate::services::{AuthService, MatchService, PlayerService, PointsService};

use super::responses::{BearerErrors, BodyErrors, PathErrors, TokenErrors};
use super::{AppState, HandlePath};

pub fn player_router() -> Router<AppState> {
    Router::new()
        .route("/api/me", get(me).layer(no_store()))
        .route("/api/me/password", post(change_password).layer(no_store()))
        .route("/api/me/handle", put(change_handle).layer(no_store()))
        .route("/api/me/cycles", get(my_cycles).layer(no_store()))
        .route("/api/me/matches", get(my_matches).layer(no_store()))
        .route("/api/players", get(list_players))
        .route("/api/players/{handle}", get(get_player))
        .route("/api/players/{handle}/cycles", get(cycles_history))
        .route("/api/players/{handle}/matches", get(match_log))
        .route("/api/cycles/rules", get(cycles_rules))
}

/// Takes a temporary password too, so the site can see that a change is due.
#[utoipa::path(
    get,
    path = "/api/me",
    tag = "players",
    security(("bearer" = [])),
    responses(
        TokenErrors,
        (status = 200, body = Account, description = "Sent with `Cache-Control: no-store`, like every `/api/me` answer"),
    )
)]
pub(super) async fn me(
    State(players): State<PlayerService>,
    PendingPassword(caller): PendingPassword,
) -> Result<Json<Account>, ApiError> {
    Ok(Json(players.account(caller.id).await?))
}

#[utoipa::path(
    post,
    path = "/api/me/password",
    tag = "players",
    security(("bearer" = [])),
    request_body = PasswordChange,
    responses(
        TokenErrors,
        BodyErrors,
        (status = 200, body = TokenResponse, description = "A fresh token. Older tokens are refused from now on. The one route besides `/api/me` that a temporary password opens"),
        (status = 400, body = ApiErrorBody, description = "The current password is wrong, or the body is not valid JSON"),
    )
)]
pub(super) async fn change_password(
    State(auth): State<AuthService>,
    PendingPassword(caller): PendingPassword,
    ValidJson(change): ValidJson<PasswordChange>,
) -> Result<Json<TokenResponse>, ApiError> {
    Ok(Json(auth.change_password(caller.id, change).await?))
}

#[utoipa::path(
    put,
    path = "/api/me/handle",
    tag = "players",
    security(("bearer" = [])),
    request_body = HandleChange,
    responses(
        BearerErrors,
        BodyErrors,
        (status = 200, body = Player, description = "The player with the new handle. The glyph does not change"),
        (status = 409, body = ApiErrorBody),
    )
)]
pub(super) async fn change_handle(
    State(players): State<PlayerService>,
    Authenticated(caller): Authenticated,
    ValidJson(change): ValidJson<HandleChange>,
) -> Result<Json<Player>, ApiError> {
    Ok(Json(players.rename(caller.id, change.handle).await?))
}

#[utoipa::path(
    get,
    path = "/api/players",
    tag = "players",
    params(RosterQuery),
    responses(
        (status = 200, body = Paginated<Player>, description = "The leaderboard: first place first"),
        (status = 400, body = ApiErrorBody, description = "A query parameter is malformed"),
    )
)]
pub(super) async fn list_players(
    State(players): State<PlayerService>,
    ValidQuery(query): ValidQuery<RosterQuery>,
) -> Result<Json<Paginated<Player>>, ApiError> {
    Ok(Json(players.leaderboard(&query).await?))
}

/// The caller's own history. Unlike `/api/players/{handle}/cycles`, it answers
/// for an inactive account too.
#[utoipa::path(
    get,
    path = "/api/me/cycles",
    tag = "players",
    security(("bearer" = [])),
    params(PageQuery),
    responses(
        BearerErrors,
        (status = 200, body = CyclesLog, description = "Newest first, with the totals by kind"),
        (status = 400, body = ApiErrorBody, description = "A query parameter is malformed"),
    )
)]
pub(super) async fn my_cycles(
    State(points): State<PointsService>,
    Authenticated(caller): Authenticated,
    ValidQuery(query): ValidQuery<PageQuery>,
) -> Result<Json<CyclesLog>, ApiError> {
    Ok(Json(points.history_of(caller.id, query).await?))
}

/// The caller's own match log. Unlike `/api/players/{handle}/matches`, it
/// answers for an inactive account too.
#[utoipa::path(
    get,
    path = "/api/me/matches",
    tag = "players",
    security(("bearer" = [])),
    params(PageQuery),
    responses(
        BearerErrors,
        (status = 200, body = MatchLog, description = "Newest first, with the record and the nemesis"),
        (status = 400, body = ApiErrorBody, description = "A query parameter is malformed"),
    )
)]
pub(super) async fn my_matches(
    State(matches): State<MatchService>,
    Authenticated(caller): Authenticated,
    ValidQuery(query): ValidQuery<PageQuery>,
) -> Result<Json<MatchLog>, ApiError> {
    Ok(Json(matches.log_of(caller.id, query).await?))
}

#[utoipa::path(
    get,
    path = "/api/players/{handle}/cycles",
    tag = "players",
    params(HandlePath, PageQuery),
    responses(
        (status = 200, body = CyclesLog, description = "Newest first, with the totals by kind"),
        (status = 400, body = ApiErrorBody, description = "The handle or a query parameter is malformed"),
        (status = 404, body = ApiErrorBody),
    )
)]
pub(super) async fn cycles_history(
    State(points): State<PointsService>,
    ValidPath(path): ValidPath<HandlePath>,
    ValidQuery(query): ValidQuery<PageQuery>,
) -> Result<Json<CyclesLog>, ApiError> {
    Ok(Json(points.history(&path.handle, query).await?))
}

#[utoipa::path(
    get,
    path = "/api/players/{handle}/matches",
    tag = "players",
    params(HandlePath, PageQuery),
    responses(
        (status = 200, body = MatchLog, description = "Newest first, with the record and the nemesis"),
        (status = 400, body = ApiErrorBody, description = "The handle or a query parameter is malformed"),
        (status = 404, body = ApiErrorBody),
    )
)]
pub(super) async fn match_log(
    State(matches): State<MatchService>,
    ValidPath(path): ValidPath<HandlePath>,
    ValidQuery(query): ValidQuery<PageQuery>,
) -> Result<Json<MatchLog>, ApiError> {
    Ok(Json(matches.log(&path.handle, query).await?))
}

#[utoipa::path(
    get,
    path = "/api/players/{handle}",
    tag = "players",
    params(HandlePath),
    responses(
        PathErrors,
        (status = 200, body = Player),
        (status = 404, body = ApiErrorBody),
    )
)]
pub(super) async fn get_player(
    State(players): State<PlayerService>,
    ValidPath(path): ValidPath<HandlePath>,
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
