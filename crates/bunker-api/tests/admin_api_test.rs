//! The backoffice routes. A user is refused, an admin manages players.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod support;

use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use bunker_models::{Account, Paginated, Player, Role};
use serde_json::{Value, json};
use support::{TestApi, assert_error, read_json};

#[tokio::test]
async fn a_new_player_is_a_user() {
    let api = TestApi::with_database().await;

    let player = api.signup_player("dave").await;

    assert_eq!(player.role, Role::User);
}

#[tokio::test]
async fn a_user_cannot_open_the_backoffice() {
    let api = TestApi::with_database().await;
    let token = api.signup("dave").await;

    let response = api.get_as("/api/admin/players", &token.token).await;

    assert_error(response, StatusCode::FORBIDDEN, "Forbidden").await;
}

#[tokio::test]
async fn the_backoffice_needs_a_token() {
    let api = TestApi::with_database().await;

    let response = api.get("/api/admin/players").await;

    assert_error(response, StatusCode::UNAUTHORIZED, "Unauthorized").await;
}

#[tokio::test]
async fn an_admin_lists_every_player() {
    let api = TestApi::with_database().await;
    let admin = api.signup_admin("root").await;
    let _user = api.signup_player("dave").await;

    let response = api.get_as("/api/admin/players", &admin).await;

    assert_eq!(response.status(), StatusCode::OK);
    let page: Paginated<Player> = read_json(response).await;
    let handles: Vec<&str> = page.items.iter().map(|p| p.handle.as_ref()).collect();
    assert_eq!(handles, ["dave", "root"]);
    assert_eq!(page.total, 2);
}

#[tokio::test]
async fn an_admin_promotes_and_demotes_a_player() {
    let api = TestApi::with_database().await;
    let admin = api.signup_admin("root").await;
    let dave = api.signup_player("dave").await;
    let path = format!("/api/admin/players/{}", dave.id);

    let promoted = api
        .patch_as(&path, &json!({ "role": "admin" }), &admin)
        .await;
    assert_eq!(promoted.status(), StatusCode::OK);
    assert_eq!(read_json::<Player>(promoted).await.role, Role::Admin);

    let demoted = api
        .patch_as(&path, &json!({ "role": "user" }), &admin)
        .await;
    assert_eq!(read_json::<Player>(demoted).await.role, Role::User);

    let public: Value = read_json(api.get("/api/players/dave").await).await;
    assert_eq!(public["role"], json!("user"));
}

/// The role comes from the database on each request, so a demoted admin loses
/// the backoffice at once, with the token they still hold.
#[tokio::test]
async fn a_demoted_admin_is_locked_out_immediately() {
    let api = TestApi::with_database().await;
    let root = api.signup_admin("root").await;
    let second = api.signup_admin("second").await;
    let second_player = read_json::<Account>(api.get_as("/api/me", &second).await)
        .await
        .player;

    let demote = api
        .patch_as(
            &format!("/api/admin/players/{}", second_player.id),
            &json!({ "role": "user" }),
            &root,
        )
        .await;
    assert_eq!(demote.status(), StatusCode::OK);

    let response = api.get_as("/api/admin/players", &second).await;
    assert_error(response, StatusCode::FORBIDDEN, "Forbidden").await;
}

#[tokio::test]
async fn an_unknown_role_is_rejected_at_the_boundary() {
    let api = TestApi::with_database().await;
    let admin = api.signup_admin("root").await;
    let dave = api.signup_player("dave").await;

    let response = api
        .patch_as(
            &format!("/api/admin/players/{}", dave.id),
            &json!({ "role": "superuser" }),
            &admin,
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
async fn setting_the_role_of_an_unknown_player_is_not_found() {
    let api = TestApi::with_database().await;
    let admin = api.signup_admin("root").await;

    let response = api
        .patch_as(
            &format!("/api/admin/players/{}", uuid::Uuid::new_v4()),
            &json!({ "role": "admin" }),
            &admin,
        )
        .await;

    assert_error(response, StatusCode::NOT_FOUND, "ItemNotFound").await;
}

#[tokio::test]
async fn an_admin_deletes_a_player_and_the_delete_is_idempotent() {
    let api = TestApi::with_database().await;
    let admin = api.signup_admin("root").await;
    let dave = api.signup_player("dave").await;
    let path = format!("/api/admin/players/{}", dave.id);

    let first = api
        .send(
            Request::builder()
                .method(Method::DELETE)
                .uri(&path)
                .header(header::AUTHORIZATION, format!("Bearer {admin}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(first.status(), StatusCode::NO_CONTENT);
    assert_eq!(
        api.get("/api/players/dave").await.status(),
        StatusCode::NOT_FOUND
    );

    let second = api
        .send(
            Request::builder()
                .method(Method::DELETE)
                .uri(&path)
                .header(header::AUTHORIZATION, format!("Bearer {admin}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(second.status(), StatusCode::NO_CONTENT);
}

/// A deleted player still holds a signed token. It is refused from then on.
#[tokio::test]
async fn a_deleted_player_is_logged_out() {
    let api = TestApi::with_database().await;
    let admin = api.signup_admin("root").await;
    let dave_token = api.signup("dave").await;
    let dave = read_json::<Account>(api.get_as("/api/me", &dave_token.token).await)
        .await
        .player;

    let _deleted = api
        .send(
            Request::builder()
                .method(Method::DELETE)
                .uri(format!("/api/admin/players/{}", dave.id))
                .header(header::AUTHORIZATION, format!("Bearer {admin}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await;

    let response = api.get_as("/api/me", &dave_token.token).await;
    assert_error(response, StatusCode::UNAUTHORIZED, "Unauthorized").await;
}

#[tokio::test]
async fn an_admin_cannot_demote_or_delete_themself() {
    let api = TestApi::with_database().await;
    let admin = api.signup_admin("root").await;
    let me = read_json::<Account>(api.get_as("/api/me", &admin).await)
        .await
        .player;

    let demote = api
        .patch_as(
            &format!("/api/admin/players/{}", me.id),
            &json!({ "role": "user" }),
            &admin,
        )
        .await;
    assert_error(demote, StatusCode::FORBIDDEN, "Forbidden").await;

    let delete = api
        .send(
            Request::builder()
                .method(Method::DELETE)
                .uri(format!("/api/admin/players/{}", me.id))
                .header(header::AUTHORIZATION, format!("Bearer {admin}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_error(delete, StatusCode::FORBIDDEN, "Forbidden").await;
    assert_eq!(
        api.get_as("/api/admin/players", &admin).await.status(),
        StatusCode::OK
    );
}
