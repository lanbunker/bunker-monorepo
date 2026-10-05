//! The parse of the environment variables.
//!
//! `Env::read_with` takes the lookup as a parameter, so these tests do not use
//! the process environment. One test binary shares one environment, and `set_var`
//! leaks between tests that run in parallel.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::collections::HashMap;

use bunker_api::config::{
    API_KEY_MIN_LEN, API_KEY_SYMBOLS, AppEnv, DEV_JWT_SECRET, Env, EnvError, JWT_SECRET_MIN_LEN,
    Lookup, MaxConnections, NotUnicode, read_app_env, resolve_app_config,
};

const LONG_SECRET: &str = "a-secret-that-is-long-enough-for-hs256-use";

const API_KEY: &str = "an-api-key-that-is-long-enough-to-use";

#[test]
fn an_empty_environment_yields_working_defaults() {
    let env = Env::read_with(&lookup(&[])).unwrap();

    assert_eq!(env.app_env, AppEnv::Local);
    assert_eq!(env.port, 3000);
    assert_eq!(env.database.max_connections, MaxConnections::default());
    assert!(env.database.url.starts_with("sqlite://"));
    assert!(env.jwt_secret.as_ref().len() >= JWT_SECRET_MIN_LEN);
}

#[test]
fn every_app_env_spelling_parses() {
    for app_env in AppEnv::ALL {
        let values = [
            ("APP_ENV", app_env.as_str()),
            ("JWT_SECRET", LONG_SECRET),
            ("API_KEY", API_KEY),
        ];
        let env = Env::read_with(&lookup(&values)).unwrap();

        assert_eq!(env.app_env, app_env);
    }
}

/// A typing error must not get the `local` default, because a deployment would
/// then run with development settings.
#[test]
fn an_unknown_app_env_is_rejected() {
    let error = Env::read_with(&lookup(&[("APP_ENV", "production")])).unwrap_err();

    assert!(
        matches!(&error, EnvError::InvalidValue { name, .. } if name == "APP_ENV"),
        "got {error:?}"
    );
}

/// A deployment with the development secret is an open door.
#[test]
fn prod_refuses_to_start_without_a_jwt_secret() {
    let error = Env::read_with(&lookup(&[("APP_ENV", "prod")])).unwrap_err();

    assert!(
        matches!(&error, EnvError::Missing { name } if name == "JWT_SECRET"),
        "got {error:?}"
    );
}

/// The development secret is in the repository, so a token signed with it is a
/// token anyone can sign.
#[test]
fn prod_refuses_the_development_secret() {
    let error = Env::read_with(&lookup(&[
        ("APP_ENV", "prod"),
        ("JWT_SECRET", DEV_JWT_SECRET),
    ]))
    .unwrap_err();

    assert!(
        matches!(&error, EnvError::InvalidValue { name, .. } if name == "JWT_SECRET"),
        "got {error:?}"
    );
    let local = Env::read_with(&lookup(&[("JWT_SECRET", DEV_JWT_SECRET)])).unwrap();
    assert_eq!(local.jwt_secret.as_ref(), DEV_JWT_SECRET);
}

#[test]
fn prod_starts_with_a_jwt_secret() {
    let env = Env::read_with(&lookup(&[
        ("APP_ENV", "prod"),
        ("JWT_SECRET", LONG_SECRET),
        ("API_KEY", API_KEY),
    ]))
    .unwrap();

    assert_eq!(env.jwt_secret.as_ref(), LONG_SECRET);
    assert_eq!(env.bind_address, std::net::Ipv4Addr::LOCALHOST);
}

/// HS256 with a short secret is brute-forced offline from one token.
#[test]
fn a_short_jwt_secret_is_rejected_everywhere() {
    let short = "a".repeat(JWT_SECRET_MIN_LEN - 1);
    for app_env in AppEnv::ALL {
        let error = Env::read_with(&lookup(&[
            ("APP_ENV", app_env.as_str()),
            ("JWT_SECRET", &short),
        ]))
        .unwrap_err();

        assert!(
            matches!(&error, EnvError::InvalidValue { name, .. } if name == "JWT_SECRET"),
            "{app_env} accepted a short secret: {error:?}"
        );
    }
}

#[test]
fn a_bind_address_is_parsed_and_a_bad_one_is_rejected() {
    let env = Env::read_with(&lookup(&[("BIND_ADDRESS", "0.0.0.0")])).unwrap();
    assert_eq!(env.bind_address, std::net::Ipv4Addr::UNSPECIFIED);

    let error = Env::read_with(&lookup(&[("BIND_ADDRESS", "everywhere")])).unwrap_err();
    assert!(matches!(&error, EnvError::InvalidValue { name, .. } if name == "BIND_ADDRESS"));
}

