//! The tournament wire types, from the contract.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use bunker_models::{
    Description, GameMode, GameName, NewTournament, StatusChange, TournamentName, TournamentStatus,
    TournamentUpdate,
};
use time::macros::date;

#[test]
fn names_are_trimmed_and_bounded() {
    assert_eq!(
        TournamentName::try_new("  Sniper Cup ").unwrap().as_ref(),
        "Sniper Cup"
    );
    assert!(TournamentName::try_new("   ").is_err());
    assert!(TournamentName::try_new("x".repeat(60)).is_ok());
    assert!(TournamentName::try_new("x".repeat(61)).is_err());
    assert!(GameName::try_new("x".repeat(41)).is_err());
    assert!(GameMode::try_new("").is_err());
    assert!(Description::try_new("").is_ok());
    assert!(Description::try_new("x".repeat(1001)).is_err());
}

#[test]
fn a_status_round_trips_through_text() {
    for status in [
        TournamentStatus::Draft,
        TournamentStatus::Open,
        TournamentStatus::Live,
        TournamentStatus::Concluded,
    ] {
        assert_eq!(status.as_str().parse::<TournamentStatus>().unwrap(), status);
    }
    assert!("closed".parse::<TournamentStatus>().is_err());
}

#[test]
fn a_new_tournament_reads_an_iso_day_and_an_rfc3339_deadline() {
    let body: NewTournament = serde_json::from_str(
        r#"{"name":"Sniper Cup","game":"COD MW2","mode":"1v1","date":"2026-10-24","registrationClosesAt":"2026-10-23T21:59:00Z"}"#,
    )
    .unwrap();

    assert_eq!(body.date, date!(2026 - 10 - 24));
    assert_eq!(body.description.as_ref(), "", "the description is optional");
    assert!(
        serde_json::from_str::<NewTournament>(
            r#"{"name":"Sniper Cup","game":"COD MW2","mode":"1v1","date":"24/10/2026","registrationClosesAt":"2026-10-23T21:59:00Z"}"#
        )
        .is_err(),
        "a European date is refused"
    );
}

#[test]
fn an_update_accepts_nothing_and_refuses_a_stranger() {
    let empty: TournamentUpdate = serde_json::from_str("{}").unwrap();
    assert!(empty.name.is_none() && empty.registration_closes_at.is_none());

    let one: TournamentUpdate = serde_json::from_str(r#"{"mode":"2v2"}"#).unwrap();
    assert_eq!(one.mode.unwrap().as_ref(), "2v2");

    assert!(serde_json::from_str::<TournamentUpdate>(r#"{"status":"open"}"#).is_err());
}

#[test]
fn a_status_change_carries_an_optional_winner() {
    let plain: StatusChange = serde_json::from_str(r#"{"status":"open"}"#).unwrap();
    assert_eq!(plain.status, TournamentStatus::Open);
    assert!(plain.winner.is_none());

    let with_winner: StatusChange = serde_json::from_str(
        r#"{"status":"concluded","winner":"00000000-0000-0000-0000-000000000001"}"#,
    )
    .unwrap();
    assert!(with_winner.winner.is_some());
}

#[test]
fn a_day_serializes_as_iso_text() {
    let day = date!(2026 - 10 - 24);
    assert_eq!(serde_json::to_string(&day).unwrap(), "\"2026-10-24\"");
}
