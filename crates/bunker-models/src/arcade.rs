use nutype::nutype;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

pub const ROM_NAME_MAX_LEN: usize = 32;

/// The name of an arcade ROM set as MAME and FBNeo know it, such as `bublbobl`.
/// Lowercase letters, digits and `_` only, so the name is safe as a file stem, in
/// a URL path and in a log line.
#[nutype(
    sanitize(trim),
    validate(
        len_char_min = 1,
        len_char_max = ROM_NAME_MAX_LEN,
        predicate = is_rom_charset
    ),
    derive(Debug, Clone, PartialEq, Eq, Hash, Display, AsRef, Serialize, Deserialize)
)]
pub struct RomName(String);

fn is_rom_charset(value: &str) -> bool {
    value
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// A score as the game counts it. Some shooters pass the range of `u32`.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, ToSchema,
)]
#[serde(transparent)]
pub struct Score(pub u64);
