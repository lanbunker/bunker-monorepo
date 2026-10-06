//! The public player routes against a real SQLite file.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod support;

use axum::http::StatusCode;
use bunker_models::{Account, CyclesLog, MatchLog, Paginated, Player};
use serde_json::Value;
use support::{TestApi, assert_error, read_json};

#[tokio::test]
async fn a_player_is_readable_by_handle_without_a_token() {
    let api = TestApi::with_database().await;
    let created = api.signup_active("dave").await;

    let response = api.get("/api/players/dave").await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(read_json::<Player>(response).await, created);
}

#[tokio::test]
async fn a_handle_lookup_ignores_case() {
    let api = TestApi::with_database().await;
    let created = api.signup_active("Fede_88").await;

    let response = api.get("/api/players/fede_88").await;

    assert_eq!(response.status(), StatusCode::OK);
    let player: Player = read_json(response).await;
    assert_eq!(player.id, created.id);
    assert_eq!(player.handle.as_ref(), "Fede_88");
}

#[tokio::test]
async fn an_unknown_handle_is_not_found() {
    let api = TestApi::with_database().await;

    let response = api.get("/api/players/nobody").await;

    assert_error(response, StatusCode::NOT_FOUND, "ItemNotFound").await;
}

/// Nobody has cycles, so every player shares the first place and the tie goes
/// to the newest signup. Three signups in a row, so a text sort of the
/// timestamps with a trimmed fraction would show.
#[tokio::test]
async fn the_leaderboard_breaks_a_shared_place_with_the_newest_signup() {
    let api = TestApi::with_database().await;
    let mut expected = Vec::new();
    for handle in ["dave", "ziopera", "ciccio"] {
        expected.push(api.signup_active(handle).await.id);
    }
    expected.reverse();

    let response = api.get("/api/players").await;

    assert_eq!(response.status(), StatusCode::OK);
    let page: Paginated<Player> = read_json(response).await;
    let players = page.items;
    assert_eq!(page.total, 3);
    let ids: Vec<_> = players.iter().map(|p| p.id).collect();
    assert_eq!(ids, expected);
    let mut stamps: Vec<_> = players.iter().map(|p| p.created_at).collect();
    stamps.sort_unstable_by(|a, b| b.cmp(a));
    assert_eq!(
        players.iter().map(|p| p.created_at).collect::<Vec<_>>(),
        stamps
    );
}

#[tokio::test]
async fn an_empty_roster_is_an_empty_list() {
    let api = TestApi::with_database().await;

    let response = api.get("/api/players").await;

    assert_eq!(response.status(), StatusCode::OK);
    let page: Paginated<Player> = read_json(response).await;
    assert!(page.items.is_empty());
    assert_eq!(page.total, 0);
    assert_eq!(page.total_pages, 0);
}

/// The wire shape is the contract the Astro site reads. Field names are camelCase
/// and the glyph carries the hex color.
#[tokio::test]
async fn a_player_has_the_documented_wire_shape() {
    let api = TestApi::with_database().await;
    let _created = api.signup_active("dave").await;

    let body: Value = read_json(api.get("/api/players/dave").await).await;

    let object = body.as_object().unwrap();
    let mut keys: Vec<&str> = object.keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        [
            "active",
            "createdAt",
            "glyph",
            "handle",
            "id",
            "role",
            "standing"
        ]
    );
    assert_eq!(body["role"], "user");
    assert_eq!(body["active"], true);
    assert_eq!(body["standing"]["cycles"], 0);
    assert_eq!(body["standing"]["rank"], "zombie");
    assert_eq!(body["standing"]["place"], 1);
    assert_eq!(body["standing"]["players"], 1);
    assert_eq!(body["standing"]["next"]["rank"], "guest");
    assert_eq!(body["standing"]["next"]["floor"], 80);
    assert_eq!(body["glyph"]["color"], "#ff6b57");
    assert_eq!(body["glyph"]["bits"], 4_554_623);
    assert!(body["createdAt"].as_str().unwrap().ends_with('Z'));
}

