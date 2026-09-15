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

    for (id, skill) in [
        ("50000000-0000-0000-0000-000000000001", 0),
        ("50000000-0000-0000-0000-000000000002", 6),
    ] {
        let result = sqlx::query(
            "insert into tournament_entrants (id, tournament_id, player_id, skill, registered_at)
             values (?1, ?2, null, ?3, 1)",
        )
        .bind(id)
        .bind(T1)
        .bind(skill)
        .execute(&pool)
        .await;
        assert!(result.is_err(), "the database accepted skill {skill}");
    }

    // The ledger. A valid adjustment and a valid award go in, then each bad row
    // must bounce, and a second row with the same key must bounce too.
    const P1: &str = "60000000-0000-0000-0000-000000000001";
    sqlx::query(
        "insert into players (id, handle, password_hash, glyph_bits, glyph_color, created_at)
         values (?1, 'ledger', 'x', 1, '#ffb000', 1)",
    )
    .bind(P1)
    .execute(&pool)
    .await
    .unwrap();
    let insert = "insert into point_entries (id, player_id, amount, kind, source_ref, tournament_id, note, created_at)
                  values (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)";
    type LedgerRow<'a> = (
        &'a str,
        i64,
        &'a str,
        &'a str,
        Option<&'a str>,
        Option<&'a str>,
        i64,
    );
    let good_entries: [LedgerRow; 2] = [
        (
            "70000000-0000-0000-0000-000000000001",
            -30,
            "adjustment",
            "70000000-0000-0000-0000-000000000001",
            None,
            Some("late"),
            1,
        ),
        (
            "70000000-0000-0000-0000-000000000002",
            40,
            "tournament_entry",
            T1,
            Some(T1),
            None,
            1,
        ),
    ];
    let bad_entries: [LedgerRow; 10] = [
        // Zero changes nothing.
        (
            "80000000-0000-0000-0000-000000000001",
            0,
            "adjustment",
            "a",
            None,
            Some("x"),
            1,
        ),
        // Past the adjustment range, both ways.
        (
            "80000000-0000-0000-0000-000000000002",
            10_001,
            "adjustment",
            "b",
            None,
            Some("x"),
            1,
        ),
        (
            "80000000-0000-0000-0000-000000000003",
            -10_001,
            "adjustment",
            "c",
            None,
            Some("x"),
            1,
        ),
        // An adjustment needs a note.
        (
            "80000000-0000-0000-0000-000000000004",
            10,
            "adjustment",
            "d",
            None,
            None,
            1,
        ),
        // Only an adjustment takes cycles away.
        (
            "80000000-0000-0000-0000-000000000005",
            -10,
            "match_win",
            "e",
            Some(T1),
            None,
            1,
        ),
        // A kind the model does not know.
        (
            "80000000-0000-0000-0000-000000000006",
            10,
            "arcade_score",
            "f",
            None,
            Some("x"),
            1,
        ),
        // A check-in without its event.
        (
            "80000000-0000-0000-0000-00000000000a",
            100,
            "checkin",
            "g",
            None,
            None,
            1,
        ),
        // The source is never empty.
        (
            "80000000-0000-0000-0000-000000000007",
            10,
            "champion",
            "",
            Some(T1),
            None,
            1,
        ),
        // A moment before time.
        (
            "80000000-0000-0000-0000-000000000008",
            10,
            "champion",
            "h",
            Some(T1),
            None,
            0,
        ),
        // The same key as the valid entry row: a source pays one time.
        (
            "80000000-0000-0000-0000-000000000009",
            40,
            "tournament_entry",
            T1,
            Some(T1),
            None,
            1,
        ),
    ];
    for (rows, accepted) in [(&good_entries[..], true), (&bad_entries[..], false)] {
        for (id, amount, kind, source_ref, tournament, note, created_at) in rows {
            let result = sqlx::query(insert)
                .bind(id)
                .bind(P1)
                .bind(amount)
                .bind(kind)
                .bind(source_ref)
                .bind(tournament)
                .bind(note)
                .bind(created_at)
                .execute(&pool)
                .await;
            assert_eq!(
                result.is_ok(),
                accepted,
                "amount {amount}, kind {kind:?}, source {source_ref:?}, note {note:?}, created_at {created_at}: {result:?}"
            );
        }
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

/// The event tables mirror the domain the same way: a window in order, a code
/// of twelve lowercase characters, and one check-in per player per event.
#[tokio::test]
async fn the_database_refuses_event_rows_the_domain_refuses() {
    let dir = tempfile::tempdir().unwrap();
    let config = DbConfig {
        url: format!(
            "sqlite://{}?mode=rwc",
            dir.path().join("events.db").display()
        ),
        max_connections: MaxConnections::try_new(1).unwrap(),
    };
    let pool = connect(&config).unwrap();
    run_pending_migrations(&pool).await.unwrap();

    const EVENT: &str = "90000000-0000-0000-0000-000000000001";
    const PLAYER: &str = "90000000-0000-0000-0000-000000000002";
    let insert = "insert into events (id, name, location, games, description, image, starts_at, ends_at, status, checkin_code, created_at)
                  values (?1, ?2, '', '', '', ?3, ?4, ?5, ?6, ?7, 1)";
    sqlx::query(insert)
        .bind(EVENT)
        .bind("Session 04")
        .bind(Some("cover.webp"))
        .bind(10)
        .bind(20)
        .bind("published")
        .bind("abcdefghij12")
        .execute(&pool)
        .await
        .unwrap();

    type EventRow<'a> = (
        &'a str,
        &'a str,
        Option<&'a str>,
        i64,
        i64,
        &'a str,
        &'a str,
    );
    let bad_events: [EventRow; 6] = [
        // The end before the start, and the end at the start.
        (
            "a0000000-0000-0000-0000-000000000001",
            "Night",
            None,
            20,
            10,
            "draft",
            "abcdefghij13",
        ),
        (
            "a0000000-0000-0000-0000-000000000002",
            "Night",
            None,
            20,
            20,
            "draft",
            "abcdefghij14",
        ),
        // A status the model does not know.
        (
            "a0000000-0000-0000-0000-000000000003",
            "Night",
            None,
            10,
            20,
            "open",
            "abcdefghij15",
        ),
        // A code of the wrong length, and one with an uppercase letter.
        (
            "a0000000-0000-0000-0000-000000000004",
            "Night",
            None,
            10,
            20,
            "draft",
            "short",
        ),
        (
            "a0000000-0000-0000-0000-000000000005",
            "Night",
            None,
            10,
            20,
            "draft",
            "ABCDEFGHIJ16",
        ),
        // A cover with a path in it.
        (
            "a0000000-0000-0000-0000-000000000006",
            "Night",
            Some("../x.webp"),
            10,
            20,
            "draft",
            "abcdefghij17",
        ),
    ];
    for (id, name, image, starts_at, ends_at, status, code) in bad_events {
        let result = sqlx::query(insert)
            .bind(id)
            .bind(name)
            .bind(image)
            .bind(starts_at)
            .bind(ends_at)
            .bind(status)
            .bind(code)
            .execute(&pool)
            .await;
        assert!(
            result.is_err(),
            "the database accepted image {image:?}, window {starts_at}..{ends_at}, status {status:?}, code {code:?}"
        );
    }

    // The same code as the valid event.
    let taken = sqlx::query(insert)
        .bind("a0000000-0000-0000-0000-000000000007")
        .bind("Night")
        .bind(None::<&str>)
        .bind(10)
        .bind(20)
        .bind("draft")
        .bind("abcdefghij12")
        .execute(&pool)
        .await;
    assert!(taken.is_err(), "two events share a check-in code");

    sqlx::query(
        "insert into players (id, handle, password_hash, glyph_bits, glyph_color, created_at)
         values (?1, 'door', 'x', 1, '#ffb000', 1)",
    )
    .bind(PLAYER)
    .execute(&pool)
    .await
    .unwrap();
    let checkin =
        "insert into event_checkins (event_id, player_id, checked_in_at) values (?1, ?2, ?3)";
    sqlx::query(checkin)
        .bind(EVENT)
        .bind(PLAYER)
        .bind(15)
        .execute(&pool)
        .await
        .unwrap();
    let twice = sqlx::query(checkin)
        .bind(EVENT)
        .bind(PLAYER)
        .bind(16)
        .execute(&pool)
        .await;
    assert!(twice.is_err(), "a player checked in twice to one event");
    let nowhere = sqlx::query(checkin)
        .bind("a0000000-0000-0000-0000-000000000099")
        .bind(PLAYER)
        .bind(16)
        .execute(&pool)
        .await;
    assert!(
        nowhere.is_err(),
        "a check-in to an event that does not exist"
    );
}

