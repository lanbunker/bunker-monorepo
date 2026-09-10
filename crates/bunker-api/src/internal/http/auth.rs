use axum::extract::{FromRef, FromRequestParts};
use axum::http::header::AUTHORIZATION;
use axum::http::request::Parts;
use bunker_models::PlayerId;

use crate::services::{AuthService, ErrorCode};

use super::api_error::ApiError;

/// The player behind a valid `Authorization: Bearer <jwt>` header. A handler that
/// asks for it runs only for a logged-in player, and holds the id with no lookup.
///
/// The check is a signature check and needs no database read. The service that
/// verifies the token comes from the state, so this extractor imports no secret.
#[derive(Debug, Clone, Copy)]
pub struct Authenticated(pub PlayerId);

impl<S> FromRequestParts<S> for Authenticated
where
    S: Send + Sync,
    AuthService: FromRef<S>,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let header = parts
            .headers
            .get(AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .ok_or_else(|| {
                ApiError::new(ErrorCode::Unauthorized, "This route needs a bearer token")
            })?;

        // The scheme is case-insensitive per RFC 7235. The token is not.
        let token = header
            .split_once(' ')
            .filter(|(scheme, _)| scheme.eq_ignore_ascii_case("bearer"))
            .map(|(_, token)| token.trim())
            .filter(|token| !token.is_empty())
            .ok_or_else(|| {
                ApiError::new(
                    ErrorCode::Unauthorized,
                    "The Authorization header must be `Bearer <token>`",
                )
            })?;

        let player = AuthService::from_ref(state).verify_token(token)?;

        Ok(Self(player))
    }
}
