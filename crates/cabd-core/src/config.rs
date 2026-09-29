//! The one place that reads the environment and `games.toml`. Every other
//! module receives a value from here and never looks at the environment.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

use bunker_models::{GameTitle, RomName};
use serde::Deserialize;

use crate::cabinet::{CabinetConfig, Game};
use crate::hiscore::Decoder;
use crate::launcher::{Launcher, LauncherError};
use crate::view::{
    DisplayAspect, Orientation, OverscanPercent, ScreenConfig, TateTurn, WindowSize,
};

/// The shortest idle timeout. Below this the select screen goes away while a
/// player reads it.
const IDLE_SECONDS_MIN: u64 = 10;
const IDLE_SECONDS_MAX: u64 = 3600;

/// Every value the binary needs, from flags or from `CABD_*` variables. No
/// path has a default, so a missing one fails at startup and names its flag.
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

    /// Seconds without input before the screen returns to attract, 10 to 3600.
    #[arg(long, env = "CABD_IDLE_SECONDS", default_value_t = 180, value_parser = clap::value_parser!(u64).range(IDLE_SECONDS_MIN..=IDLE_SECONDS_MAX))]
    pub idle_seconds: u64,

    /// `landscape` or `tate`.
    #[arg(long, env = "CABD_ORIENTATION", default_value = "landscape")]
    pub orientation: Orientation,

    /// Which way the tube was turned for TATE, `left` or `right`.
    #[arg(long, env = "CABD_TATE_TURN", default_value = "left")]
    pub tate_turn: TateTurn,

    /// The shape of the display, such as `4:3`, when its pixels are not
    /// square. Composite modes need it. Leave it unset on a flat screen.
    #[arg(long, env = "CABD_DISPLAY_ASPECT")]
    pub display_aspect: Option<DisplayAspect>,

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
    #[error("the games list at {path} names `{rom}` two times")]
    DuplicateRom { path: PathBuf, rom: RomName },
    #[error("the launcher command is not valid")]
    Launcher(#[source] LauncherError),
}

impl Config {
    /// The games from `games.toml`, in file order.
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
            tate_turn: self.tate_turn,
            display_aspect: self.display_aspect,
            overscan_percent: self.overscan_percent,
            window: self.window,
            upright: self.upright,
        }
    }

    pub fn launcher(&self) -> Result<Launcher, ConfigError> {
        Launcher::try_new(
            &self.launcher,
            self.rom_dir.clone(),
            self.hiscore_dir.clone(),
            self.template_dir.clone(),
        )
        .map_err(ConfigError::Launcher)
    }
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
    title: GameTitle,
    orientation: Orientation,
    decoder: Decoder,
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
    let mut seen = HashSet::new();
    let mut games = Vec::with_capacity(file.games.len());
    for entry in file.games {
        if !seen.insert(entry.rom.clone()) {
            return Err(ConfigError::DuplicateRom {
                path: path.to_owned(),
                rom: entry.rom,
            });
        }
        games.push(Game {
            rom: entry.rom,
            title: entry.title,
            orientation: entry.orientation,
            decoder: entry.decoder,
        });
    }
    Ok(games)
}
