use tracing_subscriber::EnvFilter;

use crate::config::AppConfig;

/// Installs the global subscriber. It writes JSON where a collector reads the
/// logs, and human-readable lines during development. `RUST_LOG` replaces the
/// filter.
///
/// A second call does nothing, so a test that builds more than one server is
/// safe.
pub fn init_tracing(config: AppConfig) {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(default_directives(config)));

    let builder = tracing_subscriber::fmt().with_env_filter(filter);

    let installed = if config.json_logs {
        builder
            .json()
            .flatten_event(true)
            .with_current_span(true)
            .with_span_list(false)
            .try_init()
    } else {
        builder.pretty().try_init()
    };

    drop(installed);
}

fn default_directives(config: AppConfig) -> String {
    let app_level = if config.debug_logs { "debug" } else { "info" };

    format!("bunker_api={app_level},tower_http=info,sqlx=warn,info")
}
