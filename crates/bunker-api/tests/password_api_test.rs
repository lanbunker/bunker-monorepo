//! Password change by the owner, and reset by an admin with a forced change.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod support;

use axum::http::StatusCode;
use bunker_models::{Account, TemporaryPassword, TokenResponse};
use serde_json::json;
use support::{PASSWORD, TestApi, assert_error, read_json};

const NEW_PASSWORD: &str = "a-brand-new-passphrase";

async fn login(api: &TestApi, handle: &str, password: &str) -> axum::response::Response {
    api.post(
        "/api/auth/login",
        &json!({ "handle": handle, "password": password }),
    )
    .await
}

#[tokio::test]
async fn a_player_changes_their_password_and_the_old_one_stops_working() {
    let api = TestApi::with_database().await;
    let token = api.signup("dave").await.token;

    let response = api
        .post_as(
            "/api/me/password",
            &json!({ "currentPassword": PASSWORD, "newPassword": NEW_PASSWORD }),
            &token,
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let fresh: TokenResponse = read_json(response).await;
    assert_eq!(
        api.get_as("/api/me", &fresh.token).await.status(),
        StatusCode::OK
    );

    assert_eq!(
        login(&api, "dave", PASSWORD).await.status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        login(&api, "dave", NEW_PASSWORD).await.status(),
        StatusCode::OK
    );
}

#[tokio::test]
async fn a_wrong_current_password_is_refused_and_changes_nothing() {
    let api = TestApi::with_database().await;
    let token = api.signup("dave").await.token;

    let response = api
        .post_as(
            "/api/me/password",
            &json!({ "currentPassword": "not-the-password", "newPassword": NEW_PASSWORD }),
            &token,
        )
        .await;

    assert_error(response, StatusCode::BAD_REQUEST, "WrongPassword").await;
    assert_eq!(login(&api, "dave", PASSWORD).await.status(), StatusCode::OK);
}

#[tokio::test]
async fn a_short_new_password_is_rejected_at_the_boundary() {
    let api = TestApi::with_database().await;
    let token = api.signup("dave").await.token;

    let response = api
        .post_as(
            "/api/me/password",
            &json!({ "currentPassword": PASSWORD, "newPassword": "short" }),
            &token,
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
async fn a_password_change_needs_a_token() {
    let api = TestApi::with_database().await;

    let response = api
        .post(
            "/api/me/password",
            &json!({ "currentPassword": PASSWORD, "newPassword": NEW_PASSWORD }),
        )
        .await;

    assert_error(response, StatusCode::UNAUTHORIZED, "Unauthorized").await;
}

/// The whole reset flow: the admin gets a temporary password once, the player
/// logs in with it, the account says a change is due, the change clears it.
#[tokio::test]
async fn an_admin_reset_forces_a_change_at_the_next_login() {
    let api = TestApi::with_database().await;
    let admin = api.signup_admin("root").await;
    let dave = api.signup_player("dave").await;

    let reset = api
        .post_as(
            &format!("/api/admin/players/{}/password-reset", dave.id),
            &json!({}),
            &admin,
        )
        .await;
    assert_eq!(reset.status(), StatusCode::OK);
    let temporary: TemporaryPassword = read_json(reset).await;
    assert_eq!(temporary.temporary_password.len(), 32);
    assert!(
        temporary
            .temporary_password
            .chars()
            .all(|c| c.is_ascii_hexdigit())
    );

    assert_eq!(
        login(&api, "dave", PASSWORD).await.status(),
        StatusCode::UNAUTHORIZED
    );
    let session: TokenResponse =
        read_json(login(&api, "dave", &temporary.temporary_password).await).await;

    let account: Account = read_json(api.get_as("/api/me", &session.token).await).await;
    assert!(account.must_change_password);

    let change = api
        .post_as(
            "/api/me/password",
            &json!({ "currentPassword": temporary.temporary_password, "newPassword": NEW_PASSWORD }),
            &session.token,
        )
        .await;
    assert_eq!(change.status(), StatusCode::OK);
    let session: TokenResponse = read_json(change).await;

    let account: Account = read_json(api.get_as("/api/me", &session.token).await).await;
    assert!(!account.must_change_password);
    assert_eq!(
        login(&api, "dave", NEW_PASSWORD).await.status(),
        StatusCode::OK
    );
}

#[tokio::test]
async fn two_resets_give_two_different_temporary_passwords() {
    let api = TestApi::with_database().await;
    let admin = api.signup_admin("root").await;
    let dave = api.signup_player("dave").await;
    let path = format!("/api/admin/players/{}/password-reset", dave.id);

    let first: TemporaryPassword = read_json(api.post_as(&path, &json!({}), &admin).await).await;
    let second: TemporaryPassword = read_json(api.post_as(&path, &json!({}), &admin).await).await;

    assert_ne!(first.temporary_password, second.temporary_password);
    assert_eq!(
        login(&api, "dave", &first.temporary_password)
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        login(&api, "dave", &second.temporary_password)
            .await
            .status(),
        StatusCode::OK
    );
}

#[tokio::test]
async fn a_user_cannot_reset_another_password() {
    let api = TestApi::with_database().await;
    let user = api.signup("dave").await.token;
    let victim = api.signup_player("ziopera").await;

    let response = api
        .post_as(
            &format!("/api/admin/players/{}/password-reset", victim.id),
            &json!({}),
            &user,
        )
        .await;

    assert_error(response, StatusCode::FORBIDDEN, "Forbidden").await;
}

#[tokio::test]
async fn resetting_an_unknown_player_is_not_found() {
    let api = TestApi::with_database().await;
    let admin = api.signup_admin("root").await;

    let response = api
        .post_as(
            &format!("/api/admin/players/{}/password-reset", uuid::Uuid::new_v4()),
            &json!({}),
            &admin,
        )
        .await;

    assert_error(response, StatusCode::NOT_FOUND, "ItemNotFound").await;
}

/// A password change is how a player evicts an intruder. Every token from before
/// the change must die, including the one that made the change.
#[tokio::test]
async fn a_password_change_kills_every_older_token() {
    let api = TestApi::with_database().await;
    let old_token = api.signup("dave").await.token;
    // Tokens carry seconds. The change must land in a later second than the signup.
    tokio::time::sleep(std::time::Duration::from_millis(1100)).await;

    let response = api
        .post_as(
            "/api/me/password",
            &json!({ "currentPassword": PASSWORD, "newPassword": NEW_PASSWORD }),
            &old_token,
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);

    assert_error(
        api.get_as("/api/me", &old_token).await,
        StatusCode::UNAUTHORIZED,
        "Unauthorized",
    )
    .await;
}

#[tokio::test]
async fn an_admin_reset_kills_the_player_session() {
    let api = TestApi::with_database().await;
    let admin = api.signup_admin("root").await;
    let dave_token = api.signup("dave").await.token;
    let dave = read_json::<Account>(api.get_as("/api/me", &dave_token).await)
        .await
        .player;
    tokio::time::sleep(std::time::Duration::from_millis(1100)).await;

    let reset = api
        .post_as(
            &format!("/api/admin/players/{}/password-reset", dave.id),
            &json!({}),
            &admin,
        )
        .await;
    assert_eq!(reset.status(), StatusCode::OK);

    assert_error(
        api.get_as("/api/me", &dave_token).await,
        StatusCode::UNAUTHORIZED,
        "Unauthorized",
    )
    .await;
}