#[tokio::test]
async fn readiness_passes_with_a_migrated_database() {
    let api = TestApi::with_database().await;

    let response = api.get("/health/ready").await;

    assert_eq!(response.status(), StatusCode::OK);
}

/// The window arithmetic end to end. Consecutive pages are disjoint, ordered, and
/// the total ignores the window.
#[tokio::test]
async fn consecutive_pages_are_disjoint_and_report_the_total() {
    let api = TestApi::with_database().await;
    for index in 0..5_u8 {
        let _player = api.signup_active(&format!("player{index}")).await;
    }

    let first: Paginated<Player> = read_json(api.get("/api/players?page=1&pageSize=2").await).await;
    let second: Paginated<Player> =
        read_json(api.get("/api/players?page=2&pageSize=2").await).await;
    let third: Paginated<Player> = read_json(api.get("/api/players?page=3&pageSize=2").await).await;

    assert_eq!(
        (first.items.len(), second.items.len(), third.items.len()),
        (2, 2, 1)
    );
    assert_eq!((first.total, first.total_pages), (5, 3));
    assert_eq!(first.page.into_inner(), 1);
    assert_eq!(first.page_size.into_inner(), 2);

    let all: Vec<_> = [&first.items, &second.items, &third.items]
        .into_iter()
        .flatten()
        .map(|p| p.handle.as_ref().to_owned())
        .collect();
    let unique: std::collections::BTreeSet<_> = all.iter().collect();
    assert_eq!(unique.len(), 5, "pages overlapped: {all:?}");
    assert_eq!(all, ["player4", "player3", "player2", "player1", "player0"]);
}

#[tokio::test]
async fn a_page_past_the_end_is_empty_but_keeps_the_total() {
    let api = TestApi::with_database().await;
    let _player = api.signup_active("dave").await;

    let page: Paginated<Player> = read_json(api.get("/api/players?page=50").await).await;

    assert!(page.items.is_empty());
    assert_eq!(page.total, 1);
    assert_eq!(page.total_pages, 1);
}

#[tokio::test]
async fn the_default_window_is_page_one_of_twenty() {
    let api = TestApi::with_database().await;

    let page: Paginated<Player> = read_json(api.get("/api/players").await).await;

    assert_eq!(page.page.into_inner(), 1);
    assert_eq!(page.page_size.into_inner(), 20);
}

/// The search is a substring match without case. `total` follows the filter,
/// so a pager over a search counts the matches and not the roster.
#[tokio::test]
async fn the_roster_search_matches_part_of_a_handle_without_case() {
    let api = TestApi::with_database().await;
    for handle in ["dave", "BigDave", "ziopera"] {
        api.signup_active(handle).await;
    }

    let response = api.get("/api/players?q=DAV").await;

    assert_eq!(response.status(), StatusCode::OK);
    let page: Paginated<Player> = read_json(response).await;
    let mut handles: Vec<&str> = page.items.iter().map(|p| p.handle.as_ref()).collect();
    handles.sort_unstable();
    assert_eq!(handles, ["BigDave", "dave"]);
    assert_eq!(page.total, 2);
    assert_eq!(page.total_pages, 1);
}

/// `_` is a handle character. A `like` match would read it as any character
/// and return `ziopera` for `z_o`.
#[tokio::test]
async fn an_underscore_in_a_search_term_is_a_character() {
    let api = TestApi::with_database().await;
    for handle in ["z_opera", "ziopera"] {
        api.signup_active(handle).await;
    }

    let response = api.get("/api/players?q=z_o").await;

    assert_eq!(response.status(), StatusCode::OK);
    let page: Paginated<Player> = read_json(response).await;
    let handles: Vec<&str> = page.items.iter().map(|p| p.handle.as_ref()).collect();
    assert_eq!(handles, ["z_opera"]);
}