#[test]
fn a_blank_jwt_secret_counts_as_absent() {
    let error = Env::read_with(&lookup(&[("APP_ENV", "prod"), ("JWT_SECRET", "")])).unwrap_err();

    assert!(matches!(&error, EnvError::Missing { .. }));
}

#[test]
fn a_non_numeric_port_is_rejected() {
    let error = Env::read_with(&lookup(&[("PORT", "http")])).unwrap_err();

    assert!(matches!(&error, EnvError::InvalidValue { name, .. } if name == "PORT"));
}

#[test]
fn a_port_past_the_maximum_is_rejected() {
    let error = Env::read_with(&lookup(&[("PORT", "99999")])).unwrap_err();

    assert!(matches!(&error, EnvError::InvalidValue { name, .. } if name == "PORT"));
}

#[test]
fn a_zero_connection_pool_is_rejected() {
    let error = Env::read_with(&lookup(&[("DB_MAX_CONNECTIONS", "0")])).unwrap_err();

    assert!(
        matches!(&error, EnvError::InvalidValue { name, .. } if name == "DB_MAX_CONNECTIONS"),
        "got {error:?}"
    );
}

#[test]
fn an_error_never_echoes_the_value() {
    let error = Env::read_with(&lookup(&[("PORT", "s3cr3t")])).unwrap_err();

    assert!(
        !format!("{error}").contains("s3cr3t"),
        "the raw value reached the message: {error}"
    );
}

#[test]
fn the_debug_output_redacts_the_secret() {
    let env = Env::read_with(&lookup(&[("JWT_SECRET", LONG_SECRET)])).unwrap();

    assert!(!format!("{env:?}").contains(LONG_SECRET));
}

/// A blank variable in a `.env` file must behave as an absent variable.
#[test]
fn an_empty_value_falls_back_to_the_default() {
    let env = Env::read_with(&lookup(&[("DATABASE_URL", ""), ("PORT", "")])).unwrap();

    assert_eq!(env.port, 3000);
    assert!(env.database.url.starts_with("sqlite://"));
}

#[test]
fn rust_log_is_read_and_a_blank_one_is_absent() {
    let env = Env::read_with(&lookup(&[("RUST_LOG", "bunker_api=trace")])).unwrap();
    assert_eq!(env.rust_log.as_deref(), Some("bunker_api=trace"));

    let blank = Env::read_with(&lookup(&[("RUST_LOG", "")])).unwrap();
    assert_eq!(blank.rust_log, None);
}

/// A typing error in `RUST_LOG` would otherwise start a server that logs
/// nothing, and nobody would see why.
#[test]
fn an_invalid_log_filter_fails_the_start() {
    let result = bunker_api::internal::init_tracing(
        resolve_app_config(AppEnv::Test),
        Some("bunker_api=loud"),
    );

    assert!(result.is_err());
}

#[test]
fn a_supplied_database_url_wins() {
    let env = Env::read_with(&lookup(&[(
        "DATABASE_URL",
        "sqlite:///srv/bunker/bunker.db",
    )]))
    .unwrap();

    assert_eq!(env.database.url, "sqlite:///srv/bunker/bunker.db");
}

#[test]
fn a_value_that_is_not_utf8_is_reported_as_such() {
    let not_utf8 = |name: &str| {
        if name == "PORT" {
            Err(NotUnicode)
        } else {
            Ok(None)
        }
    };

    let error = Env::read_with(&not_utf8).unwrap_err();

    assert!(matches!(&error, EnvError::NotUnicode { name } if name == "PORT"));
}

#[test]
fn every_environment_resolves_a_configuration() {
    for app_env in AppEnv::ALL {
        assert_eq!(resolve_app_config(app_env).app_env, app_env);
    }
}

#[test]
fn production_hides_causes_logs_json_and_requires_a_secret() {
    let config = resolve_app_config(AppEnv::Prod);

    assert!(!config.verbose_errors);
    assert!(config.json_logs);
    assert!(config.jwt_secret_required);
    assert!(!config.fast_password_hash);
}

