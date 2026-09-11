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

/// The tournament tables mirror the domain the same way. A valid tournament and
/// entrant go in first, then each bad row must bounce.
#[tokio::test]
async fn the_database_refuses_tournament_rows_the_domain_refuses() {
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

    const T1: &str = "10000000-0000-0000-0000-000000000001";
    const T2: &str = "10000000-0000-0000-0000-000000000002";
    const E1: &str = "20000000-0000-0000-0000-000000000001";
    const E2: &str = "20000000-0000-0000-0000-000000000002";
    const E_OTHER: &str = "20000000-0000-0000-0000-000000000003";

    for id in [T1, T2] {
        sqlx::query(
            "insert into tournaments (id, name, game, mode, description, date, registration_closes_at, status, created_at)
             values (?1, 'Cup', 'COD', '1v1', '', '2026-10-24', 1, 'draft', 1)",
        )
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    }
    for (id, tournament) in [(E1, T1), (E2, T1), (E_OTHER, T2)] {
        sqlx::query(
            "insert into tournament_entrants (id, tournament_id, player_id, registered_at)
             values (?1, ?2, null, 1)",
        )
        .bind(id)
        .bind(tournament)
        .execute(&pool)
        .await
        .unwrap();
    }

    let long_name = "x".repeat(61);
    let bad_tournaments: [(&str, &str, &str, &str, Option<&str>); 4] = [
        (
            "30000000-0000-0000-0000-000000000001",
            "Cup",
            "2026-10-24",
            "closed",
            None,
        ),
        (
            "30000000-0000-0000-0000-000000000002",
            &long_name,
            "2026-10-24",
            "draft",
            None,
        ),
        (
            "30000000-0000-0000-0000-000000000003",
            "Cup",
            "24/10/2026",
            "draft",
            None,
        ),
        (
            "30000000-0000-0000-0000-000000000004",
            "Cup",
            "2026-10-24",
            "draft",
            Some(E1),
        ),
    ];
    for (id, name, date, status, winner) in bad_tournaments {
        let result = sqlx::query(
            "insert into tournaments (id, name, game, mode, description, date, registration_closes_at, status, winner_entrant_id, created_at)
             values (?1, ?2, 'COD', '1v1', '', ?3, 1, ?4, ?5, 1)",
        )
        .bind(id)
        .bind(name)
        .bind(date)
        .bind(status)
        .bind(winner)
        .execute(&pool)
        .await;
        assert!(
            result.is_err(),
            "the database accepted name of {} chars, date {date:?}, status {status:?}, winner {winner:?}",
            name.len()
        );
    }

    type MatchRow<'a> = (&'a str, Option<&'a str>, Option<&'a str>, Option<&'a str>);
    let bad_matches: [MatchRow; 2] = [
        // The winner is not one of the two sides.
        (
            "40000000-0000-0000-0000-000000000001",
            Some(E1),
            Some(E2),
            Some(E_OTHER),
        ),
        // An entrant of another tournament.
        (
            "40000000-0000-0000-0000-000000000002",
            Some(E1),
            Some(E_OTHER),
            None,
        ),
    ];
    for (id, a, b, winner) in bad_matches {
        let result = sqlx::query(
            "insert into matches (id, tournament_id, round, slot, entrant_a, entrant_b, winner)
             values (?1, ?2, 1, 0, ?3, ?4, ?5)",
        )
        .bind(id)
        .bind(T1)
        .bind(a)
        .bind(b)
        .bind(winner)
        .execute(&pool)
        .await;
        assert!(
            result.is_err(),
            "the database accepted match a {a:?}, b {b:?}, winner {winner:?}"
        );
    }

    let good = sqlx::query(
        "insert into matches (id, tournament_id, round, slot, entrant_a, entrant_b, winner)
         values ('40000000-0000-0000-0000-000000000003', ?1, 1, 0, ?2, ?3, ?2)",
    )
    .bind(T1)
    .bind(E1)
    .bind(E2)
    .execute(&pool)
    .await;
    assert!(
        good.is_ok(),
        "a match between two entrants of the tournament is accepted"
    );
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
