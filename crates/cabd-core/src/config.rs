//! The one place that reads the environment and `games.toml`. Every other
//! module receives a value from here and never looks at the environment.

use std::path::{Path, PathBuf};
use std::time::Duration;

use bunker_models::RomName;
use serde::Deserialize;

use crate::cabinet::{CabinetConfig, Game};
use crate::hiscore::Decoder;
use crate::launcher::{Launcher, LauncherError};
use crate::view::{Orientation, OverscanPercent, ScreenConfig, WindowSize};

/// Every value the binary needs, from flags or from `CABD_*` variables. No
/// path has a default, so a missing one fails at startup and names itself.
#[derive(Debug, Clone, clap::Args)]
pub struct Config {
    /// The games list.
    #[arg(long, env = "CABD_GAMES")]
    pub games: PathBuf,

    /// The folder with `<rom>.zip` files.
    #[arg(long, env = "CABD_ROM_DIR")]
    pub rom_dir: PathBuf,

    /// The command that runs a game. `{rom}` is the ROM path and `{hi}` the
    /// live hiscore file.
    #[arg(long, env = "CABD_LAUNCHER")]
    pub launcher: String,

    /// The folder where the core writes `<rom>.hi`.
    #[arg(long, env = "CABD_HISCORE_DIR")]
    pub hiscore_dir: PathBuf,

    /// The folder with the pristine `<rom>.hi` templates.
    #[arg(long, env = "CABD_TEMPLATE_DIR")]
    pub template_dir: PathBuf,

    /// Seconds without input before the screen returns to attract.
    #[arg(long, env = "CABD_IDLE_SECONDS", default_value_t = 180)]
    pub idle_seconds: u64,

    /// `landscape` or `tate`.
    #[arg(long, env = "CABD_ORIENTATION", default_value = "landscape")]
    pub orientation: Orientation,

    /// The safe-area inset on each side, as a percent of the short side, up
    /// to 25.
    #[arg(long, env = "CABD_OVERSCAN_PERCENT", default_value = "0")]
    pub overscan_percent: OverscanPercent,

    /// A window such as `960x720` instead of the full display. Development only.
    #[arg(long, env = "CABD_WINDOW")]
    pub window: Option<WindowSize>,

    /// Show the TATE canvas upright in a portrait window, instead of turned as
    /// the tube receives it. Needs `--window`. Development only.
    #[arg(long, requires = "window")]
    pub upright: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("cannot read the games list at {path}")]
    ReadGames {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("the games list at {path} is not valid")]
    ParseGames {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },
    #[error("the games list at {path} has no games")]
    NoGames { path: PathBuf },
    #[error("the launcher command is not valid")]
    Launcher(#[source] LauncherError),
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct GamesFile {
    games: Vec<GameEntry>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct GameEntry {
    rom: RomName,
    title: String,
    orientation: Orientation,
    decoder: Decoder,
}

impl Config {
    pub fn load_games(&self) -> Result<Vec<Game>, ConfigError> {
        load_games(&self.games)
    }

    pub fn cabinet(&self, games: Vec<Game>) -> CabinetConfig {
        CabinetConfig {
            games,
            idle: Duration::from_secs(self.idle_seconds),
        }
    }

    pub fn screen(&self) -> ScreenConfig {
        ScreenConfig {
            orientation: self.orientation,
            overscan_percent: self.overscan_percent,
            window: self.window,
            upright: self.upright,
        }
    }

    pub fn launcher(&self) -> Result<Launcher, ConfigError> {
        Launcher::new(
            &self.launcher,
            self.rom_dir.clone(),
            self.hiscore_dir.clone(),
            self.template_dir.clone(),
        )
        .map_err(ConfigError::Launcher)
    }
}

fn load_games(path: &Path) -> Result<Vec<Game>, ConfigError> {
    let text = std::fs::read_to_string(path).map_err(|source| ConfigError::ReadGames {
        path: path.to_owned(),
        source,
    })?;
    let file: GamesFile = toml::from_str(&text).map_err(|source| ConfigError::ParseGames {
        path: path.to_owned(),
        source,
    })?;
    if file.games.is_empty() {
        return Err(ConfigError::NoGames {
            path: path.to_owned(),
        });
    }
    Ok(file
        .games
        .into_iter()
        .map(|entry| Game {
            rom: entry.rom,
            title: entry.title,
            orientation: entry.orientation,
            decoder: entry.decoder,
        })
        .collect())
}
