//! Domain types shared by the API, the cabinet daemon and every future Rust
//! client. No I/O, no HTTP, no SQL.
//!
//! `nutype` declares each constraint one time and makes the constructor, the
//! error type and the serde code. A value that crosses a boundary one time is
//! valid everywhere below.

mod auth;
mod glyph;
mod handle;
mod pagination;
mod player;
mod role;
mod schema;

pub use auth::{
    LoginRequest, PASSWORD_MAX_LEN, PASSWORD_MIN_LEN, Password, PasswordChange, PasswordError,
    SignupRequest, TemporaryPassword, TokenResponse,
};
pub use glyph::{
    GLYPH_CELLS, GLYPH_COLORS, GLYPH_SIZE, Glyph, GlyphBits, GlyphBitsError, GlyphColor,
    generate_glyph,
};
pub use handle::{HANDLE_MAX_LEN, HANDLE_MIN_LEN, Handle, HandleChange, HandleError};
pub use pagination::{
    PAGE_SIZE_MAX, PageNumber, PageNumberError, PageQuery, PageSize, PageSizeError, Paginated,
};
pub use player::{Account, Player, PlayerId};
pub use role::{Role, RoleUpdate, UnknownRole};
