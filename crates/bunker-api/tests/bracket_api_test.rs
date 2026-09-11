//! Brackets from the contract: generation, seeds, results and the conclusion.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod support;

use axum::http::StatusCode;
use bunker_models::{
    Bracket, Entrant, EntrantId, Match, Tournament, TournamentDetail, TournamentId,
    TournamentStatus,
};
use serde_json::json;
use support::{TestApi, assert_error, read_json};
use uuid::Uuid;

const HOUR: i64 = 3600;

/// A live tournament with `count` entrants, ready for a bracket.
async fn live_tournament(api: &TestApi, admin: &str, count: usize) -> (Tournament, Vec<Entrant>) {
    let created = api.create_tournament(admin, HOUR).await;
    let mut entrants = Vec::new();
    for index in 0..count {
        entrants.push(
            api.add_entrant(admin, created.id, &format!("player{index}"))
                .await,
        );
    }
    let live = api.set_status(admin, created.id, "live").await;

    (live, entrants)
}

async fn generate(api: &TestApi, admin: &str, tournament: TournamentId) -> Bracket {
    let response = api
        .post_as(
            &format!("/api/admin/tournaments/{tournament}/bracket"),
            &json!({}),
            admin,
        )
        .await;
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "bracket generation failed"
    );

    read_json(response).await
}

async fn report(
    api: &TestApi,
    admin: &str,
    tournament: TournamentId,
    m: &Match,
    winner: EntrantId,
) -> Bracket {
    let response = api
        .put_as(
            &format!(
                "/api/admin/tournaments/{tournament}/matches/{}/result",
                m.id
            ),
            &json!({ "winner": winner }),
            admin,
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK, "report failed");

    read_json(response).await
}

#[tokio::test]
async fn a_bracket_needs_a_live_tournament_with_two_entrants() {
    let api = TestApi::with_database().await;
    let admin = api.signup_admin("root").await;

    let draft = api.create_tournament(&admin, HOUR).await;
    api.add_entrant(&admin, draft.id, "alpha").await;
    api.add_entrant(&admin, draft.id, "bravo").await;
    let not_live = api
        .post_as(
            &format!("/api/admin/tournaments/{}/bracket", draft.id),
            &json!({}),
            &admin,
        )
        .await;
    assert_error(not_live, StatusCode::CONFLICT, "InvalidState").await;

    let (lonely, _) = live_tournament(&api, &admin, 1).await;
    let too_few = api
        .post_as(
            &format!("/api/admin/tournaments/{}/bracket", lonely.id),
            &json!({}),
            &admin,
        )
        .await;
    assert_error(too_few, StatusCode::CONFLICT, "InvalidState").await;
}

#[tokio::test]
async fn five_entrants_give_three_rounds_and_three_byes() {
    let api = TestApi::with_database().await;
    let admin = api.signup_admin("root").await;
    let (tournament, entrants) = live_tournament(&api, &admin, 5).await;

    let bracket = generate(&api, &admin, tournament.id).await;

    assert_eq!(
        bracket.rounds.iter().map(Vec::len).collect::<Vec<_>>(),
        [4, 2, 1]
    );
    let byes = bracket.rounds[0].iter().filter(|m| !m.is_ready()).count();
    assert_eq!(byes, 3);
    assert!(!bracket.has_results());

    let detail: TournamentDetail = read_json(
        api.get(&format!("/api/tournaments/{}", tournament.id))
            .await,
    )
    .await;
    assert!(detail.tournament.has_bracket);
    assert_eq!(detail.bracket.unwrap(), bracket);
    let mut seeds: Vec<u32> = detail.entrants.iter().map(|e| e.seed.unwrap()).collect();
    seeds.sort_unstable();
    assert_eq!(seeds, [1, 2, 3, 4, 5]);
    let ids: Vec<EntrantId> = detail.entrants.iter().map(|e| e.id).collect();
    for entrant in &entrants {
        assert!(ids.contains(&entrant.id));
    }
}

