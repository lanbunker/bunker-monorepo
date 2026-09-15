//! The cabinet binary. `run` starts the conductor from `cabd-core` and gives it
//! the SDL2 screen. `screenshots` renders every screen to PNG files, so the
//! output can be reviewed without a display.

mod screen;

use std::path::PathBuf;

use cabd_core::config::Config;
use cabd_core::view::{Orientation, OverscanPercent, ScreenConfig};
use clap::{Parser, Subcommand};
use tracing::info;
use tracing_subscriber::EnvFilter;

#[derive(Debug, Parser)]
#[command(name = "bunker-cabd", version, about = "The LAN BUNKER arcade cabinet")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Run the cabinet.
    Run(Config),
    /// Render every screen from sample data into PNG files in a folder.
    Screenshots {
        out: PathBuf,
        #[arg(long, default_value = "landscape")]
        orientation: Orientation,
        #[arg(long, default_value = "0")]
        overscan_percent: OverscanPercent,
    },
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_target(false)
        .with_thread_names(true)
        .init();
    info!(
        version = env!("CARGO_PKG_VERSION"),
        profile = build_profile(),
        "bunker-cabd"
    );

    match Cli::parse().command {
        Command::Run(config) => {
            let screen_config = config.screen();
            let handle = cabd_core::start(config)?;
            screen::run(screen_config, handle)?;
        }
        Command::Screenshots {
            out,
            orientation,
            overscan_percent,
        } => {
            screen::screenshots(
                &out,
                ScreenConfig {
                    orientation,
                    overscan_percent,
                    window: None,
                    upright: false,
                },
            )?;
        }
    }
    Ok(())
}

fn build_profile() -> &'static str {
    if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    }
}