#[tokio::test]
async fn a_search_with_no_match_is_an_empty_page() {
    let api = TestApi::with_database().await;
    api.signup_active("dave").await;

    let response = api.get("/api/players?q=nobody").await;

    assert_eq!(response.status(), StatusCode::OK);
    let page: Paginated<Player> = read_json(response).await;
    assert!(page.items.is_empty());
    assert_eq!(page.total, 0);
    assert_eq!(page.total_pages, 0);
}

#[tokio::test]
async fn a_player_who_never_checked_in_has_no_public_trace() {
    let api = TestApi::with_database().await;
    api.signup_active("erin").await;
    let token = api.signup("dave").await.token;

    for path in [
        "/api/players/dave",
        "/api/players/dave/cycles",
        "/api/players/dave/matches",
    ] {
        assert_error(api.get(path).await, StatusCode::NOT_FOUND, "ItemNotFound").await;
    }
    let board: Paginated<Player> = read_json(api.get("/api/players").await).await;
    let handles: Vec<&str> = board.items.iter().map(|p| p.handle.as_ref()).collect();
    assert_eq!(handles, ["erin"]);
    assert_eq!(board.total, 1);
    assert_eq!(board.items[0].standing.players, 1, "dave is not counted");

    let me: Account = read_json(api.get_as("/api/me", &token).await).await;
    assert!(!me.player.active);
    assert_eq!(me.player.standing.place, None);
    assert_eq!(me.player.standing.players, 1);
}

#[tokio::test]
async fn an_inactive_player_with_cycles_moves_nobody_on_the_board() {
    let api = TestApi::with_database().await;
    let admin = api.signup_admin("root").await;
    let erin = api.signup_active("erin").await;
    let dave = api.signup_player("dave").await;
    for (id, amount) in [(erin.id, 50), (dave.id, 500)] {
        let response = api
            .post_as(
                &format!("/api/admin/players/{id}/cycles"),
                &serde_json::json!({ "amount": amount, "note": "x" }),
                &admin,
            )
            .await;
        assert_eq!(response.status(), StatusCode::CREATED);
    }

    let erin = api.player("erin").await;
    assert_eq!(
        erin.standing.place,
        Some(1),
        "dave has more, but is not on the board"
    );
    assert_eq!(erin.standing.players, 2, "erin and the admin");
    let dave = api.lookup(&admin, "dave").await;
    assert_eq!(dave.standing.cycles, 500);
    assert_eq!(dave.standing.place, None);
}

#[tokio::test]
async fn the_public_board_refuses_the_backoffice_filter() {
    let api = TestApi::with_database().await;

    let refused = api.get("/api/players?active=false").await;
    assert_eq!(refused.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn an_inactive_player_reads_their_own_logs() {
    let api = TestApi::with_database().await;
    let admin = api.signup_admin("root").await;
    let token = api.signup("dave").await.token;
    let dave = api.lookup(&admin, "dave").await;
    let adjusted = api
        .post_as(
            &format!("/api/admin/players/{}/cycles", dave.id),
            &serde_json::json!({ "amount": 40, "note": "early bird" }),
            &admin,
        )
        .await;
    assert_eq!(adjusted.status(), StatusCode::CREATED);

    let cycles: CyclesLog = read_json(api.get_as("/api/me/cycles", &token).await).await;
    assert_eq!(cycles.entries.total, 1);
    assert_eq!(cycles.entries.items[0].amount, 40);
    let matches: MatchLog = read_json(api.get_as("/api/me/matches", &token).await).await;
    assert_eq!(matches.matches.total, 0);

    let hidden = api.get("/api/players/dave/cycles").await;
    assert_error(hidden, StatusCode::NOT_FOUND, "ItemNotFound").await;
    let anonymous = api.get("/api/me/cycles").await;
    assert_error(anonymous, StatusCode::UNAUTHORIZED, "Unauthorized").await;
}
