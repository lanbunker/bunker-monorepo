//! What storage promises when a service writes from a stale read, and how it
//! names a failure of the driver. A race is hard to stage through HTTP, so
//! these tests make the service's read stale by hand and call storage.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod support;

use std::str::FromStr as _;
use std::time::Duration;

use bunker_api::storage::{
    BracketWrite, Enrolled, EntrantGuard, NewEntrant, StorageError, TournamentStorage,
};
use bunker_models::{Bracket, EntrantId, SkillLevel, TournamentStatus, tournament_awards};
use serde_json::json;
use sqlx::Connection as _;
use sqlx::sqlite::{SqliteConnectOptions, SqliteConnection};
use support::{HOUR, TestApi, migrated_pool};
use time::OffsetDateTime;

#[tokio::test]
async fn an_entrant_write_from_a_stale_status_writes_nothing() {
    let api = TestApi::with_database().await;
    let admin = api.signup_admin("root").await;
    let created = api.create_tournament(&admin, HOUR).await;
    api.set_status(&admin, created.id, "open").await;
    let dave = api.signup("dave").await.token;
    let joined = api
        .post_as(
            &format!("/api/tournaments/{}/registration", created.id),
            &json!({ "skill": 3 }),
            &dave,
        )
        .await;
    assert_eq!(joined.status(), axum::http::StatusCode::OK);
    api.set_status(&admin, created.id, "live").await;
    let storage = TournamentStorage::new(api.pool().clone());
    let now = OffsetDateTime::now_utc();
    let read_while_open = EntrantGuard {
        status: TournamentStatus::Open,
        open_at: Some(now),
    };
    let erin = api.signup_player("erin").await;
    let dave = api.player("dave").await;

    let added = storage
        .add_entrant(
            &NewEntrant {
                id: EntrantId::generate(),
                tournament: created.id,
                player: erin.id,
                skill: Some(SkillLevel::try_new(2).unwrap()),
                registered_at: now,
            },
            read_while_open,
        )
        .await
        .unwrap();
    let retired = storage
        .remove_entrant_of(created.id, dave.id, read_while_open)
        .await
        .unwrap();
    let relevelled = storage
        .set_skill(
            created.id,
            dave.id,
            SkillLevel::try_new(5).unwrap(),
            read_while_open,
        )
        .await
        .unwrap();

    assert_eq!(added, Enrolled::Refused);
    assert!(!retired, "the live tournament keeps its entrant");
    assert!(!relevelled);
    let entrants = storage.entrants(created.id).await.unwrap();
    assert_eq!(entrants.len(), 1);
    assert_eq!(entrants[0].skill.map(SkillLevel::value), Some(3));
}

#[tokio::test]
async fn a_passed_deadline_refuses_the_write_that_read_it_open() {
    let api = TestApi::with_database().await;
    let admin = api.signup_admin("root").await;
    let created = api.create_tournament(&admin, 1).await;
    api.set_status(&admin, created.id, "open").await;
    let erin = api.signup_player("erin").await;
    let storage = TournamentStorage::new(api.pool().clone());
    let after_the_deadline = OffsetDateTime::now_utc() + time::Duration::seconds(2);

    let added = storage
        .add_entrant(
            &NewEntrant {
                id: EntrantId::generate(),
                tournament: created.id,
                player: erin.id,
                skill: None,
                registered_at: OffsetDateTime::now_utc(),
            },
            EntrantGuard {
                status: TournamentStatus::Open,
                open_at: Some(after_the_deadline),
            },
        )
        .await
        .unwrap();

    assert_eq!(added, Enrolled::Refused);
}

#[tokio::test]
async fn a_result_over_a_rebuilt_bracket_is_refused() {
    let api = TestApi::with_database().await;
    let admin = api.signup_admin("root").await;
    let created = api.create_tournament(&admin, HOUR).await;
    api.add_entrant(&admin, created.id, "dave").await;
    api.add_entrant(&admin, created.id, "erin").await;
    api.set_status(&admin, created.id, "live").await;
    let read = api.generate_bracket(&admin, created.id).await;
    let rebuilt = api.generate_bracket(&admin, created.id).await;
    let storage = TournamentStorage::new(api.pool().clone());
    let before = read.rounds[0][0].clone();
    let mut after = before.clone();
    after.winner = before.entrant_a;

    let written = storage
        .update_matches(created.id, TournamentStatus::Live, &[(&before, &after)])
        .await
        .unwrap();

    assert!(!written, "the match of the old bracket is gone");
    let stored = Bracket::from_matches(storage.matches(created.id).await.unwrap()).unwrap();
    assert_eq!(stored, rebuilt, "the rebuilt bracket is untouched");
}

#[tokio::test]
async fn a_result_from_a_stale_read_of_its_match_is_refused() {
    let api = TestApi::with_database().await;
    let admin = api.signup_admin("root").await;
    let created = api.create_tournament(&admin, HOUR).await;
    api.add_entrant(&admin, created.id, "dave").await;
    api.add_entrant(&admin, created.id, "erin").await;
    api.set_status(&admin, created.id, "live").await;
    let read = api.generate_bracket(&admin, created.id).await;
    let final_match = read.rounds[0][0].clone();
    api.report(
        &admin,
        created.id,
        &final_match,
        final_match.entrant_b.unwrap(),
    )
    .await;
    let storage = TournamentStorage::new(api.pool().clone());
    let mut stale = final_match.clone();
    stale.winner = final_match.entrant_a;

    let written = storage
        .update_matches(
            created.id,
            TournamentStatus::Live,
            &[(&final_match, &stale)],
        )
        .await
        .unwrap();

    assert!(!written, "the row no longer holds the value that was read");
    let stored = storage.matches(created.id).await.unwrap();
    assert_eq!(stored[0].winner, final_match.entrant_b);
}

