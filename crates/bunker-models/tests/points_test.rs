//! Cycles from the contract: the ladder, the standing, and what a tournament
//! pays.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use bunker_models::{
    ADJUSTMENT_MAX, Adjustment, Amount, Award, Bracket, CyclesRules, ENTRY_CYCLES, Entrant,
    EntrantId, FieldTier, Glyph, GlyphBits, GlyphColor, Handle, MATCH_WIN_CYCLES, Note, Player,
    PlayerId, PointKind, Rank, Role, Standing, TournamentId, tournament_awards,
};
use time::OffsetDateTime;

/// What a placement pays in a field of `count` entrants.
fn pays(kind: PointKind, count: usize) -> i64 {
    kind.cycles(FieldTier::for_entrants(count)).unwrap()
}

fn cup() -> TournamentId {
    TournamentId::new(uuid::Uuid::from_u128(7))
}

fn player(handle: &str) -> Player {
    Player {
        id: PlayerId::generate(),
        handle: Handle::try_new(handle).unwrap(),
        glyph: Glyph {
            bits: GlyphBits::try_new(1).unwrap(),
            color: GlyphColor::from_hex("#ffb000").unwrap(),
        },
        role: Role::User,
        standing: Standing::new(0, 1, 1),
        created_at: OffsetDateTime::UNIX_EPOCH,
    }
}

fn entrant(player: Option<Player>) -> Entrant {
    Entrant {
        id: EntrantId::generate(),
        player,
        seed: None,
        skill: None,
        registered_at: OffsetDateTime::UNIX_EPOCH,
    }
}

fn total_of(awards: &[Award], player: PlayerId) -> i64 {
    awards
        .iter()
        .filter(|a| a.player == player)
        .map(|a| a.amount)
        .sum()
}

#[test]
fn the_ladder_is_a_threshold_on_the_total() {
    assert_eq!(Rank::for_cycles(0), Rank::Zombie);
    assert_eq!(Rank::for_cycles(-50), Rank::Zombie);
    assert_eq!(Rank::for_cycles(99), Rank::Zombie);
    assert_eq!(Rank::for_cycles(100), Rank::Guest);
    assert_eq!(Rank::for_cycles(599), Rank::Guest);
    assert_eq!(Rank::for_cycles(600), Rank::User);
    assert_eq!(Rank::for_cycles(1500), Rank::Sudoer);
    assert_eq!(Rank::for_cycles(3000), Rank::Daemon);
    assert_eq!(Rank::for_cycles(6000), Rank::Kernel);
    assert_eq!(Rank::for_cycles(1_000_000), Rank::Kernel);

    assert_eq!(Rank::Zombie.next(), Some(Rank::Guest));
    assert_eq!(Rank::Daemon.next(), Some(Rank::Kernel));
    assert_eq!(Rank::Kernel.next(), None);
    assert!(
        Rank::ALL
            .windows(2)
            .all(|pair| pair[0].floor() < pair[1].floor())
    );
}

#[test]
fn a_standing_carries_the_floor_and_the_next_rank() {
    let standing = Standing::new(860, 3, 42);
    assert_eq!(standing.rank, Rank::User);
    assert_eq!(standing.floor, 600);
    assert_eq!(standing.next.unwrap().rank, Rank::Sudoer);
    assert_eq!(standing.next.unwrap().floor, 1500);
    assert_eq!(standing.place, 3);
    assert_eq!(standing.players, 42);

    let top = Standing::new(7000, 1, 42);
    assert_eq!(top.rank, Rank::Kernel);
    assert!(top.next.is_none());

    let json = serde_json::to_value(standing).unwrap();
    assert_eq!(json["rank"], "user");
    assert_eq!(json["next"]["rank"], "sudoer");
}

