//! The cabinet binary. `run` starts the conductor from `cabd-core` and gives it
//! the SDL2 screen. `screenshots` renders every screen to PNG files, so the
//! output can be reviewed without a display.

mod screen;

use std::path::PathBuf;

use cabd_core::config::Config;
use cabd_core::view::{DisplayAspect, Orientation, OverscanPercent, ScreenConfig, TateTurn};
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
        /// The shape of the display, such as `4:3`. Only the canvas shape
        /// changes, because a PNG has square pixels.
        #[arg(long)]
        display_aspect: Option<DisplayAspect>,
    },
}

fn main() -> anyhow::Result<()> {
    let filter = match EnvFilter::try_from_default_env() {
        Ok(filter) => filter,
        Err(e) => {
            // The log is not set up yet, so this is the one place that prints
            // to stderr by hand.
            eprintln!("RUST_LOG is not valid ({e}), using `info`");
            EnvFilter::new("info")
        }
    };
    tracing_subscriber::fmt()
        .with_env_filter(filter)
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
            let handle = screen::run(screen_config, handle)?;
            handle.join()?;
        }
        Command::Screenshots {
            out,
            orientation,
            overscan_percent,
            display_aspect,
        } => {
            screen::screenshots(
                &out,
                ScreenConfig {
                    orientation,
                    tate_turn: TateTurn::Left,
                    display_aspect,
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
