//! The enums that a client and the database read by name. Each one spells its
//! names three times: the serde rename, `as_str` and `FromStr`. These tests
//! hold the three together.
//!
//! Each `name` below is a match with no wildcard, so a new variant does not
//! compile here until it has a name, and the count next to it must follow.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::HashSet;
use std::fmt::Debug;
use std::str::FromStr;

use bunker_models::{EventStatus, PointKind, Rank, Role, TournamentStatus};
use serde::Serialize;
use serde::de::DeserializeOwned;

/// The wire name, the stored name and the parse all agree with `expected`.
fn assert_spelled<T>(value: T, expected: &str, stored: &str)
where
    T: Copy + Debug + PartialEq + Serialize + DeserializeOwned + FromStr,
    T::Err: Debug,
{
    assert_eq!(
        serde_json::to_value(value).unwrap(),
        serde_json::json!(expected),
        "the wire name of {value:?}"
    );
    assert_eq!(stored, expected, "the stored name of {value:?}");
    assert_eq!(stored.parse::<T>().unwrap(), value, "{stored} parses back");
    assert_eq!(
        serde_json::from_value::<T>(serde_json::json!(expected)).unwrap(),
        value
    );
}

#[test]
fn every_point_kind_is_listed_and_spelled_one_way() {
    let name = |kind: PointKind| match kind {
        PointKind::Checkin => "checkin",
        PointKind::TournamentEntry => "tournament_entry",
        PointKind::MatchWin => "match_win",
        PointKind::Champion => "champion",
        PointKind::Finalist => "finalist",
        PointKind::Semifinalist => "semifinalist",
        PointKind::Adjustment => "adjustment",
    };
    let count = 7;

    assert_eq!(PointKind::ALL.len(), count, "a kind is missing from ALL");
    for kind in PointKind::ALL {
        assert_spelled(kind, name(kind), kind.as_str());
    }
    let unique: HashSet<PointKind> = PointKind::ALL.into_iter().collect();
    assert_eq!(unique.len(), count, "ALL lists a kind twice");
}

#[test]
fn every_rank_is_listed_bottom_first_and_spelled_one_way() {
    let name = |rank: Rank| match rank {
        Rank::Zombie => "zombie",
        Rank::Guest => "guest",
        Rank::User => "user",
        Rank::Sudoer => "sudoer",
        Rank::Daemon => "daemon",
        Rank::Kernel => "kernel",
    };
    let count = 6;

    assert_eq!(Rank::ALL.len(), count, "a rank is missing from ALL");
    assert!(Rank::ALL.is_sorted(), "ALL is bottom first");
    for rank in Rank::ALL {
        assert_eq!(serde_json::to_value(rank).unwrap(), name(rank));
        assert_eq!(
            serde_json::from_value::<Rank>(serde_json::json!(name(rank))).unwrap(),
            rank
        );
    }
}

#[test]
fn every_role_is_spelled_one_way() {
    let name = |role: Role| match role {
        Role::User => "user",
        Role::Admin => "admin",
    };

    for role in [Role::User, Role::Admin] {
        assert_spelled(role, name(role), role.as_str());
    }
    assert!("root".parse::<Role>().is_err());
}

#[test]
fn every_event_status_is_spelled_one_way() {
    let name = |status: EventStatus| match status {
        EventStatus::Draft => "draft",
        EventStatus::Published => "published",
    };

    for status in [EventStatus::Draft, EventStatus::Published] {
        assert_spelled(status, name(status), status.as_str());
    }
    assert!("open".parse::<EventStatus>().is_err());
}

#[test]
fn every_tournament_status_is_spelled_one_way() {
    let name = |status: TournamentStatus| match status {
        TournamentStatus::Draft => "draft",
        TournamentStatus::Open => "open",
        TournamentStatus::Live => "live",
        TournamentStatus::Concluded => "concluded",
    };

    for status in [
        TournamentStatus::Draft,
        TournamentStatus::Open,
        TournamentStatus::Live,
        TournamentStatus::Concluded,
    ] {
        assert_spelled(status, name(status), status.as_str());
    }
    assert!("closed".parse::<TournamentStatus>().is_err());
}
