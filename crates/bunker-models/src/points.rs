//! Cycles: the points a player collects. The ledger is a list of signed
//! entries, and everything else is derived: the total is a sum, the rank is a
//! threshold on the total, the place is the position among all players.
//!
//! The rules that turn a tournament into entries are here as pure functions, so
//! they are tested without a database and any crate can reuse them.

use nutype::nutype;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use utoipa::ToSchema;
use uuid::Uuid;

use super::bracket::Bracket;
use super::pagination::Paginated;
use super::player::PlayerId;
use super::tournament::{Entrant, EntrantId, TournamentId, TournamentName};

// The one place that says how many cycles anything pays. `CyclesRules::current`
// sends these to every client, so a legend on the site can never disagree with
// what the ledger writes.

/// Every entrant of a concluded tournament gets this for being there.
pub const ENTRY_CYCLES: i64 = 40;
/// Each match won in the bracket. A bye is not a win.
pub const MATCH_WIN_CYCLES: i64 = 30;

/// The size of the field. A placement pays more in a bigger field. The cuts sit
/// on bracket sizes, so a tier changes where a round is added, and the top
/// tier is a cap: forty players are not twice the cup of twenty.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, ToSchema,
)]
#[serde(rename_all = "lowercase")]
pub enum FieldTier {
    Small,
    Medium,
    Large,
}

impl FieldTier {
    pub const ALL: [Self; 3] = [Self::Small, Self::Medium, Self::Large];

    /// The smallest field of the tier.
    pub const fn min_entrants(self) -> u32 {
        match self {
            Self::Small => 2,
            Self::Medium => 8,
            Self::Large => 16,
        }
    }

    pub fn for_entrants(count: usize) -> Self {
        Self::ALL
            .iter()
            .rev()
            .copied()
            .find(|tier| count >= tier.min_entrants() as usize)
            .unwrap_or(Self::Small)
    }
}

/// The largest single adjustment an admin can make, in either direction.
pub const ADJUSTMENT_MAX: i64 = 10_000;
pub const NOTE_MAX_LEN: usize = 200;

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
pub struct PointEntryId(Uuid);

impl PointEntryId {
    pub fn generate() -> Self {
        Self::new(Uuid::new_v4())
    }
}

/// A signed amount of cycles for one admin adjustment. Zero changes nothing
/// and is refused, so every row in the ledger means something.
#[nutype(
    validate(predicate = is_adjustment_amount),
    derive(Debug, Clone, Copy, PartialEq, Eq, Display, Serialize, Deserialize)
)]
pub struct Amount(i64);

fn is_adjustment_amount(value: &i64) -> bool {
    *value != 0 && value.abs() <= ADJUSTMENT_MAX
}

/// Why an admin gave or took cycles. Shown to the player.
#[nutype(
    sanitize(trim),
    validate(len_char_min = 1, len_char_max = NOTE_MAX_LEN),
    derive(Debug, Clone, PartialEq, Eq, Display, AsRef, Serialize, Deserialize)
)]
pub struct Note(String);

/// The tiers of the ladder, in order. A rank never goes down on its own: only
/// an adjustment can take cycles away.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, ToSchema,
)]
#[serde(rename_all = "lowercase")]
pub enum Rank {
    Zombie,
    Guest,
    User,
    Sudoer,
    Daemon,
    Kernel,
}

impl Rank {
    pub const ALL: [Self; 6] = [
        Self::Zombie,
        Self::Guest,
        Self::User,
        Self::Sudoer,
        Self::Daemon,
        Self::Kernel,
    ];

    /// The cycles a player needs for this rank.
    pub const fn floor(self) -> i64 {
        match self {
            Self::Zombie => 0,
            Self::Guest => 100,
            Self::User => 600,
            Self::Sudoer => 1500,
            Self::Daemon => 3000,
            Self::Kernel => 6000,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Zombie => "zombie",
            Self::Guest => "guest",
            Self::User => "user",
            Self::Sudoer => "sudoer",
            Self::Daemon => "daemon",
            Self::Kernel => "kernel",
        }
    }

    /// The highest rank whose floor the total reaches. A negative total is a
    /// zombie.
    pub fn for_cycles(cycles: i64) -> Self {
        Self::ALL
            .iter()
            .rev()
            .copied()
            .find(|rank| cycles >= rank.floor())
            .unwrap_or(Self::Zombie)
    }

    /// The rank above this one, or `None` at the top.
    pub fn next(self) -> Option<Self> {
        let index = Self::ALL.iter().position(|rank| *rank == self)?;
        Self::ALL.get(index + 1).copied()
    }
}

