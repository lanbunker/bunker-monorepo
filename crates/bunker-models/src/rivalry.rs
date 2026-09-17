//! The match history of a player, and who beats them the most. A record is
//! always from the view of the profile owner: `wins` are theirs.

use serde::{Deserialize, Serialize};
use time::Date;
use utoipa::ToSchema;

use super::bracket::MatchId;
use super::pagination::Paginated;
use super::player::Player;
use super::tournament::{GameName, TournamentId, TournamentName};

/// How many times one opponent must beat a player before the profile names
/// them. One loss is a bad night, two is a pattern.
pub const NEMESIS_MIN_LOSSES: u32 = 2;

/// Wins and losses over every played match. `Record` alone is the name of a
/// built-in type in TypeScript, and the site generates its types from this one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MatchRecord {
    pub wins: u32,
    pub losses: u32,
}

/// A head-to-head record against one opponent, from the view of the profile
/// owner.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Rivalry {
    pub opponent: Player,
    pub wins: u32,
    pub losses: u32,
}

/// One played match from the view of the profile owner.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PlayedMatch {
    pub id: MatchId,
    pub tournament_id: TournamentId,
    pub tournament_name: TournamentName,
    pub game: GameName,
    /// The day of the tournament. A match carries no clock of its own.
    #[schema(value_type = String, format = Date)]
    pub date: Date,
    /// 1 is the first round.
    pub round: u32,
    /// How many rounds the bracket has, so a client can name the final.
    pub rounds: u32,
    /// Absent once the opponent deleted their account.
    pub opponent: Option<Player>,
    pub won: bool,
}

/// Answer of `GET /api/players/{handle}/matches`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MatchLog {
    /// Over every played match, not only the page.
    pub record: MatchRecord,
    /// Absent until one opponent reaches [`NEMESIS_MIN_LOSSES`] wins over the
    /// player.
    pub nemesis: Option<Rivalry>,
    pub matches: Paginated<PlayedMatch>,
}
