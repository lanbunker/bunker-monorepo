//! The shared test client.

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
use bunker_api::storage::{DbPool, connect, run_pending_migrations};
use bunker_models::{
    Account, Bracket, CyclesLog, Entrant, EntrantId, Event, EventDetail, EventId, Match, Player,
    PlayerId, Role, TemporaryPassword, TokenResponse, Tournament, TournamentDetail, TournamentId,
};
use http_body_util::BodyExt as _;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use tempfile::TempDir;
use time::format_description::well_known::Rfc3339;
use time::{Duration, OffsetDateTime};
use tower::ServiceExt as _;

/// The secret every test router signs with. A test that needs a token from
/// another issuer builds a second [`Secrets`].
pub const TEST_JWT_SECRET: &str = "test-secret-not-for-production-32-chars-long";

/// A password that passes validation. Tests that check the rules use their own.
pub const PASSWORD: &str = "correct-horse-battery";

pub const HOUR: i64 = 3600;

pub struct TestApi {
    router: Router,
    pool: Option<DbPool>,
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
            pool: None,
            _dir: None,
        }
    }

    /// A router with a fresh, migrated SQLite file. Nothing is shared between
    /// tests.
    pub async fn with_database() -> Self {
        let (dir, pool) = migrated_pool().await;

        Self {
            router: build_router(
                pool.clone(),
                resolve_app_config(AppEnv::Test),
                &test_secrets(),
            ),
            pool: Some(pool),
            _dir: Some(dir),
        }
    }

    /// The pool behind the router, for a test that must write a row the API
    /// refuses to write, such as a status no transition reaches.
    pub fn pool(&self) -> &DbPool {
        self.pool.as_ref().expect("this test needs a database")
    }

    /// Signs a player up and returns the token.
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

        read_json::<Account>(response).await.player
    }

    /// Signs a player up and promotes them with SQL, the way `make admin` does.
    /// Returns the bearer token.
    pub async fn signup_admin(&self, handle: &str) -> String {
        let token = self.signup(handle).await;
        sqlx::query("update players set role = ?1 where handle = ?2 collate nocase")
            .bind(Role::Admin.as_str())
            .bind(handle)
            .execute(self.pool())
            .await
            .unwrap();

        token.token
    }

    /// An admin resets the password of `player`, and the player logs in with
    /// the temporary one. Returns the token of that session, which must change
    /// the password before anything else, and the temporary password.
    pub async fn forced_session(
        &self,
        admin: &str,
        player: PlayerId,
        handle: &str,
    ) -> (String, String) {
        let reset = self
            .post_as(
                &format!("/api/admin/players/{player}/password-reset"),
                &json!({}),
                admin,
            )
            .await;
        assert_eq!(reset.status(), StatusCode::OK, "the reset failed");
        let temporary: TemporaryPassword = read_json(reset).await;
        let login = self
            .post(
                "/api/auth/login",
                &json!({ "handle": handle, "password": temporary.temporary_password }),
            )
            .await;
        assert_eq!(login.status(), StatusCode::OK, "the temporary login failed");

        (
            read_json::<TokenResponse>(login).await.token,
            temporary.temporary_password,
        )
    }

    /// The public player behind a handle.
    pub async fn player(&self, handle: &str) -> Player {
        let response = self.get(&format!("/api/players/{handle}")).await;
        assert_eq!(response.status(), StatusCode::OK, "no player {handle}");

        read_json(response).await
    }

    /// The public cycles log of a handle.
    pub async fn cycles_log(&self, handle: &str) -> CyclesLog {
        let response = self.get(&format!("/api/players/{handle}/cycles")).await;
        assert_eq!(
            response.status(),
            StatusCode::OK,
            "no cycles log of {handle}"
        );

        read_json(response).await
    }

    /// A draft tournament for tomorrow whose registration closes in
    /// `closes_in_seconds`. A negative value makes a deadline in the past.
    pub async fn create_tournament(&self, admin: &str, closes_in_seconds: i64) -> Tournament {
        let closes_at = OffsetDateTime::now_utc() + Duration::seconds(closes_in_seconds);
        let response = self
            .post_as(
                "/api/admin/tournaments",
                &json!({
                    "name": "Sniper Cup",
                    "game": "COD MW2",
                    "mode": "1v1 sniper only",
                    "description": "One life, one shot.",
                    "date": "2026-10-24",
                    "registrationClosesAt": closes_at.format(&Rfc3339).unwrap(),
                }),
                admin,
            )
            .await;
        assert_eq!(
            response.status(),
            StatusCode::CREATED,
            "tournament creation failed"
        );

        read_json(response).await
    }

    pub async fn set_status(
        &self,
        admin: &str,
        tournament: TournamentId,
        status: &str,
    ) -> Tournament {
        let response = self
            .post_as(
                &format!("/api/admin/tournaments/{tournament}/status"),
                &json!({ "status": status }),
                admin,
            )
            .await;
        assert_eq!(
            response.status(),
            StatusCode::OK,
            "status change to {status} failed"
        );

        read_json(response).await
    }

    /// Signs up a player and adds them as an entrant through the admin route,
    /// with no level.
    pub async fn add_entrant(
        &self,
        admin: &str,
        tournament: TournamentId,
        handle: &str,
    ) -> Entrant {
        self.add_rated_entrant(admin, tournament, handle, None)
            .await
    }

    /// Signs up a player and adds them as an entrant through the admin route,
    /// with the given level.
    pub async fn add_rated_entrant(
        &self,
        admin: &str,
        tournament: TournamentId,
        handle: &str,
        skill: Option<u8>,
    ) -> Entrant {
        let player = self.signup_player(handle).await;
        let response = self
            .post_as(
                &format!("/api/admin/tournaments/{tournament}/entrants"),
                &json!({ "playerId": player.id, "skill": skill }),
                admin,
            )
            .await;
        assert_eq!(
            response.status(),
            StatusCode::CREATED,
            "adding {handle} failed"
        );

        read_json(response).await
    }

    /// The admin view of a tournament, drafts included.
    pub async fn detail(&self, admin: &str, tournament: TournamentId) -> TournamentDetail {
        let response = self
            .get_as(&format!("/api/admin/tournaments/{tournament}"), admin)
            .await;
        assert_eq!(response.status(), StatusCode::OK);

        read_json(response).await
    }

    pub async fn generate_bracket(&self, admin: &str, tournament: TournamentId) -> Bracket {
        let response = self
            .post_as(
                &format!("/api/admin/tournaments/{tournament}/bracket"),
                &json!({}),
                admin,
            )
            .await;
        assert_eq!(
            response.status(),
            StatusCode::OK,
            "bracket generation failed"
        );

        read_json(response).await
    }

    /// Enters `winner` as the result of `m`, and returns the bracket after it.
    pub async fn report(
        &self,
        admin: &str,
        tournament: TournamentId,
        m: &Match,
        winner: EntrantId,
    ) -> Bracket {
        let response = self
            .put_as(
                &format!(
                    "/api/admin/tournaments/{tournament}/matches/{}/result",
                    m.id
                ),
                &json!({ "winner": winner }),
                admin,
            )
            .await;
        assert_eq!(response.status(), StatusCode::OK, "report failed");

        read_json(response).await
    }

    /// Plays the first match that waits for a result, and side a wins it.
    pub async fn play_next(
        &self,
        admin: &str,
        tournament: TournamentId,
        bracket: &Bracket,
    ) -> Bracket {
        let next = bracket
            .flat()
            .find(|m| m.is_ready() && m.winner.is_none())
            .cloned()
            .expect("no match waits for a result");

        self.report(admin, tournament, &next, next.entrant_a.unwrap())
            .await
    }

    /// A draft event whose doors open in `opens_in_seconds` and close
    /// `lasts_seconds` later. Negative values put the night in the past.
    pub async fn create_event(
        &self,
        admin: &str,
        opens_in_seconds: i64,
        lasts_seconds: i64,
    ) -> Event {
        let starts_at = OffsetDateTime::now_utc() + Duration::seconds(opens_in_seconds);
        let ends_at = starts_at + Duration::seconds(lasts_seconds);
        let response = self
            .post_as(
                "/api/admin/events",
                &json!({
                    "name": "BUNKER//SESSION 04",
                    "location": "@theoffice",
                    "games": "Halo 3, Mario Kart 8",
                    "description": "Doors at nine.",
                    "image": "feb2026-cover.webp",
                    "startsAt": starts_at.format(&Rfc3339).unwrap(),
                    "endsAt": ends_at.format(&Rfc3339).unwrap(),
                }),
                admin,
            )
            .await;
        assert_eq!(
            response.status(),
            StatusCode::CREATED,
            "event creation failed"
        );

        read_json(response).await
    }

    /// Publishes an event and reads its check-in code from the admin detail.
    pub async fn publish_event(&self, admin: &str, event: EventId) -> EventDetail {
        let response = self
            .post_as(
                &format!("/api/admin/events/{event}/status"),
                &json!({ "status": "published" }),
                admin,
            )
            .await;
        assert_eq!(response.status(), StatusCode::OK, "publishing failed");
        let response = self
            .get_as(&format!("/api/admin/events/{event}"), admin)
            .await;
        assert_eq!(response.status(), StatusCode::OK);

        read_json(response).await
    }

    pub async fn get(&self, path: &str) -> Response {
        self.send(Request::get(path).body(Body::empty()).unwrap())
            .await
    }

    pub async fn get_as(&self, path: &str, token: &str) -> Response {
        self.send(with_bearer(
            Request::get(path).body(Body::empty()).unwrap(),
            token,
        ))
        .await
    }

    pub async fn post(&self, path: &str, body: &Value) -> Response {
        self.send(json_request(Method::POST, path, body)).await
    }

    pub async fn post_as(&self, path: &str, body: &Value, token: &str) -> Response {
        self.send(with_bearer(json_request(Method::POST, path, body), token))
            .await
    }

    pub async fn put(&self, path: &str, body: &Value) -> Response {
        self.send(json_request(Method::PUT, path, body)).await
    }

    pub async fn put_as(&self, path: &str, body: &Value, token: &str) -> Response {
        self.send(with_bearer(json_request(Method::PUT, path, body), token))
            .await
    }

    pub async fn patch_as(&self, path: &str, body: &Value, token: &str) -> Response {
        self.send(with_bearer(json_request(Method::PATCH, path, body), token))
            .await
    }

    pub async fn delete_as(&self, path: &str, token: &str) -> Response {
        self.send(with_bearer(
            Request::delete(path).body(Body::empty()).unwrap(),
            token,
        ))
        .await
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

/// A fresh SQLite file in a temporary directory, with every migration applied.
/// The directory goes when the returned guard drops.
pub async fn migrated_pool() -> (TempDir, DbPool) {
    let dir = tempfile::tempdir().unwrap();
    let config = DbConfig {
        url: format!("sqlite://{}?mode=rwc", dir.path().join("test.db").display()),
        max_connections: MaxConnections::try_new(4).unwrap(),
    };
    let pool = connect(&config).unwrap();
    run_pending_migrations(&pool).await.unwrap();

    (dir, pool)
}

fn json_request(method: Method, path: &str, body: &Value) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(path)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(serde_json::to_vec(body).unwrap()))
        .unwrap()
}

fn with_bearer(mut request: Request<Body>, token: &str) -> Request<Body> {
    request.headers_mut().insert(
        header::AUTHORIZATION,
        format!("Bearer {token}").parse().unwrap(),
    );
    request
}
