use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::{delete, get, post, put};
use axum::{Json, Router};
use bunker_models::{
    Bracket, Entrant, EntrantAdd, EntrantId, MatchId, MatchResult, NewTournament, PageQuery,
    Paginated, SeedOrder, StatusChange, Tournament, TournamentDetail, TournamentId,
    TournamentUpdate,
};
use serde::Deserialize;

use crate::internal::http::{ApiError, ApiErrorBody, ValidJson, ValidPath, ValidQuery};
use crate::services::{TournamentService, Visibility};

use super::responses::{AdminErrors, BodyErrors, PathErrors};
use super::{AppState, TournamentPath, created_or_existing};

pub fn admin_tournament_router() -> Router<AppState> {
    Router::new()
        .route(
            "/api/admin/tournaments",
            get(list_tournaments).post(create_tournament),
        )
        .route(
            "/api/admin/tournaments/{id}",
            get(get_tournament)
                .patch(update_tournament)
                .delete(delete_tournament),
        )
        .route("/api/admin/tournaments/{id}/status", post(change_status))
        .route("/api/admin/tournaments/{id}/entrants", post(add_entrant))
        .route(
            "/api/admin/tournaments/{id}/entrants/{entrantId}",
            delete(remove_entrant),
        )
        .route(
            "/api/admin/tournaments/{id}/bracket",
            post(generate_bracket).delete(delete_bracket),
        )
        .route("/api/admin/tournaments/{id}/seeds", put(reorder_seeds))
        .route(
            "/api/admin/tournaments/{id}/matches/{matchId}/result",
            put(report_result).delete(clear_result),
        )
}

#[derive(Debug, Deserialize, utoipa::IntoParams)]
#[serde(rename_all = "camelCase")]
pub(super) struct EntrantPath {
    id: TournamentId,
    entrant_id: EntrantId,
}

#[derive(Debug, Deserialize, utoipa::IntoParams)]
#[serde(rename_all = "camelCase")]
pub(super) struct MatchPath {
    id: TournamentId,
    match_id: MatchId,
}

#[utoipa::path(
    get,
    path = "/api/admin/tournaments",
    tag = "admin",
    security(("bearer" = [])),
    params(PageQuery),
    responses(
        AdminErrors,
        (status = 200, body = Paginated<Tournament>, description = "Every tournament, drafts included"),
        (status = 400, body = ApiErrorBody, description = "A query parameter is malformed"),
    )
)]
pub(super) async fn list_tournaments(
    State(tournaments): State<TournamentService>,
    ValidQuery(query): ValidQuery<PageQuery>,
) -> Result<Json<Paginated<Tournament>>, ApiError> {
    Ok(Json(tournaments.list(query, Visibility::WithDrafts).await?))
}

#[utoipa::path(
    post,
    path = "/api/admin/tournaments",
    tag = "admin",
    security(("bearer" = [])),
    request_body = NewTournament,
    responses(
        AdminErrors,
        BodyErrors,
        (status = 201, body = Tournament, description = "A new draft"),
    )
)]
pub(super) async fn create_tournament(
    State(tournaments): State<TournamentService>,
    ValidJson(new): ValidJson<NewTournament>,
) -> Result<(StatusCode, Json<Tournament>), ApiError> {
    Ok((StatusCode::CREATED, Json(tournaments.create(new).await?)))
}

#[utoipa::path(
    get,
    path = "/api/admin/tournaments/{id}",
    tag = "admin",
    security(("bearer" = [])),
    params(TournamentPath),
    responses(
        AdminErrors,
        PathErrors,
        (status = 200, body = TournamentDetail),
        (status = 404, body = ApiErrorBody),
    )
)]
pub(super) async fn get_tournament(
    State(tournaments): State<TournamentService>,
    ValidPath(path): ValidPath<TournamentPath>,
) -> Result<Json<TournamentDetail>, ApiError> {
    Ok(Json(
        tournaments.detail(path.id, Visibility::WithDrafts).await?,
    ))
}

