//! The public player routes against a real SQLite file.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod support;

use axum::http::StatusCode;
use bunker_models::{Paginated, Player};
use serde_json::Value;
use support::{TestApi, assert_error, read_json};

#[tokio::test]
async fn a_player_is_readable_by_handle_without_a_token() {
    let api = TestApi::with_database().await;
    let created = api.signup_player("dave").await;

    let response = api.get("/api/players/dave").await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(read_json::<Player>(response).await, created);
}

#[tokio::test]
async fn a_handle_lookup_ignores_case() {
    let api = TestApi::with_database().await;
    let created = api.signup_player("Fede_88").await;

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

/// Three signups in a row, so a text sort of the timestamps with a trimmed
/// fraction would show. The order must be chronological, not lexical.
#[tokio::test]
async fn the_roster_lists_every_player_newest_first() {
    let api = TestApi::with_database().await;
    let mut expected = Vec::new();
    for handle in ["dave", "ziopera", "ciccio"] {
        expected.push(api.signup_player(handle).await.id);
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
    let _created = api.signup_player("dave").await;

    let body: Value = read_json(api.get("/api/players/dave").await).await;

    let object = body.as_object().unwrap();
    let mut keys: Vec<&str> = object.keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(keys, ["createdAt", "glyph", "handle", "id", "role"]);
    assert_eq!(body["role"], "user");
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
        let _player = api.signup_player(&format!("player{index}")).await;
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
    let _player = api.signup_player("dave").await;

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
