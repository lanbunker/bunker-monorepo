//! What a client is told after a failure.
//!
//! The other tests assert statuses and codes. This file asserts the `message` and
//! the `cause` fields, because an internal detail leaks through those two. A
//! driver message names tables and quotes row values.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod support;

use axum::response::IntoResponse as _;
use bunker_api::internal::http::ApiError;
use bunker_api::services::{ErrorCode, ServiceError};
use bunker_api::storage::StorageError;
use bunker_models::{Handle, HandleError};
use serde_json::{Value, json};
use support::{HOUR, TestApi, cause_chain, read_json};

/// The generic message is part of the contract. This test writes it out, and does
/// not import it, so a change to the constant fails the test.
const GENERIC: &str = "An unexpected error occurred";

#[tokio::test]
async fn an_internal_failure_never_describes_itself_to_the_client() {
    let error: ApiError = ServiceError::Storage(malformed_row()).into();

    let body: Value = read_json(error.into_response()).await;

    assert_eq!(body["code"], json!("GenericError"));
    assert_eq!(body["message"], json!(GENERIC));
    let rendered = body.to_string();
    assert!(
        !rendered.contains("players"),
        "the response named a database table: {rendered}"
    );
    assert!(
        !rendered.contains("domain model"),
        "the response quoted the storage error: {rendered}"
    );
}

/// `IntoResponse` renders no cause. Only the middleware adds a cause, and only if
/// the configuration permits it. The basic render must therefore be safe.
#[tokio::test]
async fn a_bare_rendering_carries_no_cause() {
    let error: ApiError = ServiceError::Unavailable(StorageError::connection(unreachable())).into();

    let body: Value = read_json(error.into_response()).await;

    assert_eq!(body["code"], json!("ServiceUnavailable"));
    assert_eq!(
        body["message"],
        json!("The service is temporarily unavailable")
    );
    assert!(body.get("cause").is_none(), "leaked a cause: {body}");
}

/// The message for a failure of the client is a fixed sentence. The internal
/// text quotes the handle, and it goes to the cause chain and the log.
#[tokio::test]
async fn a_client_failure_is_described_plainly() {
    let handle = Handle::try_new("nobody").unwrap();
    let error: ApiError = ServiceError::PlayerNotFound(handle).into();

    let body: Value = read_json(error.into_response()).await;

    assert_eq!(body["code"], json!("ItemNotFound"));
    assert_eq!(body["message"], json!("The player was not found"));
}

/// A token failure carries one fixed message. The library error says which check
/// failed, and that text stays in the logs.
#[tokio::test]
async fn a_token_failure_hides_which_check_failed() {
    let error: ApiError =
        ServiceError::invalid_token(std::io::Error::other("ExpiredSignature")).into();

    let body: Value = read_json(error.into_response()).await;

    assert_eq!(body["code"], json!("Unauthorized"));
    assert!(!body["message"].as_str().unwrap().contains("Expired"));
}

/// The client gets a generic message, so the log is the only record of the
/// failure. `#[error(transparent)]` on `Storage` removes the storage message from
/// the chain.
#[test]
fn an_internal_failure_names_the_failing_layer_in_its_cause_chain() {
    let reported = cause_chain(&ServiceError::Storage(malformed_row()));

    assert!(
        reported.contains("a row in `players` does not satisfy the domain model"),
        "the chain skipped the storage error: {reported}"
    );
    assert!(
        reported.contains("Handle"),
        "the chain skipped the original cause: {reported}"
    );
}

#[tokio::test]
async fn the_status_in_the_body_matches_the_status_of_the_response() {
    let error = ApiError::new(ErrorCode::HandleTaken, "taken");
    let status = error.status();

    let response = error.into_response();
    assert_eq!(response.status(), status);

    let body: Value = read_json(response).await;
    assert_eq!(body["status"], json!(status.as_u16()));
}

fn malformed_row() -> StorageError {
    StorageError::malformed_row("players", HandleError::LenCharMinViolated)
}

fn unreachable() -> std::io::Error {
    std::io::Error::other("unable to open database file")
}

/// A not found names what is missing in a sentence, and never the id or the
/// handle the client sent: the client has those already, and the log has the
/// internal text.
#[tokio::test]
async fn every_not_found_is_a_sentence_without_the_key() {
    let api = TestApi::with_database().await;
    let admin = api.signup_admin("root").await;
    let missing = uuid::Uuid::new_v4().to_string();
    let live = api.create_tournament(&admin, HOUR).await;
    api.add_entrant(&admin, live.id, "dave").await;
    api.add_entrant(&admin, live.id, "erin").await;
    api.set_status(&admin, live.id, "live").await;
    api.generate_bracket(&admin, live.id).await;

    let answers = [
        api.get("/api/players/nobody").await,
        api.get("/api/players/nobody/cycles").await,
        api.get(&format!("/api/tournaments/{missing}")).await,
        api.get("/api/checkin/abcdefghij12").await,
        api.get_as(&format!("/api/admin/events/{missing}"), &admin)
            .await,
        api.patch_as(
            &format!("/api/admin/players/{missing}"),
            &json!({ "role": "admin" }),
            &admin,
        )
        .await,
        api.put_as(
            &format!(
                "/api/admin/tournaments/{}/matches/{missing}/result",
                live.id
            ),
            &json!({ "winner": missing }),
            &admin,
        )
        .await,
    ];
    for response in answers {
        let status = response.status();
        let body: Value = read_json(response).await;
        let message = body["message"].as_str().unwrap();
        assert_eq!(status, axum::http::StatusCode::NOT_FOUND, "{body}");
        assert_eq!(body["code"], json!("ItemNotFound"));
        assert!(message.starts_with(char::is_uppercase), "{message:?}");
        assert!(
            !message.contains(&missing) && !message.contains("nobody"),
            "{message:?}"
        );
    }
}
