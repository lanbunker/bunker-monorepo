use axum::http::HeaderValue;
use axum::http::header::CACHE_CONTROL;
use tower_http::set_header::SetResponseHeaderLayer;

/// For a route whose answer carries a token, a temporary password or the
/// caller's own account. No cache between the site and the API may keep one.
pub fn no_store() -> SetResponseHeaderLayer<HeaderValue> {
    SetResponseHeaderLayer::overriding(CACHE_CONTROL, HeaderValue::from_static("no-store"))
}
