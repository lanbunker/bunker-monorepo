//! Cycles from the contract: adjustments, the history and who reads its notes,
//! the leaderboard, and what a concluded tournament pays.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod support;

use axum::http::StatusCode;
use bunker_models::{
    Bracket, CyclesLog, ENTRY_CYCLES, FieldTier, MATCH_WIN_CYCLES, Paginated, Player, PlayerId,
    PointEntry, PointKind, Rank,
};
use serde_json::json;
use support::{HOUR, TestApi, assert_error, read_json};
use uuid::Uuid;

/// What a placement pays in a small field, which every tournament here is.
fn pays(kind: PointKind) -> i64 {
    kind.cycles(FieldTier::Small).unwrap()
}

async fn adjust(
    api: &TestApi,
    admin: &str,
    player: PlayerId,
    amount: i64,
    note: &str,
) -> PointEntry {
    let response = api
        .post_as(
            &format!("/api/admin/players/{player}/cycles"),
            &json!({ "amount": amount, "note": note }),
            admin,
        )
        .await;
    assert_eq!(response.status(), StatusCode::CREATED, "adjustment failed");

    read_json(response).await
}

/// The page of the log. `log` answers the whole body when the totals matter.
async fn history(api: &TestApi, handle: &str, token: Option<&str>) -> Paginated<PointEntry> {
    log(api, handle, token).await.entries
}

async fn log(api: &TestApi, handle: &str, token: Option<&str>) -> CyclesLog {
    let path = format!("/api/players/{handle}/cycles");
    let response = match token {
        Some(token) => api.get_as(&path, token).await,
        None => api.get(&path).await,
    };
    assert_eq!(response.status(), StatusCode::OK);

    read_json(response).await
}

#[tokio::test]
async fn a_new_player_is_a_zombie_in_first_place_with_no_history() {
    let api = TestApi::with_database().await;
    api.signup_player("dave").await;

    let dave = api.player("dave").await;
    assert_eq!(dave.standing.cycles, 0);
    assert_eq!(dave.standing.rank, Rank::Zombie);
    assert_eq!(dave.standing.place, 1);
    assert_eq!(dave.standing.players, 1);
    assert_eq!(dave.standing.floor, 0);
    assert_eq!(dave.standing.next.unwrap().rank, Rank::Guest);

    let log = history(&api, "dave", None).await;
    assert!(log.items.is_empty());
    assert_eq!(log.total, 0);
}

#[tokio::test]
async fn an_admin_adjusts_cycles_and_the_standing_follows() {
    let api = TestApi::with_database().await;
    let admin = api.signup_admin("root").await;
    let dave = api.signup_player("dave").await;

    let entry = adjust(&api, &admin, dave.id, 120, "helped at the door").await;
    assert_eq!(entry.amount, 120);
    assert_eq!(entry.kind, PointKind::Adjustment);
    assert_eq!(entry.note.unwrap().as_ref(), "helped at the door");
    assert!(entry.tournament_id.is_none());

    let after = api.player("dave").await;
    assert_eq!(after.standing.cycles, 120);
    assert_eq!(after.standing.rank, Rank::Guest);
    assert_eq!(after.standing.place, 1);

    adjust(&api, &admin, dave.id, -50, "took it back").await;
    let corrected = api.player("dave").await;
    assert_eq!(corrected.standing.cycles, 70);
    assert_eq!(
        corrected.standing.rank,
        Rank::Zombie,
        "a rank follows the total down too"
    );

    let log = history(&api, "dave", None).await;
    assert_eq!(log.total, 2);
    assert_eq!(log.items[0].amount, -50, "newest first");
    assert_eq!(log.items[1].amount, 120);

    // A total can go below zero. It stays a zombie and keeps its sign.
    adjust(&api, &admin, dave.id, -150, "and more").await;
    let negative = api.player("dave").await;
    assert_eq!(negative.standing.cycles, -80);
    assert_eq!(negative.standing.rank, Rank::Zombie);
    assert_eq!(negative.standing.next.unwrap().floor, 80);
}

/// The answer of the write is the row the history shows, to the microsecond
/// the column keeps.
#[tokio::test]
async fn an_adjustment_answers_the_line_as_the_ledger_holds_it() {
    let api = TestApi::with_database().await;
    let admin = api.signup_admin("root").await;
    let dave = api.signup_player("dave").await;

    let entry = adjust(&api, &admin, dave.id, 25, "fixed a cable").await;

    let log = history(&api, "dave", None).await;
    assert_eq!(log.items, [entry]);
}

