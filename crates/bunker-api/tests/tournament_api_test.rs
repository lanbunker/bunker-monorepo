//! Tournaments from the contract: who sees what, the status machine, and who
//! may enter or leave.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod support;

use std::collections::HashSet;

use axum::http::StatusCode;
use bunker_models::{
    Entrant, Paginated, Registrations, SkillLevel, Tournament, TournamentDetail, TournamentId,
    TournamentStatus,
};
use serde_json::json;
use support::{HOUR, TestApi, assert_error, read_json};
use uuid::Uuid;

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

    let response = api.post_as(&path, &json!({ "skill": 2 }), &dave).await;
    assert_eq!(response.status(), StatusCode::OK);
    let entrant: Entrant = read_json(response).await;
    assert_eq!(entrant.player.as_ref().unwrap().handle.as_ref(), "dave");
    assert!(entrant.seed.is_none());
    assert_eq!(entrant.skill.map(SkillLevel::value), Some(2));

    let again: Entrant = read_json(api.post_as(&path, &json!({ "skill": 5 }), &dave).await).await;
    assert_eq!(again.id, entrant.id, "a second apply is the same entry");
    assert_eq!(
        again.skill.map(SkillLevel::value),
        Some(5),
        "a second apply changes the level"
    );

    let detail: TournamentDetail =
        read_json(api.get(&format!("/api/tournaments/{}", created.id)).await).await;
    assert_eq!(detail.tournament.entrant_count, 1);
    assert_eq!(detail.entrants.len(), 1);
    assert_eq!(detail.entrants[0].skill.map(SkillLevel::value), Some(5));

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
            &json!({ "skill": 3 }),
            &dave,
        )
        .await;
    assert_error(hidden, StatusCode::NOT_FOUND, "ItemNotFound").await;

    let expired = api.create_tournament(&admin, -HOUR).await;
    api.set_status(&admin, expired.id, "open").await;
    let late = api
        .post_as(
            &format!("/api/tournaments/{}/registration", expired.id),
            &json!({ "skill": 3 }),
            &dave,
        )
        .await;
    assert_error(late, StatusCode::CONFLICT, "RegistrationClosed").await;

    let live = api.create_tournament(&admin, HOUR).await;
    api.set_status(&admin, live.id, "live").await;
    let frozen = api
        .post_as(
            &format!("/api/tournaments/{}/registration", live.id),
            &json!({ "skill": 3 }),
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
            &json!({ "skill": 3 }),
        )
        .await;
    assert_error(anonymous, StatusCode::UNAUTHORIZED, "Unauthorized").await;
}

#[tokio::test]
async fn an_admin_add_keeps_one_entry_and_corrects_the_level() {
    let api = TestApi::with_database().await;
    let admin = api.signup_admin("root").await;
    let created = api.create_tournament(&admin, -HOUR).await;
    let path = format!("/api/admin/tournaments/{}/entrants", created.id);

    let dave = api.add_entrant(&admin, created.id, "dave").await;
    assert!(
        dave.skill.is_none(),
        "an admin can add a player with no level"
    );
    let again = api
        .post_as(
            &path,
            &json!({ "playerId": dave.player.as_ref().unwrap().id, "skill": 4 }),
            &admin,
        )
        .await;
    assert_eq!(again.status(), StatusCode::OK, "already in answers 200");
    let same: Entrant = read_json(again).await;
    assert_eq!(same.id, dave.id);
    assert_eq!(
        same.skill.map(SkillLevel::value),
        Some(4),
        "an admin add with a level corrects the level"
    );
    let untouched = api
        .post_as(
            &path,
            &json!({ "playerId": dave.player.as_ref().unwrap().id }),
            &admin,
        )
        .await;
    let kept: Entrant = read_json(untouched).await;
    assert_eq!(
        kept.skill.map(SkillLevel::value),
        Some(4),
        "an admin add without a level leaves the level alone"
    );

    let erin = api
        .add_rated_entrant(&admin, created.id, "erin", Some(5))
        .await;
    assert_eq!(erin.skill.map(SkillLevel::value), Some(5));

    let ghost = api
        .post_as(&path, &json!({ "playerId": Uuid::new_v4() }), &admin)
        .await;
    assert_error(ghost, StatusCode::NOT_FOUND, "ItemNotFound").await;

    let removed = api.delete_as(&format!("{path}/{}", dave.id), &admin).await;
    assert_eq!(removed.status(), StatusCode::NO_CONTENT);
    let removed_again = api.delete_as(&format!("{path}/{}", dave.id), &admin).await;
    assert_eq!(removed_again.status(), StatusCode::NO_CONTENT);
    let removed_erin = api.delete_as(&format!("{path}/{}", erin.id), &admin).await;
    assert_eq!(removed_erin.status(), StatusCode::NO_CONTENT);

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
        &json!({ "skill": 3 }),
        &dave,
    )
    .await;
    api.post_as(
        &format!("/api/tournaments/{}/registration", second.id),
        &json!({ "skill": 3 }),
        &dave,
    )
    .await;
    let both: Registrations = read_json(api.get_as("/api/me/registrations", &dave).await).await;
    let both: HashSet<TournamentId> = both.tournaments.into_iter().collect();
    assert_eq!(both, HashSet::from([first.id, second.id]));

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

