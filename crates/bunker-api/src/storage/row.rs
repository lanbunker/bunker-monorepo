//! Conversions every row type needs. Each one names the table, so a malformed
//! value points at the place to look.

use bunker_models::{Glyph, GlyphBits, GlyphColor, Handle, Player, PlayerId};
use time::OffsetDateTime;
use uuid::Uuid;

use super::error::StorageError;

const PLAYERS: &str = "players";

/// A column value that the domain refuses and that has no error type of its own.
#[derive(Debug, thiserror::Error)]
#[error("column `{0}` holds a value outside the domain")]
pub(super) struct MalformedField(pub(super) &'static str);

/// Unix micros. One unit for every timestamp column, so a query can compare them.
pub(super) fn to_micros(table: &'static str, at: OffsetDateTime) -> Result<i64, StorageError> {
    let micros = at.unix_timestamp_nanos() / 1_000;
    i64::try_from(micros).map_err(|error| StorageError::malformed_row(table, error))
}

pub(super) fn from_micros(
    table: &'static str,
    micros: i64,
) -> Result<OffsetDateTime, StorageError> {
    OffsetDateTime::from_unix_timestamp_nanos(i128::from(micros) * 1_000)
        .map_err(|error| StorageError::malformed_row(table, error))
}

pub(super) fn parse_uuid(table: &'static str, raw: &str) -> Result<Uuid, StorageError> {
    Uuid::parse_str(raw).map_err(|error| StorageError::malformed_row(table, error))
}

/// The public columns of a player, as every query that joins players selects them.
#[derive(Debug)]
pub(super) struct PlayerRow {
    pub(super) id: String,
    pub(super) handle: String,
    pub(super) glyph_bits: i64,
    pub(super) glyph_color: String,
    pub(super) role: String,
    pub(super) created_at: i64,
}

impl TryFrom<PlayerRow> for Player {
    type Error = StorageError;

    fn try_from(row: PlayerRow) -> Result<Self, Self::Error> {
        let bits = u32::try_from(row.glyph_bits)
            .ok()
            .and_then(|raw| GlyphBits::try_new(raw).ok())
            .ok_or_else(|| StorageError::malformed_row(PLAYERS, MalformedField("glyph_bits")))?;
        let color = GlyphColor::from_hex(&row.glyph_color)
            .ok_or_else(|| StorageError::malformed_row(PLAYERS, MalformedField("glyph_color")))?;

        Ok(Self {
            id: PlayerId::new(parse_uuid(PLAYERS, &row.id)?),
            handle: Handle::try_new(row.handle)
                .map_err(|error| StorageError::malformed_row(PLAYERS, error))?,
            glyph: Glyph { bits, color },
            role: row
                .role
                .parse()
                .map_err(|error| StorageError::malformed_row(PLAYERS, error))?,
            created_at: from_micros(PLAYERS, row.created_at)?,
        })
    }
}
