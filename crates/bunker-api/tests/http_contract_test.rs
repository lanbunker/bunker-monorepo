//! The HTTP boundary, with no database. The server rejects or answers each case
//! here before a handler reaches SQLite.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod support;

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use bunker_api::config::AppEnv;
use serde_json::{Value, json};
use support::{PASSWORD, TestApi, assert_error, read_json};

#[tokio::test]
async fn an_unmounted_route_answers_route_not_found() {
    let response = TestApi::without_database().await.get("/nope").await;

    assert_error(response, StatusCode::NOT_FOUND, "RouteNotFound").await;
}

/// By default, axum answers a method that does not match with an empty body. That
/// is the one failure that escapes the error contract.
#[tokio::test]
async fn a_wrong_method_answers_with_the_error_body() {
    let api = TestApi::without_database().await;

    let response = api
        .send(Request::put("/api/auth/login").body(Body::empty()).unwrap())
        .await;

    assert_error(response, StatusCode::METHOD_NOT_ALLOWED, "MethodNotAllowed").await;
}

#[tokio::test]
async fn a_short_handle_is_rejected_before_the_handler() {
    let response = TestApi::without_database()
        .await
        .post(
            "/api/auth/signup",
            &json!({ "handle": "ab", "password": PASSWORD }),
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
async fn a_short_password_is_rejected_before_the_handler() {
    let response = TestApi::without_database()
        .await
        .post(
            "/api/auth/signup",
            &json!({ "handle": "dave", "password": "short" }),
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
async fn a_missing_field_is_rejected() {
    let response = TestApi::without_database()
        .await
        .post("/api/auth/signup", &json!({ "handle": "dave" }))
        .await;

    assert_error(
        response,
        StatusCode::UNPROCESSABLE_ENTITY,
        "UnprocessableRequest",
    )
    .await;
}

#[tokio::test]
async fn an_unknown_body_field_is_rejected() {
    let response = TestApi::without_database()
        .await
        .post(
            "/api/auth/signup",
            &json!({ "handle": "dave", "password": PASSWORD, "role": "admin" }),
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
async fn a_body_that_is_not_json_is_rejected() {
    let api = TestApi::without_database().await;

    let response = api
        .send(
            Request::post("/api/auth/signup")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from("{"))
                .unwrap(),
        )
        .await;

    assert_error(response, StatusCode::BAD_REQUEST, "InvalidRequest").await;
}

#[tokio::test]
async fn a_body_without_a_json_content_type_is_unsupported_media_type() {
    let api = TestApi::without_database().await;

    let response = api
        .send(
            Request::post("/api/auth/signup")
                .body(Body::from(
                    r#"{"handle":"dave","password":"correct-horse-battery"}"#,
                ))
                .unwrap(),
        )
        .await;

    assert_error(
        response,
        StatusCode::UNSUPPORTED_MEDIA_TYPE,
        "UnsupportedMediaType",
    )
    .await;
}

#[tokio::test]
async fn a_path_that_is_not_a_handle_is_rejected() {
    let response = TestApi::without_database()
        .await
        .get("/api/players/not%20a%20handle!")
        .await;

    assert_error(response, StatusCode::BAD_REQUEST, "InvalidRequest").await;
}

#[tokio::test]
async fn a_protected_route_without_a_token_is_unauthorized() {
    let response = TestApi::without_database().await.get("/api/me").await;

    assert_error(response, StatusCode::UNAUTHORIZED, "Unauthorized").await;
}

#[tokio::test]
async fn a_protected_route_with_a_malformed_header_is_unauthorized() {
    let api = TestApi::without_database().await;

    let response = api
        .send(
            Request::get("/api/me")
                .header(header::AUTHORIZATION, "Basic abc")
                .body(Body::empty())
                .unwrap(),
        )
        .await;

    assert_error(response, StatusCode::UNAUTHORIZED, "Unauthorized").await;
}

#[tokio::test]
async fn a_bearer_header_with_an_empty_token_is_unauthorized() {
    let api = TestApi::without_database().await;

    let response = api
        .send(
            Request::get("/api/me")
                .header(header::AUTHORIZATION, "Bearer ")
                .body(Body::empty())
                .unwrap(),
        )
        .await;

    assert_error(response, StatusCode::UNAUTHORIZED, "Unauthorized").await;
}

#[tokio::test]
async fn a_protected_route_with_a_garbage_token_is_unauthorized() {
    let response = TestApi::without_database()
        .await
        .get_as("/api/me", "not.a.jwt")
        .await;

    assert_error(response, StatusCode::UNAUTHORIZED, "Unauthorized").await;
}

#[tokio::test]
async fn the_request_id_is_returned_to_the_caller() {
    let response = TestApi::without_database().await.get("/nope").await;

    assert!(
        response.headers().contains_key("x-request-id"),
        "responses must carry the id their logs are tagged with"
    );
}

#[tokio::test]
async fn a_caller_supplied_request_id_is_kept() {
    let api = TestApi::without_database().await;

    let response = api
        .send(
            Request::get("/nope")
                .header("x-request-id", "from-the-caller")
                .body(Body::empty())
                .unwrap(),
        )
        .await;

    assert_eq!(
        response.headers().get("x-request-id").unwrap(),
        "from-the-caller"
    );
}

#[tokio::test]
async fn liveness_needs_no_database_and_reports_the_version() {
    let response = TestApi::without_database().await.get("/health/live").await;

    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = read_json(response).await;
    assert_eq!(body["status"], json!("ok"));
    assert_eq!(body["version"], json!(env!("CARGO_PKG_VERSION")));
}

/// A connection opens only at the first query. Without this route, a wrong
/// `DATABASE_URL` gives a process that looks correct and fails each request.
#[tokio::test]
async fn readiness_fails_when_the_database_is_unreachable() {
    let response = TestApi::without_database().await.get("/health/ready").await;

    assert_error(
        response,
        StatusCode::SERVICE_UNAVAILABLE,
        "ServiceUnavailable",
    )
    .await;
}

/// A database outage is temporary. The correct status lets a client retry, and
/// does not tell the client that its request is bad.
#[tokio::test]
async fn an_unreachable_database_is_service_unavailable() {
    let response = TestApi::without_database().await.get("/api/players").await;

    assert_error(
        response,
        StatusCode::SERVICE_UNAVAILABLE,
        "ServiceUnavailable",
    )
    .await;
}

/// The part of the error contract that concerns security. A deployed environment
/// must not give a client the chain of causes, which names files and tables.
#[tokio::test]
async fn a_deployed_environment_hides_the_cause_chain() {
    let response = TestApi::without_database_as(AppEnv::Prod)
        .await
        .get("/api/players")
        .await;

    let body: Value = read_json(response).await;

    assert_eq!(body["code"], json!("ServiceUnavailable"));
    assert!(
        body.get("cause").is_none(),
        "production leaked a cause chain: {body}"
    );
}

#[tokio::test]
async fn a_local_environment_reports_the_cause_chain() {
    let response = TestApi::without_database_as(AppEnv::Local)
        .await
        .get("/api/players")
        .await;

    let body: Value = read_json(response).await;

    assert!(
        body.get("cause").is_some(),
        "local development needs the cause to debug with: {body}"
    );
}
