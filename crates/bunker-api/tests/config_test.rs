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
    AppEnv, Env, EnvError, JWT_SECRET_MIN_LEN, Lookup, MaxConnections, NotUnicode,
    resolve_app_config,
};

const LONG_SECRET: &str = "a-secret-that-is-long-enough-for-hs256-use";

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
        let values = [("APP_ENV", app_env.as_str()), ("JWT_SECRET", LONG_SECRET)];
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

#[test]
fn prod_starts_with_a_jwt_secret() {
    let env = Env::read_with(&lookup(&[("APP_ENV", "prod"), ("JWT_SECRET", LONG_SECRET)])).unwrap();

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

fn lookup(values: &[(&str, &str)]) -> Box<Lookup<'static>> {
    let values: HashMap<String, String> = values
        .iter()
        .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
        .collect();

    Box::new(move |name: &str| Ok(values.get(name).cloned()))
}
