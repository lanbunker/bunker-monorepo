//! Single elimination brackets, as pure data and rules. Nothing here touches a
//! database, so the rules are tested on their own and any crate can reuse them.

use std::collections::BTreeMap;

use nutype::nutype;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use super::tournament::EntrantId;

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
pub struct MatchId(Uuid);

impl MatchId {
    pub fn generate() -> Self {
        Self::new(Uuid::new_v4())
    }
}

/// One match. `round` starts at 1, `slot` at 0. An empty side is a bye in round
/// 1 and an undecided feeder afterwards.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Match {
    pub id: MatchId,
    pub round: u32,
    pub slot: u32,
    pub entrant_a: Option<EntrantId>,
    pub entrant_b: Option<EntrantId>,
    pub winner: Option<EntrantId>,
}

impl Match {
    /// Both sides are known, so a result can be entered.
    pub const fn is_ready(&self) -> bool {
        self.entrant_a.is_some() && self.entrant_b.is_some()
    }

    /// A winner that came from playing, not from a bye.
    pub const fn is_played(&self) -> bool {
        self.is_ready() && self.winner.is_some()
    }
}

/// Every match of a tournament, round by round. `rounds[0]` is round 1 and the
/// last round holds the final alone.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Bracket {
    pub rounds: Vec<Vec<Match>>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BracketError {
    #[error("a bracket needs at least two entrants, got {0}")]
    TooFewEntrants(usize),
    #[error("match {0} is not in this bracket")]
    UnknownMatch(MatchId),
    #[error("match {0} does not have both entrants yet")]
    NotReady(MatchId),
    #[error("entrant {0} does not play in match {1}")]
    NotAParticipant(EntrantId, MatchId),
    #[error("the match after {0} already has a result")]
    NextDecided(MatchId),
    #[error("the matches do not form a single elimination bracket")]
    Malformed,
}

impl Bracket {
    /// Builds every match from the entrants in seed order: the first is seed 1.
    /// The size is the next power of two, and the missing entrants are byes
    /// that go to the top seeds. A bye match already holds its winner and that
    /// winner already sits in round 2.
    pub fn generate(seeded: &[EntrantId]) -> Result<Self, BracketError> {
        let count = seeded.len();
        if count < 2 {
            return Err(BracketError::TooFewEntrants(count));
        }
        let size = count.next_power_of_two();
        let positions = seeding_order(size);

        let first_round: Vec<Match> = positions
            .chunks(2)
            .enumerate()
            .map(|(slot, pair)| {
                let entrant = |index: usize| {
                    pair.get(index)
                        .and_then(|seed| seeded.get(seed - 1))
                        .copied()
                };
                let (entrant_a, entrant_b) = (entrant(0), entrant(1));
                let winner = match (entrant_a, entrant_b) {
                    (Some(lone), None) | (None, Some(lone)) => Some(lone),
                    _ => None,
                };
                Match {
                    id: MatchId::generate(),
                    round: 1,
                    slot: as_u32(slot),
                    entrant_a,
                    entrant_b,
                    winner,
                }
            })
            .collect();

        let round_count = size.trailing_zeros();
        let later_rounds = (2..=round_count).map(|round| {
            let slots = size >> round;
            (0..slots)
                .map(|slot| Match {
                    id: MatchId::generate(),
                    round,
                    slot: as_u32(slot),
                    entrant_a: None,
                    entrant_b: None,
                    winner: None,
                })
                .collect::<Vec<_>>()
        });

        let mut bracket = Self {
            rounds: std::iter::once(first_round).chain(later_rounds).collect(),
        };
        let byes: Vec<(usize, EntrantId)> = bracket
            .rounds
            .first()
            .into_iter()
            .flatten()
            .filter_map(|m| Some((usize::try_from(m.slot).ok()?, m.winner?)))
            .collect();
        for (slot, winner) in byes {
            bracket.feed(0, slot, Some(winner));
        }

        Ok(bracket)
    }

    /// Groups flat rows by round and slot, as storage returns them. The rows must
    /// form a whole bracket: rounds 1 to k, each half the size of the one before,
    /// down to one final. Anything else is refused, because `report` and `clear`
    /// walk the rounds by index.
    pub fn from_matches(matches: Vec<Match>) -> Result<Self, BracketError> {
        let mut by_round: BTreeMap<u32, Vec<Match>> = BTreeMap::new();
        for m in matches {
            by_round.entry(m.round).or_default().push(m);
        }
        let rounds: Vec<Vec<Match>> = by_round
            .into_iter()
            .enumerate()
            .map(|(index, (round, mut slots))| {
                if usize::try_from(round).ok() != Some(index + 1) {
                    return Err(BracketError::Malformed);
                }
                slots.sort_by_key(|m| m.slot);
                let in_order = slots
                    .iter()
                    .enumerate()
                    .all(|(slot, m)| usize::try_from(m.slot).ok() == Some(slot));
                if !in_order {
                    return Err(BracketError::Malformed);
                }
                Ok(slots)
            })
            .collect::<Result<_, _>>()?;
        let sizes_halve = rounds
            .windows(2)
            .all(|pair| pair.first().map(Vec::len) == pair.get(1).map(|next| next.len() * 2));
        if rounds.last().map(Vec::len) != Some(1) || !sizes_halve {
            return Err(BracketError::Malformed);
        }
        Ok(Self { rounds })
    }

