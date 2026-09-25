use std::error::Error;

use axum::Json;
use axum::extract::Request;
use axum::http::StatusCode;
use axum::http::header::{CONTENT_LENGTH, CONTENT_TYPE};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use utoipa::ToSchema;

use crate::services::{ErrorCode, ServiceError};

/// A failure that goes to a client. A handler returns it through `?`, so a route
/// builds no error response by hand.
///
/// It renders the code it gets. `services` declares which codes exist, and
/// `server.rs` gives each code a status.
#[derive(Debug, Clone)]
pub struct ApiError {
    code: ErrorCode,
    message: String,
    cause: Option<String>,
}

/// The one shape every failure has on the wire.
#[derive(Debug, Serialize, ToSchema)]
pub struct ApiErrorBody<'error> {
    code: ErrorCode,
    message: &'error str,
    status: u16,
    /// The chain of causes. It is absent if a user can see the response.
    #[serde(skip_serializing_if = "Option::is_none")]
    cause: Option<&'error str>,
}

impl ApiError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            cause: None,
        }
    }

    /// Adds `error` and the causes below it. The log always holds them, and a
    /// client sees them only if the configuration permits it. The client
    /// message is a fixed sentence, so the chain is where the ids and the driver
    /// text survive.
    pub fn with_cause(mut self, error: &dyn Error) -> Self {
        let mut chain = vec![error.to_string()];
        let mut current = error.source();
        while let Some(cause) = current {
            chain.push(cause.to_string());
            current = cause.source();
        }
        self.cause = Some(chain.join(": "));

        self
    }

    pub fn status(&self) -> StatusCode {
        self.code.into()
    }

    fn render(&self, verbose: bool) -> Response {
        let status = self.status();

        (
            status,
            Json(ApiErrorBody {
                code: self.code,
                message: &self.message,
                status: status.as_u16(),
                cause: if verbose { self.cause.as_deref() } else { None },
            }),
        )
            .into_response()
    }
}

/// Logs the failure and puts it on the response for [`render_errors`].
///
/// The cause waits, because the permission to show a cause is configuration and
/// `IntoResponse` cannot read it. An extension moves the error to the wiring, and
/// keeps a process-wide global out of the code.
impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = self.status();

        if status.is_server_error() {
            tracing::error!(
                code = ?self.code,
                message = %self.message,
                cause = self.cause.as_deref(),
                "request failed"
            );
        } else if is_access_refusal(self.code) {
            // An operator watches these for a guessing attack. The code is
            // enough, and a handle or a token would put a secret in the log.
            tracing::info!(code = ?self.code, "access refused");
        } else {
            tracing::debug!(
                code = ?self.code,
                message = %self.message,
                "request rejected"
            );
        }

        let mut response = self.render(false);
        response.extensions_mut().insert(self);
        response
    }
}

/// Serializes the [`ApiError`] from a handler, and adds the cause if `verbose` is
/// true. `server.rs` installs it one time.
pub async fn render_errors(verbose: bool, request: Request, next: Next) -> Response {
    let response = next.run(request).await;

    if !verbose || response.status().is_success() {
        return response;
    }

    let Some(error) = response.extensions().get::<ApiError>() else {
        return response;
    };
    // A route layer can set a header on the error, such as `Cache-Control`, and
    // the new body must keep it. The length and the type are the new body's.
    let mut rendered = error.render(true);
    for (name, value) in response.headers() {
        if name != CONTENT_LENGTH && name != CONTENT_TYPE {
            rendered.headers_mut().append(name.clone(), value.clone());
        }
    }

    rendered
}

/// This lets a handler write `?` and touch no error.
impl From<ServiceError> for ApiError {
    fn from(error: ServiceError) -> Self {
        let (code, message) = error.public();

        Self::new(code, message).with_cause(&error)
    }
}

pub async fn route_not_found() -> ApiError {
    ApiError::new(
        ErrorCode::RouteNotFound,
        "The requested route is invalid or not mounted",
    )
}

/// Without this, axum answers `405` with an empty body.
pub async fn method_not_allowed() -> ApiError {
    ApiError::new(
        ErrorCode::MethodNotAllowed,
        "The requested method is not allowed for this route",
    )
}

/// A refused login or a refused token. `Forbidden` is here too: a user who
/// tries every admin route is worth a line.
const fn is_access_refusal(code: ErrorCode) -> bool {
    matches!(
        code,
        ErrorCode::InvalidCredentials
            | ErrorCode::Unauthorized
            | ErrorCode::Forbidden
            | ErrorCode::PasswordChangeRequired
    )
}
