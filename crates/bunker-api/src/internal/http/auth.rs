use axum::extract::{FromRef, FromRequestParts};
use axum::http::header::AUTHORIZATION;
use axum::http::request::Parts;
use bunker_models::{Account, Role};

use crate::services::{AuthService, ErrorCode};

use super::api_error::ApiError;

/// The account behind a valid `Authorization: Bearer <jwt>` header. One database
/// read per request: it is what refuses a deleted player and a token older than
/// the last password change.
#[derive(Debug, Clone)]
pub struct Authenticated(pub Account);

/// The account behind the token, checked to be an admin. The role comes from the
/// database, so a demotion takes effect at once.
#[derive(Debug, Clone)]
pub struct AdminOnly(pub Account);

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

        let account = AuthService::from_ref(state).authenticate(token).await?;

        Ok(Self(account))
    }
}

impl<S> FromRequestParts<S> for AdminOnly
where
    S: Send + Sync,
    AuthService: FromRef<S>,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let Authenticated(account) = Authenticated::from_request_parts(parts, state).await?;

        match account.player.role {
            Role::Admin => Ok(Self(account)),
            Role::User => Err(ApiError::new(
                ErrorCode::Forbidden,
                "This route is for admins",
            )),
        }
    }
}
