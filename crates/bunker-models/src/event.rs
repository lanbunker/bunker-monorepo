//! Events: the nights the bunker opens. An event has a window, from the doors
//! to the last game, and a check-in code for the QR at the door. A player who
//! scans it inside the window is checked in one time and gets cycles for it.

use nutype::nutype;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use utoipa::ToSchema;
use uuid::Uuid;

use super::player::{Player, PlayerId};
use super::tournament::Description;

pub const EVENT_NAME_MAX_LEN: usize = 60;
pub const LOCATION_MAX_LEN: usize = 60;
pub const GAMES_MAX_LEN: usize = 200;
pub const IMAGE_NAME_MAX_LEN: usize = 80;
pub const CHECKIN_CODE_LEN: usize = 12;

#[nutype(derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    Display,
    Serialize,
    Deserialize
))]
pub struct EventId(Uuid);

impl EventId {
    pub fn generate() -> Self {
        Self::new(Uuid::new_v4())
    }
}

#[nutype(
    sanitize(trim),
    validate(len_char_min = 1, len_char_max = EVENT_NAME_MAX_LEN),
    derive(Debug, Clone, PartialEq, Eq, Hash, Display, AsRef, Serialize, Deserialize)
)]
pub struct EventName(String);

/// Where the night happens. Free text, and empty until it is decided.
#[nutype(
    sanitize(trim),
    validate(len_char_max = LOCATION_MAX_LEN),
    derive(Debug, Clone, PartialEq, Eq, Display, AsRef, Serialize, Deserialize, Default),
    default = ""
)]
pub struct Location(String);

/// What is played, as one line of text.
#[nutype(
    sanitize(trim),
    validate(len_char_max = GAMES_MAX_LEN),
    derive(Debug, Clone, PartialEq, Eq, Display, AsRef, Serialize, Deserialize, Default),
    default = ""
)]
pub struct Games(String);

/// The file name of a cover under the images of the site: no path, no space.
/// The site owns the files, and a name it does not know renders no cover.
#[nutype(
    sanitize(trim),
    validate(predicate = is_image_name),
    derive(Debug, Clone, PartialEq, Eq, Display, AsRef, Serialize, Deserialize)
)]
pub struct ImageName(String);

fn is_image_name(value: &str) -> bool {
    (1..=IMAGE_NAME_MAX_LEN).contains(&value.chars().count())
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

/// The secret in the check-in link. Lowercase letters and digits only, so it
/// survives a QR code, a phone keyboard and a URL untouched.
#[nutype(
    validate(predicate = is_checkin_code),
    derive(Debug, Clone, PartialEq, Eq, Hash, Display, AsRef, Serialize, Deserialize)
)]
pub struct CheckinCode(String);

fn is_checkin_code(value: &str) -> bool {
    value.len() == CHECKIN_CODE_LEN
        && value
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
}

impl CheckinCode {
    /// The characters a generated code is made of.
    pub const ALPHABET: &'static [u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";
}

/// `Draft` is visible to admins only, and its code opens nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum EventStatus {
    Draft,
    Published,
}

impl EventStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Published => "published",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("unknown event status `{0}`")]
pub struct UnknownEventStatus(String);

impl std::str::FromStr for EventStatus {
    type Err = UnknownEventStatus;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        match raw {
            "draft" => Ok(Self::Draft),
            "published" => Ok(Self::Published),
            other => Err(UnknownEventStatus(other.to_owned())),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("the end must come after the start")]
pub struct EventWindowError;

/// Where `now` sits against the window of an event. The door pays only while
/// it is `Open`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum CheckinWindow {
    Early,
    Open,
    Over,
}

/// An event as every client sees it. The check-in code is not here: only an
/// admin reads it, through [`EventDetail`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Event {
    pub id: EventId,
    pub name: EventName,
    pub location: Location,
    pub games: Games,
    pub description: Description,
    pub image: Option<ImageName>,
    /// The doors open, and with them the check-in.
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub starts_at: OffsetDateTime,
    /// The last game. Past it the check-in is over.
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub ends_at: OffsetDateTime,
    pub status: EventStatus,
    pub checkin_count: u32,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub created_at: OffsetDateTime,
}

impl Event {
    /// The check-in opens with the doors and closes with the last game.
    pub fn checkin_window(&self, now: OffsetDateTime) -> CheckinWindow {
        if now < self.starts_at {
            CheckinWindow::Early
        } else if now < self.ends_at {
            CheckinWindow::Open
        } else {
            CheckinWindow::Over
        }
    }
}

/// Body of `POST /api/admin/events` and of `PUT /api/admin/events/{id}`. A
/// put replaces every field, so an absent cover clears the cover. A new event
/// is always a draft.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EventFields {
    pub name: EventName,
    #[serde(default)]
    pub location: Location,
    #[serde(default)]
    pub games: Games,
    #[serde(default)]
    pub description: Description,
    #[serde(default)]
    pub image: Option<ImageName>,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub starts_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub ends_at: OffsetDateTime,
}

impl EventFields {
    /// The one rule two fields share, so no single field can hold it: the
    /// night ends after it starts.
    pub fn check_window(&self) -> Result<(), EventWindowError> {
        if self.ends_at <= self.starts_at {
            return Err(EventWindowError);
        }
        Ok(())
    }
}

/// Body of `POST /api/admin/events/{id}/status`.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EventStatusChange {
    pub status: EventStatus,
}

/// Body of `POST /api/admin/events/{id}/checkins`: an admin checks a player in
/// by hand, for a night that is over or a phone that did not scan.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CheckinAdd {
    pub player_id: PlayerId,
}

/// One player at the door. The row goes with the player, so the player is
/// always there.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Checkin {
    pub player: Player,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub checked_in_at: OffsetDateTime,
}

/// Answer of `GET /api/admin/events/{id}`: the event, the code for the QR at
/// the door, and everyone who came.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct EventDetail {
    pub event: Event,
    pub checkin_code: CheckinCode,
    /// First at the door first.
    pub checkins: Vec<Checkin>,
}

/// Answer of `GET /api/checkin/{code}`: what the door shows before a player
/// confirms. The server decides the window, so no client reads a clock.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CheckinGate {
    pub event: Event,
    pub window: CheckinWindow,
}

/// Answer of `POST /api/checkin/{code}`. A repeat answers the time of the
/// first check-in and pays nothing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CheckinReceipt {
    pub event: Event,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub checked_in_at: OffsetDateTime,
    /// What this call paid: the check-in cycles the first time, zero after.
    pub cycles: i64,
}

/// Answer of `GET /api/me/checkins`: the events the caller checked in to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Checkins {
    pub events: Vec<EventId>,
}