#[tokio::test]
async fn an_adjustment_is_checked_at_the_boundary_and_by_the_rules() {
    let api = TestApi::with_database().await;
    let admin = api.signup_admin("root").await;
    let dave = api.signup_player("dave").await;
    let user = api.signup("erin").await.token;
    let path = format!("/api/admin/players/{}/cycles", dave.id);

    for (label, body) in [
        ("zero", json!({ "amount": 0, "note": "x" })),
        ("too much", json!({ "amount": 10_001, "note": "x" })),
        ("too little", json!({ "amount": -10_001, "note": "x" })),
        ("no note", json!({ "amount": 10 })),
        ("a blank note", json!({ "amount": 10, "note": "   " })),
        (
            "a stranger",
            json!({ "amount": 10, "note": "x", "kind": "checkin" }),
        ),
    ] {
        let refused = api.post_as(&path, &body, &admin).await;
        assert_eq!(
            refused.status(),
            StatusCode::UNPROCESSABLE_ENTITY,
            "{label} was accepted"
        );
    }

    let as_user = api
        .post_as(&path, &json!({ "amount": 10, "note": "x" }), &user)
        .await;
    assert_error(as_user, StatusCode::FORBIDDEN, "Forbidden").await;
    let anonymous = api.post(&path, &json!({ "amount": 10, "note": "x" })).await;
    assert_error(anonymous, StatusCode::UNAUTHORIZED, "Unauthorized").await;

    let ghost = api
        .post_as(
            &format!("/api/admin/players/{}/cycles", Uuid::new_v4()),
            &json!({ "amount": 10, "note": "x" }),
            &admin,
        )
        .await;
    assert_error(ghost, StatusCode::NOT_FOUND, "ItemNotFound").await;

    let root = api.player("root").await;
    let selfish = api
        .post_as(
            &format!("/api/admin/players/{}/cycles", root.id),
            &json!({ "amount": 10, "note": "x" }),
            &admin,
        )
        .await;
    assert_error(selfish, StatusCode::FORBIDDEN, "Forbidden").await;

    assert_eq!(
        api.player("dave").await.standing.cycles,
        0,
        "nothing landed"
    );
}

#[tokio::test]
async fn the_history_is_public_with_its_notes() {
    let api = TestApi::with_database().await;
    let admin = api.signup_admin("root").await;
    let dave = api.signup("dave").await.token;
    let dave_id = api.player("dave").await.id;
    let erin = api.signup("erin").await.token;
    adjust(&api, &admin, dave_id, 100, "won the raffle").await;

    for (label, token) in [
        ("a stranger", None),
        ("another player", Some(erin.as_str())),
        ("the owner", Some(dave.as_str())),
        ("an admin", Some(admin.as_str())),
    ] {
        let page = history(&api, "dave", token).await;
        assert_eq!(page.items[0].amount, 100, "{label}");
        assert_eq!(
            page.items[0].note.as_ref().unwrap().as_ref(),
            "won the raffle",
            "{label} reads the note"
        );
    }

    // A public route reads no token, so a dead one is not refused.
    let dead = api.get_as("/api/players/dave/cycles", "not-a-token").await;
    assert_eq!(dead.status(), StatusCode::OK);

    let unknown = api.get("/api/players/nobody/cycles").await;
    assert_error(unknown, StatusCode::NOT_FOUND, "ItemNotFound").await;
}

#[tokio::test]
async fn the_history_paginates_like_every_list() {
    let api = TestApi::with_database().await;
    let admin = api.signup_admin("root").await;
    let dave = api.signup_player("dave").await;
    for amount in 1..=5 {
        adjust(&api, &admin, dave.id, amount, "one more").await;
    }

    let first: CyclesLog =
        read_json(api.get("/api/players/dave/cycles?page=1&pageSize=2").await).await;
    let third: CyclesLog =
        read_json(api.get("/api/players/dave/cycles?page=3&pageSize=2").await).await;
    assert_eq!((first.entries.total, first.entries.total_pages), (5, 3));
    assert_eq!(first.entries.items.len(), 2);
    assert_eq!(third.entries.items.len(), 1);
    assert_eq!(
        first.totals, third.totals,
        "the totals cover the whole log on every page"
    );
    assert_eq!(first.totals.len(), 1);
    assert_eq!(first.totals[0].kind, PointKind::Adjustment);
    assert_eq!(first.totals[0].cycles, 15);

    let stranger = api.get("/api/players/dave/cycles?sort=asc").await;
    assert_error(stranger, StatusCode::BAD_REQUEST, "InvalidRequest").await;
}

