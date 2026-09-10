use std::time::Duration;

use axum::body::Body;
use axum::extract::Request;
use axum::http::{HeaderName, Response};
use tower_http::classify::{ServerErrorsAsFailures, SharedClassifier};
use tower_http::request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer};
use tower_http::trace::TraceLayer;
use tracing::{Level, Span};

const REQUEST_ID: HeaderName = HeaderName::from_static("x-request-id");

/// The longest identifier to accept from a caller. A header value can hold 400 KB,
/// and each event in the span writes the identifier again.
const REQUEST_ID_MAX_LEN: usize = 64;

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
/// renderer gets `request_id`, `method` and `path`, and no code passes them.
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
        .filter(|id| {
            !id.is_empty()
                && id.len() <= REQUEST_ID_MAX_LEN
                && id
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b"-_.".contains(&byte))
        })
        .unwrap_or("unknown");

    tracing::info_span!(
        "http.request",
        method = %request.method(),
        path = %request.uri().path(),
        request_id = %request_id,
        status = tracing::field::Empty,
    )
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
