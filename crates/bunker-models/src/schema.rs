//! OpenAPI schemas for the `nutype` newtypes. The derive macro cannot see through
//! a generated wrapper, so each one says what it is on the wire.

use utoipa::openapi::schema::{ObjectBuilder, Type};
use utoipa::openapi::{KnownFormat, RefOr, Schema, SchemaFormat};
use utoipa::{PartialSchema, ToSchema};

use crate::auth::{PASSWORD_MAX_LEN, PASSWORD_MIN_LEN, Password};
use crate::bracket::MatchId;
use crate::glyph::GlyphBits;
use crate::handle::{HANDLE_MAX_LEN, HANDLE_MIN_LEN, Handle};
use crate::player::PlayerId;
use crate::tournament::{
    DESCRIPTION_MAX_LEN, Description, EntrantId, GAME_MODE_MAX_LEN, GAME_NAME_MAX_LEN, GameMode,
    GameName, TOURNAMENT_NAME_MAX_LEN, TournamentId, TournamentName,
};

/// A uuid newtype on the wire is a string with the uuid format.
macro_rules! uuid_schema {
    ($($id:ty),+ $(,)?) => {$(
        impl PartialSchema for $id {
            fn schema() -> RefOr<Schema> {
                ObjectBuilder::new()
                    .schema_type(Type::String)
                    .format(Some(SchemaFormat::Custom("uuid".to_owned())))
                    .build()
                    .into()
            }
        }
        impl ToSchema for $id {}
    )+};
}

/// A trimmed text newtype with a length window.
macro_rules! text_schema {
    ($($name:ty => $min:expr, $max:expr);+ $(;)?) => {$(
        impl PartialSchema for $name {
            fn schema() -> RefOr<Schema> {
                ObjectBuilder::new()
                    .schema_type(Type::String)
                    .min_length(Some($min))
                    .max_length(Some($max))
                    .build()
                    .into()
            }
        }
        impl ToSchema for $name {}
    )+};
}

uuid_schema!(PlayerId, TournamentId, EntrantId, MatchId);
text_schema!(
    TournamentName => 1, TOURNAMENT_NAME_MAX_LEN;
    GameName => 1, GAME_NAME_MAX_LEN;
    GameMode => 1, GAME_MODE_MAX_LEN;
    Description => 0, DESCRIPTION_MAX_LEN;
);

impl PartialSchema for Handle {
    fn schema() -> RefOr<Schema> {
        ObjectBuilder::new()
            .schema_type(Type::String)
            .min_length(Some(HANDLE_MIN_LEN))
            .max_length(Some(HANDLE_MAX_LEN))
            .pattern(Some("^[A-Za-z0-9_.-]+$"))
            .build()
            .into()
    }
}
impl ToSchema for Handle {}

impl PartialSchema for Password {
    fn schema() -> RefOr<Schema> {
        ObjectBuilder::new()
            .schema_type(Type::String)
            .format(Some(SchemaFormat::KnownFormat(KnownFormat::Password)))
            .min_length(Some(PASSWORD_MIN_LEN))
            .max_length(Some(PASSWORD_MAX_LEN))
            .build()
            .into()
    }
}
impl ToSchema for Password {}

impl PartialSchema for GlyphBits {
    fn schema() -> RefOr<Schema> {
        ObjectBuilder::new()
            .schema_type(Type::Integer)
            .minimum(Some(0))
            .maximum(Some(33_554_431))
            .description(Some(
                "Row-major 5x5 grid as one integer. Bit 0 is the top left cell.",
            ))
            .build()
            .into()
    }
}
impl ToSchema for GlyphBits {}