#[tokio::test]
async fn a_bracket_regenerates_and_deletes_until_a_result_lands() {
    let api = TestApi::with_database().await;
    let admin = api.signup_admin("root").await;
    let (tournament, _) = live_tournament(&api, &admin, 4).await;
    let bracket_path = format!("/api/admin/tournaments/{}/bracket", tournament.id);

    let first = generate(&api, &admin, tournament.id).await;
    let second = generate(&api, &admin, tournament.id).await;
    assert_ne!(
        first.rounds[0][0].id, second.rounds[0][0].id,
        "regeneration makes new matches"
    );

    let m = second.rounds[0][0].clone();
    report(&api, &admin, tournament.id, &m, m.entrant_a.unwrap()).await;

    let locked = api.post_as(&bracket_path, &json!({}), &admin).await;
    assert_error(locked, StatusCode::CONFLICT, "InvalidState").await;
    let locked = api.delete_as(&bracket_path, &admin).await;
    assert_error(locked, StatusCode::CONFLICT, "InvalidState").await;

    // Clear the result, and the bracket is free again.
    let cleared = api
        .delete_as(
            &format!(
                "/api/admin/tournaments/{}/matches/{}/result",
                tournament.id, m.id
            ),
            &admin,
        )
        .await;
    assert_eq!(cleared.status(), StatusCode::OK);
    assert_eq!(
        api.delete_as(&bracket_path, &admin).await.status(),
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        api.delete_as(&bracket_path, &admin).await.status(),
        StatusCode::NO_CONTENT,
        "deleting twice is fine"
    );

    let detail: TournamentDetail = read_json(
        api.get(&format!("/api/tournaments/{}", tournament.id))
            .await,
    )
    .await;
    assert!(detail.bracket.is_none());
    assert!(!detail.tournament.has_bracket);
    assert!(detail.entrants.iter().all(|e| e.seed.is_none()));
}

#[tokio::test]
async fn seeds_follow_the_given_order() {
    let api = TestApi::with_database().await;
    let admin = api.signup_admin("root").await;
    let (tournament, entrants) = live_tournament(&api, &admin, 4).await;
    let seeds_path = format!("/api/admin/tournaments/{}/seeds", tournament.id);
    let ids: Vec<EntrantId> = entrants.iter().map(|e| e.id).collect();

    let before = api
        .put_as(&seeds_path, &json!({ "entrants": ids }), &admin)
        .await;
    assert_error(before, StatusCode::CONFLICT, "InvalidState").await;

    generate(&api, &admin, tournament.id).await;

    let response = api
        .put_as(&seeds_path, &json!({ "entrants": ids }), &admin)
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let bracket: Bracket = read_json(response).await;
    assert_eq!(bracket.rounds[0][0].entrant_a, Some(ids[0]));
    assert_eq!(bracket.rounds[0][0].entrant_b, Some(ids[3]));
    assert_eq!(bracket.rounds[0][1].entrant_a, Some(ids[1]));
    assert_eq!(bracket.rounds[0][1].entrant_b, Some(ids[2]));

    let detail: TournamentDetail = read_json(
        api.get(&format!("/api/tournaments/{}", tournament.id))
            .await,
    )
    .await;
    let first = detail.entrants.iter().find(|e| e.id == ids[0]).unwrap();
    assert_eq!(first.seed, Some(1));

    let short = api
        .put_as(&seeds_path, &json!({ "entrants": ids[..3] }), &admin)
        .await;
    assert_error(
        short,
        StatusCode::UNPROCESSABLE_ENTITY,
        "UnprocessableRequest",
    )
    .await;
    let mut with_stranger = ids.clone();
    with_stranger[0] = EntrantId::new(Uuid::new_v4());
    let stranger = api
        .put_as(&seeds_path, &json!({ "entrants": with_stranger }), &admin)
        .await;
    assert_error(
        stranger,
        StatusCode::UNPROCESSABLE_ENTITY,
        "UnprocessableRequest",
    )
    .await;
}

#[tokio::test]
async fn results_move_winners_up_and_the_champion_concludes_the_tournament() {
    let api = TestApi::with_database().await;
    let admin = api.signup_admin("root").await;
    let (tournament, _) = live_tournament(&api, &admin, 4).await;
    let status_path = format!("/api/admin/tournaments/{}/status", tournament.id);
    let bracket = generate(&api, &admin, tournament.id).await;

    let incomplete = api
        .post_as(&status_path, &json!({ "status": "concluded" }), &admin)
        .await;
    assert_error(incomplete, StatusCode::CONFLICT, "InvalidState").await;

    let top = bracket.rounds[0][0].clone();
    let bottom = bracket.rounds[0][1].clone();
    let after_top = report(&api, &admin, tournament.id, &top, top.entrant_b.unwrap()).await;
    assert_eq!(after_top.rounds[1][0].entrant_a, top.entrant_b);
    let after_bottom = report(
        &api,
        &admin,
        tournament.id,
        &bottom,
        bottom.entrant_a.unwrap(),
    )
    .await;
    assert_eq!(after_bottom.rounds[1][0].entrant_b, bottom.entrant_a);

    let final_match = after_bottom.rounds[1][0].clone();
    let done = report(
        &api,
        &admin,
        tournament.id,
        &final_match,
        bottom.entrant_a.unwrap(),
    )
    .await;
    assert_eq!(done.champion(), bottom.entrant_a);

    let named = api
        .post_as(
            &status_path,
            &json!({ "status": "concluded", "winner": bottom.entrant_a }),
            &admin,
        )
        .await;
    assert_error(named, StatusCode::CONFLICT, "InvalidState").await;

    let concluded = api.set_status(&admin, tournament.id, "concluded").await;
    assert_eq!(concluded.status, TournamentStatus::Concluded);
    assert_eq!(concluded.winner.unwrap().id, bottom.entrant_a.unwrap());

    let frozen = api
        .delete_as(
            &format!(
                "/api/admin/tournaments/{}/matches/{}/result",
                tournament.id, final_match.id
            ),
            &admin,
        )
        .await;
    assert_error(frozen, StatusCode::CONFLICT, "InvalidState").await;
}

