//! The shared test client.
//!
//! Cargo compiles each `tests/*.rs` as its own binary, but it ignores
//! subdirectories. This file is therefore `support/mod.rs` and not `support.rs`.
//! It is the one file in the repository that can use `mod.rs`.
//!
//! There are two ways to get a router:
//!
//! - [`TestApi::without_database`] for a request that the boundary rejects. Its
//!   pool points at a path that cannot be opened.
//! - [`TestApi::with_database`] gives each test its own SQLite file in a
//!   temporary directory, migrated. Tests share nothing, so they run in parallel
//!   with no locks and no unique-name tricks.

// Each test binary compiles its own copy of this module, so some items are unused.
#![allow(
    dead_code,
    unreachable_pub,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use axum::response::Response;
use bunker_api::config::{AppEnv, DbConfig, JwtSecret, MaxConnections, resolve_app_config};
use bunker_api::server::{Secrets, build_router};
use bunker_api::storage::{connect, run_pending_migrations};
use bunker_models::{Player, TokenResponse};
use http_body_util::BodyExt as _;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use tempfile::TempDir;
use tower::ServiceExt as _;

/// The secret every test router signs with. A test that needs a token from
/// another issuer builds a second [`Secrets`].
pub const TEST_JWT_SECRET: &str = "test-secret-not-for-production-32-chars-long";

/// A password that passes validation. Tests that check the rules use their own.
pub const PASSWORD: &str = "correct-horse-battery";

/// A router, and the helpers that send requests to it.
pub struct TestApi {
    router: Router,
    /// Removes the database file when the test ends.
    _dir: Option<TempDir>,
}

impl TestApi {
    /// A router with a pool that can open nothing. Each request that reaches the
    /// database fails as unavailable, and each request that the boundary rejects
    /// never gets there.
    pub async fn without_database() -> Self {
        Self::without_database_as(AppEnv::Test).await
    }

    /// The same as [`Self::without_database`], but with a given environment. Use it
    /// to test what a deployed configuration shows.
    pub async fn without_database_as(app_env: AppEnv) -> Self {
        let config = resolve_app_config(app_env);
        let pool = connect(&unreachable_database()).unwrap();

        Self {
            router: build_router(pool, config, &test_secrets()),
            _dir: None,
        }
    }

    /// A router with a fresh, migrated SQLite file. Nothing is shared between
    /// tests.
    pub async fn with_database() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");
        let config = DbConfig {
            url: format!("sqlite://{}?mode=rwc", path.display()),
            max_connections: MaxConnections::try_new(4).unwrap(),
        };

        let pool = connect(&config).unwrap();
        run_pending_migrations(&pool).await.unwrap();

        Self {
            router: build_router(pool, resolve_app_config(AppEnv::Test), &test_secrets()),
            _dir: Some(dir),
        }
    }

    /// Signs a player up and returns the token, for a test with another subject.
    pub async fn signup(&self, handle: &str) -> TokenResponse {
        let response = self
            .post(
                "/api/auth/signup",
                &json!({ "handle": handle, "password": PASSWORD }),
            )
            .await;
        assert_eq!(
            response.status(),
            StatusCode::CREATED,
            "signup of {handle} failed"
        );

        read_json(response).await
    }

    /// Signs a player up and reads the player back through `/api/me`.
    pub async fn signup_player(&self, handle: &str) -> Player {
        let token = self.signup(handle).await;
        let response = self.get_as("/api/me", &token.token).await;
        assert_eq!(response.status(), StatusCode::OK);

        read_json(response).await
    }

    pub async fn get(&self, path: &str) -> Response {
        self.send(Request::get(path).body(Body::empty()).unwrap())
            .await
    }

    pub async fn get_as(&self, path: &str, token: &str) -> Response {
        self.send(
            Request::get(path)
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
    }

    pub async fn post(&self, path: &str, body: &Value) -> Response {
        self.send(json_request(Method::POST, path, body)).await
    }

    /// For a request that the typed helpers cannot make: a wrong method, an absent
    /// header or a bad body.
    pub async fn send(&self, request: Request<Body>) -> Response {
        self.router.clone().oneshot(request).await.unwrap()
    }
}

/// Asserts the status and the error code together, because the contract gives
/// both.
pub async fn assert_error(response: Response, status: StatusCode, code: &str) {
    let actual = response.status();
    let body: Value = read_json(response).await;

    assert_eq!(
        actual, status,
        "expected {status} with code {code}, body was {body}"
    );
    assert_eq!(
        body.get("code").and_then(Value::as_str),
        Some(code),
        "body was {body}"
    );
}

pub async fn read_json<T: DeserializeOwned>(response: Response) -> T {
    let bytes = response.into_body().collect().await.unwrap().to_bytes();

    serde_json::from_slice(&bytes)
        .unwrap_or_else(|error| panic!("not the expected shape: {error}: {bytes:?}"))
}

/// The `Display` of `error`, then the `Display` of each cause below it.
pub fn cause_chain(error: &dyn std::error::Error) -> String {
    let mut chain = vec![error.to_string()];
    let mut current = error.source();
    while let Some(cause) = current {
        chain.push(cause.to_string());
        current = cause.source();
    }

    chain.join(": ")
}

pub fn test_secrets() -> Secrets {
    Secrets {
        jwt_secret: JwtSecret::try_new(TEST_JWT_SECRET).unwrap(),
    }
}

/// A pool whose file lives in a directory that does not exist. `mode=rw` refuses
/// to create it, so the first query fails and the connect itself does not.
pub fn unreachable_database() -> DbConfig {
    DbConfig {
        url: "sqlite:///nonexistent-bunker-dir/unreachable.db?mode=rw".to_owned(),
        max_connections: MaxConnections::try_new(1).unwrap(),
    }
}

fn json_request(method: Method, path: &str, body: &Value) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(path)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(serde_json::to_vec(body).unwrap()))
        .unwrap()
}
