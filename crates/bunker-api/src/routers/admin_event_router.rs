use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use bunker_models::{
    CheckinAdd, CheckinReceipt, Event, EventDetail, EventFields, EventId, EventStatusChange,
    PageQuery, Paginated,
};
use serde::Deserialize;

use crate::internal::http::{ApiError, ApiErrorBody, ValidJson, ValidPath, ValidQuery};
use crate::services::{EventService, Visibility};

use super::responses::{AdminErrors, BodyErrors, PathErrors};
use super::{AppState, created_or_existing};

pub fn admin_event_router() -> Router<AppState> {
    Router::new()
        .route("/api/admin/events", get(list_all_events).post(create_event))
        .route(
            "/api/admin/events/{id}",
            get(get_event).put(update_event).delete(delete_event),
        )
        .route("/api/admin/events/{id}/status", post(change_event_status))
        .route("/api/admin/events/{id}/checkins", post(add_checkin))
}

#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub(super) struct EventPath {
    id: EventId,
}

#[utoipa::path(
    get,
    path = "/api/admin/events",
    tag = "admin",
    security(("bearer" = [])),
    params(PageQuery),
    responses(
        AdminErrors,
        (status = 200, body = Paginated<Event>, description = "Every event, drafts included, the latest night first"),
        (status = 400, body = ApiErrorBody, description = "A query parameter is malformed"),
    )
)]
pub(super) async fn list_all_events(
    State(events): State<EventService>,
    ValidQuery(query): ValidQuery<PageQuery>,
) -> Result<Json<Paginated<Event>>, ApiError> {
    Ok(Json(events.list(query, Visibility::WithDrafts).await?))
}

#[utoipa::path(
    post,
    path = "/api/admin/events",
    tag = "admin",
    security(("bearer" = [])),
    request_body = EventFields,
    responses(
        AdminErrors,
        BodyErrors,
        (status = 201, body = Event, description = "A new draft with its own check-in code"),
        (status = 422, body = ApiErrorBody, description = "A field out of range, or the end before the start"),
    )
)]
pub(super) async fn create_event(
    State(events): State<EventService>,
    ValidJson(fields): ValidJson<EventFields>,
) -> Result<(StatusCode, Json<Event>), ApiError> {
    Ok((StatusCode::CREATED, Json(events.create(fields).await?)))
}

#[utoipa::path(
    get,
    path = "/api/admin/events/{id}",
    tag = "admin",
    security(("bearer" = [])),
    params(EventPath),
    responses(
        AdminErrors,
        PathErrors,
        (status = 200, body = EventDetail, description = "The event, its check-in code and who came"),
        (status = 404, body = ApiErrorBody),
    )
)]
pub(super) async fn get_event(
    State(events): State<EventService>,
    ValidPath(path): ValidPath<EventPath>,
) -> Result<Json<EventDetail>, ApiError> {
    Ok(Json(events.detail(path.id).await?))
}

#[utoipa::path(
    put,
    path = "/api/admin/events/{id}",
    tag = "admin",
    security(("bearer" = [])),
    params(EventPath),
    request_body = EventFields,
    responses(
        AdminErrors,
        BodyErrors,
        (status = 200, body = Event, description = "Every field replaced. The status and the code stay"),
        (status = 404, body = ApiErrorBody),
        (status = 422, body = ApiErrorBody, description = "A field out of range, or the end before the start"),
    )
)]
pub(super) async fn update_event(
    State(events): State<EventService>,
    ValidPath(path): ValidPath<EventPath>,
    ValidJson(fields): ValidJson<EventFields>,
) -> Result<Json<Event>, ApiError> {
    Ok(Json(events.update(path.id, fields).await?))
}

#[utoipa::path(
    post,
    path = "/api/admin/events/{id}/status",
    tag = "admin",
    security(("bearer" = [])),
    params(EventPath),
    request_body = EventStatusChange,
    responses(
        AdminErrors,
        BodyErrors,
        (status = 200, body = Event, description = "Published or back to draft. A draft closes its door"),
        (status = 404, body = ApiErrorBody),
    )
)]
pub(super) async fn change_event_status(
    State(events): State<EventService>,
    ValidPath(path): ValidPath<EventPath>,
    ValidJson(change): ValidJson<EventStatusChange>,
) -> Result<Json<Event>, ApiError> {
    Ok(Json(events.set_status(path.id, change.status).await?))
}

#[utoipa::path(
    post,
    path = "/api/admin/events/{id}/checkins",
    tag = "admin",
    security(("bearer" = [])),
    params(EventPath),
    request_body = CheckinAdd,
    responses(
        AdminErrors,
        BodyErrors,
        (status = 201, body = CheckinReceipt, description = "Checked in by hand, cycles paid. Any status, any time"),
        (status = 200, body = CheckinReceipt, description = "Already in. The time of the first check-in, and zero cycles"),
        (status = 404, body = ApiErrorBody, description = "Unknown event or player"),
    )
)]
pub(super) async fn add_checkin(
    State(events): State<EventService>,
    ValidPath(path): ValidPath<EventPath>,
    ValidJson(add): ValidJson<CheckinAdd>,
) -> Result<(StatusCode, Json<CheckinReceipt>), ApiError> {
    Ok(created_or_existing(
        events.add_checkin(path.id, add.player_id).await?,
    ))
}

#[utoipa::path(
    delete,
    path = "/api/admin/events/{id}",
    tag = "admin",
    security(("bearer" = [])),
    params(EventPath),
    responses(
        AdminErrors,
        PathErrors,
        (status = 204, description = "Gone, with its check-ins and their cycles"),
    )
)]
pub(super) async fn delete_event(
    State(events): State<EventService>,
    ValidPath(path): ValidPath<EventPath>,
) -> Result<StatusCode, ApiError> {
    events.delete(path.id).await?;

    Ok(StatusCode::NO_CONTENT)
}
