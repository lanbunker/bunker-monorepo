use std::error::Error;

use axum::Json;
use axum::extract::Request;
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use serde::Serialize;

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

#[derive(Debug, Serialize)]
struct ApiErrorBody<'error> {
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

    /// Adds the causes below `error`. The log always holds them, and a client sees
    /// them only if the configuration permits it.
    ///
    /// This skips the `Display` of `error`, because that text is the message. A
    /// `cause` that repeats the message helps nobody.
    pub fn with_cause(mut self, error: &dyn Error) -> Self {
        let mut chain = Vec::new();
        let mut current = error.source();
        while let Some(cause) = current {
            chain.push(cause.to_string());
            current = cause.source();
        }

        if !chain.is_empty() {
            self.cause = Some(chain.join(": "));
        }

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

    match response.extensions().get::<ApiError>() {
        Some(error) => error.render(true),
        None => response,
    }
}

/// This lets a handler write `?` and touch no error.
impl From<ServiceError> for ApiError {
    fn from(error: ServiceError) -> Self {
        let (code, public) = error.public();
        let message = public.map_or_else(|| error.to_string(), str::to_owned);

        Self::new(code, message).with_cause(&error)
    }
}

pub async fn route_not_found() -> ApiError {
    ApiError::new(
        ErrorCode::RouteNotFound,
        "The requested route is invalid or not mounted",
    )
}

/// Without this, axum answers `405` with an empty body. That is the one failure
/// that escapes the error contract.
pub async fn method_not_allowed() -> ApiError {
    ApiError::new(
        ErrorCode::MethodNotAllowed,
        "The requested method is not allowed for this route",
    )
}
