//! Tournaments from the contract: who sees what, the status machine, and who
//! may enter or leave.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod support;

use axum::http::StatusCode;
use bunker_models::{
    Entrant, Paginated, Registrations, Tournament, TournamentDetail, TournamentStatus,
};
use serde_json::json;
use support::{TestApi, assert_error, read_json};
use uuid::Uuid;

const HOUR: i64 = 3600;

#[tokio::test]
async fn a_new_tournament_is_a_draft_that_only_admins_see() {
    let api = TestApi::with_database().await;
    let admin = api.signup_admin("root").await;
    let user = api.signup("dave").await.token;

    let refused = api
        .post_as("/api/admin/tournaments", &json!({}), &user)
        .await;
    assert_error(refused, StatusCode::FORBIDDEN, "Forbidden").await;

    let created = api.create_tournament(&admin, HOUR).await;
    assert_eq!(created.status, TournamentStatus::Draft);
    assert_eq!(created.entrant_count, 0);
    assert!(!created.has_bracket);
    assert!(created.winner.is_none());
    assert_eq!(created.name.as_ref(), "Sniper Cup");

    let public: Paginated<Tournament> = read_json(api.get("/api/tournaments").await).await;
    assert_eq!(public.total, 0, "a draft is not public");
    let hidden = api.get(&format!("/api/tournaments/{}", created.id)).await;
    assert_error(hidden, StatusCode::NOT_FOUND, "ItemNotFound").await;

    let all: Paginated<Tournament> =
        read_json(api.get_as("/api/admin/tournaments", &admin).await).await;
    assert_eq!(all.total, 1);
    let detail: TournamentDetail = read_json(
        api.get_as(&format!("/api/admin/tournaments/{}", created.id), &admin)
            .await,
    )
    .await;
    assert!(detail.entrants.is_empty());
    assert!(detail.bracket.is_none());
}

#[tokio::test]
async fn an_open_tournament_is_public_with_its_entrants() {
    let api = TestApi::with_database().await;
    let admin = api.signup_admin("root").await;
    let created = api.create_tournament(&admin, HOUR).await;
    api.set_status(&admin, created.id, "open").await;

    let response = api.get(&format!("/api/tournaments/{}", created.id)).await;
    assert_eq!(response.status(), StatusCode::OK);
    let detail: TournamentDetail = read_json(response).await;
    assert_eq!(detail.tournament.status, TournamentStatus::Open);
    assert!(detail.entrants.is_empty());
    assert!(detail.bracket.is_none());

    let public: Paginated<Tournament> = read_json(api.get("/api/tournaments").await).await;
    assert_eq!(public.items.len(), 1);
}