/// A cheap hash outside a test is a weak hash.
#[test]
fn only_tests_use_the_fast_password_hash() {
    for app_env in [AppEnv::Local, AppEnv::Prod] {
        assert!(
            !resolve_app_config(app_env).fast_password_hash,
            "{app_env} would store weak hashes"
        );
    }
    assert!(resolve_app_config(AppEnv::Test).fast_password_hash);
}

/// A deployed `/api` without a key is open to anyone who finds the address.
#[test]
fn every_environment_that_requires_a_key_refuses_to_start_without_one() {
    let required: Vec<AppEnv> = AppEnv::ALL
        .into_iter()
        .filter(|app_env| resolve_app_config(*app_env).api_key_required)
        .collect();
    assert_eq!(required, [AppEnv::Test, AppEnv::Prod]);

    for app_env in required {
        let error = Env::read_with(&lookup(&[
            ("APP_ENV", app_env.as_str()),
            ("JWT_SECRET", LONG_SECRET),
        ]))
        .unwrap_err();

        assert!(
            matches!(&error, EnvError::Missing { name } if name == "API_KEY"),
            "{app_env} started without a key: {error:?}"
        );
    }
}

#[test]
fn only_local_work_runs_without_an_api_key() {
    assert!(!resolve_app_config(AppEnv::Local).api_key_required);
    assert!(resolve_app_config(AppEnv::Test).api_key_required);
    assert!(resolve_app_config(AppEnv::Prod).api_key_required);
}

#[test]
fn local_starts_without_an_api_key() {
    let env = Env::read_with(&lookup(&[])).unwrap();

    assert!(env.api_key.is_none());
}

#[test]
fn a_short_api_key_is_refused() {
    let short = "k".repeat(API_KEY_MIN_LEN - 1);
    let error = Env::read_with(&lookup(&[("API_KEY", &short)])).unwrap_err();

    assert!(
        matches!(&error, EnvError::InvalidValue { name, .. } if name == "API_KEY"),
        "got {error:?}"
    );
}

/// The deploy script accepts the same characters, and a header carries them
/// without encoding.
#[test]
fn an_api_key_with_a_space_or_a_non_ascii_character_is_refused() {
    let padding = "k".repeat(API_KEY_MIN_LEN);
    for refused in [
        format!("{padding} space"),
        format!("{padding}\ttab"),
        format!("{padding}\u{e8}"),
        format!("{padding}:colon"),
    ] {
        let error = Env::read_with(&lookup(&[("API_KEY", &refused)])).unwrap_err();

        assert!(
            matches!(&error, EnvError::InvalidValue { name, .. } if name == "API_KEY"),
            "{refused:?} was accepted: {error:?}"
        );
    }

    let symbols = format!("{padding}{API_KEY_SYMBOLS}");
    let env = Env::read_with(&lookup(&[("API_KEY", &symbols)])).unwrap();
    assert!(env.api_key.unwrap().matches(&symbols));
}

#[test]
fn the_api_key_matches_only_itself_and_stays_out_of_debug() {
    let env = Env::read_with(&lookup(&[("API_KEY", API_KEY)])).unwrap();
    let key = env.api_key.as_ref().unwrap();

    assert!(key.matches(API_KEY));
    assert!(!key.matches("another-key-of-the-same-length-000000"));
    assert!(!format!("{env:?}").contains(API_KEY));
}

/// A release binary without `APP_ENV` would run as `local`: `/api` open and the
/// public development secret accepted.
#[test]
fn a_release_build_refuses_to_start_without_app_env() {
    let error = read_app_env(&lookup(&[]), true).unwrap_err();
    assert!(
        matches!(&error, EnvError::Missing { name } if name == "APP_ENV"),
        "got {error:?}"
    );

    let blank = read_app_env(&lookup(&[("APP_ENV", "")]), true).unwrap_err();
    assert!(matches!(&blank, EnvError::Missing { name } if name == "APP_ENV"));

    let prod = read_app_env(&lookup(&[("APP_ENV", "prod")]), true).unwrap();
    assert_eq!(prod, AppEnv::Prod);
}

#[test]
fn a_debug_build_defaults_to_local() {
    assert_eq!(read_app_env(&lookup(&[]), false).unwrap(), AppEnv::Local);
    assert_eq!(
        read_app_env(&lookup(&[("APP_ENV", "test")]), false).unwrap(),
        AppEnv::Test
    );
}

fn lookup(values: &[(&str, &str)]) -> Box<Lookup<'static>> {
    let values: HashMap<String, String> = values
        .iter()
        .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
        .collect();

    Box::new(move |name: &str| Ok(values.get(name).cloned()))
}
