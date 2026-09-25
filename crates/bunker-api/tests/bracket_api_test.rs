//! Brackets from the contract: generation, seeds, results and the conclusion.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod support;

use std::sync::atomic::{AtomicUsize, Ordering};

use axum::http::StatusCode;
use bunker_models::{
    Bracket, Entrant, EntrantId, Match, Tournament, TournamentDetail, TournamentStatus,
};
use serde_json::json;
use support::{HOUR, TestApi, assert_error, read_json};
use uuid::Uuid;

/// A live tournament with `count` entrants, ready for a bracket. Handles stay
/// unique across calls, so one test can hold two tournaments.
async fn live_tournament(api: &TestApi, admin: &str, count: usize) -> (Tournament, Vec<Entrant>) {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let created = api.create_tournament(admin, HOUR).await;
    let mut entrants = Vec::new();
    for _ in 0..count {
        let number = NEXT.fetch_add(1, Ordering::Relaxed);
        entrants.push(
            api.add_entrant(admin, created.id, &format!("player{number}"))
                .await,
        );
    }
    let live = api.set_status(admin, created.id, "live").await;

    (live, entrants)
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

    let bracket = api.generate_bracket(&admin, tournament.id).await;

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

    let first = api.generate_bracket(&admin, tournament.id).await;
    let second = api.generate_bracket(&admin, tournament.id).await;
    assert_ne!(
        first.rounds[0][0].id, second.rounds[0][0].id,
        "regeneration makes new matches"
    );

    let m = second.rounds[0][0].clone();
    api.report(&admin, tournament.id, &m, m.entrant_a.unwrap())
        .await;

    let locked = api.post_as(&bracket_path, &json!({}), &admin).await;
    assert_error(locked, StatusCode::CONFLICT, "InvalidState").await;
    let locked = api.delete_as(&bracket_path, &admin).await;
    assert_error(locked, StatusCode::CONFLICT, "InvalidState").await;

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

    api.generate_bracket(&admin, tournament.id).await;

    let response = api
        .put_as(&seeds_path, &json!({ "entrants": ids }), &admin)
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let bracket: Bracket = read_json(response).await;
    assert_eq!(bracket.rounds[0][0].entrant_a, Some(ids[0]));
    assert_eq!(bracket.rounds[0][0].entrant_b, Some(ids[1]));
    assert_eq!(bracket.rounds[0][1].entrant_a, Some(ids[2]));
    assert_eq!(bracket.rounds[0][1].entrant_b, Some(ids[3]));

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
    let bracket = api.generate_bracket(&admin, tournament.id).await;

    let incomplete = api
        .post_as(&status_path, &json!({ "status": "concluded" }), &admin)
        .await;
    assert_error(incomplete, StatusCode::CONFLICT, "InvalidState").await;

    let top = bracket.rounds[0][0].clone();
    let bottom = bracket.rounds[0][1].clone();
    let after_top = api
        .report(&admin, tournament.id, &top, top.entrant_b.unwrap())
        .await;
    assert_eq!(after_top.rounds[1][0].entrant_a, top.entrant_b);
    let after_bottom = api
        .report(&admin, tournament.id, &bottom, bottom.entrant_a.unwrap())
        .await;
    assert_eq!(after_bottom.rounds[1][0].entrant_b, bottom.entrant_a);

    let final_match = after_bottom.rounds[1][0].clone();
    let done = api
        .report(
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
    let bracket = api.generate_bracket(&admin, tournament.id).await;
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

    api.report(&admin, tournament.id, &top, top.entrant_a.unwrap())
        .await;
    api.report(&admin, tournament.id, &bottom, bottom.entrant_a.unwrap())
        .await;
    api.report(&admin, tournament.id, &final_match, top.entrant_a.unwrap())
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
    let bracket = api.generate_bracket(&admin, tournament.id).await;
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

/// Thirty players, the size of a real session. The bracket is played to the
/// end through the API, then the tournament concludes with the champion.
#[tokio::test]
async fn thirty_entrants_play_through_the_api_to_a_champion() {
    let api = TestApi::with_database().await;
    let admin = api.signup_admin("root").await;
    let (tournament, entrants) = live_tournament(&api, &admin, 30).await;
    let mut bracket = api.generate_bracket(&admin, tournament.id).await;
    assert_eq!(
        bracket.rounds.iter().map(Vec::len).collect::<Vec<_>>(),
        [16, 8, 4, 2, 1]
    );

    let reversed: Vec<EntrantId> = entrants.iter().rev().map(|e| e.id).collect();
    let response = api
        .put_as(
            &format!("/api/admin/tournaments/{}/seeds", tournament.id),
            &json!({ "entrants": reversed }),
            &admin,
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    bracket = read_json(response).await;
    let detail: TournamentDetail = read_json(
        api.get_as(&format!("/api/admin/tournaments/{}", tournament.id), &admin)
            .await,
    )
    .await;
    let last = detail
        .entrants
        .iter()
        .find(|e| e.id == entrants[29].id)
        .unwrap();
    assert_eq!(last.seed, Some(1));
    assert_eq!(bracket.rounds[0][0].entrant_a, Some(entrants[29].id));

    let mut played = 0;
    while bracket.champion().is_none() {
        let next = bracket
            .flat()
            .find(|m| m.is_ready() && m.winner.is_none())
            .cloned()
            .expect("a playable match");
        bracket = api
            .report(&admin, tournament.id, &next, next.entrant_b.unwrap())
            .await;
        played += 1;
    }
    assert_eq!(played, 29);

    let concluded = api.set_status(&admin, tournament.id, "concluded").await;
    assert_eq!(concluded.winner.unwrap().id, bracket.champion().unwrap());
    assert_eq!(concluded.entrant_count, 30);

    let public: TournamentDetail = read_json(
        api.get(&format!("/api/tournaments/{}", tournament.id))
            .await,
    )
    .await;
    assert_eq!(public.bracket.unwrap(), bracket);
}

#[tokio::test]
async fn a_player_deleted_during_a_live_bracket_leaves_the_bracket_intact() {
    let api = TestApi::with_database().await;
    let admin = api.signup_admin("root").await;
    let (tournament, entrants) = live_tournament(&api, &admin, 4).await;
    let bracket = api.generate_bracket(&admin, tournament.id).await;
    let gone = entrants[0].player.as_ref().unwrap().id;

    let deleted = api
        .delete_as(&format!("/api/admin/players/{gone}"), &admin)
        .await;
    assert_eq!(deleted.status(), StatusCode::NO_CONTENT);

    let detail: TournamentDetail = read_json(
        api.get(&format!("/api/tournaments/{}", tournament.id))
            .await,
    )
    .await;
    assert_eq!(
        detail.bracket.unwrap(),
        bracket,
        "the matches still point at the entrant"
    );
    let orphan = detail
        .entrants
        .iter()
        .find(|e| e.id == entrants[0].id)
        .unwrap();
    assert!(orphan.player.is_none());

    // The orphan can still win: the bracket is about entrants, not accounts.
    let m = bracket.rounds[0]
        .iter()
        .find(|m| m.entrant_a == Some(entrants[0].id) || m.entrant_b == Some(entrants[0].id))
        .unwrap();
    let after = api.report(&admin, tournament.id, m, entrants[0].id).await;
    assert!(
        after.rounds[1]
            .iter()
            .any(|n| n.entrant_a == Some(entrants[0].id) || n.entrant_b == Some(entrants[0].id))
    );
}

#[tokio::test]
async fn a_regenerated_bracket_reseeds_every_entrant() {
    let api = TestApi::with_database().await;
    let admin = api.signup_admin("root").await;
    let (tournament, _) = live_tournament(&api, &admin, 7).await;

    for _ in 0..3 {
        api.generate_bracket(&admin, tournament.id).await;
        let detail: TournamentDetail = read_json(
            api.get_as(&format!("/api/admin/tournaments/{}", tournament.id), &admin)
                .await,
        )
        .await;
        let mut seeds: Vec<u32> = detail.entrants.iter().map(|e| e.seed.unwrap()).collect();
        seeds.sort_unstable();
        assert_eq!(seeds, [1, 2, 3, 4, 5, 6, 7]);
        let bracket = detail.bracket.unwrap();
        assert_eq!(
            bracket.rounds[0].iter().filter(|m| !m.is_ready()).count(),
            1
        );
    }
}

#[tokio::test]
async fn match_and_entrant_ids_are_scoped_to_their_tournament() {
    let api = TestApi::with_database().await;
    let admin = api.signup_admin("root").await;
    let (a, entrants_a) = live_tournament(&api, &admin, 2).await;
    let bracket_a = api.generate_bracket(&admin, a.id).await;
    let (b, _) = live_tournament(&api, &admin, 2).await;
    let final_a = &bracket_a.rounds[0][0];

    let stolen = api
        .put_as(
            &format!(
                "/api/admin/tournaments/{}/matches/{}/result",
                b.id, final_a.id
            ),
            &json!({ "winner": final_a.entrant_a }),
            &admin,
        )
        .await;
    assert_error(stolen, StatusCode::NOT_FOUND, "ItemNotFound").await;

    let removed = api
        .delete_as(
            &format!(
                "/api/admin/tournaments/{}/entrants/{}",
                b.id, entrants_a[0].id
            ),
            &admin,
        )
        .await;
    assert_eq!(
        removed.status(),
        StatusCode::NO_CONTENT,
        "nothing to remove in b"
    );
    let detail: TournamentDetail =
        read_json(api.get(&format!("/api/tournaments/{}", a.id)).await).await;
    assert_eq!(detail.entrants.len(), 2, "the entrant of a stays");
}

#[tokio::test]
async fn a_duplicated_seed_is_refused() {
    let api = TestApi::with_database().await;
    let admin = api.signup_admin("root").await;
    let (tournament, entrants) = live_tournament(&api, &admin, 2).await;
    api.generate_bracket(&admin, tournament.id).await;

    let response = api
        .put_as(
            &format!("/api/admin/tournaments/{}/seeds", tournament.id),
            &json!({ "entrants": [entrants[0].id, entrants[0].id] }),
            &admin,
        )
        .await;

    assert_error(
        response,
        StatusCode::UNPROCESSABLE_ENTITY,
        "UnprocessableRequest",
    )
    .await;
}

#[tokio::test]
async fn a_played_bracket_goes_with_its_tournament() {
    let api = TestApi::with_database().await;
    let admin = api.signup_admin("root").await;
    let (tournament, _) = live_tournament(&api, &admin, 3).await;
    let mut bracket = api.generate_bracket(&admin, tournament.id).await;
    while bracket.champion().is_none() {
        let next = bracket
            .flat()
            .find(|m| m.is_ready() && m.winner.is_none())
            .cloned()
            .unwrap();
        bracket = api
            .report(&admin, tournament.id, &next, next.entrant_a.unwrap())
            .await;
    }

    let deleted = api
        .delete_as(&format!("/api/admin/tournaments/{}", tournament.id), &admin)
        .await;
    assert_eq!(deleted.status(), StatusCode::NO_CONTENT);
    let gone = api
        .get(&format!("/api/tournaments/{}", tournament.id))
        .await;
    assert_error(gone, StatusCode::NOT_FOUND, "ItemNotFound").await;
}

#[tokio::test]
async fn every_admin_route_refuses_a_user() {
    let api = TestApi::with_database().await;
    let admin = api.signup_admin("root").await;
    let user = api.signup("dave").await.token;
    let (tournament, entrants) = live_tournament(&api, &admin, 2).await;
    let bracket = api.generate_bracket(&admin, tournament.id).await;
    let t = tournament.id;
    let m = bracket.rounds[0][0].id;

    let refused = [
        api.get_as("/api/admin/tournaments", &user).await,
        api.post_as("/api/admin/tournaments", &json!({}), &user)
            .await,
        api.get_as(&format!("/api/admin/tournaments/{t}"), &user)
            .await,
        api.patch_as(&format!("/api/admin/tournaments/{t}"), &json!({}), &user)
            .await,
        api.delete_as(&format!("/api/admin/tournaments/{t}"), &user)
            .await,
        api.post_as(
            &format!("/api/admin/tournaments/{t}/status"),
            &json!({ "status": "open" }),
            &user,
        )
        .await,
        api.post_as(
            &format!("/api/admin/tournaments/{t}/entrants"),
            &json!({ "playerId": entrants[0].id }),
            &user,
        )
        .await,
        api.delete_as(
            &format!("/api/admin/tournaments/{t}/entrants/{}", entrants[0].id),
            &user,
        )
        .await,
        api.delete_as(&format!("/api/admin/tournaments/{t}/bracket"), &user)
            .await,
        api.put_as(
            &format!("/api/admin/tournaments/{t}/seeds"),
            &json!({ "entrants": [] }),
            &user,
        )
        .await,
        api.put_as(
            &format!("/api/admin/tournaments/{t}/matches/{m}/result"),
            &json!({ "winner": entrants[0].id }),
            &user,
        )
        .await,
        api.delete_as(
            &format!("/api/admin/tournaments/{t}/matches/{m}/result"),
            &user,
        )
        .await,
    ];
    for response in refused {
        assert_error(response, StatusCode::FORBIDDEN, "Forbidden").await;
    }
}

#[tokio::test]
async fn the_bracket_is_seeded_by_level_and_neighbours_meet_in_round_one() {
    let api = TestApi::with_database().await;
    let admin = api.signup_admin("root").await;
    let created = api.create_tournament(&admin, HOUR).await;
    // Distinct levels, so the order has no random part. The unrated entrant
    // counts as a 3.
    let rookie = api
        .add_rated_entrant(&admin, created.id, "rookie", Some(1))
        .await;
    let menace = api
        .add_rated_entrant(&admin, created.id, "menace", Some(5))
        .await;
    let casual = api
        .add_rated_entrant(&admin, created.id, "casual", Some(2))
        .await;
    let unrated = api.add_entrant(&admin, created.id, "unrated").await;
    let sharp = api
        .add_rated_entrant(&admin, created.id, "sharp", Some(4))
        .await;
    api.set_status(&admin, created.id, "live").await;

    let bracket = api.generate_bracket(&admin, created.id).await;

    // Five entrants: the three top seeds get the byes, seeds 4 and 5 play.
    assert_eq!(bracket.rounds[0][0].winner, Some(menace.id));
    assert_eq!(bracket.rounds[0][1].winner, Some(sharp.id));
    assert_eq!(bracket.rounds[0][2].winner, Some(unrated.id));
    assert_eq!(bracket.rounds[0][3].entrant_a, Some(casual.id));
    assert_eq!(bracket.rounds[0][3].entrant_b, Some(rookie.id));

    let detail: TournamentDetail = read_json(
        api.get_as(&format!("/api/admin/tournaments/{}", created.id), &admin)
            .await,
    )
    .await;
    let seed_of = |id: EntrantId| detail.entrants.iter().find(|e| e.id == id).unwrap().seed;
    assert_eq!(seed_of(menace.id), Some(1));
    assert_eq!(seed_of(sharp.id), Some(2));
    assert_eq!(seed_of(unrated.id), Some(3));
    assert_eq!(seed_of(casual.id), Some(4));
    assert_eq!(seed_of(rookie.id), Some(5));
    let handles: Vec<&str> = detail
        .entrants
        .iter()
        .map(|e| e.player.as_ref().unwrap().handle.as_ref())
        .collect();
    assert_eq!(
        handles,
        ["menace", "sharp", "unrated", "casual", "rookie"],
        "the entrants come seeded first, in seed order"
    );
}

#[tokio::test]
async fn entrants_of_one_level_are_drawn_at_random() {
    let api = TestApi::with_database().await;
    let admin = api.signup_admin("root").await;
    let created = api.create_tournament(&admin, HOUR).await;
    for number in 0..8 {
        api.add_rated_entrant(&admin, created.id, &format!("even{number}"), Some(3))
            .await;
    }
    api.set_status(&admin, created.id, "live").await;

    let mut draws = std::collections::HashSet::new();
    for _ in 0..12 {
        let bracket = api.generate_bracket(&admin, created.id).await;
        let order: Vec<EntrantId> = bracket.rounds[0]
            .iter()
            .flat_map(|m| [m.entrant_a.unwrap(), m.entrant_b.unwrap()])
            .collect();
        draws.insert(order);
    }
    // Twelve draws of eight equal entrants: 40320 orders exist, so two runs
    // that agree every time would mean the shuffle is gone.
    assert!(draws.len() > 1, "every regeneration gave the same bracket");
}
