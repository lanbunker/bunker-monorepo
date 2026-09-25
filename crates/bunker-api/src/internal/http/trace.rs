use std::time::Duration;

use axum::body::Body;
use axum::extract::{MatchedPath, Request};
use axum::http::{HeaderName, Response};
use axum::middleware::Next;
use tower_http::classify::{ServerErrorsAsFailures, SharedClassifier};
use tower_http::request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer};
use tower_http::trace::TraceLayer;
use tracing::{Level, Span};

const REQUEST_ID: HeaderName = HeaderName::from_static("x-request-id");

/// The longest identifier to accept from a caller. A header value can hold 400 KB,
/// and each event in the span writes the identifier again.
const REQUEST_ID_MAX_LEN: usize = 64;

/// Removes an `x-request-id` from the caller that is too long or holds other
/// characters than letters, digits, `-`, `_` and `.`. [`set_request_id`] then
/// makes a fresh one, so the id in the log and the id in the response agree.
pub async fn drop_invalid_request_id(mut request: Request, next: Next) -> axum::response::Response {
    let valid = request
        .headers()
        .get(&REQUEST_ID)
        .and_then(|value| value.to_str().ok())
        .is_some_and(is_valid_request_id);
    if !valid {
        request.headers_mut().remove(&REQUEST_ID);
    }

    next.run(request).await
}

/// Uses the `x-request-id` of the caller, or makes one. A request then keeps one
/// identity across the site and the API.
pub fn set_request_id() -> SetRequestIdLayer<MakeRequestUuid> {
    SetRequestIdLayer::new(REQUEST_ID, MakeRequestUuid)
}

/// Returns the identifier to the caller, so a bug report can find the log lines.
pub fn propagate_request_id() -> PropagateRequestIdLayer {
    PropagateRequestIdLayer::new(REQUEST_ID)
}

/// The layer that [`trace_requests`] builds. The callbacks are function pointers,
/// and not closures, because a closure type has no name.
pub type RequestTraceLayer = TraceLayer<
    SharedClassifier<ServerErrorsAsFailures>,
    MakeSpanFn,
    OnRequestFn,
    OnResponseFn,
    (),
    (),
    (),
>;

type MakeSpanFn = fn(&Request) -> Span;
type OnRequestFn = fn(&Request, &Span);
type OnResponseFn = fn(&Response<Body>, Duration, &Span);

/// Opens one span per request. Each log from a handler, a service or the error
/// renderer gets `request_id`, `method` and `route`, and no code passes them.
/// `route` is the template, such as `/api/checkin/{code}`: the raw path would
/// put a check-in code in the journal.
pub fn trace_requests() -> RequestTraceLayer {
    TraceLayer::new_for_http()
        .make_span_with(make_span as MakeSpanFn)
        .on_request(on_request as OnRequestFn)
        .on_response(on_response as OnResponseFn)
        .on_body_chunk(())
        .on_eos(())
        .on_failure(())
}

fn make_span(request: &Request) -> Span {
    let request_id = request
        .headers()
        .get(&REQUEST_ID)
        .and_then(|value| value.to_str().ok())
        .filter(|id| is_valid_request_id(id))
        .unwrap_or("unknown");
    let route = request
        .extensions()
        .get::<MatchedPath>()
        .map_or("unmatched", MatchedPath::as_str);

    tracing::info_span!(
        "http.request",
        method = %request.method(),
        route = %route,
        request_id = %request_id,
        status = tracing::field::Empty,
    )
}

fn is_valid_request_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= REQUEST_ID_MAX_LEN
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"-_.".contains(&byte))
}

fn on_request(_request: &Request, _span: &Span) {
    tracing::debug!("request received");
}

fn on_response(response: &Response<Body>, latency: Duration, span: &Span) {
    let status = response.status();
    span.record("status", status.as_u16());

    tracing::event!(
        Level::INFO,
        status = status.as_u16(),
        latency_ms = latency.as_millis(),
        "request completed"
    );
}
