//! Events from the contract: who sees a draft, what the door shows, and what
//! one scan pays.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod support;

use axum::http::StatusCode;
use bunker_models::{
    CHECKIN_CYCLES, CheckinGate, CheckinReceipt, CheckinWindow, Checkins, CyclesLog, Event,
    EventDetail, EventStatus, Paginated, Player, PointKind, Rank,
};
use serde_json::json;
use support::{TestApi, assert_error, read_json};
use time::format_description::well_known::Rfc3339;
use time::{Duration, OffsetDateTime};

const HOUR: i64 = 3600;

async fn player(api: &TestApi, handle: &str) -> Player {
    read_json(api.get(&format!("/api/players/{handle}")).await).await
}

async fn log(api: &TestApi, handle: &str) -> CyclesLog {
    read_json(api.get(&format!("/api/players/{handle}/cycles")).await).await
}

#[tokio::test]
async fn a_new_event_is_a_draft_that_only_admins_see() {
    let api = TestApi::with_database().await;
    let admin = api.signup_admin("root").await;
    let user = api.signup("dave").await.token;

    let refused = api.post_as("/api/admin/events", &json!({}), &user).await;
    assert_error(refused, StatusCode::FORBIDDEN, "Forbidden").await;

    let created = api.create_event(&admin, HOUR, 6 * HOUR).await;
    assert_eq!(created.status, EventStatus::Draft);
    assert_eq!(created.checkin_count, 0);
    assert_eq!(created.name.as_ref(), "BUNKER//SESSION 04");
    assert_eq!(created.location.as_ref(), "@theoffice");
    assert_eq!(
        created.image.as_ref().map(AsRef::as_ref),
        Some("feb2026-cover.webp")
    );

    let public: Paginated<Event> = read_json(api.get("/api/events").await).await;
    assert_eq!(public.total, 0, "a draft is not public");

    let all: Paginated<Event> = read_json(api.get_as("/api/admin/events", &admin).await).await;
    assert_eq!(all.total, 1);
    assert_eq!(all.items[0].id, created.id);

    let detail: EventDetail = read_json(
        api.get_as(&format!("/api/admin/events/{}", created.id), &admin)
            .await,
    )
    .await;
    assert!(detail.checkins.is_empty());
    assert_eq!(detail.checkin_code.as_ref().len(), 12);

    // The code of a draft opens nothing and says nothing.
    let gate = api
        .get(&format!("/api/checkin/{}", detail.checkin_code))
        .await;
    assert_error(gate, StatusCode::NOT_FOUND, "ItemNotFound").await;
    let scan = api
        .post_as(
            &format!("/api/checkin/{}", detail.checkin_code),
            &json!({}),
            &user,
        )
        .await;
    assert_error(scan, StatusCode::NOT_FOUND, "ItemNotFound").await;
}

#[tokio::test]
async fn publishing_opens_the_gate_and_a_draft_closes_it_again() {
    let api = TestApi::with_database().await;
    let admin = api.signup_admin("root").await;
    let created = api.create_event(&admin, HOUR, 6 * HOUR).await;

    let detail = api.publish_event(&admin, created.id).await;
    assert_eq!(detail.event.status, EventStatus::Published);

    let public: Paginated<Event> = read_json(api.get("/api/events").await).await;
    assert!(
        public.items.iter().any(|e| e.id == created.id),
        "published, so listed"
    );

    let response = api
        .get(&format!("/api/checkin/{}", detail.checkin_code))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let gate: CheckinGate = read_json(response).await;
    assert_eq!(gate.event.id, created.id);
    assert_eq!(
        gate.window,
        CheckinWindow::Early,
        "the doors open in an hour"
    );

    let back = api
        .post_as(
            &format!("/api/admin/events/{}/status", created.id),
            &json!({ "status": "draft" }),
            &admin,
        )
        .await;
    assert_eq!(back.status(), StatusCode::OK);
    let gone = api
        .get(&format!("/api/checkin/{}", detail.checkin_code))
        .await;
    assert_error(gone, StatusCode::NOT_FOUND, "ItemNotFound").await;
}