    pub fn flat(&self) -> impl Iterator<Item = &Match> {
        self.rounds.iter().flatten()
    }

    /// True once any match was played. A bye is not a result.
    pub fn has_results(&self) -> bool {
        self.flat().any(Match::is_played)
    }

    /// The winner of the final, once it has one.
    pub fn champion(&self) -> Option<EntrantId> {
        self.rounds.last()?.first()?.winner
    }

    /// Enters a result. The winner moves into the next round. A result can be
    /// changed as long as the next match is still undecided. Returns the matches
    /// that changed, so storage writes only those.
    pub fn report(&mut self, id: MatchId, winner: EntrantId) -> Result<Vec<Match>, BracketError> {
        let (round, slot) = self.locate(id)?;
        let current = self
            .get(round, slot)
            .ok_or(BracketError::UnknownMatch(id))?;
        if !current.is_ready() {
            return Err(BracketError::NotReady(id));
        }
        if current.entrant_a != Some(winner) && current.entrant_b != Some(winner) {
            return Err(BracketError::NotAParticipant(winner, id));
        }
        self.guard_next_undecided(round, slot, id)?;

        if let Some(m) = self.get_mut(round, slot) {
            m.winner = Some(winner);
        }
        self.feed(round, slot, Some(winner));

        Ok(self.changed(round, slot))
    }

    /// Removes a result and empties the slot it fed. A bye cannot be cleared: the
    /// lone entrant has nobody to lose to.
    pub fn clear(&mut self, id: MatchId) -> Result<Vec<Match>, BracketError> {
        let (round, slot) = self.locate(id)?;
        let current = self
            .get(round, slot)
            .ok_or(BracketError::UnknownMatch(id))?;
        if !current.is_ready() {
            return Err(BracketError::NotReady(id));
        }
        if current.winner.is_none() {
            return Ok(Vec::new());
        }
        self.guard_next_undecided(round, slot, id)?;

        if let Some(m) = self.get_mut(round, slot) {
            m.winner = None;
        }
        self.feed(round, slot, None);

        Ok(self.changed(round, slot))
    }

    fn locate(&self, id: MatchId) -> Result<(usize, usize), BracketError> {
        self.rounds
            .iter()
            .enumerate()
            .find_map(|(round, slots)| {
                slots
                    .iter()
                    .position(|m| m.id == id)
                    .map(|slot| (round, slot))
            })
            .ok_or(BracketError::UnknownMatch(id))
    }

    fn get(&self, round: usize, slot: usize) -> Option<&Match> {
        self.rounds.get(round)?.get(slot)
    }

    fn get_mut(&mut self, round: usize, slot: usize) -> Option<&mut Match> {
        self.rounds.get_mut(round)?.get_mut(slot)
    }

    fn guard_next_undecided(
        &self,
        round: usize,
        slot: usize,
        id: MatchId,
    ) -> Result<(), BracketError> {
        match self.get(round + 1, slot / 2) {
            Some(next) if next.winner.is_some() => Err(BracketError::NextDecided(id)),
            _ => Ok(()),
        }
    }

    /// Writes an entrant into the side of the next match that this slot feeds:
    /// an even slot is side a, an odd slot side b. The final feeds nothing.
    fn feed(&mut self, round: usize, slot: usize, entrant: Option<EntrantId>) {
        if let Some(next) = self.get_mut(round + 1, slot / 2) {
            if slot.is_multiple_of(2) {
                next.entrant_a = entrant;
            } else {
                next.entrant_b = entrant;
            }
        }
    }

    fn changed(&self, round: usize, slot: usize) -> Vec<Match> {
        [self.get(round, slot), self.get(round + 1, slot / 2)]
            .into_iter()
            .flatten()
            .cloned()
            .collect()
    }
}

/// Seeds by position for a bracket of `size`, so that seed 1 meets seed `size`
/// and the two best seeds can only meet in the final. Size 8 gives
/// `[1, 8, 4, 5, 2, 7, 3, 6]`.
fn seeding_order(size: usize) -> Vec<usize> {
    let mut order = vec![1];
    while order.len() < size {
        let len = order.len();
        order = order
            .iter()
            .flat_map(|&seed| [seed, 2 * len + 1 - seed])
            .collect();
    }
    order
}

/// A slot index is bounded by the bracket size, which a `u32` holds with room.
fn as_u32(index: usize) -> u32 {
    u32::try_from(index).unwrap_or(u32::MAX)
}