#[tokio::test]
async fn an_apply_needs_a_level_from_one_to_five() {
    let api = TestApi::with_database().await;
    let admin = api.signup_admin("root").await;
    let created = api.create_tournament(&admin, HOUR).await;
    api.set_status(&admin, created.id, "open").await;
    let dave = api.signup("dave").await.token;
    let path = format!("/api/tournaments/{}/registration", created.id);

    for (label, body) in [
        ("no level", json!({})),
        ("level 0", json!({ "skill": 0 })),
        ("level 6", json!({ "skill": 6 })),
        ("a text level", json!({ "skill": "3" })),
        ("a seed", json!({ "skill": 3, "seed": 1 })),
    ] {
        let refused = api.post_as(&path, &body, &dave).await;
        assert_eq!(
            refused.status(),
            StatusCode::UNPROCESSABLE_ENTITY,
            "{label} was accepted"
        );
        let error: serde_json::Value = read_json(refused).await;
        assert_eq!(error["code"], "UnprocessableRequest", "{label}");
    }

    let detail: TournamentDetail =
        read_json(api.get(&format!("/api/tournaments/{}", created.id)).await).await;
    assert_eq!(
        detail.tournament.entrant_count, 0,
        "a refused apply enters nobody"
    );

    for level in 1..=5 {
        let entrant: Entrant =
            read_json(api.post_as(&path, &json!({ "skill": level }), &dave).await).await;
        assert_eq!(entrant.skill.map(SkillLevel::value), Some(level));
    }
}

#[tokio::test]
async fn an_admin_adds_and_removes_entrants_in_draft_open_and_live() {
    let api = TestApi::with_database().await;
    let admin = api.signup_admin("root").await;

    for (index, status) in ["draft", "open", "live"].into_iter().enumerate() {
        let created = api.create_tournament(&admin, -HOUR).await;
        if status != "draft" {
            api.set_status(&admin, created.id, status).await;
        }
        let entrant = api
            .add_entrant(&admin, created.id, &format!("player{index}"))
            .await;

        let removed = api
            .delete_as(
                &format!(
                    "/api/admin/tournaments/{}/entrants/{}",
                    created.id, entrant.id
                ),
                &admin,
            )
            .await;
        assert_eq!(removed.status(), StatusCode::NO_CONTENT, "{status}");
        assert!(api.detail(&admin, created.id).await.entrants.is_empty());
    }
}

/// `concluded` is final: the entrants and the fields of the tournament stay as
/// they were when it paid.
#[tokio::test]
async fn a_concluded_tournament_refuses_entrants_and_edits() {
    let api = TestApi::with_database().await;
    let admin = api.signup_admin("root").await;
    let created = api.create_tournament(&admin, HOUR).await;
    let dave = api.add_entrant(&admin, created.id, "dave").await;
    api.set_status(&admin, created.id, "concluded").await;
    let erin = api.signup_player("erin").await;
    let entrants = format!("/api/admin/tournaments/{}/entrants", created.id);

    let added = api
        .post_as(&entrants, &json!({ "playerId": erin.id }), &admin)
        .await;
    let removed = api
        .delete_as(&format!("{entrants}/{}", dave.id), &admin)
        .await;
    let edited = api
        .patch_as(
            &format!("/api/admin/tournaments/{}", created.id),
            &json!({ "name": "Renamed Cup" }),
            &admin,
        )
        .await;

    for refused in [added, removed, edited] {
        let status = refused.status();
        let body: serde_json::Value = read_json(refused).await;
        assert_eq!(status, StatusCode::CONFLICT, "{body}");
        assert_eq!(body["code"], "InvalidState");
        assert_eq!(
            body["message"],
            "The tournament is concluded and takes no more changes"
        );
    }
    let detail = api.detail(&admin, created.id).await;
    assert_eq!(detail.tournament.name.as_ref(), "Sniper Cup");
    assert_eq!(detail.entrants.len(), 1);
}

