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
use axum::http::{Method, Request, StatusCode, header};
use bunker_api::config::AppEnv;
use bunker_api::routers::ApiDoc;
use http_body_util::BodyExt as _;
use serde_json::{Value, json};
use support::{PASSWORD, TestApi, assert_error, read_json};
use utoipa::OpenApi as _;

/// The two routes a player with a temporary password can reach: they are how
/// the player finishes the change.
const PENDING_PASSWORD_ROUTES: [(&str, &str); 2] =
    [("GET", "/api/me"), ("POST", "/api/me/password")];

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
async fn a_match_log_path_that_is_not_a_handle_is_rejected() {
    let response = TestApi::without_database()
        .await
        .get("/api/players/not%20a%20handle!/matches")
        .await;

    assert_error(response, StatusCode::BAD_REQUEST, "InvalidRequest").await;
}

#[tokio::test]
async fn page_zero_is_rejected() {
    let response = TestApi::without_database()
        .await
        .get("/api/players?page=0")
        .await;

    assert_error(response, StatusCode::BAD_REQUEST, "InvalidRequest").await;
}

#[tokio::test]
async fn a_page_size_past_the_maximum_is_rejected() {
    let response = TestApi::without_database()
        .await
        .get("/api/players?pageSize=101")
        .await;

    assert_error(response, StatusCode::BAD_REQUEST, "InvalidRequest").await;
}

#[tokio::test]
async fn an_empty_search_term_is_rejected() {
    let response = TestApi::without_database()
        .await
        .get("/api/players?q=%20")
        .await;

    assert_error(response, StatusCode::BAD_REQUEST, "InvalidRequest").await;
}

#[tokio::test]
async fn a_search_term_longer_than_a_handle_is_rejected() {
    let response = TestApi::without_database()
        .await
        .get("/api/players?q=abcdefghijklmnopqrstu")
        .await;

    assert_error(response, StatusCode::BAD_REQUEST, "InvalidRequest").await;
}

