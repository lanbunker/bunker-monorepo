use std::sync::Arc;

use axum::extract::{Request, State};
use axum::http::HeaderName;
use axum::middleware::Next;
use axum::response::Response;

use crate::config::ApiKey;
use crate::services::ErrorCode;

use super::api_error::ApiError;

/// The header that carries the shared API key.
pub const API_KEY: HeaderName = HeaderName::from_static("x-api-key");

/// Refuses a request without the shared key, before any handler runs.
pub async fn require_api_key(
    State(key): State<Arc<ApiKey>>,
    request: Request,
    next: Next,
) -> Result<Response, ApiError> {
    let known = request
        .headers()
        .get(&API_KEY)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|candidate| key.matches(candidate));
    if !known {
        return Err(ApiError::new(
            ErrorCode::ApiKeyRequired,
            "This API needs a valid X-Api-Key header",
        ));
    }

    Ok(next.run(request).await)
}
