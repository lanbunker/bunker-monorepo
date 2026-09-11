//! The bracket rules, from the contract: sizes, byes, propagation and guards.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use bunker_models::{Bracket, BracketError, EntrantId, Match, MatchId};

fn entrants(n: usize) -> Vec<EntrantId> {
    (0..n).map(|_| EntrantId::generate()).collect()
}

fn ready_matches(bracket: &Bracket) -> Vec<&Match> {
    bracket.flat().filter(|m| m.is_ready()).collect()
}

#[test]
fn one_entrant_is_not_a_bracket() {
    assert_eq!(
        Bracket::generate(&entrants(1)),
        Err(BracketError::TooFewEntrants(1))
    );
    assert_eq!(Bracket::generate(&[]), Err(BracketError::TooFewEntrants(0)));
}

#[test]
fn two_entrants_make_one_final() {
    let ids = entrants(2);
    let bracket = Bracket::generate(&ids).unwrap();

    assert_eq!(bracket.rounds.len(), 1);
    let final_match = &bracket.rounds[0][0];
    assert_eq!(final_match.entrant_a, Some(ids[0]));
    assert_eq!(final_match.entrant_b, Some(ids[1]));
    assert_eq!(final_match.winner, None);
    assert!(!bracket.has_results());
    assert_eq!(bracket.champion(), None);
}

#[test]
fn eight_entrants_fill_three_rounds_with_no_bye() {
    let ids = entrants(8);
    let bracket = Bracket::generate(&ids).unwrap();

    assert_eq!(
        bracket.rounds.iter().map(Vec::len).collect::<Vec<_>>(),
        [4, 2, 1]
    );
    assert_eq!(ready_matches(&bracket).len(), 4);
    assert!(bracket.flat().all(|m| m.winner.is_none()));
    // Seed 1 opens the bracket against seed 8, seed 2 sits in the bottom half.
    assert_eq!(bracket.rounds[0][0].entrant_a, Some(ids[0]));
    assert_eq!(bracket.rounds[0][0].entrant_b, Some(ids[7]));
    assert_eq!(bracket.rounds[0][2].entrant_a, Some(ids[1]));
    assert_eq!(bracket.rounds[0][3].entrant_b, Some(ids[5]));
}

#[test]
fn five_entrants_give_byes_to_the_top_three_seeds() {
    let ids = entrants(5);
    let bracket = Bracket::generate(&ids).unwrap();

    assert_eq!(
        bracket.rounds.iter().map(Vec::len).collect::<Vec<_>>(),
        [4, 2, 1]
    );
    let byes: Vec<EntrantId> = bracket.rounds[0]
        .iter()
        .filter(|m| !m.is_ready())
        .map(|m| m.winner.unwrap())
        .collect();
    assert_eq!(byes, [ids[0], ids[1], ids[2]]);
    // Seeds 4 and 5 play the only real match of round 1.
    let played: Vec<&Match> = bracket.rounds[0].iter().filter(|m| m.is_ready()).collect();
    assert_eq!(played.len(), 1);
    assert_eq!(played[0].entrant_a, Some(ids[3]));
    assert_eq!(played[0].entrant_b, Some(ids[4]));
    // The byes already wait in round 2, and none of them counts as a result.
    assert_eq!(bracket.rounds[1][0].entrant_a, Some(ids[0]));
    assert_eq!(bracket.rounds[1][1].entrant_a, Some(ids[1]));
    assert_eq!(bracket.rounds[1][1].entrant_b, Some(ids[2]));
    assert_eq!(bracket.rounds[1][0].entrant_b, None);
    assert!(!bracket.has_results());
}

#[test]
fn a_result_moves_the_winner_into_the_next_slot() {
    let ids = entrants(4);
    let mut bracket = Bracket::generate(&ids).unwrap();
    let top = bracket.rounds[0][0].clone();
    let bottom = bracket.rounds[0][1].clone();

    let changed = bracket.report(top.id, ids[0]).unwrap();
    assert_eq!(changed.len(), 2);
    assert_eq!(bracket.rounds[0][0].winner, Some(ids[0]));
    assert_eq!(bracket.rounds[1][0].entrant_a, Some(ids[0]));
    assert!(bracket.has_results());

    bracket.report(bottom.id, ids[2]).unwrap();
    assert_eq!(bracket.rounds[1][0].entrant_b, Some(ids[2]));

    let final_id = bracket.rounds[1][0].id;
    let changed = bracket.report(final_id, ids[2]).unwrap();
    assert_eq!(changed.len(), 1, "the final feeds nothing");
    assert_eq!(bracket.champion(), Some(ids[2]));
}

#[test]
fn a_result_can_change_until_the_next_match_is_decided() {
    let ids = entrants(4);
    let mut bracket = Bracket::generate(&ids).unwrap();
    let top = bracket.rounds[0][0].id;
    let bottom = bracket.rounds[0][1].id;

    bracket.report(top, ids[0]).unwrap();
    bracket.report(top, ids[3]).unwrap();
    assert_eq!(bracket.rounds[1][0].entrant_a, Some(ids[3]));

    bracket.report(bottom, ids[1]).unwrap();
    let final_id = bracket.rounds[1][0].id;
    bracket.report(final_id, ids[3]).unwrap();

    assert_eq!(
        bracket.report(top, ids[0]),
        Err(BracketError::NextDecided(top))
    );
    assert_eq!(bracket.clear(top), Err(BracketError::NextDecided(top)));
}

#[test]
fn a_result_needs_a_ready_match_and_one_of_its_two_entrants() {
    let ids = entrants(4);
    let mut bracket = Bracket::generate(&ids).unwrap();
    let top = bracket.rounds[0][0].id;
    let final_id = bracket.rounds[1][0].id;

    assert_eq!(
        bracket.report(final_id, ids[0]),
        Err(BracketError::NotReady(final_id))
    );
    assert_eq!(
        bracket.report(top, ids[1]),
        Err(BracketError::NotAParticipant(ids[1], top))
    );
    let stranger = MatchId::generate();
    assert_eq!(
        bracket.report(stranger, ids[0]),
        Err(BracketError::UnknownMatch(stranger))
    );
}

#[test]
fn clearing_a_result_empties_the_slot_it_fed() {
    let ids = entrants(4);
    let mut bracket = Bracket::generate(&ids).unwrap();
    let bottom = bracket.rounds[0][1].id;
    bracket.report(bottom, ids[2]).unwrap();
    assert_eq!(bracket.rounds[1][0].entrant_b, Some(ids[2]));

    let changed = bracket.clear(bottom).unwrap();
    assert_eq!(changed.len(), 2);
    assert_eq!(bracket.rounds[0][1].winner, None);
    assert_eq!(bracket.rounds[1][0].entrant_b, None);
    assert!(!bracket.has_results());

    assert!(
        bracket.clear(bottom).unwrap().is_empty(),
        "nothing left to clear"
    );
}

#[test]
fn a_bye_cannot_be_cleared() {
    let ids = entrants(3);
    let mut bracket = Bracket::generate(&ids).unwrap();
    let bye = bracket.rounds[0].iter().find(|m| !m.is_ready()).unwrap().id;

    assert_eq!(bracket.clear(bye), Err(BracketError::NotReady(bye)));
}

#[test]
fn flat_rows_group_back_into_rounds_and_slots() {
    let ids = entrants(5);
    let generated = Bracket::generate(&ids).unwrap();
    let mut rows: Vec<Match> = generated.flat().cloned().collect();
    rows.reverse();

    assert_eq!(Bracket::from_matches(rows), generated);
}
