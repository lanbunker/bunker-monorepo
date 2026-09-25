use bunker_models::{
    LoginRequest, Password, PasswordChange, PlayerId, Role, SignupRequest, TemporaryPassword,
    TokenResponse, generate_glyph,
};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::storage::{Created, NewPlayer, PlayerStorage};

use super::error::ServiceError;
use super::password::PasswordHasher;
use super::token::TokenIssuer;

/// Who sends a request, as a valid token and the players table say.
#[derive(Debug, Clone, Copy)]
pub struct Caller {
    pub id: PlayerId,
    pub role: Role,
    /// Set after an admin reset. Only `/api/me` and `/api/me/password` take a
    /// caller with this flag.
    pub must_change_password: bool,
}

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

    /// The current password must match. The new hash clears any pending reset and
    /// kills every older token, so the caller gets a fresh one back.
    pub async fn change_password(
        &self,
        id: PlayerId,
        change: PasswordChange,
    ) -> Result<TokenResponse, ServiceError> {
        let credentials = self
            .players
            .credentials_by_id(id)
            .await?
            .ok_or(ServiceError::PlayerIdNotFound(id))?;

        let valid = self
            .hasher
            .verify(change.current_password, credentials.password_hash)
            .await?;
        if !valid {
            return Err(ServiceError::WrongPassword);
        }

        let hash = self.hasher.hash(change.new_password).await?;
        if !self
            .players
            .set_password(id, &hash, false, OffsetDateTime::now_utc())
            .await?
        {
            return Err(ServiceError::PlayerIdNotFound(id));
        }

        self.tokens.issue(id)
    }

    /// Replaces the password with a random one and marks the account, so the
    /// next login must change it. The clear text goes to the admin once.
    pub async fn reset_password(&self, id: PlayerId) -> Result<TemporaryPassword, ServiceError> {
        let temporary = Uuid::new_v4().simple().to_string();
        let password = Password::try_new(temporary.clone()).map_err(ServiceError::crypto)?;
        let hash = self.hasher.hash(password).await?;

        if !self
            .players
            .set_password(id, &hash, true, OffsetDateTime::now_utc())
            .await?
        {
            return Err(ServiceError::PlayerIdNotFound(id));
        }

        Ok(TemporaryPassword {
            temporary_password: temporary,
        })
    }

    /// Signature, expiry, then the account: a deleted player and a token issued
    /// before the last password change are both refused as unauthorized.
    pub async fn authenticate(&self, token: &str) -> Result<Caller, ServiceError> {
        let verified = self.tokens.verify(token)?;
        let stored = self
            .players
            .access(verified.player)
            .await?
            .ok_or_else(|| ServiceError::invalid_token(StaleToken::UnknownPlayer))?;

        if verified.issued_at < stored.credentials_changed_at {
            return Err(ServiceError::invalid_token(StaleToken::PasswordChanged));
        }

        Ok(Caller {
            id: stored.id,
            role: stored.role,
            must_change_password: stored.must_change_password,
        })
    }
}

/// Why a well-formed token is refused. It goes to the log, never to the client.
#[derive(Debug, thiserror::Error)]
enum StaleToken {
    #[error("the player no longer exists")]
    UnknownPlayer,
    #[error("the password changed after the token was issued")]
    PasswordChanged,
}
