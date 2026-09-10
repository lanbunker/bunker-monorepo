use std::str::FromStr;

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// What a player may do. Admins reach the backoffice and manage players.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    Admin,
}

impl Role {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Admin => "admin",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("unknown role `{0}`, expected user or admin")]
pub struct UnknownRole(String);

impl FromStr for Role {
    type Err = UnknownRole;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        match raw {
            "user" => Ok(Self::User),
            "admin" => Ok(Self::Admin),
            other => Err(UnknownRole(other.to_owned())),
        }
    }
}

/// Request body of `PATCH /api/admin/players/{id}`.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RoleUpdate {
    pub role: Role,
}
