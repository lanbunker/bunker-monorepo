//! Signup, login and the bearer token, against a real SQLite file. Each test uses
//! HTTP, so the full chain runs: extractor, router, service, storage, migrations.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod support;

use std::time::Duration;

use axum::http::StatusCode;
use bunker_api::server::Secrets;
use bunker_api::services::TokenIssuer;
use bunker_models::{Player, PlayerId, TokenResponse, generate_glyph};
use serde_json::json;
use support::{PASSWORD, TestApi, assert_error, read_json};
use time::OffsetDateTime;

#[tokio::test]
async fn signup_returns_a_token_that_opens_the_profile() {
    let api = TestApi::with_database().await;

    let response = api
        .post(
            "/api/auth/signup",
            &json!({ "handle": "dave", "password": PASSWORD }),
        )
        .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let token: TokenResponse = read_json(response).await;
    assert!(token.expires_at > OffsetDateTime::now_utc());

    let me = api.get_as("/api/me", &token.token).await;
    assert_eq!(me.status(), StatusCode::OK);
    let player: Player = read_json(me).await;
    assert_eq!(player.handle.as_ref(), "dave");
}

/// The glyph is generated from the handle and stored. It must be the same mark the
/// web renderer draws for the same handle.
#[tokio::test]
async fn signup_stores_the_generated_glyph() {
    let api = TestApi::with_database().await;

    let player = api.signup_player("dave").await;

    assert_eq!(player.glyph, generate_glyph("dave"));
}

#[tokio::test]
async fn signup_keeps_the_case_of_the_handle() {
    let api = TestApi::with_database().await;

    let player = api.signup_player("Fede_88").await;

    assert_eq!(player.handle.as_ref(), "Fede_88");
}

#[tokio::test]
async fn a_taken_handle_conflicts() {
    let api = TestApi::with_database().await;
    let _first = api.signup("dave").await;

    let response = api
        .post(
            "/api/auth/signup",
            &json!({ "handle": "dave", "password": PASSWORD }),
        )
        .await;

    assert_error(response, StatusCode::CONFLICT, "HandleTaken").await;
}

/// `Dave` and `dave` are one player. Two accounts that differ only in case would
/// look identical on every board.
#[tokio::test]
async fn a_handle_is_taken_regardless_of_case() {
    let api = TestApi::with_database().await;
    let _first = api.signup("dave").await;

    let response = api
        .post(
            "/api/auth/signup",
            &json!({ "handle": "DAVE", "password": PASSWORD }),
        )
        .await;

    assert_error(response, StatusCode::CONFLICT, "HandleTaken").await;
}

#[tokio::test]
async fn login_with_the_right_password_returns_a_token() {
    let api = TestApi::with_database().await;
    let _signup = api.signup("dave").await;

    let response = api
        .post(
            "/api/auth/login",
            &json!({ "handle": "dave", "password": PASSWORD }),
        )
        .await;

    assert_eq!(response.status(), StatusCode::OK);
    let token: TokenResponse = read_json(response).await;
    assert_eq!(
        api.get_as("/api/me", &token.token).await.status(),
        StatusCode::OK
    );
}