#[utoipa::path(
    patch,
    path = "/api/admin/tournaments/{id}",
    tag = "admin",
    security(("bearer" = [])),
    params(TournamentPath),
    request_body = TournamentUpdate,
    responses(
        AdminErrors,
        BodyErrors,
        (status = 200, body = Tournament),
        (status = 404, body = ApiErrorBody),
        (status = 409, body = ApiErrorBody, description = "The tournament is concluded, or changed in the meantime"),
    )
)]
pub(super) async fn update_tournament(
    State(tournaments): State<TournamentService>,
    ValidPath(path): ValidPath<TournamentPath>,
    ValidJson(update): ValidJson<TournamentUpdate>,
) -> Result<Json<Tournament>, ApiError> {
    Ok(Json(tournaments.update(path.id, update).await?))
}

#[utoipa::path(
    delete,
    path = "/api/admin/tournaments/{id}",
    tag = "admin",
    security(("bearer" = [])),
    params(TournamentPath),
    responses(
        AdminErrors,
        PathErrors,
        (status = 204, description = "Gone, with its entrants and matches"),
    )
)]
pub(super) async fn delete_tournament(
    State(tournaments): State<TournamentService>,
    ValidPath(path): ValidPath<TournamentPath>,
) -> Result<StatusCode, ApiError> {
    tournaments.delete(path.id).await?;

    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    post,
    path = "/api/admin/tournaments/{id}/status",
    tag = "admin",
    security(("bearer" = [])),
    params(TournamentPath),
    request_body = StatusChange,
    responses(
        AdminErrors,
        BodyErrors,
        (status = 200, body = Tournament, description = "The tournament after the move. A repeat of a move answers the same, also when two arrive at once, and a conclusion pays one time"),
        (status = 404, body = ApiErrorBody),
        (status = 409, body = ApiErrorBody, description = "The transition is not allowed in this state, or another request moved the status first"),
        (status = 422, body = ApiErrorBody, description = "The winner is not an entrant"),
    )
)]
pub(super) async fn change_status(
    State(tournaments): State<TournamentService>,
    ValidPath(path): ValidPath<TournamentPath>,
    ValidJson(change): ValidJson<StatusChange>,
) -> Result<Json<Tournament>, ApiError> {
    Ok(Json(tournaments.change_status(path.id, change).await?))
}

#[utoipa::path(
    post,
    path = "/api/admin/tournaments/{id}/entrants",
    tag = "admin",
    security(("bearer" = [])),
    params(TournamentPath),
    request_body = EntrantAdd,
    responses(
        AdminErrors,
        BodyErrors,
        (status = 201, body = Entrant),
        (status = 200, body = Entrant, description = "The player was already in"),
        (status = 404, body = ApiErrorBody, description = "Unknown tournament or player"),
        (status = 409, body = ApiErrorBody, description = "A bracket exists, the tournament is concluded, or the field is full"),
    )
)]
pub(super) async fn add_entrant(
    State(tournaments): State<TournamentService>,
    ValidPath(path): ValidPath<TournamentPath>,
    ValidJson(add): ValidJson<EntrantAdd>,
) -> Result<(StatusCode, Json<Entrant>), ApiError> {
    Ok(created_or_existing(
        tournaments
            .add_entrant(path.id, add.player_id, add.skill)
            .await?,
    ))
}

#[utoipa::path(
    delete,
    path = "/api/admin/tournaments/{id}/entrants/{entrantId}",
    tag = "admin",
    security(("bearer" = [])),
    params(EntrantPath),
    responses(
        AdminErrors,
        PathErrors,
        (status = 204, description = "Gone, or never in"),
        (status = 404, body = ApiErrorBody),
        (status = 409, body = ApiErrorBody, description = "A bracket exists, or the tournament is concluded"),
    )
)]
pub(super) async fn remove_entrant(
    State(tournaments): State<TournamentService>,
    ValidPath(path): ValidPath<EntrantPath>,
) -> Result<StatusCode, ApiError> {
    tournaments.remove_entrant(path.id, path.entrant_id).await?;

    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    post,
    path = "/api/admin/tournaments/{id}/bracket",
    tag = "admin",
    security(("bearer" = [])),
    params(TournamentPath),
    responses(
        AdminErrors,
        PathErrors,
        (status = 200, body = Bracket, description = "A bracket seeded by level. Replaces one without results"),
        (status = 404, body = ApiErrorBody),
        (status = 409, body = ApiErrorBody, description = "Not live, too few entrants, or results exist"),
    )
)]
pub(super) async fn generate_bracket(
    State(tournaments): State<TournamentService>,
    ValidPath(path): ValidPath<TournamentPath>,
) -> Result<Json<Bracket>, ApiError> {
    Ok(Json(tournaments.generate_bracket(path.id).await?))
}

