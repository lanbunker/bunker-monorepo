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

/// `http_contract_test` walks every documented route, so this test only proves
/// that the document is served, and is the one in the code.
#[tokio::test]
async fn the_document_is_served() {
    let api = TestApi::without_database().await;

    let response = api.get("/api/openapi.json").await;

    assert_eq!(response.status(), StatusCode::OK);
    let served: Value = read_json(response).await;
    assert_eq!(served, serde_json::to_value(ApiDoc::openapi()).unwrap());
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

    assert_eq!(
        fields,
        ["createdAt", "glyph", "handle", "id", "role", "standing"]
    );
    assert_eq!(
        document["components"]["schemas"]["Rank"]["enum"],
        serde_json::json!(["zombie", "guest", "user", "sudoer", "daemon", "kernel"])
    );
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

#[test]
fn the_event_schema_has_the_documented_fields_and_hides_the_code() {
    let document: Value = serde_json::to_value(ApiDoc::openapi()).unwrap();
    let event = &document["components"]["schemas"]["Event"];
    let mut fields: Vec<&str> = event["properties"]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    fields.sort_unstable();

    assert_eq!(
        fields,
        [
            "checkinCount",
            "createdAt",
            "description",
            "endsAt",
            "games",
            "id",
            "image",
            "location",
            "name",
            "startsAt",
            "status",
        ]
    );
    assert_eq!(
        document["components"]["schemas"]["EventStatus"]["enum"],
        serde_json::json!(["draft", "published"])
    );
    assert_eq!(
        document["components"]["schemas"]["CheckinWindow"]["enum"],
        serde_json::json!(["early", "open", "over"])
    );
    assert_eq!(
        document["components"]["schemas"]["PointKind"]["enum"][0],
        serde_json::json!("checkin")
    );
}
