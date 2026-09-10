use nutype::nutype;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

use super::glyph::Glyph;
use super::handle::Handle;

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

/// A player, as the API shows it to anyone. It holds no credential.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Player {
    pub id: PlayerId,
    pub handle: Handle,
    pub glyph: Glyph,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
}
