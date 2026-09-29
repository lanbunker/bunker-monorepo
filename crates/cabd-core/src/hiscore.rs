//! Decoding of a `.hi` file into a score. A `.hi` file is the raw
//! concatenation of the RAM ranges that `hiscore.dat` lists for the game, with
//! no header, so each game needs its own decoder.

use bunker_models::{SCORE_MAX, Score};
use serde::Deserialize;

/// How the bytes of a `.hi` file become a score. Named in `games.toml`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Decoder {
    /// The score as decimal digits in ASCII. Only the fake launcher for
    /// development writes this. No arcade game does.
    AsciiDecimal,
}

#[derive(Debug, thiserror::Error)]
pub enum HiscoreError {
    #[error("the hiscore file is empty")]
    Empty,
    #[error("the hiscore bytes are not decimal digits")]
    NotDecimal,
    #[error("the score is above the limit of {SCORE_MAX}")]
    TooLarge,
}

/// The score in `bytes`, read the way `decoder` says.
pub fn decode(decoder: Decoder, bytes: &[u8]) -> Result<Score, HiscoreError> {
    match decoder {
        Decoder::AsciiDecimal => decode_ascii_decimal(bytes),
    }
}

fn decode_ascii_decimal(bytes: &[u8]) -> Result<Score, HiscoreError> {
    let text = str::from_utf8(bytes).map_err(|_| HiscoreError::NotDecimal)?;
    let text = text.trim();
    if text.is_empty() {
        return Err(HiscoreError::Empty);
    }
    if !text.bytes().all(|b| b.is_ascii_digit()) {
        return Err(HiscoreError::NotDecimal);
    }
    let value: u64 = text.parse().map_err(|_| HiscoreError::TooLarge)?;
    Score::try_new(value).map_err(|_| HiscoreError::TooLarge)
}
