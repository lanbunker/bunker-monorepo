use std::process::ExitCode;

use bunker_api::routers::ApiDoc;
use bunker_api::server;
use utoipa::OpenApi as _;

#[tokio::main]
async fn main() -> ExitCode {
    // `bunker-api openapi` prints the contract and exits. `make openapi` uses it.
    if std::env::args().nth(1).as_deref() == Some("openapi") {
        return match ApiDoc::openapi().to_pretty_json() {
            Ok(json) => {
                println!("{json}");
                ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("{error}");
                ExitCode::FAILURE
            }
        };
    }

    // A missing `.env` is normal: a deployment sets the variables directly.
    drop(dotenvy::dotenv());

    match server::run().await {
        Ok(()) => ExitCode::SUCCESS,
        // Startup can fail before telemetry starts, so write to stderr.
        Err(error) => {
            eprintln!("{error:?}");
            ExitCode::FAILURE
        }
    }
}
