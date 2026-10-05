//! The environment variables, and the capabilities that come from them.

mod app_config;
mod env;

pub use app_config::{AppConfig, AppEnv, UnknownAppEnv, resolve_app_config};
pub use env::{
    API_KEY_MIN_LEN, API_KEY_SYMBOLS, ApiKey, ApiKeyError, DEV_JWT_SECRET, DbConfig, Env, EnvError,
    JWT_SECRET_MIN_LEN, JwtSecret, JwtSecretError, Lookup, MaxConnections, MaxConnectionsError,
    NotUnicode, load_dotenv, read_app_env,
};