#[tokio::test]
async fn the_leaderboard_sorts_by_cycles_and_equal_totals_share_a_place() {
    let api = TestApi::with_database().await;
    let admin = api.signup_admin("root").await;
    let low = api.signup_player("low").await;
    let mid_a = api.signup_player("mida").await;
    let mid_b = api.signup_player("midb").await;
    let top = api.signup_player("top").await;
    adjust(&api, &admin, low.id, 10, "x").await;
    adjust(&api, &admin, mid_a.id, 300, "x").await;
    adjust(&api, &admin, mid_b.id, 300, "x").await;
    adjust(&api, &admin, top.id, 700, "x").await;

    let board: Paginated<Player> = read_json(api.get("/api/players").await).await;
    let order: Vec<(&str, u32)> = board
        .items
        .iter()
        .map(|p| (p.handle.as_ref(), p.standing.place))
        .collect();
    assert_eq!(
        order,
        [
            ("top", 1),
            ("midb", 2),
            ("mida", 2),
            ("low", 4),
            ("root", 5),
        ],
        "first place first, a shared place, then the newest signup first"
    );
    assert_eq!(board.items[0].standing.rank, Rank::User);
    assert!(
        board.items.iter().all(|p| p.standing.players == 5),
        "every standing counts the whole field"
    );

    let roster: Paginated<Player> = read_json(api.get_as("/api/admin/players", &admin).await).await;
    let handles: Vec<&str> = roster.items.iter().map(|p| p.handle.as_ref()).collect();
    assert_eq!(
        handles,
        ["top", "midb", "mida", "low", "root"],
        "the backoffice keeps the roster order: the last signup first"
    );
    assert_eq!(
        roster.items[0].standing.cycles, 700,
        "with the standing on every row"
    );
}

#[tokio::test]
async fn a_concluded_bracket_pays_entry_wins_and_placements_one_time() {
    let api = TestApi::with_database().await;
    let admin = api.signup_admin("root").await;
    let created = api.create_tournament(&admin, HOUR).await;
    for handle in ["ada", "bob", "cid", "dee"] {
        api.add_entrant(&admin, created.id, handle).await;
    }
    api.set_status(&admin, created.id, "live").await;
    let generated = api
        .post_as(
            &format!("/api/admin/tournaments/{}/bracket", created.id),
            &json!({}),
            &admin,
        )
        .await;
    let mut bracket: Bracket = read_json(generated).await;
    // Side a always wins: seed 1 takes two matches and the cup.
    for _ in 0..3 {
        bracket = api.play_next(&admin, created.id, &bracket).await;
    }
    let detail: bunker_models::TournamentDetail = read_json(
        api.get_as(&format!("/api/admin/tournaments/{}", created.id), &admin)
            .await,
    )
    .await;
    let handle_of = |seed: u32| {
        detail
            .entrants
            .iter()
            .find(|e| e.seed == Some(seed))
            .unwrap()
            .player
            .as_ref()
            .unwrap()
            .handle
            .as_ref()
            .to_owned()
    };
    let (champion, finalist, semi_a, semi_b) =
        (handle_of(1), handle_of(3), handle_of(2), handle_of(4));

    assert_eq!(
        api.player(&champion).await.standing.cycles,
        0,
        "nothing before the conclusion"
    );
    let concluded = api.set_status(&admin, created.id, "concluded").await;
    assert_eq!(
        concluded.winner.unwrap().player.unwrap().handle.as_ref(),
        champion
    );

    assert_eq!(
        api.player(&champion).await.standing.cycles,
        ENTRY_CYCLES + 2 * MATCH_WIN_CYCLES + pays(PointKind::Champion)
    );
    assert_eq!(
        api.player(&finalist).await.standing.cycles,
        ENTRY_CYCLES + MATCH_WIN_CYCLES + pays(PointKind::Finalist)
    );
    for semi in [semi_a, semi_b] {
        assert_eq!(
            api.player(&semi).await.standing.cycles,
            ENTRY_CYCLES + pays(PointKind::Semifinalist)
        );
    }
    assert_eq!(
        api.player("root").await.standing.cycles,
        0,
        "the admin was not in"
    );

    let whole = log(&api, &champion, None).await;
    let log = whole.entries;
    assert_eq!(log.total, 4, "entry, two wins, the placement");
    assert!(
        log.items
            .iter()
            .all(|e| e.tournament_id == Some(created.id))
    );
    assert_eq!(
        log.items[0].tournament_name.as_ref().unwrap().as_ref(),
        "Sniper Cup"
    );
    let total_of = |kind: PointKind| {
        whole
            .totals
            .iter()
            .find(|t| t.kind == kind)
            .map(|t| t.cycles)
    };
    assert_eq!(total_of(PointKind::MatchWin), Some(2 * MATCH_WIN_CYCLES));
    assert_eq!(
        total_of(PointKind::Champion),
        Some(pays(PointKind::Champion))
    );
    assert_eq!(total_of(PointKind::Adjustment), None, "no line, no total");
    let order: Vec<PointKind> = whole.totals.iter().map(|t| t.kind).collect();
    assert_eq!(
        order,
        [
            PointKind::TournamentEntry,
            PointKind::MatchWin,
            PointKind::Champion
        ],
        "the totals follow the order of the legend, not of the write"
    );

    api.set_status(&admin, created.id, "concluded").await;
    assert_eq!(
        history(&api, &champion, None).await.total,
        4,
        "a second conclude pays nobody twice"
    );

    let deleted = api
        .delete_as(&format!("/api/admin/tournaments/{}", created.id), &admin)
        .await;
    assert_eq!(deleted.status(), StatusCode::NO_CONTENT);
    assert_eq!(api.player(&champion).await.standing.cycles, 0);
    assert_eq!(history(&api, &champion, None).await.total, 0);
}

