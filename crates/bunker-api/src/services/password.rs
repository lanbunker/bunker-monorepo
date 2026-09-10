use argon2::password_hash::{self, PasswordHasher as _, PasswordVerifier as _};
use argon2::{Algorithm, Argon2, Params, Version};
use bunker_models::Password;

use super::error::ServiceError;

/// Argon2id with the OWASP baseline: 19 MiB of memory, two passes. A verify costs
/// tens of milliseconds, which is the point.
const MEMORY_KIB: u32 = 19 * 1024;
const ITERATIONS: u32 = 2;

/// The smallest parameters the library accepts. Only for tests.
const FAST_MEMORY_KIB: u32 = 8;
const FAST_ITERATIONS: u32 = 1;

/// The work runs on the blocking pool, so a login never stalls the async workers.
#[derive(Debug, Clone)]
pub struct PasswordHasher {
    params: Params,
}

impl PasswordHasher {
    /// `fast` selects the test parameters. Never pass `true` outside a test: a
    /// hash made this way is cheap to brute force.
    pub fn new(fast: bool) -> Self {
        let (memory, iterations) = if fast {
            (FAST_MEMORY_KIB, FAST_ITERATIONS)
        } else {
            (MEMORY_KIB, ITERATIONS)
        };

        // The values above are inside the ranges the library accepts, so this
        // cannot fail. The default keeps the constructor total.
        let params = Params::new(memory, iterations, 1, None).unwrap_or_default();

        Self { params }
    }

    /// Returns the PHC string that the players table stores. It carries the salt
    /// and the parameters, so a later parameter change verifies old hashes. The
    /// library draws the salt from the OS.
    pub async fn hash(&self, password: Password) -> Result<String, ServiceError> {
        let argon = self.argon();

        tokio::task::spawn_blocking(move || {
            argon
                .hash_password(password.as_ref().as_bytes())
                .map(|hash| hash.to_string())
                .map_err(ServiceError::crypto)
        })
        .await
        .map_err(ServiceError::crypto)?
    }

    /// `Ok(false)` is a wrong password. `Err` is a stored hash that cannot be
    /// parsed, which is a bug or a corrupted row.
    pub async fn verify(&self, password: Password, hash: String) -> Result<bool, ServiceError> {
        let argon = self.argon();

        tokio::task::spawn_blocking(move || {
            match argon.verify_password(password.as_ref().as_bytes(), hash.as_str()) {
                Ok(()) => Ok(true),
                Err(password_hash::Error::PasswordInvalid) => Ok(false),
                Err(error) => Err(ServiceError::crypto(error)),
            }
        })
        .await
        .map_err(ServiceError::crypto)?
    }

    /// Spends one hash worth of time and discards the result. A login for an
    /// unknown handle calls this, so it takes as long as a wrong password.
    pub async fn burn(&self, password: Password) -> Result<(), ServiceError> {
        let _discarded = self.hash(password).await?;

        Ok(())
    }

    fn argon(&self) -> Argon2<'static> {
        Argon2::new(Algorithm::Argon2id, Version::V0x13, self.params.clone())
    }
}
