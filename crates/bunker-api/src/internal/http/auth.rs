use axum::extract::{FromRef, FromRequestParts, Request, State};
use axum::http::header::AUTHORIZATION;
use axum::http::request::Parts;
use axum::middleware::Next;
use axum::response::Response;
use bunker_models::Role;

use crate::services::{AuthService, Caller, ErrorCode};

use super::api_error::ApiError;

/// The caller behind a valid `Authorization: Bearer <jwt>` header, with a
/// settled password. One read of the players table per request: it is what
/// refuses a deleted player, a token older than the last password change, and
/// an account that must choose a new password first.
#[derive(Debug, Clone, Copy)]
pub struct Authenticated(pub Caller);

/// The caller behind a valid token, even one with a temporary password. Only
/// `GET /api/me` and `POST /api/me/password` take it: they are how the player
/// finishes the change.
#[derive(Debug, Clone, Copy)]
pub struct PendingPassword(pub Caller);

/// An authenticated admin. The role comes from the database, so a demotion
/// takes effect at once.
#[derive(Debug, Clone, Copy)]
pub struct AdminOnly(pub Caller);

/// Refuses every request that is not from an admin, before a handler runs.
/// `server.rs` puts it on the admin routers, so a new admin route cannot forget
/// the check. The admin goes into the request, and a handler that names the
/// actor reads it back through [`AdminOnly`] without a second lookup.
pub async fn require_admin(
    State(auth): State<AuthService>,
    request: Request,
    next: Next,
) -> Result<Response, ApiError> {
    let (mut parts, body) = request.into_parts();
    let admin = AdminOnly::from_request_parts(&mut parts, &auth).await?;
    parts.extensions.insert(admin);

    Ok(next.run(Request::from_parts(parts, body)).await)
}

impl<S> FromRequestParts<S> for PendingPassword
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

        let caller = AuthService::from_ref(state).authenticate(token).await?;

        Ok(Self(caller))
    }
}

impl<S> FromRequestParts<S> for Authenticated
where
    S: Send + Sync,
    AuthService: FromRef<S>,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let PendingPassword(caller) = PendingPassword::from_request_parts(parts, state).await?;

        if caller.must_change_password {
            return Err(ApiError::new(
                ErrorCode::PasswordChangeRequired,
                "Choose a new password first",
            ));
        }

        Ok(Self(caller))
    }
}

impl<S> FromRequestParts<S> for AdminOnly
where
    S: Send + Sync,
    AuthService: FromRef<S>,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        if let Some(admin) = parts.extensions.get::<Self>() {
            return Ok(*admin);
        }
        let Authenticated(caller) = Authenticated::from_request_parts(parts, state).await?;

        match caller.role {
            Role::Admin => Ok(Self(caller)),
            Role::User => Err(ApiError::new(
                ErrorCode::Forbidden,
                "This route is for admins",
            )),
        }
    }
}
