//! The composition root. It reads the environment, connects the layers and
//! serves. Each item that knows the full application is here, with the one table
//! that makes a status from an error code.

use std::time::Duration;

use anyhow::Context as _;
use axum::http::StatusCode;
use axum::{Router, middleware};
use tokio::net::TcpListener;
use tokio::signal::unix::{SignalKind, signal};
use tower_http::catch_panic::CatchPanicLayer;
use tower_http::cors::CorsLayer;

use crate::config::{AppConfig, Env, JwtSecret, resolve_app_config};
use crate::internal::http::{
    ApiError, enforce_timeout, method_not_allowed, propagate_request_id, render_errors,
    route_not_found, set_request_id, trace_requests,
};
use crate::internal::init_tracing;
use crate::routers::{
    AppState, admin_router, admin_tournament_router, auth_router, health_router, openapi_router,
    player_router, tournament_router,
};
use crate::services::{
    AuthService, ErrorCode, PasswordHasher, PlayerService, TokenIssuer, TournamentService,
};
use crate::storage::{DbPool, PlayerStorage, TournamentStorage, connect, run_pending_migrations};

/// Without this limit, a stopped request holds a connection and a task for ever.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// How long a token stays valid. A LAN community logs in from a phone at an
/// event and should not type a password every night.
const TOKEN_TTL: Duration = Duration::from_secs(30 * 24 * 60 * 60);

/// Everything the router needs besides the database. The tests build it by hand
/// with a known secret.
#[derive(Clone)]
pub struct Secrets {
    pub jwt_secret: JwtSecret,
}

impl std::fmt::Debug for Secrets {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("Secrets(<redacted>)")
    }
}

/// Connects storage to services to routes, by hand. There is no container and no
/// reflection.
pub fn build_router(pool: DbPool, config: AppConfig, secrets: &Secrets) -> Router {
    let players = PlayerStorage::new(pool.clone());
    let tokens = TokenIssuer::new(secrets.jwt_secret.as_ref().as_bytes(), TOKEN_TTL);
    let hasher = PasswordHasher::new(config.fast_password_hash);

    let state = AppState {
        auth: AuthService::new(players.clone(), tokens, hasher),
        tournaments: TournamentService::new(TournamentStorage::new(pool), players.clone()),
        players: PlayerService::new(players),
    };

    let verbose_errors = config.verbose_errors;

    Router::new()
        .merge(auth_router())
        .merge(player_router())
        .merge(admin_router())
        .merge(tournament_router())
        .merge(admin_tournament_router())
        .merge(health_router())
        .merge(openapi_router())
        .fallback(route_not_found)
        .method_not_allowed_fallback(method_not_allowed)
        // Innermost first. `propagate_request_id` is above the three layers that
        // can replace a response, so the header stays after a replacement.
        .layer(middleware::from_fn(move |request, next| {
            render_errors(verbose_errors, request, next)
        }))
        .layer(CatchPanicLayer::custom(catch_panic))
        .layer(middleware::from_fn(|request, next| {
            enforce_timeout(REQUEST_TIMEOUT, request, next)
        }))
        .layer(propagate_request_id())
        .layer(trace_requests())
        // Outside the trace layer, so the id exists when the span opens.
        .layer(set_request_id())
        // The browser never calls this API directly: the Astro site proxies each
        // call server side, and the API holds no cookie. Tighten this before any
        // browser client talks here.
        .layer(CorsLayer::permissive())
        .with_state(state)
}

/// Each code, and the status it answers with. The match is exhaustive, so a new
/// [`ErrorCode`] cannot reach a client without a status.
impl From<ErrorCode> for StatusCode {
    fn from(code: ErrorCode) -> Self {
        match code {
            ErrorCode::GenericError => Self::INTERNAL_SERVER_ERROR,
            ErrorCode::ServiceUnavailable => Self::SERVICE_UNAVAILABLE,
            ErrorCode::ItemNotFound | ErrorCode::RouteNotFound => Self::NOT_FOUND,
            ErrorCode::HandleTaken | ErrorCode::RegistrationClosed | ErrorCode::InvalidState => {
                Self::CONFLICT
            }
            ErrorCode::NotAnEntrant => Self::UNPROCESSABLE_ENTITY,
            ErrorCode::InvalidCredentials | ErrorCode::Unauthorized => Self::UNAUTHORIZED,
            ErrorCode::Forbidden => Self::FORBIDDEN,
            ErrorCode::WrongPassword => Self::BAD_REQUEST,
            ErrorCode::InvalidRequest => Self::BAD_REQUEST,
            ErrorCode::UnprocessableRequest => Self::UNPROCESSABLE_ENTITY,
            ErrorCode::UnsupportedMediaType => Self::UNSUPPORTED_MEDIA_TYPE,
            ErrorCode::PayloadTooLarge => Self::PAYLOAD_TOO_LARGE,
            ErrorCode::MethodNotAllowed => Self::METHOD_NOT_ALLOWED,
        }
    }
}

/// Without this, a panic closes the connection with no response. That is the one
/// way a request can escape the error body.
fn catch_panic(panic: Box<dyn std::any::Any + Send + 'static>) -> axum::response::Response {
    use axum::response::IntoResponse as _;

    // A panic here comes from something that the `panic` and `unwrap_used` lints
    // cannot stop, such as an `expect` in a dependency or an arithmetic overflow.
    let detail = panic
        .downcast_ref::<&str>()
        .map(|message| (*message).to_owned())
        .or_else(|| panic.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "no message".to_owned());

    tracing::error!(detail, "a handler panicked");

    ApiError::new(ErrorCode::GenericError, "An unexpected error occurred").into_response()
}

/// Starts the server. It returns after the shutdown.
pub async fn run() -> anyhow::Result<()> {
    let env = Env::from_env().context("invalid environment configuration")?;
    let config = resolve_app_config(env.app_env);

    init_tracing(config);

    let pool = connect(&env.database).context("could not open the database")?;

    if config.migrate_on_startup {
        run_pending_migrations(&pool)
            .await
            .context("could not apply migrations")?;
    }

    let address = format!("{}:{}", env.bind_address, env.port);
    let listener = TcpListener::bind(&address)
        .await
        .with_context(|| format!("could not bind to {address}"))?;

    tracing::info!(%address, app_env = %config.app_env, "server started");

    let secrets = Secrets {
        jwt_secret: env.jwt_secret,
    };

    axum::serve(listener, build_router(pool, config, &secrets))
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("the HTTP server failed")
}

/// Returns on SIGTERM or SIGINT, so the server can complete the requests it
/// holds. systemd sends SIGTERM.
///
/// If a handler cannot install, this waits for ever. A return would stop the
/// server before it accepts one request.
async fn shutdown_signal() {
    let signalled = |kind: SignalKind, name: &'static str| async move {
        match signal(kind) {
            Ok(mut stream) => {
                let _received = stream.recv().await;
                tracing::info!("{name} received, draining");
            }
            Err(error) => {
                tracing::error!(%error, "could not listen for {name}");
                std::future::pending::<()>().await;
            }
        }
    };

    tokio::select! {
        () = signalled(SignalKind::terminate(), "SIGTERM") => {}
        () = signalled(SignalKind::interrupt(), "SIGINT") => {}
    }
}
