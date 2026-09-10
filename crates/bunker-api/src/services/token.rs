use std::fmt;
use std::time::Duration;

use bunker_models::{PlayerId, TokenResponse};
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use super::error::ServiceError;

/// HS256 with one shared secret. The keys hold it, so `Debug` is written by hand.
#[derive(Clone)]
pub struct TokenIssuer {
    encoding: EncodingKey,
    decoding: DecodingKey,
    ttl: Duration,
}

/// `sub` is the player id, `exp` and `iat` are Unix seconds.
#[derive(Debug, Serialize, Deserialize)]
struct Claims {
    sub: PlayerId,
    exp: i64,
    iat: i64,
}

impl TokenIssuer {
    pub fn new(secret: &[u8], ttl: Duration) -> Self {
        Self {
            encoding: EncodingKey::from_secret(secret),
            decoding: DecodingKey::from_secret(secret),
            ttl,
        }
    }

    pub fn issue(&self, player: PlayerId) -> Result<TokenResponse, ServiceError> {
        let now = OffsetDateTime::now_utc();
        let ttl = time::Duration::try_from(self.ttl).map_err(ServiceError::crypto)?;
        let expires_at = now
            .checked_add(ttl)
            .ok_or_else(|| ServiceError::crypto(std::io::Error::other("token expiry overflow")))?;

        let claims = Claims {
            sub: player,
            exp: expires_at.unix_timestamp(),
            iat: now.unix_timestamp(),
        };

        let token = jsonwebtoken::encode(&Header::new(Algorithm::HS256), &claims, &self.encoding)
            .map_err(ServiceError::crypto)?;

        Ok(TokenResponse { token, expires_at })
    }

    /// A token from another issuer, an altered token and an expired token all
    /// fail here. The message is one message, so a caller learns nothing about
    /// which check failed.
    pub fn verify(&self, token: &str) -> Result<PlayerId, ServiceError> {
        let mut validation = Validation::new(Algorithm::HS256);
        validation.set_required_spec_claims(&["exp", "sub"]);
        // The default gives a client one minute after `exp`. A token is valid
        // until `exp` and not later.
        validation.leeway = 0;

        let data = jsonwebtoken::decode::<Claims>(token, &self.decoding, &validation)
            .map_err(ServiceError::invalid_token)?;

        Ok(data.claims.sub)
    }
}

impl fmt::Debug for TokenIssuer {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TokenIssuer")
            .field("ttl", &self.ttl)
            .finish_non_exhaustive()
    }
}
