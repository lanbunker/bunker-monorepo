use std::time::Duration;

use axum::extract::Request;
use axum::middleware::Next;
use axum::response::{IntoResponse as _, Response};

use crate::services::ErrorCode;

use super::api_error::ApiError;

/// Stops a request that runs longer than `limit`.
///
/// This is written by hand, because `tower_http::TimeoutLayer` answers with a
/// status and no body.
pub async fn enforce_timeout(limit: Duration, request: Request, next: Next) -> Response {
    match tokio::time::timeout(limit, next.run(request)).await {
        Ok(response) => response,
        Err(_elapsed) => {
            tracing::error!(seconds = limit.as_secs(), "request timed out");

            // Not 408. A 408 means the client was too slow, and a client retries
            // it without help. Here the server stops work that it accepted.
            ApiError::new(
                ErrorCode::ServiceUnavailable,
                "The request took too long to process",
            )
            .into_response()
        }
    }
}
