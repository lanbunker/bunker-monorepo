//! A player renames themself, an admin renames anyone. The glyph never moves.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use axum::http::StatusCode;
use bunker_models::{Account, Player};
use serde_json::json;
use support::{PASSWORD, TestApi, assert_error, read_json};

#[tokio::test]
async fn a_player_renames_themself_and_keeps_the_glyph() {
    let api = TestApi::with_database().await;
    let token = api.signup("dave").await.token;
    let before = read_json::<Account>(api.get_as("/api/me", &token).await)
        .await
        .player;

    let response = api
        .put_as("/api/me/handle", &json!({ "handle": "david" }), &token)
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let renamed: Player = read_json(response).await;

    assert_eq!(renamed.id, before.id);
    assert_eq!(renamed.handle.as_ref(), "david");
    assert_eq!(renamed.glyph, before.glyph);

    let me = read_json::<Account>(api.get_as("/api/me", &token).await).await;
    assert_eq!(me.player.handle.as_ref(), "david");
    assert_eq!(
        api.get("/api/players/dave").await.status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(api.get("/api/players/david").await.status(), StatusCode::OK);
}

#[tokio::test]
async fn a_rename_moves_the_login_identity() {
    let api = TestApi::with_database().await;
    let token = api.signup("dave").await.token;
    api.put_as("/api/me/handle", &json!({ "handle": "david" }), &token)
        .await;

    let old = api
        .post(
            "/api/auth/login",
            &json!({ "handle": "dave", "password": PASSWORD }),
        )
        .await;
    assert_error(old, StatusCode::UNAUTHORIZED, "InvalidCredentials").await;

    let new = api
        .post(
            "/api/auth/login",
            &json!({ "handle": "david", "password": PASSWORD }),
        )
        .await;
    assert_eq!(new.status(), StatusCode::OK);
}

#[tokio::test]
async fn an_unknown_field_in_the_body_is_refused() {
    let api = TestApi::with_database().await;
    let token = api.signup("dave").await.token;

    let response = api
        .put_as(
            "/api/me/handle",
            &json!({ "handle": "david", "role": "admin" }),
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
async fn a_handle_that_another_player_holds_is_refused_in_any_case() {
    let api = TestApi::with_database().await;
    api.signup("Dave").await;
    let token = api.signup("zio").await.token;

    let response = api
        .put_as("/api/me/handle", &json!({ "handle": "dAVE" }), &token)
        .await;

    assert_error(response, StatusCode::CONFLICT, "HandleTaken").await;
}

#[tokio::test]
async fn a_player_can_change_only_the_case_of_their_own_handle() {
    let api = TestApi::with_database().await;
    let token = api.signup("dave").await.token;

    let response = api
        .put_as("/api/me/handle", &json!({ "handle": "DAVE" }), &token)
        .await;

    assert_eq!(response.status(), StatusCode::OK);
    let renamed: Player = read_json(response).await;
    assert_eq!(renamed.handle.as_ref(), "DAVE");
}

#[tokio::test]
async fn an_invalid_handle_is_refused() {
    let api = TestApi::with_database().await;
    let token = api.signup("dave").await.token;

    let response = api
        .put_as("/api/me/handle", &json!({ "handle": "no spaces" }), &token)
        .await;

    assert_error(
        response,
        StatusCode::UNPROCESSABLE_ENTITY,
        "UnprocessableRequest",
    )
    .await;
}

#[tokio::test]
async fn an_admin_renames_a_player_and_a_user_cannot() {
    let api = TestApi::with_database().await;
    let admin = api.signup_admin("root").await;
    let user = api.signup("zio").await.token;
    let target = api.signup_player("dave").await;
    let path = format!("/api/admin/players/{}/handle", target.id);

    let refused = api
        .put_as(&path, &json!({ "handle": "hacked" }), &user)
        .await;
    assert_error(refused, StatusCode::FORBIDDEN, "Forbidden").await;

    let response = api
        .put_as(&path, &json!({ "handle": "davide" }), &admin)
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let renamed: Player = read_json(response).await;
    assert_eq!(renamed.handle.as_ref(), "davide");
    assert_eq!(renamed.glyph, target.glyph);

    let taken = api.put_as(&path, &json!({ "handle": "ZIO" }), &admin).await;
    assert_error(taken, StatusCode::CONFLICT, "HandleTaken").await;
}

#[tokio::test]
async fn an_admin_rename_of_a_missing_player_is_not_found() {
    let api = TestApi::with_database().await;
    let admin = api.signup_admin("root").await;

    let response = api
        .put_as(
            &format!("/api/admin/players/{}/handle", uuid::Uuid::new_v4()),
            &json!({ "handle": "ghost" }),
            &admin,
        )
        .await;

    assert_error(response, StatusCode::NOT_FOUND, "ItemNotFound").await;
}