/// Where a player stands: the total, the rank it gives, and the place in the
/// leaderboard. `next` is absent at the top of the ladder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Standing {
    pub cycles: i64,
    /// 1 is the top. Equal totals share a place.
    pub place: u32,
    /// How many players the place is among.
    pub players: u32,
    pub rank: Rank,
    /// The cycles the current rank starts at.
    pub floor: i64,
    pub next: Option<NextRank>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct NextRank {
    pub rank: Rank,
    pub floor: i64,
}

impl Standing {
    pub fn new(cycles: i64, place: u32, players: u32) -> Self {
        let rank = Rank::for_cycles(cycles);
        Self {
            cycles,
            place,
            players,
            rank,
            floor: rank.floor(),
            next: rank.next().map(|next| NextRank {
                rank: next,
                floor: next.floor(),
            }),
        }
    }
}

/// The source of an entry. A client selects on these names to label the
/// history, so they are part of the contract. A placement is its own kind, so
/// no client has to read a meaning out of an amount.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum PointKind {
    TournamentEntry,
    MatchWin,
    Champion,
    Finalist,
    Semifinalist,
    Adjustment,
}

impl PointKind {
    /// Every kind, in the order a legend lists them. `CyclesRules` is built
    /// from this list, so a new variant appears there without a second edit.
    pub const ALL: [Self; 6] = [
        Self::TournamentEntry,
        Self::MatchWin,
        Self::Semifinalist,
        Self::Finalist,
        Self::Champion,
        Self::Adjustment,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::TournamentEntry => "tournament_entry",
            Self::MatchWin => "match_win",
            Self::Champion => "champion",
            Self::Finalist => "finalist",
            Self::Semifinalist => "semifinalist",
            Self::Adjustment => "adjustment",
        }
    }

    /// What a tournament of this size pays for this kind. The entry and a win
    /// are the same in every field: a bigger bracket already has more wins in
    /// it. An adjustment has no fixed amount.
    pub const fn cycles(self, tier: FieldTier) -> Option<i64> {
        use FieldTier::{Large, Medium, Small};
        match (self, tier) {
            (Self::TournamentEntry, _) => Some(ENTRY_CYCLES),
            (Self::MatchWin, _) => Some(MATCH_WIN_CYCLES),
            (Self::Champion, Small) => Some(120),
            (Self::Champion, Medium) => Some(200),
            (Self::Champion, Large) => Some(300),
            (Self::Finalist, Small) => Some(70),
            (Self::Finalist, Medium) => Some(120),
            (Self::Finalist, Large) => Some(180),
            (Self::Semifinalist, Small) => Some(40),
            (Self::Semifinalist, Medium) => Some(60),
            (Self::Semifinalist, Large) => Some(90),
            (Self::Adjustment, _) => None,
        }
    }
}

/// Answer of `GET /api/cycles/rules`: every way to earn cycles, the field tiers
/// and the ladder. Built from the same code the ledger pays from, so a legend
/// cannot disagree with it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CyclesRules {
    /// Smallest field first.
    pub tiers: Vec<TierRule>,
    /// In the order a player earns them: entry, wins, then placements.
    pub awards: Vec<AwardRule>,
    pub adjustment_max: i64,
    /// Bottom first.
    pub ranks: Vec<RankRule>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TierRule {
    pub tier: FieldTier,
    pub min_entrants: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AwardRule {
    pub kind: PointKind,
    /// One amount per tier, in the order of `tiers`.
    pub cycles: Vec<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RankRule {
    pub rank: Rank,
    pub floor: i64,
}

impl CyclesRules {
    pub fn current() -> Self {
        let tiers = FieldTier::ALL
            .into_iter()
            .map(|tier| TierRule {
                tier,
                min_entrants: tier.min_entrants(),
            })
            .collect();
        let awards = PointKind::ALL
            .into_iter()
            .filter_map(|kind| {
                let cycles: Option<Vec<i64>> = FieldTier::ALL
                    .into_iter()
                    .map(|tier| kind.cycles(tier))
                    .collect();
                cycles.map(|cycles| AwardRule { kind, cycles })
            })
            .collect();
        let ranks = Rank::ALL
            .into_iter()
            .map(|rank| RankRule {
                rank,
                floor: rank.floor(),
            })
            .collect();

        Self {
            tiers,
            awards,
            adjustment_max: ADJUSTMENT_MAX,
            ranks,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("unknown point kind `{0}`")]
pub struct UnknownPointKind(String);

impl std::str::FromStr for PointKind {
    type Err = UnknownPointKind;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        match raw {
            "tournament_entry" => Ok(Self::TournamentEntry),
            "match_win" => Ok(Self::MatchWin),
            "champion" => Ok(Self::Champion),
            "finalist" => Ok(Self::Finalist),
            "semifinalist" => Ok(Self::Semifinalist),
            "adjustment" => Ok(Self::Adjustment),
            other => Err(UnknownPointKind(other.to_owned())),
        }
    }
}

/// One line of the history. `note` is present on an adjustment: the reason an
/// admin gave, written for everyone.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PointEntry {
    pub id: PointEntryId,
    pub amount: i64,
    pub kind: PointKind,
    pub tournament_id: Option<TournamentId>,
    pub tournament_name: Option<TournamentName>,
    pub note: Option<Note>,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub created_at: OffsetDateTime,
}

/// Answer of `GET /api/players/{handle}/cycles`: one page of the log, and the
/// total of every kind over the whole log, so a page can show where the
/// cycles came from without reading every line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CyclesLog {
    pub entries: Paginated<PointEntry>,
    /// Kinds with at least one line, in the order of `PointKind::ALL`.
    pub totals: Vec<KindTotal>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct KindTotal {
    pub kind: PointKind,
    pub cycles: i64,
}

/// Body of `POST /api/admin/players/{id}/cycles`.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Adjustment {
    pub amount: Amount,
    pub note: Note,
}

/// What a concluded tournament pays one player, before it is a row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Award {
    pub player: PlayerId,
    pub amount: i64,
    pub kind: PointKind,
    /// What paid: the match for a win, the tournament otherwise. With the kind
    /// and the player it is the key that makes a retry pay nobody twice.
    pub source_ref: String,
}

