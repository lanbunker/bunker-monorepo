//! The contract between the software and the screen. Everything here is data
//! that is ready to draw: strings, not numbers to format, and choices already
//! made. The screen imports this module and nothing else from the library.

use std::fmt;
use std::str::FromStr;

use bunker_models::Score;
use nutype::nutype;
use serde::{Deserialize, Serialize};

/// The short side of the logical canvas, in logical pixels. A 240-line CRT mode
/// shows it at scale 1, and every other display at an integer scale.
pub const CANVAS_SHORT_SIDE: u32 = 240;

/// The largest safe-area inset per side, in percent of the short side. Above
/// this the screens have no room left.
pub const OVERSCAN_PERCENT_MAX: u8 = 25;

/// What the screen shows. `Suspended` means the screen must give the display
/// away: destroy the window and answer with `Handle::display_released`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Frame {
    Show(ViewModel),
    Suspended,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ViewModel {
    pub screen: Screen,
    /// The handle of the checked-in player, or nothing in guest mode.
    pub session: Option<String>,
    /// The command on the prompt line, such as `./attract`.
    pub command: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Screen {
    Attract {
        /// The URL the QR code encodes. Nothing while no nonce is live.
        qr_payload: Option<String>,
        leaderboard: Vec<Row>,
    },
    Select {
        games: Vec<GameCard>,
        selected: usize,
    },
    Postgame {
        score_display: String,
        /// The line under the score: the rank, a new best, or why there is no
        /// score. Nothing when there is nothing to say.
        status: Option<Status>,
        leaderboard: Vec<Row>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GameCard {
    pub title: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub rank: u32,
    pub handle: String,
    pub score_display: String,
    /// This row is the run that just finished.
    pub highlight: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Status {
    pub text: String,
    pub tone: Tone,
}

/// Which of the site colors a line takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Ink,
    Warn,
    Dim,
}

/// A button or a direction, after the input map. The screen sends these and
/// decides nothing about what they mean.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Input {
    Up,
    Down,
    Left,
    Right,
    Confirm,
    Back,
}

/// Debug builds only: from a function key or the control socket.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DevCommand {
    Scenario(Scenario),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scenario {
    Attract,
    Select,
    Postgame,
}

/// Landscape is a tube in its normal position. Tate is a tube turned by 90
/// degrees, fed with the same landscape signal, so the content is rotated in
/// software on both sides.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Orientation {
    Landscape,
    Tate,
}

#[derive(Debug, thiserror::Error)]
#[error("unknown orientation `{0}`, use `landscape` or `tate`")]
pub struct OrientationError(String);

impl FromStr for Orientation {
    type Err = OrientationError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "landscape" => Ok(Self::Landscape),
            "tate" => Ok(Self::Tate),
            other => Err(OrientationError(other.to_owned())),
        }
    }
}

impl fmt::Display for Orientation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Landscape => f.write_str("landscape"),
            Self::Tate => f.write_str("tate"),
        }
    }
}

/// The inset of the safe area on each side, as a percent of the short side of
/// the canvas. A tube hides its outer edge. Read the value off the tube with
/// the patterns page.
#[nutype(
    validate(less_or_equal = OVERSCAN_PERCENT_MAX),
    derive(Debug, Clone, Copy, PartialEq, Eq, Display, FromStr)
)]
pub struct OverscanPercent(u8);

/// A window size for development on a desktop, written as `960x720`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowSize {
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, thiserror::Error)]
#[error("a window size is WIDTHxHEIGHT, such as 960x720, not `{0}`")]
pub struct WindowSizeError(String);

impl FromStr for WindowSize {
    type Err = WindowSizeError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let parse = || {
            let (w, h) = s.trim().split_once('x')?;
            let width = w.parse().ok().filter(|w| *w > 0)?;
            let height = h.parse().ok().filter(|h| *h > 0)?;
            Some(Self { width, height })
        };
        parse().ok_or_else(|| WindowSizeError(s.to_owned()))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScreenConfig {
    pub orientation: Orientation,
    pub overscan_percent: OverscanPercent,
    /// A window instead of the full display. Development only.
    pub window: Option<WindowSize>,
    /// Development only: show the TATE canvas upright in a portrait window,
    /// instead of turned as the tube receives it.
    pub upright: bool,
}

/// A score with a separator every three digits, the way a leaderboard reads it.
pub fn format_score(score: Score) -> String {
    let digits = score.0.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}
