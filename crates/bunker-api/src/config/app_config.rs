use std::fmt;
use std::str::FromStr;

/// Where the process runs. Every capability comes from this enum, and there are
/// no per-feature override flags: to change one, change `APP_ENV`.
///
/// - `Test`: automated tests
/// - `Local`: development on the host
/// - `Prod`: the box in the office, behind the tunnel
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AppEnv {
    Test,
    Local,
    Prod,
}

impl AppEnv {
    /// Every variant, so a test can show that none is missing.
    pub const ALL: [Self; 3] = [Self::Test, Self::Local, Self::Prod];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Test => "test",
            Self::Local => "local",
            Self::Prod => "prod",
        }
    }
}

impl fmt::Display for AppEnv {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for AppEnv {
    type Err = UnknownAppEnv;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        match raw {
            "test" => Ok(Self::Test),
            "local" => Ok(Self::Local),
            "prod" => Ok(Self::Prod),
            other => Err(UnknownAppEnv(other.to_owned())),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("unknown APP_ENV `{0}`, expected one of: test, local, prod")]
pub struct UnknownAppEnv(String);

/// The capabilities that [`AppEnv`] gives. A new capability needs a field here
/// and a value for each variant in [`resolve_app_config`]. That match is
/// exhaustive, so you cannot forget an environment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AppConfig {
    pub app_env: AppEnv,
    /// Put error causes in HTTP responses. Off where a user can see them.
    pub verbose_errors: bool,
    /// Log at debug level. Separate from [`Self::verbose_errors`], so a hidden
    /// cause still goes to the logs.
    pub debug_logs: bool,
    /// Write logs as JSON instead of human-readable lines.
    pub json_logs: bool,
    /// Apply pending migrations at startup. One process owns the SQLite file, so
    /// there is no race between instances, and the service unit runs the binary
    /// with no separate migrate step.
    pub migrate_on_startup: bool,
    /// Hash passwords with the smallest Argon2 parameters. Only for tests, where
    /// a suite that signs up fifty players must not spend seconds on hashing.
    pub fast_password_hash: bool,
    /// Refuse to start without `JWT_SECRET`. A development default is convenient,
    /// and a deployment with the default secret is an open door.
    pub jwt_secret_required: bool,
}

pub const fn resolve_app_config(app_env: AppEnv) -> AppConfig {
    match app_env {
        AppEnv::Test => AppConfig {
            app_env,
            verbose_errors: true,
            debug_logs: true,
            json_logs: false,
            migrate_on_startup: true,
            fast_password_hash: true,
            jwt_secret_required: false,
        },
        AppEnv::Local => AppConfig {
            app_env,
            verbose_errors: true,
            debug_logs: true,
            json_logs: false,
            migrate_on_startup: true,
            fast_password_hash: false,
            jwt_secret_required: false,
        },
        AppEnv::Prod => AppConfig {
            app_env,
            verbose_errors: false,
            debug_logs: false,
            json_logs: true,
            migrate_on_startup: true,
            fast_password_hash: false,
            jwt_secret_required: true,
        },
    }
}
