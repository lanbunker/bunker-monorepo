use std::process::ExitCode;

use bunker_api::server;

#[tokio::main]
async fn main() -> ExitCode {
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
