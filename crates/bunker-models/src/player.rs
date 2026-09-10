use nutype::nutype;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use utoipa::ToSchema;
use uuid::Uuid;

use super::glyph::Glyph;
use super::handle::Handle;
use super::role::Role;

/// Differs from each other identifier, so an identifier of the wrong type is a
/// compile error and not an empty lookup.
#[nutype(derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    Display,
    Serialize,
    Deserialize
))]
pub struct PlayerId(Uuid);

impl PlayerId {
    pub fn generate() -> Self {
        Self::new(Uuid::new_v4())
    }
}

/// What `/api/me` answers: the public player plus what only the owner sees.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    pub player: Player,
    /// Set after an admin reset. The API only reports it: the site forces the
    /// change before it shows any other page.
    pub must_change_password: bool,
}

/// A player, as the API shows it to anyone. It holds no credential.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Player {
    pub id: PlayerId,
    pub handle: Handle,
    pub glyph: Glyph,
    pub role: Role,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub created_at: OffsetDateTime,
}
