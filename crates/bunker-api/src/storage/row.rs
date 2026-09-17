//! Conversions every row type needs. Each one names the table, so a malformed
//! value points at the place to look.

use bunker_models::{Glyph, GlyphBits, GlyphColor, Handle, Player, PlayerId, Standing};
use time::macros::format_description;
use time::{Date, OffsetDateTime};
use uuid::Uuid;

use super::error::StorageError;

const PLAYERS: &str = "players";

/// A date column holds an ISO day. This is the one place that spells the format.
pub(super) const DAY: &[time::format_description::BorrowedFormatItem<'_>] =
    format_description!("[year]-[month]-[day]");

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

pub(super) fn parse_day(table: &'static str, raw: &str) -> Result<Date, StorageError> {
    Date::parse(raw, DAY).map_err(|error| StorageError::malformed_row(table, error))
}

pub(super) fn parse_uuid(table: &'static str, raw: &str) -> Result<Uuid, StorageError> {
    Uuid::parse_str(raw).map_err(|error| StorageError::malformed_row(table, error))
}

/// The public columns of a player, as every query that reads players selects
/// them: the row of `players` joined with `player_standings`, the view that
/// sums the ledger. The join is what keeps the total on every player current.
#[derive(Debug)]
pub(super) struct PlayerRow {
    pub(super) id: String,
    pub(super) handle: String,
    pub(super) glyph_bits: i64,
    pub(super) glyph_color: String,
    pub(super) role: String,
    pub(super) created_at: i64,
    pub(super) cycles: i64,
    pub(super) place: i64,
    pub(super) players: i64,
}

/// The player columns of a left join, before they are a player. A query that
/// reads an optional player selects the same nine columns as [`PlayerRow`],
/// each one nullable.
#[derive(Debug)]
pub(super) struct OptionalPlayerRow {
    pub(super) id: Option<String>,
    pub(super) handle: Option<String>,
    pub(super) glyph_bits: Option<i64>,
    pub(super) glyph_color: Option<String>,
    pub(super) role: Option<String>,
    pub(super) created_at: Option<i64>,
    pub(super) cycles: Option<i64>,
    pub(super) place: Option<i64>,
    pub(super) players: Option<i64>,
}

impl OptionalPlayerRow {
    /// The columns are all present or all absent: they come from one left join.
    /// A mix means the join broke, and that is a malformed row of `table`.
    pub(super) fn into_player(self, table: &'static str) -> Result<Option<Player>, StorageError> {
        match (
            self.id,
            self.handle,
            self.glyph_bits,
            self.glyph_color,
            self.role,
            self.created_at,
            self.cycles,
            self.place,
            self.players,
        ) {
            (
                Some(id),
                Some(handle),
                Some(glyph_bits),
                Some(glyph_color),
                Some(role),
                Some(created_at),
                Some(cycles),
                Some(place),
                Some(players),
            ) => Ok(Some(
                PlayerRow {
                    id,
                    handle,
                    glyph_bits,
                    glyph_color,
                    role,
                    created_at,
                    cycles,
                    place,
                    players,
                }
                .try_into()?,
            )),
            (None, None, None, None, None, None, None, None, None) => Ok(None),
            _ => Err(StorageError::malformed_row(table, MalformedField("player"))),
        }
    }
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

        let place = u32::try_from(row.place)
            .map_err(|error| StorageError::malformed_row(PLAYERS, error))?;
        let players = u32::try_from(row.players)
            .map_err(|error| StorageError::malformed_row(PLAYERS, error))?;

        Ok(Self {
            id: PlayerId::new(parse_uuid(PLAYERS, &row.id)?),
            handle: Handle::try_new(row.handle)
                .map_err(|error| StorageError::malformed_row(PLAYERS, error))?,
            glyph: Glyph { bits, color },
            role: row
                .role
                .parse()
                .map_err(|error| StorageError::malformed_row(PLAYERS, error))?,
            standing: Standing::new(row.cycles, place, players),
            created_at: from_micros(PLAYERS, row.created_at)?,
        })
    }
}