/// The events migration builds `point_entries` again and copies every row. The
/// other tests migrate an empty file, so the copy runs on nothing there. Here
/// the ledger holds a row of each kind before that migration runs.
#[tokio::test]
async fn the_events_migration_keeps_every_ledger_row() {
    const EVENTS_MIGRATION: i64 = 20260915080435;
    const PLAYER: &str = "b0000000-0000-0000-0000-000000000001";
    const CUP: &str = "b0000000-0000-0000-0000-000000000002";

    let dir = tempfile::tempdir().unwrap();
    let config = DbConfig {
        url: format!("sqlite://{}?mode=rwc", dir.path().join("copy.db").display()),
        max_connections: MaxConnections::try_new(1).unwrap(),
    };
    let pool = connect(&config).unwrap();
    let all = sqlx::migrate!("./migrations");
    let before = sqlx::migrate::Migrator {
        migrations: std::borrow::Cow::Owned(
            all.iter()
                .filter(|m| m.version < EVENTS_MIGRATION)
                .cloned()
                .collect(),
        ),
        ..sqlx::migrate::Migrator::DEFAULT
    };
    assert!(
        before.iter().count() < all.iter().count(),
        "the events migration is in the set"
    );
    before.run(&pool).await.unwrap();

    sqlx::query(
        "insert into players (id, handle, password_hash, glyph_bits, glyph_color, created_at)
         values (?1, 'keeper', 'x', 1, '#ffb000', 1)",
    )
    .bind(PLAYER)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "insert into tournaments (id, name, game, mode, description, date, registration_closes_at, status, created_at)
         values (?1, 'Cup', 'COD', '1v1', '', '2026-10-24', 1, 'concluded', 1)",
    )
    .bind(CUP)
    .execute(&pool)
    .await
    .unwrap();
    type Seed<'a> = (
        &'a str,
        i64,
        &'a str,
        &'a str,
        Option<&'a str>,
        Option<&'a str>,
    );
    let rows: [Seed; 3] = [
        (
            "c0000000-0000-0000-0000-000000000001",
            40,
            "tournament_entry",
            CUP,
            Some(CUP),
            None,
        ),
        (
            "c0000000-0000-0000-0000-000000000002",
            120,
            "champion",
            CUP,
            Some(CUP),
            None,
        ),
        (
            "c0000000-0000-0000-0000-000000000003",
            -30,
            "adjustment",
            "c0000000-0000-0000-0000-000000000003",
            None,
            Some("late"),
        ),
    ];
    for (id, amount, kind, source_ref, tournament, note) in rows {
        sqlx::query(
            "insert into point_entries (id, player_id, amount, kind, source_ref, tournament_id, note, created_by, created_at)
             values (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?2, 7)",
        )
        .bind(id)
        .bind(PLAYER)
        .bind(amount)
        .bind(kind)
        .bind(source_ref)
        .bind(tournament)
        .bind(note)
        .execute(&pool)
        .await
        .unwrap();
    }

    all.run(&pool).await.unwrap();

    type Copied = (
        String,
        i64,
        String,
        String,
        Option<String>,
        Option<String>,
        Option<String>,
        i64,
    );
    let copied: Vec<Copied> = sqlx::query_as(
        "select id, amount, kind, source_ref, tournament_id, note, created_by, created_at
         from point_entries order by id",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(copied.len(), 3, "every row survived the copy");
    for ((id, amount, kind, source_ref, tournament, note, created_by, created_at), expected) in
        copied.iter().zip(rows)
    {
        assert_eq!(id, expected.0);
        assert_eq!(*amount, expected.1);
        assert_eq!(kind, expected.2);
        assert_eq!(source_ref, expected.3);
        assert_eq!(tournament.as_deref(), expected.4);
        assert_eq!(note.as_deref(), expected.5);
        assert_eq!(created_by.as_deref(), Some(PLAYER));
        assert_eq!(*created_at, 7);
    }
    let (cycles, place): (i64, i64) =
        sqlx::query_as("select cycles, place from player_standings where player_id = ?1")
            .bind(PLAYER)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(cycles, 130, "the view sums the copied rows");
    assert_eq!(place, 1);

    // The old key still holds on the new table.
    let twice = sqlx::query(
        "insert into point_entries (id, player_id, amount, kind, source_ref, tournament_id, created_at)
         values ('c0000000-0000-0000-0000-000000000009', ?1, 40, 'tournament_entry', ?2, ?2, 8)",
    )
    .bind(PLAYER)
    .bind(CUP)
    .execute(&pool)
    .await;
    assert!(
        twice.is_err(),
        "the unique key on kind, player and source came back"
    );
}