#[tokio::test]
async fn a_patch_changes_one_field_and_keeps_the_rest() {
    let api = TestApi::with_database().await;
    let admin = api.signup_admin("root").await;
    let created = api.create_tournament(&admin, HOUR).await;

    let response = api
        .patch_as(
            &format!("/api/admin/tournaments/{}", created.id),
            &json!({ "mode": "2v2 gun game" }),
            &admin,
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let updated: Tournament = read_json(response).await;
    assert_eq!(updated.mode.as_ref(), "2v2 gun game");
    assert_eq!(updated.name, created.name);
    assert_eq!(updated.date, created.date);
    assert_eq!(
        updated.registration_closes_at,
        created.registration_closes_at
    );

    let stranger = api
        .patch_as(
            &format!("/api/admin/tournaments/{}", created.id),
            &json!({ "status": "open" }),
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
        .patch_as(
            &format!("/api/admin/tournaments/{}", Uuid::new_v4()),
            &json!({ "mode": "x" }),
            &admin,
        )
        .await;
    assert_error(missing, StatusCode::NOT_FOUND, "ItemNotFound").await;
}

#[tokio::test]
async fn the_status_moves_forward_and_never_back_from_concluded() {
    let api = TestApi::with_database().await;
    let admin = api.signup_admin("root").await;
    let created = api.create_tournament(&admin, HOUR).await;
    let path = format!("/api/admin/tournaments/{}/status", created.id);

    let open = api.set_status(&admin, created.id, "open").await;
    assert_eq!(open.status, TournamentStatus::Open);
    let live = api.set_status(&admin, created.id, "live").await;
    assert_eq!(live.status, TournamentStatus::Live);

    // Back to open is fine while no bracket exists.
    api.set_status(&admin, created.id, "open").await;
    api.set_status(&admin, created.id, "live").await;

    let concluded = api.set_status(&admin, created.id, "concluded").await;
    assert_eq!(concluded.status, TournamentStatus::Concluded);
    assert!(
        concluded.winner.is_none(),
        "a winner is optional without a bracket"
    );

    for next in ["open", "live", "draft"] {
        let refused = api.post_as(&path, &json!({ "status": next }), &admin).await;
        assert_error(refused, StatusCode::CONFLICT, "InvalidState").await;
    }
}

#[tokio::test]
async fn a_draft_can_go_live_at_once_for_a_backfill_but_never_to_draft_again() {
    let api = TestApi::with_database().await;
    let admin = api.signup_admin("root").await;
    let created = api.create_tournament(&admin, HOUR).await;
    let path = format!("/api/admin/tournaments/{}/status", created.id);

    let live = api.set_status(&admin, created.id, "live").await;
    assert_eq!(live.status, TournamentStatus::Live);

    let refused = api
        .post_as(&path, &json!({ "status": "draft" }), &admin)
        .await;
    assert_error(refused, StatusCode::CONFLICT, "InvalidState").await;
}

#[tokio::test]
async fn a_winner_without_a_bracket_must_be_an_entrant() {
    let api = TestApi::with_database().await;
    let admin = api.signup_admin("root").await;
    let created = api.create_tournament(&admin, HOUR).await;
    let dave = api.add_entrant(&admin, created.id, "dave").await;
    let path = format!("/api/admin/tournaments/{}/status", created.id);

    let early = api
        .post_as(
            &path,
            &json!({ "status": "open", "winner": dave.id }),
            &admin,
        )
        .await;
    assert_error(early, StatusCode::CONFLICT, "InvalidState").await;

    let stranger = api
        .post_as(
            &path,
            &json!({ "status": "concluded", "winner": Uuid::new_v4() }),
            &admin,
        )
        .await;
    assert_error(stranger, StatusCode::UNPROCESSABLE_ENTITY, "NotAnEntrant").await;

    let response = api
        .post_as(
            &path,
            &json!({ "status": "concluded", "winner": dave.id }),
            &admin,
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let concluded: Tournament = read_json(response).await;
    let winner = concluded.winner.unwrap();
    assert_eq!(winner.id, dave.id);
    assert_eq!(winner.player.unwrap().handle.as_ref(), "dave");

    let public: Paginated<Tournament> = read_json(api.get("/api/tournaments").await).await;
    assert_eq!(
        public.items[0]
            .winner
            .as_ref()
            .and_then(|w| w.player.as_ref())
            .map(|p| p.handle.as_ref()),
        Some("dave"),
        "the public list carries the winner"
    );

    let leaving = api
        .delete_as(
            &format!("/api/admin/tournaments/{}/entrants/{}", created.id, dave.id),
            &admin,
        )
        .await;
    assert_error(leaving, StatusCode::CONFLICT, "InvalidState").await;
}

#[tokio::test]
async fn a_player_applies_and_retires_while_registration_is_open() {
    let api = TestApi::with_database().await;
    let admin = api.signup_admin("root").await;
    let created = api.create_tournament(&admin, HOUR).await;
    api.set_status(&admin, created.id, "open").await;
    let dave = api.signup("dave").await.token;
    let path = format!("/api/tournaments/{}/registration", created.id);

    let response = api.post_as(&path, &json!({}), &dave).await;
    assert_eq!(response.status(), StatusCode::OK);
    let entrant: Entrant = read_json(response).await;
    assert_eq!(entrant.player.as_ref().unwrap().handle.as_ref(), "dave");
    assert!(entrant.seed.is_none());

    let again: Entrant = read_json(api.post_as(&path, &json!({}), &dave).await).await;
    assert_eq!(again.id, entrant.id, "a second apply is the same entry");

    let detail: TournamentDetail =
        read_json(api.get(&format!("/api/tournaments/{}", created.id)).await).await;
    assert_eq!(detail.tournament.entrant_count, 1);
    assert_eq!(detail.entrants.len(), 1);

    assert_eq!(
        api.delete_as(&path, &dave).await.status(),
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        api.delete_as(&path, &dave).await.status(),
        StatusCode::NO_CONTENT,
        "retiring twice is fine"
    );
    let detail: TournamentDetail =
        read_json(api.get(&format!("/api/tournaments/{}", created.id)).await).await;
    assert_eq!(detail.tournament.entrant_count, 0);
}

#[tokio::test]
async fn registration_needs_an_open_tournament_before_its_deadline() {
    let api = TestApi::with_database().await;
    let admin = api.signup_admin("root").await;
    let dave = api.signup("dave").await.token;

    let draft = api.create_tournament(&admin, HOUR).await;
    let hidden = api
        .post_as(
            &format!("/api/tournaments/{}/registration", draft.id),
            &json!({}),
            &dave,
        )
        .await;
    assert_error(hidden, StatusCode::NOT_FOUND, "ItemNotFound").await;

    let expired = api.create_tournament(&admin, -HOUR).await;
    api.set_status(&admin, expired.id, "open").await;
    let late = api
        .post_as(
            &format!("/api/tournaments/{}/registration", expired.id),
            &json!({}),
            &dave,
        )
        .await;
    assert_error(late, StatusCode::CONFLICT, "RegistrationClosed").await;

    let live = api.create_tournament(&admin, HOUR).await;
    api.set_status(&admin, live.id, "live").await;
    let frozen = api
        .post_as(
            &format!("/api/tournaments/{}/registration", live.id),
            &json!({}),
            &dave,
        )
        .await;
    assert_error(frozen, StatusCode::CONFLICT, "RegistrationClosed").await;
    let leaving = api
        .delete_as(&format!("/api/tournaments/{}/registration", live.id), &dave)
        .await;
    assert_error(leaving, StatusCode::CONFLICT, "RegistrationClosed").await;

    let anonymous = api
        .post(
            &format!("/api/tournaments/{}/registration", live.id),
            &json!({}),
        )
        .await;
    assert_error(anonymous, StatusCode::UNAUTHORIZED, "Unauthorized").await;
}

#[tokio::test]
async fn an_admin_adds_and_removes_entrants_in_any_status() {
    let api = TestApi::with_database().await;
    let admin = api.signup_admin("root").await;
    let created = api.create_tournament(&admin, -HOUR).await;
    let path = format!("/api/admin/tournaments/{}/entrants", created.id);

    let dave = api.add_entrant(&admin, created.id, "dave").await;
    let again = api
        .post_as(
            &path,
            &json!({ "playerId": dave.player.as_ref().unwrap().id }),
            &admin,
        )
        .await;
    assert_eq!(again.status(), StatusCode::OK, "already in answers 200");
    let same: Entrant = read_json(again).await;
    assert_eq!(same.id, dave.id);

    let ghost = api
        .post_as(&path, &json!({ "playerId": Uuid::new_v4() }), &admin)
        .await;
    assert_error(ghost, StatusCode::NOT_FOUND, "ItemNotFound").await;

    let removed = api.delete_as(&format!("{path}/{}", dave.id), &admin).await;
    assert_eq!(removed.status(), StatusCode::NO_CONTENT);
    let removed_again = api.delete_as(&format!("{path}/{}", dave.id), &admin).await;
    assert_eq!(removed_again.status(), StatusCode::NO_CONTENT);

    let detail: TournamentDetail = read_json(
        api.get_as(&format!("/api/admin/tournaments/{}", created.id), &admin)
            .await,
    )
    .await;
    assert!(detail.entrants.is_empty());
}

#[tokio::test]
async fn a_deleted_tournament_takes_its_entrants_with_it() {
    let api = TestApi::with_database().await;
    let admin = api.signup_admin("root").await;
    let created = api.create_tournament(&admin, HOUR).await;
    api.add_entrant(&admin, created.id, "dave").await;
    api.set_status(&admin, created.id, "open").await;
    let path = format!("/api/admin/tournaments/{}", created.id);

    assert_eq!(
        api.delete_as(&path, &admin).await.status(),
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        api.delete_as(&path, &admin).await.status(),
        StatusCode::NO_CONTENT
    );

    let gone = api.get(&format!("/api/tournaments/{}", created.id)).await;
    assert_error(gone, StatusCode::NOT_FOUND, "ItemNotFound").await;
}

#[tokio::test]
async fn a_deleted_player_leaves_an_entrant_without_a_player() {
    let api = TestApi::with_database().await;
    let admin = api.signup_admin("root").await;
    let created = api.create_tournament(&admin, HOUR).await;
    let dave = api.add_entrant(&admin, created.id, "dave").await;
    api.set_status(&admin, created.id, "open").await;

    let deleted = api
        .delete_as(
            &format!("/api/admin/players/{}", dave.player.unwrap().id),
            &admin,
        )
        .await;
    assert_eq!(deleted.status(), StatusCode::NO_CONTENT);

    let detail: TournamentDetail =
        read_json(api.get(&format!("/api/tournaments/{}", created.id)).await).await;
    assert_eq!(detail.entrants.len(), 1);
    assert!(detail.entrants[0].player.is_none());
}

#[tokio::test]
async fn the_lists_paginate_newest_event_first() {
    let api = TestApi::with_database().await;
    let admin = api.signup_admin("root").await;
    for (name, date) in [("Old", "2026-02-26"), ("New", "2026-10-24")] {
        let response = api
            .post_as(
                "/api/admin/tournaments",
                &json!({
                    "name": name,
                    "game": "COD",
                    "mode": "1v1",
                    "date": date,
                    "registrationClosesAt": "2026-10-23T21:59:00Z",
                }),
                &admin,
            )
            .await;
        let created: Tournament = read_json(response).await;
        api.set_status(&admin, created.id, "open").await;
    }

    let page: Paginated<Tournament> =
        read_json(api.get("/api/tournaments?page=1&pageSize=1").await).await;
    assert_eq!(page.total, 2);
    assert_eq!(page.total_pages, 2);
    assert_eq!(page.items[0].name.as_ref(), "New");

    let second: Paginated<Tournament> =
        read_json(api.get("/api/tournaments?page=2&pageSize=1").await).await;
    assert_eq!(second.items[0].name.as_ref(), "Old");
}

#[tokio::test]
async fn a_tournament_body_is_checked_field_by_field() {
    let api = TestApi::with_database().await;
    let admin = api.signup_admin("root").await;
    let valid = json!({
        "name": "Cup",
        "game": "COD",
        "mode": "1v1",
        "date": "2026-10-24",
        "registrationClosesAt": "2026-10-23T21:59:00Z",
    });
    let mut long_name = valid.clone();
    long_name["name"] = json!("x".repeat(61));
    let mut european_date = valid.clone();
    european_date["date"] = json!("24/10/2026");
    let mut loose_deadline = valid.clone();
    loose_deadline["registrationClosesAt"] = json!("2026-10-23 21:59");
    let mut stranger = valid.clone();
    stranger["status"] = json!("open");

    for (label, body) in [
        ("a 61 character name", long_name),
        ("a European date", european_date),
        ("a deadline that is not rfc3339", loose_deadline),
        ("a status field", stranger),
    ] {
        let response = api.post_as("/api/admin/tournaments", &body, &admin).await;
        assert_eq!(
            response.status(),
            StatusCode::UNPROCESSABLE_ENTITY,
            "{label} was accepted"
        );
    }

    let ok = api.post_as("/api/admin/tournaments", &valid, &admin).await;
    assert_eq!(ok.status(), StatusCode::CREATED);
}

#[tokio::test]
async fn a_patch_with_an_empty_description_clears_it() {
    let api = TestApi::with_database().await;
    let admin = api.signup_admin("root").await;
    let created = api.create_tournament(&admin, HOUR).await;
    assert_eq!(created.description.as_ref(), "One life, one shot.");

    let response = api
        .patch_as(
            &format!("/api/admin/tournaments/{}", created.id),
            &json!({ "description": "" }),
            &admin,
        )
        .await;

    assert_eq!(response.status(), StatusCode::OK);
    let updated: Tournament = read_json(response).await;
    assert_eq!(updated.description.as_ref(), "");
}

#[tokio::test]
async fn the_same_status_twice_is_not_an_error() {
    let api = TestApi::with_database().await;
    let admin = api.signup_admin("root").await;
    let created = api.create_tournament(&admin, HOUR).await;

    api.set_status(&admin, created.id, "open").await;
    let again = api.set_status(&admin, created.id, "open").await;

    assert_eq!(again.status, TournamentStatus::Open);
}

#[tokio::test]
async fn a_client_message_is_a_sentence_and_names_no_id() {
    let api = TestApi::with_database().await;
    let admin = api.signup_admin("root").await;
    let created = api.create_tournament(&admin, HOUR).await;
    api.set_status(&admin, created.id, "concluded").await;

    let response = api
        .post_as(
            &format!("/api/admin/tournaments/{}/status", created.id),
            &json!({ "status": "open" }),
            &admin,
        )
        .await;
    assert_eq!(response.status(), StatusCode::CONFLICT);
    let body: serde_json::Value = read_json(response).await;
    let message = body["message"].as_str().unwrap();
    assert!(message.starts_with(char::is_uppercase), "got {message:?}");
    assert!(
        !message.contains(&created.id.to_string()),
        "got {message:?}"
    );
}

#[tokio::test]
async fn a_player_reads_their_registrations_in_one_call() {
    let api = TestApi::with_database().await;
    let admin = api.signup_admin("root").await;
    let first = api.create_tournament(&admin, HOUR).await;
    let second = api.create_tournament(&admin, HOUR).await;
    api.set_status(&admin, first.id, "open").await;
    api.set_status(&admin, second.id, "open").await;
    let dave = api.signup("dave").await.token;

    let none: Registrations = read_json(api.get_as("/api/me/registrations", &dave).await).await;
    assert!(none.tournaments.is_empty());

    api.post_as(
        &format!("/api/tournaments/{}/registration", first.id),
        &json!({}),
        &dave,
    )
    .await;
    api.post_as(
        &format!("/api/tournaments/{}/registration", second.id),
        &json!({}),
        &dave,
    )
    .await;
    let both: Registrations = read_json(api.get_as("/api/me/registrations", &dave).await).await;
    assert_eq!(both.tournaments, [first.id, second.id]);

    api.delete_as(
        &format!("/api/tournaments/{}/registration", first.id),
        &dave,
    )
    .await;
    let one: Registrations = read_json(api.get_as("/api/me/registrations", &dave).await).await;
    assert_eq!(one.tournaments, [second.id]);

    let anonymous = api.get("/api/me/registrations").await;
    assert_error(anonymous, StatusCode::UNAUTHORIZED, "Unauthorized").await;
}