#[tokio::test]
async fn login_ignores_the_case_of_the_handle() {
    let api = TestApi::with_database().await;
    let _signup = api.signup("dave").await;

    let response = api
        .post(
            "/api/auth/login",
            &json!({ "handle": "Dave", "password": PASSWORD }),
        )
        .await;

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn login_with_a_wrong_password_is_refused() {
    let api = TestApi::with_database().await;
    let _signup = api.signup("dave").await;

    let response = api
        .post(
            "/api/auth/login",
            &json!({ "handle": "dave", "password": "wrong-password-here" }),
        )
        .await;

    assert_error(response, StatusCode::UNAUTHORIZED, "InvalidCredentials").await;
}

/// The same code and the same message for both failures, so a caller cannot list
/// the handles that exist.
#[tokio::test]
async fn login_with_an_unknown_handle_looks_like_a_wrong_password() {
    let api = TestApi::with_database().await;
    let _signup = api.signup("dave").await;

    let unknown = api
        .post(
            "/api/auth/login",
            &json!({ "handle": "nobody", "password": PASSWORD }),
        )
        .await;
    let wrong = api
        .post(
            "/api/auth/login",
            &json!({ "handle": "dave", "password": "wrong-password-here" }),
        )
        .await;

    assert_eq!(unknown.status(), wrong.status());
    let unknown_body: serde_json::Value = read_json(unknown).await;
    let wrong_body: serde_json::Value = read_json(wrong).await;
    assert_eq!(unknown_body["code"], wrong_body["code"]);
    assert_eq!(unknown_body["message"], wrong_body["message"]);
}

#[tokio::test]
async fn a_token_from_another_secret_is_refused() {
    let api = TestApi::with_database().await;
    let player = api.signup_player("dave").await;

    let foreign = TokenIssuer::new(b"another-secret", Duration::from_secs(3600))
        .issue(player.id)
        .unwrap();

    let response = api.get_as("/api/me", &foreign.token).await;

    assert_error(response, StatusCode::UNAUTHORIZED, "Unauthorized").await;
}

/// The token is signed with the right secret and carries a real player, so only
/// the expiry can refuse it.
#[tokio::test]
async fn an_expired_token_is_refused() {
    let api = TestApi::with_database().await;
    let player = api.signup_player("dave").await;

    // Five seconds past, inside the one minute leeway the library would give by
    // default. The issuer sets the leeway to zero.
    let just_expired = OffsetDateTime::now_utc().unix_timestamp() - 5;
    let expired = signed(
        &json!({ "sub": player.id, "exp": just_expired, "iat": just_expired - 60 }),
        jsonwebtoken::Algorithm::HS256,
    );

    let response = api.get_as("/api/me", &expired).await;

    assert_error(response, StatusCode::UNAUTHORIZED, "Unauthorized").await;
}

/// A token signed with the right secret but another HMAC algorithm must not pass,
/// or a client could pick the weakest algorithm the library knows.
#[tokio::test]
async fn a_token_with_another_algorithm_is_refused() {
    let api = TestApi::with_database().await;
    let player = api.signup_player("dave").await;

    let in_an_hour = OffsetDateTime::now_utc().unix_timestamp() + 3600;
    let hs512 = signed(
        &json!({ "sub": player.id, "exp": in_an_hour, "iat": in_an_hour - 3600 }),
        jsonwebtoken::Algorithm::HS512,
    );

    let response = api.get_as("/api/me", &hs512).await;

    assert_error(response, StatusCode::UNAUTHORIZED, "Unauthorized").await;
}

#[tokio::test]
async fn a_token_without_an_expiry_is_refused() {
    let api = TestApi::with_database().await;
    let player = api.signup_player("dave").await;

    let eternal = signed(&json!({ "sub": player.id }), jsonwebtoken::Algorithm::HS256);

    let response = api.get_as("/api/me", &eternal).await;

    assert_error(response, StatusCode::UNAUTHORIZED, "Unauthorized").await;
}

/// The scheme is case-insensitive per RFC 7235.
#[tokio::test]
async fn a_lowercase_bearer_scheme_is_accepted() {
    let api = TestApi::with_database().await;
    let token = api.signup("dave").await;

    let response = api
        .send(
            axum::http::Request::get("/api/me")
                .header(
                    axum::http::header::AUTHORIZATION,
                    format!("bearer {}", token.token),
                )
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await;

    assert_eq!(response.status(), StatusCode::OK);
}

fn signed(claims: &serde_json::Value, algorithm: jsonwebtoken::Algorithm) -> String {
    jsonwebtoken::encode(
        &jsonwebtoken::Header::new(algorithm),
        claims,
        &jsonwebtoken::EncodingKey::from_secret(support::TEST_JWT_SECRET.as_bytes()),
    )
    .unwrap()
}

/// A signed token for a player that never existed passes the signature check and
/// fails the lookup. The route must not answer 500 for it.
#[tokio::test]
async fn a_valid_token_for_a_missing_player_is_not_found() {
    let api = TestApi::with_database().await;

    let ghost = TokenIssuer::new(
        support::TEST_JWT_SECRET.as_bytes(),
        Duration::from_secs(3600),
    )
    .issue(PlayerId::generate())
    .unwrap();

    let response = api.get_as("/api/me", &ghost.token).await;

    assert_error(response, StatusCode::NOT_FOUND, "ItemNotFound").await;
}

/// The password never appears in a response, not even on the profile.
#[tokio::test]
async fn no_response_carries_the_password_or_its_hash() {
    let api = TestApi::with_database().await;
    let token = api.signup("dave").await;

    let me = api.get_as("/api/me", &token.token).await;
    let body: serde_json::Value = read_json(me).await;
    let rendered = body.to_string();

    assert!(!rendered.contains(PASSWORD));
    assert!(!rendered.contains("$argon2"));
}

#[test]
fn the_test_secrets_are_not_the_development_default() {
    let secrets: Secrets = support::test_secrets();

    assert_ne!(
        secrets.jwt_secret.as_ref(),
        bunker_api::config::DEV_JWT_SECRET
    );
}
