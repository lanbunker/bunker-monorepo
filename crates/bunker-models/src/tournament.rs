use nutype::nutype;
use serde::{Deserialize, Serialize};
use time::{Date, OffsetDateTime};
use utoipa::ToSchema;
use uuid::Uuid;

use super::bracket::Bracket;
use super::player::{Player, PlayerId};

pub const TOURNAMENT_NAME_MAX_LEN: usize = 60;
pub const GAME_NAME_MAX_LEN: usize = 40;
pub const GAME_MODE_MAX_LEN: usize = 30;
pub const DESCRIPTION_MAX_LEN: usize = 1000;

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
pub struct TournamentId(Uuid);

impl TournamentId {
    pub fn generate() -> Self {
        Self::new(Uuid::new_v4())
    }
}

/// One entry in a tournament. Matches point at entrants and not at players, so a
/// team can enter one day without a new bracket model.
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
pub struct EntrantId(Uuid);

impl EntrantId {
    pub fn generate() -> Self {
        Self::new(Uuid::new_v4())
    }
}

#[nutype(
    sanitize(trim),
    validate(len_char_min = 1, len_char_max = TOURNAMENT_NAME_MAX_LEN),
    derive(Debug, Clone, PartialEq, Eq, Hash, Display, AsRef, Serialize, Deserialize)
)]
pub struct TournamentName(String);

#[nutype(
    sanitize(trim),
    validate(len_char_min = 1, len_char_max = GAME_NAME_MAX_LEN),
    derive(Debug, Clone, PartialEq, Eq, Hash, Display, AsRef, Serialize, Deserialize)
)]
pub struct GameName(String);

/// How the game is played, such as `1v1 sniper only`. Short, it sits in a title.
#[nutype(
    sanitize(trim),
    validate(len_char_min = 1, len_char_max = GAME_MODE_MAX_LEN),
    derive(Debug, Clone, PartialEq, Eq, Hash, Display, AsRef, Serialize, Deserialize)
)]
pub struct GameMode(String);

#[nutype(
    sanitize(trim),
    validate(len_char_max = DESCRIPTION_MAX_LEN),
    derive(Debug, Clone, PartialEq, Eq, Hash, Display, AsRef, Serialize, Deserialize, Default),
    default = ""
)]
pub struct Description(String);

/// The life of a tournament. `Draft` is visible to admins only. `Open` takes
/// registrations until the deadline. `Live` freezes the entrants for the
/// bracket. `Concluded` is final.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum TournamentStatus {
    Draft,
    Open,
    Live,
    Concluded,
}

impl TournamentStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Open => "open",
            Self::Live => "live",
            Self::Concluded => "concluded",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("unknown tournament status `{0}`")]
pub struct UnknownStatus(String);

impl std::str::FromStr for TournamentStatus {
    type Err = UnknownStatus;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        match raw {
            "draft" => Ok(Self::Draft),
            "open" => Ok(Self::Open),
            "live" => Ok(Self::Live),
            "concluded" => Ok(Self::Concluded),
            other => Err(UnknownStatus(other.to_owned())),
        }
    }
}

/// A tournament as every client sees it. `entrant_count` and `has_bracket` ride
/// along so a list needs no second call per row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Tournament {
    pub id: TournamentId,
    pub name: TournamentName,
    pub game: GameName,
    pub mode: GameMode,
    pub description: Description,
    /// The day of the event. A day has no timezone.
    #[schema(value_type = String, format = Date)]
    pub date: Date,
    /// Registrations close at this instant. Past it, nobody applies or retires.
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub registration_closes_at: OffsetDateTime,
    pub status: TournamentStatus,
    pub entrant_count: u32,
    pub has_bracket: bool,
    pub winner: Option<Entrant>,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub created_at: OffsetDateTime,
}

/// Body of `POST /api/admin/tournaments`. A new tournament is always a draft.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NewTournament {
    pub name: TournamentName,
    pub game: GameName,
    pub mode: GameMode,
    #[serde(default)]
    pub description: Description,
    #[schema(value_type = String, format = Date)]
    pub date: Date,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub registration_closes_at: OffsetDateTime,
}

/// Body of `PATCH /api/admin/tournaments/{id}`. An absent field keeps its value.
#[derive(Debug, Clone, Default, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TournamentUpdate {
    pub name: Option<TournamentName>,
    pub game: Option<GameName>,
    pub mode: Option<GameMode>,
    pub description: Option<Description>,
    #[schema(value_type = Option<String>, format = Date)]
    pub date: Option<Date>,
    #[serde(default, with = "time::serde::rfc3339::option")]
    #[schema(value_type = Option<String>, format = DateTime)]
    pub registration_closes_at: Option<OffsetDateTime>,
}

/// Body of `POST /api/admin/tournaments/{id}/status`. A winner is accepted only
/// with `concluded` and only when the tournament has no bracket.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StatusChange {
    pub status: TournamentStatus,
    pub winner: Option<EntrantId>,
}

/// A registration. `player` is absent after the player deleted their account,
/// so an old bracket keeps its shape.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Entrant {
    pub id: EntrantId,
    pub player: Option<Player>,
    /// Set by the bracket. Seed 1 is the first in the generated order.
    pub seed: Option<u32>,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub registered_at: OffsetDateTime,
}

/// Body of `POST /api/admin/tournaments/{id}/entrants`.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EntrantAdd {
    pub player_id: PlayerId,
}

/// Body of `PUT /api/admin/tournaments/{id}/seeds`: every entrant exactly once,
/// seed 1 first.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SeedOrder {
    pub entrants: Vec<EntrantId>,
}

/// Body of `PUT /api/admin/tournaments/{id}/matches/{matchId}/result`.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MatchResult {
    pub winner: EntrantId,
}

/// Answer of `GET /api/me/registrations`: the tournaments the caller entered.
/// One call tells a page which apply buttons to turn into retire buttons.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Registrations {
    pub tournaments: Vec<TournamentId>,
}

/// One tournament with everything a page needs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TournamentDetail {
    pub tournament: Tournament,
    pub entrants: Vec<Entrant>,
    pub bracket: Option<Bracket>,
}
