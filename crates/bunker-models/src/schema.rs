//! OpenAPI schemas for the `nutype` newtypes. The derive macro cannot see through
//! a generated wrapper, so each one says what it is on the wire.

use utoipa::openapi::schema::{ObjectBuilder, Type};
use utoipa::openapi::{KnownFormat, RefOr, Schema, SchemaFormat};
use utoipa::{PartialSchema, ToSchema};

use crate::auth::{PASSWORD_MAX_LEN, PASSWORD_MIN_LEN, Password};
use crate::glyph::GlyphBits;
use crate::handle::{HANDLE_MAX_LEN, HANDLE_MIN_LEN, Handle};
use crate::player::PlayerId;

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

impl PartialSchema for PlayerId {
    fn schema() -> RefOr<Schema> {
        ObjectBuilder::new()
            .schema_type(Type::String)
            .format(Some(SchemaFormat::Custom("uuid".to_owned())))
            .build()
            .into()
    }
}
impl ToSchema for PlayerId {}

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
