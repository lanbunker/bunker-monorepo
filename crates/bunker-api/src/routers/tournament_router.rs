use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use bunker_models::{
    Entrant, PageQuery, Paginated, RegistrationRequest, Registrations, Tournament, TournamentDetail,
};

use crate::internal::http::{
    ApiError, ApiErrorBody, Authenticated, ValidJson, ValidPath, ValidQuery, no_store,
};
use crate::services::{TournamentService, Visibility};

use super::responses::{BearerErrors, BodyErrors, PathErrors};
use super::{AppState, TournamentPath};

pub fn tournament_router() -> Router<AppState> {
    Router::new()
        .route("/api/tournaments", get(list_tournaments))
        .route("/api/tournaments/{id}", get(get_tournament))
        .route(
            "/api/me/registrations",
            get(registrations).layer(no_store()),
        )
        .route(
            "/api/tournaments/{id}/registration",
            post(register).delete(retire),
        )
}

#[utoipa::path(
    get,
    path = "/api/tournaments",
    tag = "tournaments",
    params(PageQuery),
    responses(
        (status = 200, body = Paginated<Tournament>, description = "Newest event first. Drafts are hidden"),
        (status = 400, body = ApiErrorBody, description = "A query parameter is malformed"),
    )
)]
pub(super) async fn list_tournaments(
    State(tournaments): State<TournamentService>,
    ValidQuery(query): ValidQuery<PageQuery>,
) -> Result<Json<Paginated<Tournament>>, ApiError> {
    Ok(Json(tournaments.list(query, Visibility::Public).await?))
}

#[utoipa::path(
    get,
    path = "/api/tournaments/{id}",
    tag = "tournaments",
    params(TournamentPath),
    responses(
        PathErrors,
        (status = 200, body = TournamentDetail),
        (status = 404, body = ApiErrorBody, description = "Unknown, or still a draft"),
    )
)]
pub(super) async fn get_tournament(
    State(tournaments): State<TournamentService>,
    ValidPath(path): ValidPath<TournamentPath>,
) -> Result<Json<TournamentDetail>, ApiError> {
    Ok(Json(tournaments.detail(path.id, Visibility::Public).await?))
}

#[utoipa::path(
    get,
    path = "/api/me/registrations",
    tag = "tournaments",
    security(("bearer" = [])),
    responses(
        BearerErrors,
        (status = 200, body = Registrations, description = "The tournaments the caller entered"),
    )
)]
pub(super) async fn registrations(
    State(tournaments): State<TournamentService>,
    Authenticated(caller): Authenticated,
) -> Result<Json<Registrations>, ApiError> {
    Ok(Json(tournaments.registrations(caller.id).await?))
}

#[utoipa::path(
    post,
    path = "/api/tournaments/{id}/registration",
    tag = "tournaments",
    security(("bearer" = [])),
    params(TournamentPath),
    request_body = RegistrationRequest,
    responses(
        BearerErrors,
        BodyErrors,
        (status = 200, body = Entrant, description = "The caller's entry. A second call answers the same one, with the new level"),
        (status = 404, body = ApiErrorBody),
        (status = 409, body = ApiErrorBody, description = "Registration is not open, or the field is full"),
        (status = 422, body = ApiErrorBody, description = "The level is not 1 to 5"),
    )
)]
pub(super) async fn register(
    State(tournaments): State<TournamentService>,
    Authenticated(caller): Authenticated,
    ValidPath(path): ValidPath<TournamentPath>,
    ValidJson(registration): ValidJson<RegistrationRequest>,
) -> Result<Json<Entrant>, ApiError> {
    Ok(Json(
        tournaments
            .register(path.id, caller.id, registration.skill)
            .await?,
    ))
}

#[utoipa::path(
    delete,
    path = "/api/tournaments/{id}/registration",
    tag = "tournaments",
    security(("bearer" = [])),
    params(TournamentPath),
    responses(
        BearerErrors,
        PathErrors,
        (status = 204, description = "Gone, or never in"),
        (status = 404, body = ApiErrorBody),
        (status = 409, body = ApiErrorBody, description = "Registration is not open"),
    )
)]
pub(super) async fn retire(
    State(tournaments): State<TournamentService>,
    Authenticated(caller): Authenticated,
    ValidPath(path): ValidPath<TournamentPath>,
) -> Result<StatusCode, ApiError> {
    tournaments.retire(path.id, caller.id).await?;

    Ok(StatusCode::NO_CONTENT)
}