#[utoipa::path(
    put,
    path = "/api/admin/tournaments/{id}/seeds",
    tag = "admin",
    security(("bearer" = [])),
    params(TournamentPath),
    request_body = SeedOrder,
    responses(
        AdminErrors,
        BodyErrors,
        (status = 200, body = Bracket, description = "The bracket rebuilt in this order"),
        (status = 404, body = ApiErrorBody),
        (status = 409, body = ApiErrorBody, description = "No bracket, not live, or results exist"),
        (status = 422, body = ApiErrorBody, description = "The order does not list each entrant one time"),
    )
)]
pub(super) async fn reorder_seeds(
    State(tournaments): State<TournamentService>,
    ValidPath(path): ValidPath<TournamentPath>,
    ValidJson(order): ValidJson<SeedOrder>,
) -> Result<Json<Bracket>, ApiError> {
    Ok(Json(tournaments.reorder_seeds(path.id, order).await?))
}

#[utoipa::path(
    delete,
    path = "/api/admin/tournaments/{id}/bracket",
    tag = "admin",
    security(("bearer" = [])),
    params(TournamentPath),
    responses(
        AdminErrors,
        PathErrors,
        (status = 204, description = "Gone, or there was none"),
        (status = 404, body = ApiErrorBody),
        (status = 409, body = ApiErrorBody, description = "Results exist, or the tournament is concluded"),
    )
)]
pub(super) async fn delete_bracket(
    State(tournaments): State<TournamentService>,
    ValidPath(path): ValidPath<TournamentPath>,
) -> Result<StatusCode, ApiError> {
    tournaments.delete_bracket(path.id).await?;

    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    put,
    path = "/api/admin/tournaments/{id}/matches/{matchId}/result",
    tag = "admin",
    security(("bearer" = [])),
    params(MatchPath),
    request_body = MatchResult,
    responses(
        AdminErrors,
        BodyErrors,
        (status = 200, body = Bracket, description = "The whole bracket after the result"),
        (status = 404, body = ApiErrorBody),
        (status = 409, body = ApiErrorBody, description = "The match is not ready, the next one is decided, or the bracket changed in the meantime"),
        (status = 422, body = ApiErrorBody, description = "The winner is not a side of this match"),
    )
)]
pub(super) async fn report_result(
    State(tournaments): State<TournamentService>,
    ValidPath(path): ValidPath<MatchPath>,
    ValidJson(result): ValidJson<MatchResult>,
) -> Result<Json<Bracket>, ApiError> {
    Ok(Json(
        tournaments
            .report_result(path.id, path.match_id, result.winner)
            .await?,
    ))
}

#[utoipa::path(
    delete,
    path = "/api/admin/tournaments/{id}/matches/{matchId}/result",
    tag = "admin",
    security(("bearer" = [])),
    params(MatchPath),
    responses(
        AdminErrors,
        PathErrors,
        (status = 200, body = Bracket, description = "The whole bracket after the result is gone"),
        (status = 404, body = ApiErrorBody),
        (status = 409, body = ApiErrorBody, description = "A bye, the next match is decided, or the bracket changed in the meantime"),
    )
)]
pub(super) async fn clear_result(
    State(tournaments): State<TournamentService>,
    ValidPath(path): ValidPath<MatchPath>,
) -> Result<Json<Bracket>, ApiError> {
    Ok(Json(
        tournaments.clear_result(path.id, path.match_id).await?,
    ))
}
