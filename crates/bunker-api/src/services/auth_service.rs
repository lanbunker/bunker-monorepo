use bunker_models::{LoginRequest, PlayerId, SignupRequest, TokenResponse, generate_glyph};
use time::OffsetDateTime;

use crate::storage::{Created, NewPlayer, PlayerStorage};

use super::error::ServiceError;
use super::password::PasswordHasher;
use super::token::TokenIssuer;

#[derive(Debug, Clone)]
pub struct AuthService {
    players: PlayerStorage,
    tokens: TokenIssuer,
    hasher: PasswordHasher,
}

impl AuthService {
    pub const fn new(players: PlayerStorage, tokens: TokenIssuer, hasher: PasswordHasher) -> Self {
        Self {
            players,
            tokens,
            hasher,
        }
    }

    /// Creates the player and logs them in. The server sets the id, the glyph
    /// and the creation time. A client supplies only a handle and a password.
    pub async fn signup(&self, request: SignupRequest) -> Result<TokenResponse, ServiceError> {
        let glyph = generate_glyph(request.handle.as_ref());
        let password_hash = self.hasher.hash(request.password).await?;

        let player = NewPlayer {
            id: PlayerId::generate(),
            handle: request.handle,
            password_hash,
            glyph,
            created_at: OffsetDateTime::now_utc(),
        };

        match self.players.create(&player).await? {
            Created::Player(stored) => Ok(self.tokens.issue(stored.id)?),
            Created::HandleTaken => Err(ServiceError::HandleTaken(player.handle)),
        }
    }

    /// The failure is the same for an unknown handle and a wrong password. When
    /// the handle is unknown, the hasher still runs one verification, so the two
    /// answers take the same time.
    pub async fn login(&self, request: LoginRequest) -> Result<TokenResponse, ServiceError> {
        let credentials = self.players.credentials_by_handle(&request.handle).await?;

        match credentials {
            Some(credentials) => {
                let valid = self
                    .hasher
                    .verify(request.password, credentials.password_hash)
                    .await?;
                if valid {
                    Ok(self.tokens.issue(credentials.id)?)
                } else {
                    Err(ServiceError::InvalidCredentials)
                }
            }
            None => {
                self.hasher.burn(request.password).await?;
                Err(ServiceError::InvalidCredentials)
            }
        }
    }

    /// Checks the signature and the expiry. No database read: a deleted player
    /// keeps a valid token until it expires, which the TTL bounds.
    pub fn verify_token(&self, token: &str) -> Result<PlayerId, ServiceError> {
        self.tokens.verify(token)
    }
}