/// A server that accepts the key and ignores it answers page 1 of 20, and the
/// answer looks correct.
#[tokio::test]
async fn a_misspelled_query_key_is_rejected_rather_than_ignored() {
    let response = TestApi::without_database()
        .await
        .get("/api/players?page_size=50")
        .await;

    assert_error(response, StatusCode::BAD_REQUEST, "InvalidRequest").await;
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

/// The log refuses such an id, so the response must not echo it either: the id
/// a caller reads back is the one the log lines carry.
#[tokio::test]
async fn a_caller_request_id_the_log_refuses_is_replaced() {
    let api = TestApi::without_database().await;
    let refused = "has spaces and is far too long to be a request id in any log line at all";

    let response = api
        .send(
            Request::get("/nope")
                .header("x-request-id", refused)
                .body(Body::empty())
                .unwrap(),
        )
        .await;

    let echoed = response
        .headers()
        .get("x-request-id")
        .unwrap()
        .to_str()
        .unwrap();
    assert_ne!(echoed, refused);
    assert!(
        uuid::Uuid::parse_str(echoed).is_ok(),
        "a fresh id replaces the refused one: {echoed}"
    );
}

#[tokio::test]
async fn liveness_needs_no_database_and_reports_the_version_and_the_commit() {
    let response = TestApi::without_database().await.get("/health/live").await;

    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = read_json(response).await;
    assert_eq!(body["status"], json!("ok"));
    assert_eq!(body["version"], json!(env!("CARGO_PKG_VERSION")));
    assert_eq!(
        body["commit"],
        json!(option_env!("BUNKER_GIT_SHA").unwrap_or("dev"))
    );
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

#[tokio::test]
async fn a_tournament_id_that_is_not_a_uuid_is_rejected() {
    let response = TestApi::without_database()
        .await
        .get("/api/tournaments/not-a-uuid")
        .await;

    assert_error(response, StatusCode::BAD_REQUEST, "InvalidRequest").await;
}

#[tokio::test]
async fn a_status_filter_on_the_public_tournament_list_is_rejected() {
    let response = TestApi::without_database()
        .await
        .get("/api/tournaments?status=open")
        .await;

    assert_error(response, StatusCode::BAD_REQUEST, "InvalidRequest").await;
}

#[tokio::test]
async fn a_malformed_checkin_code_is_rejected_before_the_handler() {
    let api = TestApi::without_database().await;

    let response = api.get("/api/checkin/NOT-A-CODE").await;

    assert_error(response, StatusCode::BAD_REQUEST, "InvalidRequest").await;
}

/// Walks the documented contract. Every path and method is mounted, every
/// route with `security` refuses a request without a token, every admin route
/// refuses a user, and every bearer route but the two of a password change
/// refuses a temporary password.
#[tokio::test]
async fn every_documented_route_is_mounted_and_guarded() {
    let api = TestApi::with_database().await;
    let admin = api.signup_admin("root").await;
    let user = api.signup("dave").await.token;
    let erin = api.signup_player("erin").await;
    let (pending, _) = api.forced_session(&admin, erin.id, "erin").await;
    let document = serde_json::to_value(ApiDoc::openapi()).unwrap();
    let paths = document["paths"].as_object().unwrap();
    assert!(paths.len() > 30, "the walk found {} paths", paths.len());

    for (template, item) in paths {
        let path = concrete(template);
        for (method, operation) in item.as_object().unwrap() {
            let Ok(method) = Method::from_bytes(method.to_uppercase().as_bytes()) else {
                continue;
            };
            let route = format!("{method} {template}");

            let (status, body) = call(&api, &method, &path, None).await;
            assert!(
                !matches!(
                    body["code"].as_str(),
                    Some("RouteNotFound" | "MethodNotAllowed")
                ),
                "{route} is documented but not mounted: {status} {body}"
            );

            if operation.get("security").is_some() {
                assert_eq!(
                    status,
                    StatusCode::UNAUTHORIZED,
                    "{route} without a token: {body}"
                );
                assert_eq!(body["code"], "Unauthorized", "{route}");

                let reachable =
                    PENDING_PASSWORD_ROUTES.contains(&(method.as_str(), template.as_str()));
                if !reachable {
                    let (status, body) = call(&api, &method, &path, Some(&pending)).await;
                    assert_eq!(
                        status,
                        StatusCode::FORBIDDEN,
                        "{route} with a temporary password: {body}"
                    );
                    assert_eq!(body["code"], "PasswordChangeRequired", "{route}");
                }
            }

            if template.starts_with("/api/admin") {
                let (status, body) = call(&api, &method, &path, Some(&user)).await;
                assert_eq!(status, StatusCode::FORBIDDEN, "{route} as a user: {body}");
                assert_eq!(body["code"], "Forbidden", "{route}");
            }
        }
    }
}

/// A template with each parameter filled with a value of the right shape.
fn concrete(template: &str) -> String {
    let id = uuid::Uuid::new_v4().to_string();
    template
        .replace("{id}", &id)
        .replace("{entrantId}", &id)
        .replace("{matchId}", &id)
        .replace("{handle}", "dave")
        .replace("{code}", "abcdefghij12")
}

/// Sends an empty JSON object and reads the answer as JSON, or as `null` for a
/// body that is not JSON.
async fn call(
    api: &TestApi,
    method: &Method,
    path: &str,
    token: Option<&str>,
) -> (StatusCode, Value) {
    let mut request = Request::builder()
        .method(method.clone())
        .uri(path)
        .header(header::CONTENT_TYPE, "application/json");
    if let Some(token) = token {
        request = request.header(header::AUTHORIZATION, format!("Bearer {token}"));
    }
    let response = api.send(request.body(Body::from("{}")).unwrap()).await;
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();

    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}
