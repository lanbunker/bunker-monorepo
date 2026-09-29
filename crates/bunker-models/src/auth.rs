use std::fmt;

use nutype::nutype;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use utoipa::ToSchema;

use super::handle::Handle;

pub const PASSWORD_MIN_LEN: usize = 8;

/// Argon2 hashes any length, but a megabyte of password is a denial of service and
/// not a credential.
pub const PASSWORD_MAX_LEN: usize = 128;

/// A password as a client sends it. `Debug` is written by hand and redacts it, and
/// there is no `Display`.
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
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SignupRequest {
    pub handle: Handle,
    pub password: Password,
}

/// Request body of `POST /api/auth/login`.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LoginRequest {
    pub handle: Handle,
    pub password: Password,
}

/// Request body of `POST /api/me/password`.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PasswordChange {
    pub current_password: Password,
    pub new_password: Password,
}

/// The answer to an admin password reset. Shown once, never stored in clear, and
/// never printed: `Debug` redacts it like [`Password`].
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TemporaryPassword {
    pub temporary_password: String,
}

impl fmt::Debug for TemporaryPassword {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("TemporaryPassword(<redacted>)")
    }
}

/// The answer to a signup, a login or a password change. The token is a bearer
/// JWT.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TokenResponse {
    pub token: String,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub expires_at: OffsetDateTime,
}

/// The token is a credential, so a `debug!` of the response cannot log it.
impl fmt::Debug for TokenResponse {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TokenResponse")
            .field("token", &"<redacted>")
            .field("expires_at", &self.expires_at)
            .finish()
    }
}