#[tokio::test]
async fn a_conclusion_without_a_bracket_pays_the_entry_and_the_named_winner() {
    let api = TestApi::with_database().await;
    let admin = api.signup_admin("root").await;
    let created = api.create_tournament(&admin, HOUR).await;
    let dave = api.add_entrant(&admin, created.id, "dave").await;
    api.add_entrant(&admin, created.id, "erin").await;
    api.set_status(&admin, created.id, "open").await;

    let response = api
        .post_as(
            &format!("/api/admin/tournaments/{}/status", created.id),
            &json!({ "status": "concluded", "winner": dave.id }),
            &admin,
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);

    assert_eq!(
        api.player("dave").await.standing.cycles,
        ENTRY_CYCLES + pays(PointKind::Champion)
    );
    assert_eq!(api.player("erin").await.standing.cycles, ENTRY_CYCLES);
    let kinds: Vec<PointKind> = history(&api, "dave", None)
        .await
        .items
        .iter()
        .map(|e| e.kind)
        .collect();
    assert_eq!(kinds.len(), 2);
    assert!(kinds.contains(&PointKind::Champion));
    assert!(kinds.contains(&PointKind::TournamentEntry));

    let dave_id = dave.player.unwrap().id;
    let deleted = api
        .delete_as(&format!("/api/admin/players/{dave_id}"), &admin)
        .await;
    assert_eq!(deleted.status(), StatusCode::NO_CONTENT);
    let gone = api.get("/api/players/dave/cycles").await;
    assert_error(gone, StatusCode::NOT_FOUND, "ItemNotFound").await;
    let (orphans,): (i64,) =
        sqlx::query_as("select count(*) from point_entries where player_id = ?1")
            .bind(dave_id.to_string())
            .fetch_one(api.pool())
            .await
            .unwrap();
    assert_eq!(orphans, 0, "a deleted player takes their rows with them");
}

#[tokio::test]
async fn the_rules_are_public_and_name_every_way_to_earn() {
    let api = TestApi::without_database().await;

    let response = api.get("/api/cycles/rules").await;
    assert_eq!(response.status(), StatusCode::OK);
    let rules: bunker_models::CyclesRules = read_json(response).await;

    let kinds: Vec<PointKind> = rules.awards.iter().map(|a| a.kind).collect();
    assert_eq!(
        kinds,
        [
            PointKind::Checkin,
            PointKind::TournamentEntry,
            PointKind::MatchWin,
            PointKind::Semifinalist,
            PointKind::Finalist,
            PointKind::Champion,
        ]
    );
    assert_eq!(rules.ranks.len(), 6);
    assert_eq!(rules.ranks[1].rank, Rank::Guest);
    assert_eq!(rules.ranks[1].floor, 80);
    assert_eq!(rules.tiers.len(), 3);
    assert_eq!(rules.tiers[2].tier, FieldTier::Large);
    assert!(rules.awards.iter().all(|a| a.cycles.len() == 3));
}