/// Everything a concluded tournament pays. Every entrant with an account gets
/// the entry cycles. With a bracket, each played match pays its winner, the
/// champion, the finalist and the two semifinalists get their placement.
/// Without a bracket only the named winner gets a placement. The placements
/// follow the tier of the field, every entrant counted.
pub fn tournament_awards(
    tournament: TournamentId,
    entrants: &[Entrant],
    bracket: Option<&Bracket>,
    winner: Option<EntrantId>,
) -> Vec<Award> {
    let player_of = |entrant: EntrantId| {
        entrants
            .iter()
            .find(|e| e.id == entrant)
            .and_then(|e| e.player.as_ref())
            .map(|p| p.id)
    };
    let tier = FieldTier::for_entrants(entrants.len());
    let placement = |entrant: EntrantId, kind: PointKind| {
        Some(Award {
            player: player_of(entrant)?,
            amount: kind.cycles(tier)?,
            kind,
            source_ref: tournament.to_string(),
        })
    };

    let entries = entrants.iter().filter_map(|e| {
        e.player.as_ref().map(|p| Award {
            player: p.id,
            amount: ENTRY_CYCLES,
            kind: PointKind::TournamentEntry,
            source_ref: tournament.to_string(),
        })
    });

    let Some(bracket) = bracket else {
        return entries
            .chain(winner.and_then(|w| placement(w, PointKind::Champion)))
            .collect();
    };

    let wins = bracket.flat().filter(|m| m.is_played()).filter_map(|m| {
        let winner = m.winner?;
        Some(Award {
            player: player_of(winner)?,
            amount: MATCH_WIN_CYCLES,
            kind: PointKind::MatchWin,
            source_ref: m.id.to_string(),
        })
    });

    let final_match = bracket.rounds.last().and_then(|round| round.first());
    let champion = final_match.and_then(|m| m.winner);
    let finalist = final_match.and_then(|m| loser_of(m.entrant_a, m.entrant_b, m.winner));
    // The semifinal is the round before the final. A two entrant bracket has
    // none, and `checked_sub` says so instead of taking the final for it.
    let semifinalists = bracket
        .rounds
        .len()
        .checked_sub(2)
        .and_then(|index| bracket.rounds.get(index))
        .into_iter()
        .flatten()
        .filter(|m| m.is_played())
        .filter_map(|m| loser_of(m.entrant_a, m.entrant_b, m.winner));

    let placements = champion
        .and_then(|c| placement(c, PointKind::Champion))
        .into_iter()
        .chain(finalist.and_then(|f| placement(f, PointKind::Finalist)))
        .chain(semifinalists.filter_map(|s| placement(s, PointKind::Semifinalist)));

    entries.chain(wins).chain(placements).collect()
}

fn loser_of(
    a: Option<EntrantId>,
    b: Option<EntrantId>,
    winner: Option<EntrantId>,
) -> Option<EntrantId> {
    match (a, b, winner) {
        (Some(a), Some(b), Some(w)) if w == a => Some(b),
        (Some(a), Some(b), Some(w)) if w == b => Some(a),
        _ => None,
    }
}