/// Two clicks on the conclude button at once. The second write finds the
/// status moved and answers like a second click, and the ledger pays one time.
#[tokio::test]
async fn two_concludes_at_once_pay_one_time() {
    let api = TestApi::with_database().await;
    let admin = api.signup_admin("root").await;
    let created = api.create_tournament(&admin, HOUR).await;
    let dave = api.add_entrant(&admin, created.id, "dave").await;
    api.add_entrant(&admin, created.id, "erin").await;
    api.set_status(&admin, created.id, "live").await;
    let path = format!("/api/admin/tournaments/{}/status", created.id);
    let conclude = json!({ "status": "concluded", "winner": dave.id });
    let repeat = json!({ "status": "concluded" });

    let (first, second) = tokio::join!(
        api.post_as(&path, &conclude, &admin),
        api.post_as(&path, &repeat, &admin),
    );

    let statuses = [first.status(), second.status()];
    assert!(
        statuses.contains(&StatusCode::OK),
        "one conclusion lands: {statuses:?}"
    );
    assert!(
        statuses
            .iter()
            .all(|status| [StatusCode::OK, StatusCode::CONFLICT].contains(status)),
        "a lost race is a conflict at worst, never a failure: {statuses:?}"
    );
    let (entries,): (i64,) =
        sqlx::query_as("select count(*) from point_entries where tournament_id = ?1")
            .bind(created.id.to_string())
            .fetch_one(api.pool())
            .await
            .unwrap();
    let concluded = api.detail(&admin, created.id).await.tournament;
    let expected = if concluded.winner.is_some() { 3 } else { 2 };
    assert_eq!(entries, expected, "two entries and at most one champion");
    assert_eq!(concluded.status, TournamentStatus::Concluded);
}

/// The field has a size. A player already in keeps the place and can change
/// the level of a full field.
#[tokio::test]
async fn a_full_field_takes_no_new_entrant() {
    let api = TestApi::with_database().await;
    let admin = api.signup_admin("root").await;
    let created = api.create_tournament(&admin, HOUR).await;
    api.set_status(&admin, created.id, "open").await;
    let dave = api.signup("dave").await.token;
    let register = format!("/api/tournaments/{}/registration", created.id);
    let joined = api.post_as(&register, &json!({ "skill": 2 }), &dave).await;
    assert_eq!(joined.status(), StatusCode::OK);
    for index in 1..usize::from(bunker_models::MAX_ENTRANTS) {
        let player = uuid::Uuid::new_v4().to_string();
        sqlx::query(
            "insert into players (id, handle, password_hash, glyph_bits, glyph_color, created_at)
             values (?1, ?2, 'x', 1, '#ffb000', 1)",
        )
        .bind(&player)
        .bind(format!("filler{index}"))
        .execute(api.pool())
        .await
        .unwrap();
        sqlx::query(
            "insert into tournament_entrants (id, tournament_id, player_id, registered_at)
             values (?1, ?2, ?3, 1)",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(created.id.to_string())
        .bind(&player)
        .execute(api.pool())
        .await
        .unwrap();
    }
    let erin = api.signup_player("erin").await;

    let by_admin = api
        .post_as(
            &format!("/api/admin/tournaments/{}/entrants", created.id),
            &json!({ "playerId": erin.id }),
            &admin,
        )
        .await;
    assert_error(by_admin, StatusCode::CONFLICT, "InvalidState").await;
    let late = api.signup("late").await.token;
    let by_player = api.post_as(&register, &json!({ "skill": 3 }), &late).await;
    assert_error(by_player, StatusCode::CONFLICT, "InvalidState").await;

    let relevel = api.post_as(&register, &json!({ "skill": 5 }), &dave).await;
    assert_eq!(
        relevel.status(),
        StatusCode::OK,
        "a player already in stays"
    );
    let entry: Entrant = read_json(relevel).await;
    assert_eq!(entry.skill.map(SkillLevel::value), Some(5));
    assert_eq!(
        api.detail(&admin, created.id).await.entrants.len(),
        usize::from(bunker_models::MAX_ENTRANTS)
    );
}