#[test]
fn an_adjustment_is_a_nonzero_bounded_amount_with_a_note() {
    assert!(Amount::try_new(0).is_err());
    assert!(Amount::try_new(10_001).is_err());
    assert!(Amount::try_new(-10_001).is_err());
    assert_eq!(Amount::try_new(-250).unwrap().into_inner(), -250);
    assert!(Note::try_new("   ").is_err());
    assert!(Note::try_new("x".repeat(201)).is_err());
    assert_eq!(
        Note::try_new("  cheating at the door ").unwrap().as_ref(),
        "cheating at the door"
    );

    let body: Adjustment =
        serde_json::from_str(r#"{"amount":-100,"note":"unplugged a cabinet"}"#).unwrap();
    assert_eq!(body.amount.into_inner(), -100);
    assert!(
        serde_json::from_str::<Adjustment>(r#"{"amount":50}"#).is_err(),
        "a note is required"
    );
    assert!(
        serde_json::from_str::<Adjustment>(r#"{"amount":50,"note":"x","kind":"adjustment"}"#)
            .is_err()
    );
}

#[test]
fn a_bracket_of_four_pays_entry_wins_and_placements() {
    let entrants: Vec<Entrant> = (0..4)
        .map(|i| entrant(Some(player(&format!("player{i}")))))
        .collect();
    let ids: Vec<EntrantId> = entrants.iter().map(|e| e.id).collect();
    let pid = |i: usize| entrants[i].player.as_ref().unwrap().id;
    let mut bracket = Bracket::generate(&ids).unwrap();
    // Seeds 1 and 2 meet first, seeds 3 and 4 second. Seed 2 and seed 3 win, seed 3 takes the final.
    let top = bracket.rounds[0][0].id;
    let bottom = bracket.rounds[0][1].id;
    bracket.report(top, ids[1]).unwrap();
    bracket.report(bottom, ids[2]).unwrap();
    let final_id = bracket.rounds[1][0].id;
    bracket.report(final_id, ids[2]).unwrap();

    let awards = tournament_awards(cup(), &entrants, Some(&bracket), Some(ids[2]));

    assert_eq!(
        total_of(&awards, pid(2)),
        ENTRY_CYCLES + 2 * MATCH_WIN_CYCLES + pays(PointKind::Champion, 4),
        "the champion"
    );
    assert_eq!(
        total_of(&awards, pid(1)),
        ENTRY_CYCLES + MATCH_WIN_CYCLES + pays(PointKind::Finalist, 4),
        "the finalist"
    );
    assert_eq!(
        total_of(&awards, pid(0)),
        ENTRY_CYCLES + pays(PointKind::Semifinalist, 4)
    );
    assert_eq!(
        total_of(&awards, pid(3)),
        ENTRY_CYCLES + pays(PointKind::Semifinalist, 4)
    );
    let wins: Vec<&Award> = awards
        .iter()
        .filter(|a| a.kind == PointKind::MatchWin)
        .collect();
    assert_eq!(wins.len(), 3, "three played matches");
    let match_ids: Vec<String> = bracket.flat().map(|m| m.id.to_string()).collect();
    assert!(
        wins.iter().all(|a| match_ids.contains(&a.source_ref)),
        "a win names its match"
    );
    assert!(
        awards
            .iter()
            .filter(|a| a.kind != PointKind::MatchWin)
            .all(|a| a.source_ref == cup().to_string()),
        "everything else names the cup"
    );
    let keys: std::collections::BTreeSet<String> = awards
        .iter()
        .map(|a| format!("{}/{}/{}", a.kind.as_str(), a.player, a.source_ref))
        .collect();
    assert_eq!(
        keys.len(),
        awards.len(),
        "no two awards share the ledger key"
    );
}

#[test]
fn a_bracket_of_two_has_no_semifinal() {
    let entrants: Vec<Entrant> = (0..2)
        .map(|i| entrant(Some(player(&format!("player{i}")))))
        .collect();
    let ids: Vec<EntrantId> = entrants.iter().map(|e| e.id).collect();
    let mut bracket = Bracket::generate(&ids).unwrap();
    let final_id = bracket.rounds[0][0].id;
    bracket.report(final_id, ids[0]).unwrap();

    let awards = tournament_awards(cup(), &entrants, Some(&bracket), Some(ids[0]));

    let champion = entrants[0].player.as_ref().unwrap().id;
    let runner_up = entrants[1].player.as_ref().unwrap().id;
    assert_eq!(
        total_of(&awards, champion),
        ENTRY_CYCLES + MATCH_WIN_CYCLES + pays(PointKind::Champion, 2)
    );
    assert_eq!(
        total_of(&awards, runner_up),
        ENTRY_CYCLES + pays(PointKind::Finalist, 2),
        "the final is not a semifinal too"
    );
    assert!(awards.iter().all(|a| a.kind != PointKind::Semifinalist));
}

#[test]
fn a_bye_is_not_a_win_and_a_deleted_player_gets_nothing() {
    let mut entrants: Vec<Entrant> = (0..3)
        .map(|i| entrant(Some(player(&format!("player{i}")))))
        .collect();
    entrants[2].player = None;
    let ids: Vec<EntrantId> = entrants.iter().map(|e| e.id).collect();
    let mut bracket = Bracket::generate(&ids).unwrap();
    // Seed 1 has the bye. Seeds 2 and 3 play, seed 3 (no account) wins and then loses the final.
    let played = bracket.rounds[0][1].id;
    bracket.report(played, ids[2]).unwrap();
    let final_id = bracket.rounds[1][0].id;
    bracket.report(final_id, ids[0]).unwrap();

    let awards = tournament_awards(cup(), &entrants, Some(&bracket), Some(ids[0]));

    let champion = entrants[0].player.as_ref().unwrap().id;
    assert_eq!(
        total_of(&awards, champion),
        ENTRY_CYCLES + MATCH_WIN_CYCLES + pays(PointKind::Champion, 3),
        "one win for the final, none for the bye"
    );
    // With three entrants round one is the semifinal, so its loser placed there.
    let loser = entrants[1].player.as_ref().unwrap().id;
    assert_eq!(
        total_of(&awards, loser),
        ENTRY_CYCLES + pays(PointKind::Semifinalist, 3)
    );
    // Two entries, the final's win, the champion and one semifinalist. The
    // deleted account played and won, and pays out to nobody.
    assert_eq!(awards.len(), 5);
}

#[test]
fn without_a_bracket_only_the_entry_and_the_named_winner_pay() {
    let entrants: Vec<Entrant> = (0..3)
        .map(|i| entrant(Some(player(&format!("player{i}")))))
        .collect();
    let winner = entrants[1].id;

    let awards = tournament_awards(cup(), &entrants, None, Some(winner));
    assert_eq!(awards.len(), 4);
    assert_eq!(
        total_of(&awards, entrants[1].player.as_ref().unwrap().id),
        ENTRY_CYCLES + pays(PointKind::Champion, 3)
    );
    assert_eq!(
        total_of(&awards, entrants[0].player.as_ref().unwrap().id),
        ENTRY_CYCLES
    );

    let nobody = tournament_awards(cup(), &entrants, None, None);
    assert_eq!(nobody.len(), 3);
    assert!(nobody.iter().all(|a| a.kind == PointKind::TournamentEntry));
}

#[test]
fn the_field_tier_changes_where_a_round_is_added() {
    assert_eq!(FieldTier::for_entrants(0), FieldTier::Small);
    assert_eq!(FieldTier::for_entrants(2), FieldTier::Small);
    assert_eq!(FieldTier::for_entrants(7), FieldTier::Small);
    assert_eq!(FieldTier::for_entrants(8), FieldTier::Medium);
    assert_eq!(FieldTier::for_entrants(15), FieldTier::Medium);
    assert_eq!(FieldTier::for_entrants(16), FieldTier::Large);
    assert_eq!(
        FieldTier::for_entrants(400),
        FieldTier::Large,
        "the top is a cap"
    );

    // The entry and a win are the same in every field. A placement grows.
    for tier in FieldTier::ALL {
        assert_eq!(PointKind::TournamentEntry.cycles(tier), Some(ENTRY_CYCLES));
        assert_eq!(PointKind::MatchWin.cycles(tier), Some(MATCH_WIN_CYCLES));
        assert_eq!(PointKind::Adjustment.cycles(tier), None);
    }
    for kind in [
        PointKind::Champion,
        PointKind::Finalist,
        PointKind::Semifinalist,
    ] {
        let by_tier: Vec<i64> = FieldTier::ALL
            .into_iter()
            .map(|tier| kind.cycles(tier).unwrap())
            .collect();
        assert!(
            by_tier.windows(2).all(|pair| pair[0] < pair[1]),
            "{kind:?} does not grow with the field: {by_tier:?}"
        );
    }
}

#[test]
fn a_large_field_pays_its_champion_the_large_placement() {
    let entrants: Vec<Entrant> = (0..16)
        .map(|i| entrant(Some(player(&format!("player{i}")))))
        .collect();
    let ids: Vec<EntrantId> = entrants.iter().map(|e| e.id).collect();
    let mut bracket = Bracket::generate(&ids).unwrap();
    // Side a wins every match: seed 1 takes four matches and the cup.
    while bracket.champion().is_none() {
        let next = bracket
            .flat()
            .find(|m| m.is_ready() && m.winner.is_none())
            .cloned()
            .unwrap();
        bracket.report(next.id, next.entrant_a.unwrap()).unwrap();
    }

    let awards = tournament_awards(cup(), &entrants, Some(&bracket), Some(ids[0]));

    let champion = entrants[0].player.as_ref().unwrap().id;
    assert_eq!(
        total_of(&awards, champion),
        ENTRY_CYCLES + 4 * MATCH_WIN_CYCLES + PointKind::Champion.cycles(FieldTier::Large).unwrap()
    );
    let semifinalists = awards
        .iter()
        .filter(|a| a.kind == PointKind::Semifinalist)
        .count();
    assert_eq!(semifinalists, 2);
}

#[test]
fn the_rules_are_the_constants_the_ledger_pays() {
    let rules = CyclesRules::current();

    assert_eq!(
        rules
            .tiers
            .iter()
            .map(|t| t.min_entrants)
            .collect::<Vec<_>>(),
        [2, 8, 16]
    );
    let cycles_of = |kind: PointKind| {
        rules
            .awards
            .iter()
            .find(|a| a.kind == kind)
            .unwrap()
            .cycles
            .clone()
    };
    assert_eq!(cycles_of(PointKind::TournamentEntry), vec![ENTRY_CYCLES; 3]);
    assert_eq!(cycles_of(PointKind::MatchWin), vec![MATCH_WIN_CYCLES; 3]);
    for kind in [
        PointKind::Semifinalist,
        PointKind::Finalist,
        PointKind::Champion,
    ] {
        let expected: Vec<i64> = FieldTier::ALL
            .into_iter()
            .map(|tier| kind.cycles(tier).unwrap())
            .collect();
        assert_eq!(cycles_of(kind), expected, "{kind:?}");
    }
    assert!(
        rules.awards.iter().all(|a| a.kind != PointKind::Adjustment),
        "an adjustment has no fixed amount"
    );
    // Every kind with an amount is in the rules, in the order of the list, so a
    // new kind cannot be paid and left out of the legend.
    let listed: Vec<PointKind> = rules.awards.iter().map(|a| a.kind).collect();
    let paid: Vec<PointKind> = PointKind::ALL
        .into_iter()
        .filter(|kind| kind.cycles(FieldTier::Small).is_some())
        .collect();
    assert_eq!(listed, paid);
    assert_eq!(rules.adjustment_max, ADJUSTMENT_MAX);
    assert_eq!(rules.ranks.len(), Rank::ALL.len());
    assert_eq!(rules.ranks[0].rank, Rank::Zombie);
    assert_eq!(rules.ranks[5].floor, Rank::Kernel.floor());

    // Every award in the rules is what the awards function actually pays.
    let entrants: Vec<Entrant> = (0..2)
        .map(|i| entrant(Some(player(&format!("player{i}")))))
        .collect();
    let paid = tournament_awards(cup(), &entrants, None, Some(entrants[0].id));
    assert!(paid.iter().all(|a| a.amount == cycles_of(a.kind)[0]));
}
