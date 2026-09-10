use std::env::{self, VarError};
use std::fmt;
use std::fmt::Display;
use std::net::{IpAddr, Ipv4Addr};
use std::str::FromStr;

use nutype::nutype;

use super::app_config::{AppEnv, resolve_app_config};

/// The secret a development machine gets when `JWT_SECRET` is absent. `Prod`
/// refuses it, see [`super::AppConfig::jwt_secret_required`].
pub const DEV_JWT_SECRET: &str = "dev-only-secret-change-me-not-for-prod";

/// Shortest accepted `JWT_SECRET`, in characters. HS256 with a short secret is
/// brute-forced offline from one token.
pub const JWT_SECRET_MIN_LEN: usize = 32;

/// Where the database file lives when `DATABASE_URL` is absent. The directory
/// is in the root `.gitignore`.
const DEFAULT_DATABASE_URL: &str = "sqlite://.dev/bunker.db?mode=rwc";

/// Each value the process reads from its environment. The parse runs at startup,
/// so no handler and no service uses `std::env`.
#[derive(Clone)]
pub struct Env {
    pub app_env: AppEnv,
    /// `127.0.0.1` by default: the tunnel runs on the same box, and a LAN guest
    /// must go through Cloudflare like everyone else. Set `BIND_ADDRESS=0.0.0.0`
    /// when the cabinets on the LAN need the API directly.
    pub bind_address: IpAddr,
    pub port: u16,
    pub database: DbConfig,
    pub jwt_secret: JwtSecret,
}

impl Env {
    pub fn from_env() -> Result<Self, EnvError> {
        Self::read_with(&from_process)
    }

    /// Reads through `lookup` instead of the process environment. The parse is
    /// then a pure function of the closure, which makes it testable. One test
    /// binary shares one environment, so `set_var` leaks between tests.
    pub fn read_with(lookup: &Lookup<'_>) -> Result<Self, EnvError> {
        let app_env = parsed(lookup, "APP_ENV", AppEnv::Local)?;
        let config = resolve_app_config(app_env);

        let jwt_secret = match optional(lookup, "JWT_SECRET")? {
            Some(secret) => {
                JwtSecret::try_new(secret).map_err(|reason| EnvError::InvalidValue {
                    name: "JWT_SECRET".to_owned(),
                    reason: reason.to_string(),
                })?
            }
            None if config.jwt_secret_required => {
                return Err(EnvError::Missing {
                    name: "JWT_SECRET".to_owned(),
                });
            }
            None => {
                JwtSecret::try_new(DEV_JWT_SECRET).map_err(|reason| EnvError::InvalidValue {
                    name: "JWT_SECRET".to_owned(),
                    reason: reason.to_string(),
                })?
            }
        };

        Ok(Self {
            app_env,
            bind_address: parsed(lookup, "BIND_ADDRESS", IpAddr::V4(Ipv4Addr::LOCALHOST))?,
            port: parsed(lookup, "PORT", 3000)?,
            database: DbConfig::read_with(lookup)?,
            jwt_secret,
        })
    }
}

/// `jwt_secret` is a credential. This is written by hand, so a `debug!(?env)`
/// cannot put it in the logs.
impl fmt::Debug for Env {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Env")
            .field("app_env", &self.app_env)
            .field("bind_address", &self.bind_address)
            .field("port", &self.port)
            .field("database", &self.database)
            .field("jwt_secret", &"<redacted>")
            .finish()
    }
}

/// The value exists, but it is not valid UTF-8. A named type tells the call site
/// what failed. `Err(())` does not.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NotUnicode;

/// How the code reads one variable.
pub type Lookup<'a> = dyn Fn(&str) -> Result<Option<String>, NotUnicode> + 'a;

/// How to reach SQLite. The URL is what `sqlx` and the `sqlx` CLI both read, so
/// the app and the migration tool cannot disagree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DbConfig {
    pub url: String,
    pub max_connections: MaxConnections,
}

/// The HS256 key. No `Debug` and no `Display`: the value must never reach a log.
#[nutype(
    validate(len_char_min = JWT_SECRET_MIN_LEN),
    derive(Clone, PartialEq, Eq, AsRef)
)]
pub struct JwtSecret(String);

/// The size of the connection pool. SQLite serializes writers, so a large pool
/// only adds waiting readers. The type holds the limits.
#[nutype(
    validate(greater_or_equal = 1, less_or_equal = 64),
    default = 8,
    derive(Debug, Clone, Copy, PartialEq, Eq, Default, FromStr)
)]
pub struct MaxConnections(u32);

impl DbConfig {
    pub fn read_with(lookup: &Lookup<'_>) -> Result<Self, EnvError> {
        Ok(Self {
            url: or_default(lookup, "DATABASE_URL", DEFAULT_DATABASE_URL)?,
            max_connections: parsed(lookup, "DB_MAX_CONNECTIONS", MaxConnections::default())?,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EnvError {
    #[error("environment variable `{name}` is not valid UTF-8")]
    NotUnicode { name: String },

    #[error("environment variable `{name}` is invalid: {reason}")]
    InvalidValue { name: String, reason: String },

    #[error("environment variable `{name}` is required in this environment")]
    Missing { name: String },
}

fn from_process(name: &str) -> Result<Option<String>, NotUnicode> {
    match env::var(name) {
        Ok(value) => Ok(Some(value)),
        Err(VarError::NotPresent) => Ok(None),
        Err(VarError::NotUnicode(_)) => Err(NotUnicode),
    }
}

/// An empty value counts as absent. A blank variable in a `.env` file then gets
/// the default instead of an unusable configuration.
fn optional(lookup: &Lookup<'_>, name: &str) -> Result<Option<String>, EnvError> {
    match lookup(name) {
        Ok(Some(value)) if value.is_empty() => Ok(None),
        Ok(value) => Ok(value),
        Err(NotUnicode) => Err(EnvError::NotUnicode {
            name: name.to_owned(),
        }),
    }
}

fn or_default(lookup: &Lookup<'_>, name: &str, fallback: &str) -> Result<String, EnvError> {
    Ok(optional(lookup, name)?.unwrap_or_else(|| fallback.to_owned()))
}

/// The error does not hold the raw value. This helper also parses secrets, and
/// the message goes to the logs.
fn parsed<T>(lookup: &Lookup<'_>, name: &str, fallback: T) -> Result<T, EnvError>
where
    T: FromStr,
    T::Err: Display,
{
    match optional(lookup, name)? {
        None => Ok(fallback),
        Some(value) => value
            .parse()
            .map_err(|reason: T::Err| EnvError::InvalidValue {
                name: name.to_owned(),
                reason: reason.to_string(),
            }),
    }
}
