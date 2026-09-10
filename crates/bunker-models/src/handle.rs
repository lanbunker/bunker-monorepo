use nutype::nutype;

pub const HANDLE_MIN_LEN: usize = 3;

pub const HANDLE_MAX_LEN: usize = 20;

/// A player name as shown everywhere. Letters, digits, `_`, `.` and `-` only, so a
/// handle is safe in a URL path and in a terminal.
///
/// Two handles that differ only in case are the same player. The database
/// enforces that with a case-insensitive unique index, and this type keeps the
/// case the player typed.
#[nutype(
    sanitize(trim),
    validate(
        len_char_min = HANDLE_MIN_LEN,
        len_char_max = HANDLE_MAX_LEN,
        predicate = is_handle_charset
    ),
    derive(Debug, Clone, PartialEq, Eq, Hash, Display, AsRef, Serialize, Deserialize)
)]
pub struct Handle(String);

fn is_handle_charset(value: &str) -> bool {
    value
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'))
}
