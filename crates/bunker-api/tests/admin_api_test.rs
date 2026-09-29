//! The backoffice routes. A user is refused, an admin manages players.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod support;

use axum::http::StatusCode;
use bunker_api::storage::{PlayerStorage, Removal, RoleChanged};
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
async fn an_admin_lists_every_player() {
    let api = TestApi::with_database().await;
    let admin = api.signup_admin("root").await;
    api.signup_player("zed").await;
    api.signup_player("abe").await;

    let response = api.get_as("/api/admin/players", &admin).await;

    assert_eq!(response.status(), StatusCode::OK);
    let page: Paginated<Player> = read_json(response).await;
    let handles: Vec<&str> = page.items.iter().map(|p| p.handle.as_ref()).collect();
    assert_eq!(
        handles,
        ["abe", "zed", "root"],
        "the last signup first, not by handle and not the oldest first"
    );
    assert_eq!(page.total, 3);
}

#[tokio::test]
async fn an_admin_searches_the_roster_by_handle() {
    let api = TestApi::with_database().await;
    let admin = api.signup_admin("root").await;
    for handle in ["dave", "davide", "ziopera"] {
        api.signup_player(handle).await;
    }

    let response = api.get_as("/api/admin/players?q=dav", &admin).await;

    assert_eq!(response.status(), StatusCode::OK);
    let page: Paginated<Player> = read_json(response).await;
    let handles: Vec<&str> = page.items.iter().map(|p| p.handle.as_ref()).collect();
    assert_eq!(handles, ["davide", "dave"]);
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

    let first = api.delete_as(&path, &admin).await;
    assert_eq!(first.status(), StatusCode::NO_CONTENT);
    assert_eq!(
        api.get("/api/players/dave").await.status(),
        StatusCode::NOT_FOUND
    );

    let second = api.delete_as(&path, &admin).await;
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
        .delete_as(&format!("/api/admin/players/{}", dave.id), &admin)
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
        .delete_as(&format!("/api/admin/players/{}", me.id), &admin)
        .await;
    assert_error(delete, StatusCode::FORBIDDEN, "Forbidden").await;
    assert_eq!(
        api.get_as("/api/admin/players", &admin).await.status(),
        StatusCode::OK
    );
}

/// The guard is in the statement that writes. Once one admin is left, a
/// demotion and a delete of that admin find nothing to write, whatever the
/// caller checked before.
#[tokio::test]
async fn the_last_admin_is_never_demoted_or_deleted() {
    let api = TestApi::with_database().await;
    let _root = api.signup_admin("root").await;
    let _second = api.signup_admin("second").await;
    let root = api.player("root").await;
    let second = api.player("second").await;
    let storage = PlayerStorage::new(api.pool().clone());

    let first = storage.set_role(second.id, Role::User).await.unwrap();
    assert!(matches!(first, RoleChanged::Player(ref p) if p.role == Role::User));

    let last = storage.set_role(root.id, Role::User).await.unwrap();
    assert!(matches!(last, RoleChanged::LastAdmin), "got {last:?}");
    assert_eq!(storage.delete(root.id).await.unwrap(), Removal::LastAdmin);
    assert_eq!(api.player("root").await.role, Role::Admin);

    assert_eq!(storage.delete(second.id).await.unwrap(), Removal::Removed);
    assert_eq!(storage.delete(second.id).await.unwrap(), Removal::Absent);
}

/// Two admins demote each other at the same moment. The auth check of each
/// request may pass before the other write lands, so only the guard in the
/// write keeps an admin in the crew.
#[tokio::test]
async fn two_admins_who_demote_each_other_at_once_leave_one_admin() {
    let api = TestApi::with_database().await;
    let root_token = api.signup_admin("root").await;
    let second_token = api.signup_admin("second").await;
    let root = api.player("root").await;
    let second = api.player("second").await;
    let demote = json!({ "role": "user" });
    let second_path = format!("/api/admin/players/{}", second.id);
    let root_path = format!("/api/admin/players/{}", root.id);

    let (one, two) = tokio::join!(
        api.patch_as(&second_path, &demote, &root_token),
        api.patch_as(&root_path, &demote, &second_token),
    );

    let passed = [one.status(), two.status()]
        .iter()
        .filter(|status| **status == StatusCode::OK)
        .count();
    assert_eq!(
        passed,
        1,
        "one demotion wins: {} and {}",
        one.status(),
        two.status()
    );
    let admins = [api.player("root").await, api.player("second").await]
        .iter()
        .filter(|p| p.role == Role::Admin)
        .count();
    assert_eq!(admins, 1);
}

/// The token, the temporary password and the account must not sit in a cache
/// between the site and the API.
#[tokio::test]
async fn every_answer_with_a_secret_or_the_account_is_not_stored() {
    let api = TestApi::with_database().await;
    let admin = api.signup_admin("root").await;
    let dave = api.signup_player("dave").await;
    let token = api
        .post(
            "/api/auth/login",
            &json!({ "handle": "dave", "password": support::PASSWORD }),
        )
        .await;
    let signup = api
        .post(
            "/api/auth/signup",
            &json!({ "handle": "erin", "password": support::PASSWORD }),
        )
        .await;
    let erin: bunker_models::TokenResponse = read_json(signup).await;

    let answers = [
        ("login", token),
        ("me", api.get_as("/api/me", &erin.token).await),
        (
            "registrations",
            api.get_as("/api/me/registrations", &erin.token).await,
        ),
        (
            "checkins",
            api.get_as("/api/me/checkins", &erin.token).await,
        ),
        (
            "handle",
            api.put_as("/api/me/handle", &json!({ "handle": "erin2" }), &erin.token)
                .await,
        ),
        (
            "reset",
            api.post_as(
                &format!("/api/admin/players/{}/password-reset", dave.id),
                &json!({}),
                &admin,
            )
            .await,
        ),
        ("refused", api.get("/api/me").await),
    ];
    for (name, response) in answers {
        assert_eq!(
            response
                .headers()
                .get("cache-control")
                .and_then(|v| v.to_str().ok()),
            Some("no-store"),
            "{name} answered {}",
            response.status()
        );
    }
    let public = api.get("/api/players/dave").await;
    assert!(public.headers().get("cache-control").is_none());
}