#[tokio::test]
async fn a_rebuild_or_a_removal_over_a_result_is_locked() {
    let api = TestApi::with_database().await;
    let admin = api.signup_admin("root").await;
    let created = api.create_tournament(&admin, HOUR).await;
    let dave = api.add_entrant(&admin, created.id, "dave").await;
    let erin = api.add_entrant(&admin, created.id, "erin").await;
    api.set_status(&admin, created.id, "live").await;
    let read = api.generate_bracket(&admin, created.id).await;
    let final_match = read.rounds[0][0].clone();
    api.report(
        &admin,
        created.id,
        &final_match,
        final_match.entrant_a.unwrap(),
    )
    .await;
    let storage = TournamentStorage::new(api.pool().clone());
    let order = [erin.id, dave.id];

    let rebuilt = storage
        .replace_bracket(
            created.id,
            TournamentStatus::Live,
            &order,
            &Bracket::generate(&order).unwrap(),
        )
        .await
        .unwrap();
    let removed = storage
        .delete_bracket(created.id, TournamentStatus::Live)
        .await
        .unwrap();
    let from_open = storage
        .delete_bracket(created.id, TournamentStatus::Open)
        .await
        .unwrap();

    assert_eq!(rebuilt, BracketWrite::Locked);
    assert_eq!(removed, BracketWrite::Locked);
    assert_eq!(from_open, BracketWrite::Changed);
    assert_eq!(storage.matches(created.id).await.unwrap().len(), 1);
}

#[tokio::test]
async fn a_status_move_from_a_stale_status_pays_nothing() {
    let api = TestApi::with_database().await;
    let admin = api.signup_admin("root").await;
    let created = api.create_tournament(&admin, HOUR).await;
    api.add_entrant(&admin, created.id, "dave").await;
    api.set_status(&admin, created.id, "live").await;
    let storage = TournamentStorage::new(api.pool().clone());
    let entrants = storage.entrants(created.id).await.unwrap();
    let awards = tournament_awards(created.id, &entrants, None, None);

    let moved = storage
        .set_status(
            created.id,
            TournamentStatus::Open,
            TournamentStatus::Concluded,
            None,
            &awards,
            OffsetDateTime::now_utc(),
        )
        .await
        .unwrap();

    assert!(!moved);
    let (paid,): (i64,) = sqlx::query_as("select count(*) from point_entries")
        .fetch_one(api.pool())
        .await
        .unwrap();
    assert_eq!(paid, 0);
    assert_eq!(
        api.detail(&admin, created.id).await.tournament.status,
        TournamentStatus::Live
    );
}

/// A writer that cannot get the lock in time is busy, which a client may
/// retry, and not a bug.
#[tokio::test]
async fn a_held_write_lock_is_busy() {
    let (dir, _pool) = migrated_pool().await;
    let url = format!("sqlite://{}", dir.path().join("test.db").display());
    let options = SqliteConnectOptions::from_str(&url)
        .unwrap()
        .busy_timeout(Duration::from_millis(10));
    let mut holder = SqliteConnection::connect_with(&options).await.unwrap();
    let mut waiter = SqliteConnection::connect_with(&options).await.unwrap();
    sqlx::query("begin immediate")
        .execute(&mut holder)
        .await
        .unwrap();

    let error = sqlx::query("begin immediate")
        .execute(&mut waiter)
        .await
        .unwrap_err();

    let classified = StorageError::from_query(error);
    assert!(
        matches!(classified, StorageError::Busy(_)),
        "got {classified:?}"
    );
}

/// A file the process cannot write is a database it cannot use, like a disk
/// that failed. It is not a bad query.
#[tokio::test]
async fn a_read_only_database_is_a_connection_failure() {
    let (dir, _pool) = migrated_pool().await;
    let url = format!("sqlite://{}?mode=ro", dir.path().join("test.db").display());
    let mut reader = SqliteConnection::connect_with(&SqliteConnectOptions::from_str(&url).unwrap())
        .await
        .unwrap();

    let error = sqlx::query(
        "insert into players (id, handle, password_hash, glyph_bits, glyph_color, created_at)
         values ('00000000-0000-0000-0000-000000000001', 'dave', 'x', 1, '#ffb000', 1)",
    )
    .execute(&mut reader)
    .await
    .unwrap_err();

    let classified = StorageError::from_query(error);
    assert!(
        matches!(classified, StorageError::Connection(_)),
        "got {classified:?}"
    );
}

#[tokio::test]
async fn a_row_that_points_at_nothing_is_a_foreign_key_violation() {
    let (_dir, pool) = migrated_pool().await;

    let error = sqlx::query(
        "insert into event_checkins (event_id, player_id, checked_in_at)
         values ('00000000-0000-0000-0000-000000000001', '00000000-0000-0000-0000-000000000002', 1)",
    )
    .execute(&pool)
    .await
    .unwrap_err();

    let classified = StorageError::from_query(error);
    assert!(
        matches!(classified, StorageError::ForeignKeyViolation(_)),
        "got {classified:?}"
    );
}