#[tokio::test]
async fn a_player_checks_in_one_time_and_the_door_pays() {
    let api = TestApi::with_database().await;
    let admin = api.signup_admin("root").await;
    let dave = api.signup("dave").await.token;
    let erin = api.signup("erin").await.token;
    let created = api.create_event(&admin, -HOUR, 6 * HOUR).await;
    let detail = api.publish_event(&admin, created.id).await;
    let door = format!("/api/checkin/{}", detail.checkin_code);

    let gate: CheckinGate = read_json(api.get(&door).await).await;
    assert_eq!(gate.window, CheckinWindow::Open);

    let response = api.post_as(&door, &json!({}), &dave).await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let receipt: CheckinReceipt = read_json(response).await;
    assert_eq!(receipt.cycles, CHECKIN_CYCLES);
    assert_eq!(receipt.event.checkin_count, 1);

    let standing = player(&api, "dave").await.standing;
    assert_eq!(standing.cycles, CHECKIN_CYCLES);
    assert_eq!(standing.rank, Rank::Guest, "one night wakes a zombie up");

    let history = log(&api, "dave").await;
    assert_eq!(history.entries.total, 1);
    let line = &history.entries.items[0];
    assert_eq!(line.kind, PointKind::Checkin);
    assert_eq!(line.amount, CHECKIN_CYCLES);
    assert_eq!(line.event_id, Some(created.id));
    assert_eq!(
        line.event_name.as_ref().map(AsRef::as_ref),
        Some("BUNKER//SESSION 04")
    );
    assert!(line.tournament_id.is_none());
    assert_eq!(history.totals[0].kind, PointKind::Checkin);

    // A second scan pays nothing and answers the first receipt.
    let again = api.post_as(&door, &json!({}), &dave).await;
    assert_eq!(again.status(), StatusCode::OK);
    let repeat: CheckinReceipt = read_json(again).await;
    assert_eq!(repeat.checked_in_at, receipt.checked_in_at);
    assert_eq!(repeat.cycles, 0, "a repeat pays nothing");
    assert_eq!(repeat.event.checkin_count, 1);
    assert_eq!(player(&api, "dave").await.standing.cycles, CHECKIN_CYCLES);

    let mine: Checkins = read_json(api.get_as("/api/me/checkins", &dave).await).await;
    assert_eq!(mine.events, vec![created.id]);
    let theirs: Checkins = read_json(api.get_as("/api/me/checkins", &erin).await).await;
    assert!(theirs.events.is_empty());

    api.post_as(&door, &json!({}), &erin).await;
    let detail: EventDetail = read_json(
        api.get_as(&format!("/api/admin/events/{}", created.id), &admin)
            .await,
    )
    .await;
    assert_eq!(detail.event.checkin_count, 2);
    let handles: Vec<&str> = detail
        .checkins
        .iter()
        .map(|c| c.player.handle.as_ref())
        .collect();
    assert_eq!(handles, ["dave", "erin"], "first at the door first");
}

#[tokio::test]
async fn the_door_refuses_a_scan_outside_the_window() {
    let api = TestApi::with_database().await;
    let admin = api.signup_admin("root").await;
    let dave = api.signup("dave").await.token;

    let early = api.create_event(&admin, HOUR, 6 * HOUR).await;
    let early = api.publish_event(&admin, early.id).await;
    let refused = api
        .post_as(
            &format!("/api/checkin/{}", early.checkin_code),
            &json!({}),
            &dave,
        )
        .await;
    assert_error(refused, StatusCode::CONFLICT, "CheckinClosed").await;

    let over = api.create_event(&admin, -7 * HOUR, 6 * HOUR).await;
    let over = api.publish_event(&admin, over.id).await;
    let gate: CheckinGate = read_json(
        api.get(&format!("/api/checkin/{}", over.checkin_code))
            .await,
    )
    .await;
    assert_eq!(gate.window, CheckinWindow::Over);
    let refused = api
        .post_as(
            &format!("/api/checkin/{}", over.checkin_code),
            &json!({}),
            &dave,
        )
        .await;
    assert_error(refused, StatusCode::CONFLICT, "CheckinClosed").await;

    let unknown = api.get("/api/checkin/zzzzzzzzzzzz").await;
    assert_error(unknown, StatusCode::NOT_FOUND, "ItemNotFound").await;
    let anonymous = api.post("/api/checkin/zzzzzzzzzzzz", &json!({})).await;
    assert_error(anonymous, StatusCode::UNAUTHORIZED, "Unauthorized").await;

    assert_eq!(player(&api, "dave").await.standing.cycles, 0);
}

