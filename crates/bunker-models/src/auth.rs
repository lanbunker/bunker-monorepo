use std::fmt;

use nutype::nutype;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use super::handle::Handle;

pub const PASSWORD_MIN_LEN: usize = 8;

/// Argon2 hashes any length, but a megabyte of password is a denial of service and
/// not a credential.
pub const PASSWORD_MAX_LEN: usize = 128;

/// A password as a client sends it. It is never stored and never logged: `Debug`
/// and `Display` are not derived, and the wrapper below redacts it.
#[nutype(
    validate(len_char_min = PASSWORD_MIN_LEN, len_char_max = PASSWORD_MAX_LEN),
    derive(Clone, PartialEq, Eq, AsRef, Deserialize)
)]
pub struct Password(String);

impl fmt::Debug for Password {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Password(<redacted>)")
    }
}

/// Request body of `POST /api/auth/signup`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SignupRequest {
    pub handle: Handle,
    pub password: Password,
}

/// Request body of `POST /api/auth/login`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LoginRequest {
    pub handle: Handle,
    pub password: Password,
}

/// The answer to a signup or a login. The token is a bearer JWT.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenResponse {
    pub token: String,
    #[serde(with = "time::serde::rfc3339")]
    pub expires_at: OffsetDateTime,
}