#[tokio::test]
async fn a_result_respects_the_match_and_its_neighbours() {
    let api = TestApi::with_database().await;
    let admin = api.signup_admin("root").await;
    let (tournament, _) = live_tournament(&api, &admin, 4).await;
    let bracket = generate(&api, &admin, tournament.id).await;
    let top = bracket.rounds[0][0].clone();
    let bottom = bracket.rounds[0][1].clone();
    let final_match = bracket.rounds[1][0].clone();
    let result_path = |m: &Match| {
        format!(
            "/api/admin/tournaments/{}/matches/{}/result",
            tournament.id, m.id
        )
    };

    let not_ready = api
        .put_as(
            &result_path(&final_match),
            &json!({ "winner": top.entrant_a }),
            &admin,
        )
        .await;
    assert_error(not_ready, StatusCode::CONFLICT, "InvalidState").await;

    let outsider = api
        .put_as(
            &result_path(&top),
            &json!({ "winner": bottom.entrant_a }),
            &admin,
        )
        .await;
    assert_error(outsider, StatusCode::UNPROCESSABLE_ENTITY, "NotAnEntrant").await;

    let unknown = api
        .put_as(
            &format!(
                "/api/admin/tournaments/{}/matches/{}/result",
                tournament.id,
                Uuid::new_v4()
            ),
            &json!({ "winner": top.entrant_a }),
            &admin,
        )
        .await;
    assert_error(unknown, StatusCode::NOT_FOUND, "ItemNotFound").await;

    report(&api, &admin, tournament.id, &top, top.entrant_a.unwrap()).await;
    report(
        &api,
        &admin,
        tournament.id,
        &bottom,
        bottom.entrant_a.unwrap(),
    )
    .await;
    report(
        &api,
        &admin,
        tournament.id,
        &final_match,
        top.entrant_a.unwrap(),
    )
    .await;

    let decided = api
        .put_as(
            &result_path(&top),
            &json!({ "winner": top.entrant_b }),
            &admin,
        )
        .await;
    assert_error(decided, StatusCode::CONFLICT, "InvalidState").await;
    let decided = api.delete_as(&result_path(&top), &admin).await;
    assert_error(decided, StatusCode::CONFLICT, "InvalidState").await;
}

#[tokio::test]
async fn a_bye_cannot_be_cleared_and_entrants_are_frozen_by_the_bracket() {
    let api = TestApi::with_database().await;
    let admin = api.signup_admin("root").await;
    let (tournament, entrants) = live_tournament(&api, &admin, 3).await;
    let bracket = generate(&api, &admin, tournament.id).await;
    let bye = bracket.rounds[0].iter().find(|m| !m.is_ready()).unwrap();

    let cleared = api
        .delete_as(
            &format!(
                "/api/admin/tournaments/{}/matches/{}/result",
                tournament.id, bye.id
            ),
            &admin,
        )
        .await;
    assert_error(cleared, StatusCode::CONFLICT, "InvalidState").await;

    let entrants_path = format!("/api/admin/tournaments/{}/entrants", tournament.id);
    let newcomer = api.signup_player("late").await;
    let added = api
        .post_as(&entrants_path, &json!({ "playerId": newcomer.id }), &admin)
        .await;
    assert_error(added, StatusCode::CONFLICT, "InvalidState").await;
    let removed = api
        .delete_as(&format!("{entrants_path}/{}", entrants[0].id), &admin)
        .await;
    assert_error(removed, StatusCode::CONFLICT, "InvalidState").await;

    let back = api
        .post_as(
            &format!("/api/admin/tournaments/{}/status", tournament.id),
            &json!({ "status": "open" }),
            &admin,
        )
        .await;
    assert_error(back, StatusCode::CONFLICT, "InvalidState").await;
}

#[tokio::test]
async fn a_user_cannot_touch_a_bracket() {
    let api = TestApi::with_database().await;
    let admin = api.signup_admin("root").await;
    let user = api.signup("dave").await.token;
    let (tournament, _) = live_tournament(&api, &admin, 2).await;

    let refused = api
        .post_as(
            &format!("/api/admin/tournaments/{}/bracket", tournament.id),
            &json!({}),
            &user,
        )
        .await;
    assert_error(refused, StatusCode::FORBIDDEN, "Forbidden").await;
}
