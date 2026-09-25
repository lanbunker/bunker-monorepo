//! The failures that many routes share, for the `responses` list of a
//! `#[utoipa::path]`. A route that lists one of these first and its own
//! tuples second can still give a status its own description.

use std::collections::BTreeMap;

use utoipa::IntoResponses;
use utoipa::openapi::{ContentBuilder, Ref, RefOr, Response, ResponseBuilder};

/// A route that reads a JSON body.
pub(super) struct BodyErrors;

/// A route that parses a path parameter.
pub(super) struct PathErrors;

/// A route that needs a bearer token and a settled password.
pub(super) struct BearerErrors;

/// A route that takes a token with a temporary password too.
pub(super) struct TokenErrors;

/// A route for admins only.
pub(super) struct AdminErrors;

impl IntoResponses for BodyErrors {
    fn responses() -> BTreeMap<String, RefOr<Response>> {
        responses(&[
            (
                "400",
                "The body is not valid JSON, or a parameter is malformed",
            ),
            ("413", "The body is too large"),
            ("415", "The body is not `application/json`"),
            ("422", "A field is missing, unknown or out of range"),
        ])
    }
}

impl IntoResponses for PathErrors {
    fn responses() -> BTreeMap<String, RefOr<Response>> {
        responses(&[("400", "A path parameter is malformed")])
    }
}

impl IntoResponses for BearerErrors {
    fn responses() -> BTreeMap<String, RefOr<Response>> {
        responses(&[
            ("401", "No valid bearer token"),
            (
                "403",
                "`PasswordChangeRequired`: the caller must choose a new password first",
            ),
        ])
    }
}

impl IntoResponses for TokenErrors {
    fn responses() -> BTreeMap<String, RefOr<Response>> {
        responses(&[("401", "No valid bearer token")])
    }
}

impl IntoResponses for AdminErrors {
    fn responses() -> BTreeMap<String, RefOr<Response>> {
        responses(&[
            ("401", "No valid bearer token"),
            (
                "403",
                "Not an admin, or `PasswordChangeRequired`: a new password first",
            ),
        ])
    }
}

fn responses(list: &[(&str, &str)]) -> BTreeMap<String, RefOr<Response>> {
    list.iter()
        .map(|(status, description)| {
            let response = ResponseBuilder::new()
                .description(*description)
                .content(
                    "application/json",
                    ContentBuilder::new()
                        .schema(Some(Ref::from_schema_name("ApiErrorBody")))
                        .build(),
                )
                .build();
            ((*status).to_owned(), RefOr::T(response))
        })
        .collect()
}
