//! The HTTP boundary. It renders errors, parses input, checks bearer tokens and
//! traces requests. No item here knows what a player is.

mod api_error;
mod auth;
mod extract;
mod timeout;
mod trace;

pub use api_error::{ApiError, ApiErrorBody, method_not_allowed, render_errors, route_not_found};
pub use auth::{AdminOnly, Authenticated};
pub use extract::{ValidJson, ValidPath, ValidQuery};
pub use timeout::enforce_timeout;
pub use trace::{RequestTraceLayer, propagate_request_id, set_request_id, trace_requests};
