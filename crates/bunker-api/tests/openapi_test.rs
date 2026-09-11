//! `openapi.json` is the contract the site generates its types from. It is
//! written by `make openapi`, and this test fails when a route or a model changed
//! and nobody ran it.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod support;

use axum::http::StatusCode;
use bunker_api::routers::ApiDoc;
use serde_json::Value;
use support::{TestApi, read_json};
use utoipa::OpenApi as _;

const COMMITTED: &str = include_str!("../openapi.json");

#[test]
fn the_committed_document_matches_the_code() {
    let committed: Value = serde_json::from_str(COMMITTED).unwrap();
    let generated: Value = serde_json::to_value(ApiDoc::openapi()).unwrap();

    assert_eq!(
        committed, generated,
        "crates/bunker-api/openapi.json is stale. Run `make api-types` and commit the result."
    );
}

#[tokio::test]
async fn the_document_is_served_and_lists_every_route() {
    let api = TestApi::without_database().await;

    let response = api.get("/api/openapi.json").await;

    assert_eq!(response.status(), StatusCode::OK);
    let document: Value = read_json(response).await;
    let paths = document["paths"].as_object().unwrap();
    for path in [
        "/api/auth/signup",
        "/api/auth/login",
        "/api/me",
        "/api/me/password",
        "/api/admin/players/{id}/password-reset",
        "/api/players",
        "/api/players/{handle}",
        "/api/admin/players",
        "/api/admin/players/{id}",
        "/api/tournaments",
        "/api/tournaments/{id}",
        "/api/tournaments/{id}/registration",
        "/api/admin/tournaments",
        "/api/admin/tournaments/{id}",
        "/api/admin/tournaments/{id}/status",
        "/api/admin/tournaments/{id}/entrants",
        "/api/admin/tournaments/{id}/entrants/{entrantId}",
        "/api/admin/tournaments/{id}/bracket",
        "/api/admin/tournaments/{id}/seeds",
        "/api/admin/tournaments/{id}/matches/{matchId}/result",
        "/health/live",
        "/health/ready",
    ] {
        assert!(paths.contains_key(path), "{path} is not documented");
    }
}

/// The site trusts these names. A rename here is a breaking change for it.
#[test]
fn the_player_schema_has_the_documented_fields() {
    let document: Value = serde_json::to_value(ApiDoc::openapi()).unwrap();
    let player = &document["components"]["schemas"]["Player"];
    let mut fields: Vec<&str> = player["properties"]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    fields.sort_unstable();

    assert_eq!(fields, ["createdAt", "glyph", "handle", "id", "role"]);
    assert_eq!(
        document["components"]["schemas"]["Role"]["enum"],
        serde_json::json!(["user", "admin"])
    );
}

#[test]
fn the_tournament_schema_has_the_documented_fields() {
    let document: Value = serde_json::to_value(ApiDoc::openapi()).unwrap();
    let tournament = &document["components"]["schemas"]["Tournament"];
    let mut fields: Vec<&str> = tournament["properties"]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    fields.sort_unstable();

    assert_eq!(
        fields,
        [
            "createdAt",
            "date",
            "description",
            "entrantCount",
            "game",
            "hasBracket",
            "id",
            "mode",
            "name",
            "registrationClosesAt",
            "status",
            "winner",
        ]
    );
    assert_eq!(
        document["components"]["schemas"]["TournamentStatus"]["enum"],
        serde_json::json!(["draft", "open", "live", "concluded"])
    );
}
