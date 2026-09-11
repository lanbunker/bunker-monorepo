use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use bunker_models::{Entrant, PageQuery, Paginated, Tournament, TournamentDetail, TournamentId};
use serde::Deserialize;

use crate::internal::http::{ApiError, ApiErrorBody, Authenticated, ValidPath, ValidQuery};
use crate::services::TournamentService;

use super::AppState;

/// Public reads and the two routes a player uses to enter and leave.
pub fn tournament_router() -> Router<AppState> {
    Router::new()
        .route("/api/tournaments", get(list_tournaments))
        .route("/api/tournaments/{id}", get(get_tournament))
        .route(
            "/api/tournaments/{id}/registration",
            axum::routing::post(register).delete(retire),
        )
}

#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub(super) struct TournamentPath {
    id: TournamentId,
}

#[utoipa::path(
    get,
    path = "/api/tournaments",
    tag = "tournaments",
    params(PageQuery),
    responses(
        (status = 200, body = Paginated<Tournament>, description = "Newest event first. Drafts are hidden"),
        (status = 400, body = ApiErrorBody),
    )
)]
pub(super) async fn list_tournaments(
    State(tournaments): State<TournamentService>,
    ValidQuery(query): ValidQuery<PageQuery>,
) -> Result<Json<Paginated<Tournament>>, ApiError> {
    Ok(Json(tournaments.list(query, false).await?))
}

#[utoipa::path(
    get,
    path = "/api/tournaments/{id}",
    tag = "tournaments",
    params(TournamentPath),
    responses(
        (status = 200, body = TournamentDetail),
        (status = 400, body = ApiErrorBody),
        (status = 404, body = ApiErrorBody, description = "Unknown, or still a draft"),
    )
)]
pub(super) async fn get_tournament(
    State(tournaments): State<TournamentService>,
    ValidPath(path): ValidPath<TournamentPath>,
) -> Result<Json<TournamentDetail>, ApiError> {
    Ok(Json(tournaments.detail(path.id, false).await?))
}

#[utoipa::path(
    post,
    path = "/api/tournaments/{id}/registration",
    tag = "tournaments",
    security(("bearer" = [])),
    params(TournamentPath),
    responses(
        (status = 200, body = Entrant, description = "The caller's entry. A second call answers the same one"),
        (status = 400, body = ApiErrorBody),
        (status = 401, body = ApiErrorBody),
        (status = 404, body = ApiErrorBody),
        (status = 409, body = ApiErrorBody, description = "Registration is not open"),
    )
)]
pub(super) async fn register(
    State(tournaments): State<TournamentService>,
    Authenticated(account): Authenticated,
    ValidPath(path): ValidPath<TournamentPath>,
) -> Result<Json<Entrant>, ApiError> {
    Ok(Json(
        tournaments.register(path.id, account.player.id).await?,
    ))
}

#[utoipa::path(
    delete,
    path = "/api/tournaments/{id}/registration",
    tag = "tournaments",
    security(("bearer" = [])),
    params(TournamentPath),
    responses(
        (status = 204, description = "Gone, or never in"),
        (status = 400, body = ApiErrorBody),
        (status = 401, body = ApiErrorBody),
        (status = 404, body = ApiErrorBody),
        (status = 409, body = ApiErrorBody, description = "Registration is not open"),
    )
)]
pub(super) async fn retire(
    State(tournaments): State<TournamentService>,
    Authenticated(account): Authenticated,
    ValidPath(path): ValidPath<TournamentPath>,
) -> Result<StatusCode, ApiError> {
    tournaments.retire(path.id, account.player.id).await?;

    Ok(StatusCode::NO_CONTENT)
}
