//! Support code that is not domain logic: the HTTP wiring and the telemetry.

pub mod http;
mod telemetry;

pub use telemetry::init_tracing;
