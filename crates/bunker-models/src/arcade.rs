use nutype::nutype;

pub const ROM_NAME_MAX_LEN: usize = 32;

pub const GAME_TITLE_MAX_LEN: usize = 40;

/// The largest score the system stores. It fits an SQLite integer and the
/// exact range of a JSON number, and no arcade game reaches it.
pub const SCORE_MAX: u64 = 999_999_999_999;

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

/// The title of a game as a screen shows it, such as `Bubble Bobble`.
#[nutype(
    sanitize(trim),
    validate(len_char_min = 1, len_char_max = GAME_TITLE_MAX_LEN),
    derive(Debug, Clone, PartialEq, Eq, Hash, Display, AsRef, Serialize, Deserialize)
)]
pub struct GameTitle(String);

/// A score as the game counts it, bounded by `SCORE_MAX`.
#[nutype(
    validate(less_or_equal = SCORE_MAX),
    derive(
        Debug,
        Clone,
        Copy,
        PartialEq,
        Eq,
        PartialOrd,
        Ord,
        Hash,
        Display,
        Serialize,
        Deserialize
    )
)]
pub struct Score(u64);
