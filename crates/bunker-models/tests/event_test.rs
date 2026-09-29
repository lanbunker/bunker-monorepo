//! Events from the contract: the window, the code, the cover and the status.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use bunker_models::{
    CheckinCode, CheckinWindow, Description, Event, EventFields, EventId, EventName, EventStatus,
    EventWindow, Games, ImageName, Location,
};
use time::{Duration, OffsetDateTime};

fn at(hours: i64) -> OffsetDateTime {
    OffsetDateTime::UNIX_EPOCH + Duration::hours(hours)
}

fn night(starts: i64, ends: i64) -> Event {
    Event {
        id: EventId::generate(),
        name: EventName::try_new("BUNKER//SESSION 04").unwrap(),
        location: Location::default(),
        games: Games::default(),
        description: Description::default(),
        image: None,
        window: EventWindow::try_new(at(starts), at(ends)).unwrap(),
        status: EventStatus::Published,
        checkin_count: 0,
        created_at: at(0),
    }
}

#[test]
fn the_door_is_open_from_the_start_to_the_end() {
    let event = night(21, 27);

    assert_eq!(event.checkin_window(at(20)), CheckinWindow::Early);
    assert_eq!(
        event.checkin_window(at(21)),
        CheckinWindow::Open,
        "the start is in"
    );
    assert_eq!(event.checkin_window(at(24)), CheckinWindow::Open);
    assert_eq!(
        event.checkin_window(at(27)),
        CheckinWindow::Over,
        "the end is out"
    );
    assert_eq!(event.checkin_window(at(30)), CheckinWindow::Over);
}

#[test]
fn a_window_needs_its_end_after_its_start() {
    assert!(EventWindow::try_new(at(19), at(25)).is_ok());
    assert!(EventWindow::try_new(at(19), at(18)).is_err());
    assert!(EventWindow::try_new(at(19), at(19)).is_err(), "zero length");

    let fields = |starts: &str, ends: &str| {
        serde_json::from_value::<EventFields>(serde_json::json!({
            "name": "Night",
            "startsAt": starts,
            "endsAt": ends,
        }))
    };
    let parsed = fields("2026-10-24T19:00:00Z", "2026-10-25T01:30:00Z").unwrap();
    assert!(parsed.window.starts_at() < parsed.window.ends_at());
    assert!(
        fields("2026-10-24T19:00:00Z", "2026-10-24T18:00:00Z").is_err(),
        "a body out of order does not parse"
    );
    assert!(fields("2026-10-24T19:00:00Z", "2026-10-24T19:00:00Z").is_err());
}

#[test]
fn an_event_keeps_its_window_flat_on_the_wire() {
    let event = night(21, 27);

    let json = serde_json::to_value(&event).unwrap();
    assert_eq!(json["startsAt"], "1970-01-01T21:00:00Z");
    assert_eq!(json["endsAt"], "1970-01-02T03:00:00Z");
    assert!(json.get("window").is_none());
    assert_eq!(
        serde_json::from_value::<Event>(json.clone()).unwrap(),
        event
    );

    let mut backwards = json;
    backwards["endsAt"] = serde_json::json!("1970-01-01T20:00:00Z");
    assert!(serde_json::from_value::<Event>(backwards).is_err());
}

#[test]
fn the_fields_fill_in_what_the_body_leaves_out_and_refuse_a_stranger() {
    let fields: EventFields = serde_json::from_value(serde_json::json!({
        "name": "  Night  ",
        "startsAt": "2026-10-24T19:00:00Z",
        "endsAt": "2026-10-25T01:30:00Z",
    }))
    .unwrap();
    assert_eq!(fields.name.as_ref(), "Night", "the name is trimmed");
    assert_eq!(fields.location.as_ref(), "");
    assert_eq!(fields.games.as_ref(), "");
    assert!(fields.image.is_none());

    let unknown = serde_json::from_value::<EventFields>(serde_json::json!({
        "name": "Night",
        "startsAt": "2026-10-24T19:00:00Z",
        "endsAt": "2026-10-25T01:30:00Z",
        "checkinCode": "abcdefghij12",
    }));
    assert!(unknown.is_err(), "a client cannot pick the code");
}

#[test]
fn a_code_is_twelve_lowercase_letters_or_digits() {
    assert!(CheckinCode::try_new("abcdefghij12").is_ok());
    assert!(CheckinCode::try_new("abcdefghij1").is_err(), "too short");
    assert!(CheckinCode::try_new("abcdefghij123").is_err(), "too long");
    assert!(CheckinCode::try_new("ABCDEFGHIJ12").is_err(), "uppercase");
    assert!(CheckinCode::try_new("abcdefghij-2").is_err(), "punctuation");
    assert!(
        CheckinCode::ALPHABET
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit()),
        "the alphabet only makes valid codes"
    );
}

#[test]
fn a_cover_is_a_file_name_and_never_a_path() {
    assert!(ImageName::try_new("feb2026-cover.webp").is_ok());
    assert!(ImageName::try_new(" cover.webp ").is_ok(), "trimmed");
    assert!(ImageName::try_new("").is_err());
    assert!(ImageName::try_new("../secret.webp").is_err());
    assert!(ImageName::try_new("events/cover.webp").is_err());
    assert!(ImageName::try_new("cover image.webp").is_err(), "a space");
    assert!(ImageName::try_new("x".repeat(81)).is_err());
    for dotted in [".", "..", ".hidden", "-cover.webp", "_cover.webp"] {
        assert!(
            ImageName::try_new(dotted).is_err(),
            "{dotted} does not start with a letter or a digit"
        );
    }
    assert!(ImageName::try_new("9.webp").is_ok());
}
