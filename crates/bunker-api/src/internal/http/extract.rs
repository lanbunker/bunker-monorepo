// `clippy.toml` sends all other code to the wrappers below, so this is the one
// file that uses the axum extractors directly.
#![allow(clippy::disallowed_types)]

use axum::extract::rejection::{
    BytesRejection, FailedToBufferBody, JsonRejection, PathRejection, QueryRejection,
};
use axum::extract::{FromRequest, FromRequestParts, Json, Path, Query, Request};
use axum::http::request::Parts;
use serde::de::DeserializeOwned;

use crate::services::ErrorCode;

use super::api_error::ApiError;

/// A JSON body parsed into `T`. A domain type validates itself during
/// deserialization, so a handler holds valid data.
///
/// It keeps the distinctions that axum makes, because a client can correct only
/// what the error tells it.
#[derive(Debug)]
pub struct ValidJson<T>(pub T);

impl<T, S> FromRequest<S> for ValidJson<T>
where
    T: DeserializeOwned,
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request(request: Request, state: &S) -> Result<Self, Self::Rejection> {
        Json::<T>::from_request(request, state)
            .await
            .map(|Json(value)| Self(value))
            .map_err(|rejection| {
                let code = match &rejection {
                    JsonRejection::MissingJsonContentType(_) => ErrorCode::UnsupportedMediaType,
                    JsonRejection::JsonDataError(_) => ErrorCode::UnprocessableRequest,
                    // `BytesRejection` holds two failures: the body is too long,
                    // or the stream broke. Only the first is permanent. A 413 for
                    // a lost connection tells the client to stop a retry that can
                    // succeed.
                    JsonRejection::BytesRejection(BytesRejection::FailedToBufferBody(
                        FailedToBufferBody::LengthLimitError(_),
                    )) => ErrorCode::PayloadTooLarge,
                    _ => ErrorCode::InvalidRequest,
                };

                ApiError::new(
                    code,
                    format!("Invalid request body. {}", rejection.body_text()),
                )
            })
    }
}

/// Query parameters parsed into `T`. A failure is a `400 InvalidRequest`.
#[derive(Debug)]
pub struct ValidQuery<T>(pub T);

impl<T, S> FromRequestParts<S> for ValidQuery<T>
where
    T: DeserializeOwned,
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        Query::<T>::from_request_parts(parts, state)
            .await
            .map(|Query(value)| Self(value))
            .map_err(|rejection: QueryRejection| {
                ApiError::new(
                    ErrorCode::InvalidRequest,
                    format!("Invalid query parameters. {}", rejection.body_text()),
                )
            })
    }
}

#[derive(Debug)]
pub struct ValidPath<T>(pub T);

impl<T, S> FromRequestParts<S> for ValidPath<T>
where
    T: DeserializeOwned + Send,
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        Path::<T>::from_request_parts(parts, state)
            .await
            .map(|Path(value)| Self(value))
            .map_err(|rejection| match rejection {
                // The handler asks for a parameter that the route does not
                // declare. That is a bug here, and not a bad request.
                PathRejection::MissingPathParams(error) => {
                    ApiError::new(ErrorCode::GenericError, "The route is misconfigured")
                        .with_cause(&error)
                }
                other => ApiError::new(
                    ErrorCode::InvalidRequest,
                    format!("Invalid path parameters. {}", other.body_text()),
                ),
            })
    }
}
