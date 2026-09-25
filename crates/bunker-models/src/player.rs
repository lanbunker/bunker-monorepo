use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use utoipa::ToSchema;

use super::glyph::Glyph;
use super::handle::Handle;
use super::id::uuid_id;
use super::points::Standing;
use super::role::Role;

uuid_id!(PlayerId);

/// What `/api/me` answers: the public player plus what only the owner sees.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    pub player: Player,
    /// Set after an admin reset. Until the player picks a new password, every
    /// route but `/api/me` and `/api/me/password` answers `PasswordChangeRequired`.
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
    /// Cycles, rank and place. Derived from the ledger on every read, so it is
    /// never stale.
    pub standing: Standing,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub created_at: OffsetDateTime,
}