#[tokio::test]
async fn a_put_replaces_every_field_and_refuses_a_window_out_of_order() {
    let api = TestApi::with_database().await;
    let admin = api.signup_admin("root").await;
    let created = api.create_event(&admin, HOUR, 6 * HOUR).await;
    let code = api.publish_event(&admin, created.id).await.checkin_code;
    let path = format!("/api/admin/events/{}", created.id);

    let starts_at = OffsetDateTime::now_utc() + Duration::hours(48);
    let ends_at = starts_at + Duration::hours(5);
    let response = api
        .put_as(
            &path,
            &json!({
                "name": "BUNKER//SESSION 05",
                "startsAt": starts_at.format(&Rfc3339).unwrap(),
                "endsAt": ends_at.format(&Rfc3339).unwrap(),
            }),
            &admin,
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let updated: Event = read_json(response).await;
    assert_eq!(updated.name.as_ref(), "BUNKER//SESSION 05");
    assert_eq!(updated.location.as_ref(), "", "an absent field is cleared");
    assert!(updated.image.is_none(), "an absent cover is cleared");
    assert_eq!(updated.status, EventStatus::Published, "the status stays");
    let detail: EventDetail = read_json(api.get_as(&path, &admin).await).await;
    assert_eq!(detail.checkin_code, code, "the code stays");

    let backwards = api
        .put_as(
            &path,
            &json!({
                "name": "BUNKER//SESSION 05",
                "startsAt": ends_at.format(&Rfc3339).unwrap(),
                "endsAt": starts_at.format(&Rfc3339).unwrap(),
            }),
            &admin,
        )
        .await;
    assert_error(
        backwards,
        StatusCode::UNPROCESSABLE_ENTITY,
        "UnprocessableRequest",
    )
    .await;

    let stranger = api
        .put_as(
            &path,
            &json!({
                "name": "x",
                "startsAt": starts_at.format(&Rfc3339).unwrap(),
                "endsAt": ends_at.format(&Rfc3339).unwrap(),
                "checkinCode": "abcdefghij12",
            }),
            &admin,
        )
        .await;
    assert_error(
        stranger,
        StatusCode::UNPROCESSABLE_ENTITY,
        "UnprocessableRequest",
    )
    .await;

    let missing = api
        .put_as(
            "/api/admin/events/00000000-0000-4000-8000-0000000000ff",
            &json!({
                "name": "x",
                "startsAt": starts_at.format(&Rfc3339).unwrap(),
                "endsAt": ends_at.format(&Rfc3339).unwrap(),
            }),
            &admin,
        )
        .await;
    assert_error(missing, StatusCode::NOT_FOUND, "ItemNotFound").await;
}

#[tokio::test]
async fn deleting_an_event_takes_its_checkins_and_their_cycles() {
    let api = TestApi::with_database().await;
    let admin = api.signup_admin("root").await;
    let dave = api.signup("dave").await.token;
    let created = api.create_event(&admin, -HOUR, 6 * HOUR).await;
    let detail = api.publish_event(&admin, created.id).await;
    let door = format!("/api/checkin/{}", detail.checkin_code);
    api.post_as(&door, &json!({}), &dave).await;
    assert_eq!(player(&api, "dave").await.standing.cycles, CHECKIN_CYCLES);

    let deleted = api
        .delete_as(&format!("/api/admin/events/{}", created.id), &admin)
        .await;
    assert_eq!(deleted.status(), StatusCode::NO_CONTENT);

    assert_eq!(player(&api, "dave").await.standing.cycles, 0);
    assert_eq!(log(&api, "dave").await.entries.total, 0);
    let mine: Checkins = read_json(api.get_as("/api/me/checkins", &dave).await).await;
    assert!(mine.events.is_empty());
    let gone = api.get(&door).await;
    assert_error(gone, StatusCode::NOT_FOUND, "ItemNotFound").await;

    // Idempotent, like every delete.
    let again = api
        .delete_as(&format!("/api/admin/events/{}", created.id), &admin)
        .await;
    assert_eq!(again.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn an_admin_checks_a_player_in_by_hand_for_any_night() {
    let api = TestApi::with_database().await;
    let admin = api.signup_admin("root").await;
    let user = api.signup("dave").await.token;
    let dave = player(&api, "dave").await;
    // A night from long before the door existed, still a draft.
    let old = api.create_event(&admin, -400 * 24 * HOUR, 6 * HOUR).await;
    let path = format!("/api/admin/events/{}/checkins", old.id);

    let refused = api
        .post_as(&path, &json!({ "playerId": dave.id }), &user)
        .await;
    assert_error(refused, StatusCode::FORBIDDEN, "Forbidden").await;

    let response = api
        .post_as(&path, &json!({ "playerId": dave.id }), &admin)
        .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let receipt: CheckinReceipt = read_json(response).await;
    assert_eq!(receipt.cycles, CHECKIN_CYCLES);
    assert_eq!(receipt.event.checkin_count, 1);
    assert_eq!(player(&api, "dave").await.standing.cycles, CHECKIN_CYCLES);
    let line = &log(&api, "dave").await.entries.items[0];
    assert_eq!(line.kind, PointKind::Checkin);
    assert_eq!(line.event_id, Some(old.id));

    // The same rule as the door: one time per player per event.
    let again = api
        .post_as(&path, &json!({ "playerId": dave.id }), &admin)
        .await;
    assert_eq!(again.status(), StatusCode::OK);
    let repeat: CheckinReceipt = read_json(again).await;
    assert_eq!(repeat.cycles, 0);
    assert_eq!(player(&api, "dave").await.standing.cycles, CHECKIN_CYCLES);

    let detail: EventDetail = read_json(
        api.get_as(&format!("/api/admin/events/{}", old.id), &admin)
            .await,
    )
    .await;
    assert_eq!(detail.checkins.len(), 1);
    assert_eq!(detail.checkins[0].player.handle.as_ref(), "dave");

    let nobody = api
        .post_as(
            &path,
            &json!({ "playerId": "00000000-0000-4000-8000-0000000000ff" }),
            &admin,
        )
        .await;
    assert_error(nobody, StatusCode::NOT_FOUND, "ItemNotFound").await;
    let no_night = api
        .post_as(
            "/api/admin/events/00000000-0000-4000-8000-0000000000ff/checkins",
            &json!({ "playerId": dave.id }),
            &admin,
        )
        .await;
    assert_error(no_night, StatusCode::NOT_FOUND, "ItemNotFound").await;
}
