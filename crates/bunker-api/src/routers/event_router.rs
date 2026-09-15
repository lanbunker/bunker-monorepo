use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use bunker_models::{
    CheckinCode, CheckinGate, CheckinReceipt, Checkins, Event, PageQuery, Paginated,
};
use serde::Deserialize;

use crate::internal::http::{ApiError, ApiErrorBody, Authenticated, ValidPath, ValidQuery};
use crate::services::{CheckinOutcome, EventService};

use super::AppState;

/// The public list, and the door: what a scanned code shows and what it does.
pub fn event_router() -> Router<AppState> {
    Router::new()
        .route("/api/events", get(list_events))
        .route("/api/checkin/{code}", get(checkin_gate).post(check_in))
        .route("/api/me/checkins", get(checkins))
}

#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub(super) struct CheckinPath {
    code: CheckinCode,
}

#[utoipa::path(
    get,
    path = "/api/events",
    tag = "events",
    params(PageQuery),
    responses(
        (status = 200, body = Paginated<Event>, description = "The latest night first. Drafts are hidden"),
        (status = 400, body = ApiErrorBody),
    )
)]
pub(super) async fn list_events(
    State(events): State<EventService>,
    ValidQuery(query): ValidQuery<PageQuery>,
) -> Result<Json<Paginated<Event>>, ApiError> {
    Ok(Json(events.list(query, false).await?))
}

#[utoipa::path(
    get,
    path = "/api/checkin/{code}",
    tag = "events",
    params(CheckinPath),
    responses(
        (status = 200, body = CheckinGate, description = "The event behind the code, and whether its doors are open"),
        (status = 400, body = ApiErrorBody),
        (status = 404, body = ApiErrorBody, description = "Unknown code, or a draft"),
    )
)]
pub(super) async fn checkin_gate(
    State(events): State<EventService>,
    ValidPath(path): ValidPath<CheckinPath>,
) -> Result<Json<CheckinGate>, ApiError> {
    Ok(Json(events.gate(&path.code).await?))
}

#[utoipa::path(
    post,
    path = "/api/checkin/{code}",
    tag = "events",
    security(("bearer" = [])),
    params(CheckinPath),
    responses(
        (status = 201, body = CheckinReceipt, description = "Checked in, cycles paid"),
        (status = 200, body = CheckinReceipt, description = "Already in. The time of the first scan, and zero cycles"),
        (status = 400, body = ApiErrorBody),
        (status = 401, body = ApiErrorBody),
        (status = 404, body = ApiErrorBody, description = "Unknown code, or a draft"),
        (status = 409, body = ApiErrorBody, description = "The doors are not open"),
    )
)]
pub(super) async fn check_in(
    State(events): State<EventService>,
    Authenticated(account): Authenticated,
    ValidPath(path): ValidPath<CheckinPath>,
) -> Result<(StatusCode, Json<CheckinReceipt>), ApiError> {
    Ok(
        match events.check_in(&path.code, account.player.id).await? {
            CheckinOutcome::First(receipt) => (StatusCode::CREATED, Json(receipt)),
            CheckinOutcome::Repeat(receipt) => (StatusCode::OK, Json(receipt)),
        },
    )
}

#[utoipa::path(
    get,
    path = "/api/me/checkins",
    tag = "events",
    security(("bearer" = [])),
    responses(
        (status = 200, body = Checkins, description = "The events the caller checked in to"),
        (status = 401, body = ApiErrorBody),
    )
)]
pub(super) async fn checkins(
    State(events): State<EventService>,
    Authenticated(account): Authenticated,
) -> Result<Json<Checkins>, ApiError> {
    Ok(Json(events.checkins_of(account.player.id).await?))
}
