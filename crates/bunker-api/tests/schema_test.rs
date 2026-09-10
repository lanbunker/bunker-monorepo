//! `schema.sql` is the readable source of truth for the database structure. It is
//! generated from a migrated database by `make schema`, and this test fails when
//! someone adds a migration and forgets to run it.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::collections::BTreeSet;

use bunker_api::config::{DbConfig, MaxConnections};
use bunker_api::storage::{connect, run_pending_migrations};

const SCHEMA_FILE: &str = include_str!("../schema.sql");

#[tokio::test]
async fn schema_sql_matches_the_migrated_database() {
    let dir = tempfile::tempdir().unwrap();
    let config = DbConfig {
        url: format!(
            "sqlite://{}?mode=rwc",
            dir.path().join("schema.db").display()
        ),
        max_connections: MaxConnections::try_new(1).unwrap(),
    };
    let pool = connect(&config).unwrap();
    run_pending_migrations(&pool).await.unwrap();

    let rows: Vec<(String,)> = sqlx::query_as(
        "select sql from sqlite_master
         where sql is not null and name not like 'sqlite_%' and name != '_sqlx_migrations'
         order by name",
    )
    .fetch_all(&pool)
    .await
    .unwrap();

    let from_database: BTreeSet<String> = rows.into_iter().map(|(sql,)| normalize(&sql)).collect();
    let from_file: BTreeSet<String> = statements(SCHEMA_FILE).map(normalize).collect();

    assert!(
        !from_database.is_empty(),
        "the migrations produced no tables, the migrator did not run"
    );
    assert_eq!(
        from_file, from_database,
        "crates/bunker-api/schema.sql is stale. Run `make schema` and commit the result."
    );
}

/// The CHECK constraints are the last guard against a row the model refuses. A raw
/// insert must fail at the database, not poison every later read.
#[tokio::test]
async fn the_database_refuses_rows_the_domain_refuses() {
    let dir = tempfile::tempdir().unwrap();
    let config = DbConfig {
        url: format!(
            "sqlite://{}?mode=rwc",
            dir.path().join("check.db").display()
        ),
        max_connections: MaxConnections::try_new(1).unwrap(),
    };
    let pool = connect(&config).unwrap();
    run_pending_migrations(&pool).await.unwrap();

    let bad_rows: [(&str, &str, i64, &str, &str, i64); 6] = [
        (
            "00000000-0000-0000-0000-000000000001",
            "da ve",
            1,
            "#ffb000",
            "user",
            1,
        ),
        (
            "00000000-0000-0000-0000-000000000002",
            "ab",
            1,
            "#ffb000",
            "user",
            1,
        ),
        (
            "00000000-0000-0000-0000-000000000003",
            "dave",
            1 << 25,
            "#ffb000",
            "user",
            1,
        ),
        (
            "00000000-0000-0000-0000-000000000004",
            "dave",
            1,
            "#000000",
            "user",
            1,
        ),
        (
            "00000000-0000-0000-0000-000000000005",
            "dave",
            1,
            "#ffb000",
            "user",
            0,
        ),
        (
            "00000000-0000-0000-0000-000000000006",
            "dave",
            1,
            "#ffb000",
            "root",
            1,
        ),
    ];

    for (id, handle, bits, color, role, created_at) in bad_rows {
        let result = sqlx::query(
            "insert into players (id, handle, password_hash, glyph_bits, glyph_color, role, created_at)
             values (?1, ?2, 'x', ?3, ?4, ?5, ?6)",
        )
        .bind(id)
        .bind(handle)
        .bind(bits)
        .bind(color)
        .bind(role)
        .bind(created_at)
        .execute(&pool)
        .await;

        assert!(
            result.is_err(),
            "the database accepted handle {handle:?}, bits {bits}, color {color}, role {role}, created_at {created_at}"
        );
    }
}

/// The file holds one statement per `;`, plus comment lines that this skips.
fn statements(file: &str) -> impl Iterator<Item = &str> {
    file.split(';')
        .map(str::trim)
        .filter(|chunk| !chunk.is_empty())
        .filter(|chunk| {
            !chunk
                .lines()
                .all(|line| line.trim_start().starts_with("--"))
        })
}

/// Comparison ignores whitespace, comment lines and case. The stored text in
/// `sqlite_master` is the text of the migration, and the file is written from it.
fn normalize(sql: &str) -> String {
    sql.lines()
        .filter(|line| !line.trim_start().starts_with("--"))
        .flat_map(str::split_whitespace)
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}
