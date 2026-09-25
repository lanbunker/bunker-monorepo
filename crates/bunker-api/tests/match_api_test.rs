//! The match log from the contract: the record, the nemesis and the page.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod support;

use axum::http::StatusCode;
use bunker_models::{Bracket, EntrantId, MatchLog, Player, TournamentId};
use serde_json::json;
use support::{HOUR, TestApi, assert_error, read_json};

async fn log_of(api: &TestApi, handle: &str) -> MatchLog {
    page_of(api, handle, "").await
}

/// `query` is a whole query string, such as `?page=2&pageSize=1`.
async fn page_of(api: &TestApi, handle: &str, query: &str) -> MatchLog {
    let response = api
        .get(&format!("/api/players/{handle}/matches{query}"))
        .await;
    assert_eq!(response.status(), StatusCode::OK, "match log of {handle}");

    read_json(response).await
}

/// A live tournament on `date` with these players, and its bracket. The
/// entrants have no level, so the draw among them is random: every test that
/// needs a name reads it back from the detail.
async fn cup(
    api: &TestApi,
    admin: &str,
    date: &str,
    players: &[&Player],
) -> (TournamentId, Bracket) {
    let created = api.create_tournament(admin, HOUR).await;
    let dated = api
        .patch_as(
            &format!("/api/admin/tournaments/{}", created.id),
            &json!({ "date": date }),
            admin,
        )
        .await;
    assert_eq!(dated.status(), StatusCode::OK, "the date did not change");
    for player in players {
        let added = api
            .post_as(
                &format!("/api/admin/tournaments/{}/entrants", created.id),
                &json!({ "playerId": player.id }),
                admin,
            )
            .await;
        assert_eq!(
            added.status(),
            StatusCode::CREATED,
            "adding {} failed",
            player.handle
        );
    }
    api.set_status(admin, created.id, "live").await;
    let generated = api
        .post_as(
            &format!("/api/admin/tournaments/{}/bracket", created.id),
            &json!({}),
            admin,
        )
        .await;
    assert_eq!(generated.status(), StatusCode::OK, "generation failed");

    (created.id, read_json(generated).await)
}

/// The entrant a handle plays as in this tournament.
async fn entrant_of(
    api: &TestApi,
    admin: &str,
    tournament: TournamentId,
    handle: &str,
) -> EntrantId {
    api.detail(admin, tournament)
        .await
        .entrants
        .iter()
        .find(|e| {
            e.player
                .as_ref()
                .is_some_and(|p| p.handle.as_ref() == handle)
        })
        .unwrap_or_else(|| panic!("{handle} does not play in {tournament}"))
        .id
}

/// One live tournament on `date` where `winner` beats `loser`, and nothing
/// else. Two entrants make one match, so the result needs no walk.
async fn duel(
    api: &TestApi,
    admin: &str,
    date: &str,
    winner: &Player,
    loser: &Player,
) -> TournamentId {
    let (tournament, bracket) = cup(api, admin, date, &[winner, loser]).await;
    let m = bracket.flat().next().cloned().unwrap();
    let side = entrant_of(api, admin, tournament, winner.handle.as_ref()).await;
    api.report(admin, tournament, &m, side).await;

    tournament
}

/// The handles the opponent of each row carries, newest first.
fn opponents(log: &MatchLog) -> Vec<Option<String>> {
    log.matches
        .items
        .iter()
        .map(|m| m.opponent.as_ref().map(|p| p.handle.as_ref().to_owned()))
        .collect()
}

#[tokio::test]
async fn an_unknown_handle_has_no_match_log() {
    let api = TestApi::with_database().await;

    let response = api.get("/api/players/nobody/matches").await;

    assert_error(response, StatusCode::NOT_FOUND, "ItemNotFound").await;
}

#[tokio::test]
async fn a_player_who_never_played_has_an_empty_log() {
    let api = TestApi::with_database().await;
    api.signup_player("dave").await;

    let log = log_of(&api, "dave").await;

    assert_eq!(log.record.wins, 0);
    assert_eq!(log.record.losses, 0);
    assert!(
        log.nemesis.is_none(),
        "nobody beat a player who never played"
    );
    assert!(log.matches.items.is_empty());
    assert_eq!(log.matches.total, 0);
}

#[tokio::test]
async fn one_loss_names_nobody_and_a_second_names_the_nemesis() {
    let api = TestApi::with_database().await;
    let admin = api.signup_admin("root").await;
    let mallory = api.signup_player("mallory").await;
    let dave = api.signup_player("dave").await;

    duel(&api, &admin, "2026-01-01", &mallory, &dave).await;
    let after_one = log_of(&api, "dave").await;
    assert_eq!((after_one.record.wins, after_one.record.losses), (0, 1));
    assert!(
        after_one.nemesis.is_none(),
        "one loss is a bad night, not a nemesis"
    );

    duel(&api, &admin, "2026-01-02", &mallory, &dave).await;
    let nemesis = log_of(&api, "dave")
        .await
        .nemesis
        .expect("two losses name one");
    assert_eq!(nemesis.opponent.handle.as_ref(), "mallory");
    assert_eq!((nemesis.wins, nemesis.losses), (0, 2), "from dave's view");

    // A win of the owner counts on the same record, and does not unseat the
    // nemesis: the losses still stand.
    duel(&api, &admin, "2026-01-03", &dave, &mallory).await;
    let log = log_of(&api, "dave").await;
    assert_eq!((log.record.wins, log.record.losses), (1, 2));
    let nemesis = log.nemesis.expect("two losses still stand");
    assert_eq!((nemesis.wins, nemesis.losses), (1, 2));

    // The winner of two of the three has no nemesis of their own.
    let theirs = log_of(&api, "mallory").await;
    assert_eq!((theirs.record.wins, theirs.record.losses), (2, 1));
    assert!(theirs.nemesis.is_none(), "one loss names nobody");
}

#[tokio::test]
async fn the_opponent_with_the_most_wins_is_the_nemesis() {
    let api = TestApi::with_database().await;
    let admin = api.signup_admin("root").await;
    let mallory = api.signup_player("mallory").await;
    let trudy = api.signup_player("trudy").await;
    let dave = api.signup_player("dave").await;

    // Trudy wins three, mallory two. Mallory's losses are the more recent, so
    // only the count can name trudy.
    duel(&api, &admin, "2026-01-01", &trudy, &dave).await;
    duel(&api, &admin, "2026-01-02", &trudy, &dave).await;
    duel(&api, &admin, "2026-01-03", &trudy, &dave).await;
    duel(&api, &admin, "2026-01-04", &mallory, &dave).await;
    duel(&api, &admin, "2026-01-05", &mallory, &dave).await;

    let nemesis = log_of(&api, "dave").await.nemesis.unwrap();

    assert_eq!(nemesis.opponent.handle.as_ref(), "trudy", "three beats two");
    assert_eq!((nemesis.wins, nemesis.losses), (0, 3));
}

/// Two tournaments can share a day, so the creation instant decides between
/// them. Recency is the pair and not each column on its own: a separate `max`
/// per column can take the day of one loss and the instant of another, and
/// then it ranks an opponent by a loss that is not their latest.
#[tokio::test]
async fn the_creation_instant_separates_two_losses_on_one_day() {
    let api = TestApi::with_database().await;
    let admin = api.signup_admin("root").await;
    let mallory = api.signup_player("mallory").await;
    let trudy = api.signup_player("trudy").await;
    let dave = api.signup_player("dave").await;

    // Each cup is created after the one above it, and a cup is dated freely, so
    // the creation order and the day order differ on purpose. Both opponents
    // last won on the 2nd, and trudy's cup of that day was created after
    // mallory's, so trudy holds the later loss.
    //
    // Mallory's loss on the 1st is the last cup created of the four. A maximum
    // taken per column would pair that instant with the day of her other loss,
    // and would answer mallory.
    duel(&api, &admin, "2026-01-01", &trudy, &dave).await;
    duel(&api, &admin, "2026-01-02", &mallory, &dave).await;
    duel(&api, &admin, "2026-01-02", &trudy, &dave).await;
    duel(&api, &admin, "2026-01-01", &mallory, &dave).await;

    let nemesis = log_of(&api, "dave").await.nemesis.unwrap();

    assert_eq!(
        nemesis.opponent.handle.as_ref(),
        "trudy",
        "the last cup of the last day was trudy's"
    );
}

#[tokio::test]
async fn the_most_recent_loss_breaks_a_tie_on_the_count() {
    let api = TestApi::with_database().await;
    let admin = api.signup_admin("root").await;
    let mallory = api.signup_player("mallory").await;
    let trudy = api.signup_player("trudy").await;
    let dave = api.signup_player("dave").await;

    duel(&api, &admin, "2026-01-01", &mallory, &dave).await;
    duel(&api, &admin, "2026-01-02", &mallory, &dave).await;
    let earlier = duel(&api, &admin, "2026-01-03", &trudy, &dave).await;
    let latest = duel(&api, &admin, "2026-01-04", &trudy, &dave).await;

    let nemesis = log_of(&api, "dave").await.nemesis.unwrap();
    assert_eq!(
        nemesis.opponent.handle.as_ref(),
        "trudy",
        "the later loss wins"
    );
    assert_eq!((nemesis.wins, nemesis.losses), (0, 2));

    // Move both losses to trudy back before both losses to mallory. The count
    // is still two each, so the date alone decides, and it now says mallory.
    for (tournament, date) in [(earlier, "2025-01-01"), (latest, "2025-01-02")] {
        let moved = api
            .patch_as(
                &format!("/api/admin/tournaments/{tournament}"),
                &json!({ "date": date }),
                &admin,
            )
            .await;
        assert_eq!(moved.status(), StatusCode::OK);
    }

    let nemesis = log_of(&api, "dave").await.nemesis.unwrap();
    assert_eq!(nemesis.opponent.handle.as_ref(), "mallory");
}

#[tokio::test]
async fn a_full_tie_answers_the_same_handle_every_time() {
    let api = TestApi::with_database().await;
    let admin = api.signup_admin("root").await;
    let anna = api.signup_player("anna").await;
    let zoe = api.signup_player("zoe").await;
    let dave = api.signup_player("dave").await;

    let cups = [
        duel(&api, &admin, "2026-01-01", &anna, &dave).await,
        duel(&api, &admin, "2026-01-01", &anna, &dave).await,
        duel(&api, &admin, "2026-01-01", &zoe, &dave).await,
        duel(&api, &admin, "2026-01-01", &zoe, &dave).await,
    ];
    // The API gives every tournament its own creation instant, and that instant
    // breaks the tie before the handle does. Flatten it, so only the handle is
    // left to decide.
    for tournament in cups {
        sqlx::query("update tournaments set created_at = 1000 where id = ?1")
            .bind(tournament.to_string())
            .execute(api.pool())
            .await
            .unwrap();
    }

    let nemesis = log_of(&api, "dave").await.nemesis.unwrap();

    // Zoe was enrolled last, so a query with no final key could answer either
    // name. The handle is the key that makes the answer the same every time.
    assert_eq!(nemesis.opponent.handle.as_ref(), "anna");
}

#[tokio::test]
async fn only_a_live_or_concluded_tournament_counts() {
    let api = TestApi::with_database().await;
    let admin = api.signup_admin("root").await;
    let mallory = api.signup_player("mallory").await;
    let dave = api.signup_player("dave").await;

    let tournament = duel(&api, &admin, "2026-01-01", &mallory, &dave).await;
    assert_eq!(log_of(&api, "dave").await.record.losses, 1);

    // A draft is not public, and no transition puts a bracket back into one, so
    // the row goes in by hand.
    sqlx::query("update tournaments set status = 'draft' where id = ?1")
        .bind(tournament.to_string())
        .execute(api.pool())
        .await
        .unwrap();
    let hidden = log_of(&api, "dave").await;
    assert_eq!((hidden.record.wins, hidden.record.losses), (0, 0));
    assert!(hidden.matches.items.is_empty());

    sqlx::query("update tournaments set status = 'live' where id = ?1")
        .bind(tournament.to_string())
        .execute(api.pool())
        .await
        .unwrap();
    assert_eq!(log_of(&api, "dave").await.record.losses, 1, "live counts");

    api.set_status(&admin, tournament, "concluded").await;
    assert_eq!(
        log_of(&api, "dave").await.record.losses,
        1,
        "and so does concluded"
    );
}

#[tokio::test]
async fn a_cleared_result_leaves_the_log() {
    let api = TestApi::with_database().await;
    let admin = api.signup_admin("root").await;
    let mallory = api.signup_player("mallory").await;
    let dave = api.signup_player("dave").await;

    let (tournament, bracket) = cup(&api, &admin, "2026-01-01", &[&mallory, &dave]).await;
    let m = bracket.flat().next().cloned().unwrap();
    let side = entrant_of(&api, &admin, tournament, "mallory").await;
    api.report(&admin, tournament, &m, side).await;
    assert_eq!(log_of(&api, "dave").await.record.losses, 1);

    let cleared = api
        .delete_as(
            &format!(
                "/api/admin/tournaments/{tournament}/matches/{}/result",
                m.id
            ),
            &admin,
        )
        .await;
    assert_eq!(cleared.status(), StatusCode::OK);

    let log = log_of(&api, "dave").await;
    assert_eq!((log.record.wins, log.record.losses), (0, 0));
    assert!(log.matches.items.is_empty(), "a corrected result is gone");
}

#[tokio::test]
async fn a_bye_is_not_a_match() {
    let api = TestApi::with_database().await;
    let admin = api.signup_admin("root").await;
    let players = [
        api.signup_player("anna").await,
        api.signup_player("bert").await,
        api.signup_player("carl").await,
    ];
    let (tournament, bracket) = cup(
        &api,
        &admin,
        "2026-01-01",
        &[&players[0], &players[1], &players[2]],
    )
    .await;

    // Three entrants in four slots: one bye, one real match in round one, then
    // the final. The bye already holds its winner.
    let first_round = bracket.rounds.first().unwrap();
    let bye = first_round
        .iter()
        .find(|m| m.entrant_b.is_none())
        .expect("a field of three has one bye");
    let waiting = bye.winner.unwrap();
    let opening = first_round
        .iter()
        .find(|m| m.is_ready())
        .cloned()
        .expect("one real match in round one");
    let detail = api.detail(&admin, tournament).await;
    let handle_of = |entrant: EntrantId| {
        detail
            .entrants
            .iter()
            .find(|e| e.id == entrant)
            .and_then(|e| e.player.as_ref())
            .map(|p| p.handle.as_ref().to_owned())
            .unwrap()
    };
    let seeded = handle_of(waiting);

    // The bye is decided already, and it pays the waiting player nothing.
    let waiting_log = log_of(&api, &seeded).await;
    assert_eq!((waiting_log.record.wins, waiting_log.record.losses), (0, 0));
    assert!(waiting_log.matches.items.is_empty(), "a bye is not a win");

    let after = api
        .report(&admin, tournament, &opening, opening.entrant_a.unwrap())
        .await;
    let last = after.rounds.last().unwrap().first().cloned().unwrap();
    api.report(&admin, tournament, &last, waiting).await;

    let log = log_of(&api, &seeded).await;
    assert_eq!((log.record.wins, log.record.losses), (1, 0));
    assert_eq!(log.matches.items.len(), 1, "the final alone");
    assert_eq!(log.matches.items[0].round, 2);
}

/// Crosses tournaments and days: the order a profile shows.
#[tokio::test]
async fn the_log_puts_the_newest_tournament_first() {
    let api = TestApi::with_database().await;
    let admin = api.signup_admin("root").await;
    let mallory = api.signup_player("mallory").await;
    let dave = api.signup_player("dave").await;

    // The write order is neither the day order nor the answer, so a log that
    // followed it fails here. The last two share a day, so only the creation
    // instant can separate them.
    let oldest = duel(&api, &admin, "2026-01-01", &mallory, &dave).await;
    let middle = duel(&api, &admin, "2026-02-02", &mallory, &dave).await;
    let shared_first = duel(&api, &admin, "2026-03-03", &mallory, &dave).await;
    let shared_last = duel(&api, &admin, "2026-03-03", &mallory, &dave).await;

    let log = log_of(&api, "dave").await;

    let order: Vec<TournamentId> = log.matches.items.iter().map(|m| m.tournament_id).collect();
    assert_eq!(order, [shared_last, shared_first, middle, oldest]);
    let days: Vec<String> = log
        .matches
        .items
        .iter()
        .map(|m| m.date.to_string())
        .collect();
    assert_eq!(
        days,
        ["2026-03-03", "2026-03-03", "2026-02-02", "2026-01-01"]
    );
}

/// The nemesis joins `players`, so a deleted opponent drops out of the ranking
/// instead of holding a title nobody can click. The opponent below them takes
/// it.
#[tokio::test]
async fn a_deleted_opponent_hands_the_title_to_the_next_candidate() {
    let api = TestApi::with_database().await;
    let admin = api.signup_admin("root").await;
    let mallory = api.signup_player("mallory").await;
    let trudy = api.signup_player("trudy").await;
    let dave = api.signup_player("dave").await;

    duel(&api, &admin, "2026-01-01", &mallory, &dave).await;
    duel(&api, &admin, "2026-01-02", &mallory, &dave).await;
    duel(&api, &admin, "2026-01-03", &mallory, &dave).await;
    duel(&api, &admin, "2026-01-04", &trudy, &dave).await;
    duel(&api, &admin, "2026-01-05", &trudy, &dave).await;
    assert_eq!(
        log_of(&api, "dave")
            .await
            .nemesis
            .unwrap()
            .opponent
            .handle
            .as_ref(),
        "mallory",
        "three losses beat two"
    );

    let removed = api
        .delete_as(&format!("/api/admin/players/{}", mallory.id), &admin)
        .await;
    assert_eq!(removed.status(), StatusCode::NO_CONTENT);

    let nemesis = log_of(&api, "dave").await.nemesis.unwrap();

    assert_eq!(nemesis.opponent.handle.as_ref(), "trudy");
    assert_eq!((nemesis.wins, nemesis.losses), (0, 2));
    assert_eq!(
        log_of(&api, "dave").await.record.losses,
        5,
        "every match was still played"
    );
}

#[tokio::test]
async fn the_log_names_the_round_and_pages() {
    let api = TestApi::with_database().await;
    let admin = api.signup_admin("root").await;
    let players = [
        api.signup_player("anna").await,
        api.signup_player("bert").await,
        api.signup_player("carl").await,
        api.signup_player("dana").await,
    ];
    let (tournament, bracket) = cup(
        &api,
        &admin,
        "2026-02-02",
        &[&players[0], &players[1], &players[2], &players[3]],
    )
    .await;

    // Side a wins every match, so one entrant takes the semi-final and the
    // final of a bracket two rounds deep.
    let mut current = bracket;
    for _ in 0..3 {
        let next = current
            .flat()
            .find(|m| m.is_ready() && m.winner.is_none())
            .cloned()
            .unwrap();
        let side = next.entrant_a.unwrap();
        current = api.report(&admin, tournament, &next, side).await;
    }
    let champion = current.champion().unwrap();
    let detail = api.detail(&admin, tournament).await;
    let handle = detail
        .entrants
        .iter()
        .find(|e| e.id == champion)
        .and_then(|e| e.player.as_ref())
        .map(|p| p.handle.as_ref().to_owned())
        .unwrap();

    let log = log_of(&api, &handle).await;
    assert_eq!((log.record.wins, log.record.losses), (2, 0));
    let rows = &log.matches.items;
    assert_eq!(rows.len(), 2);
    assert_eq!(
        rows[0].round, 2,
        "inside one tournament the final comes first"
    );
    assert_eq!(rows[0].rounds, 2, "and it is the last round");
    assert_eq!(rows[1].round, 1);
    assert_eq!(rows[1].rounds, 2, "every row knows the depth");
    assert!(rows.iter().all(|m| m.won));
    assert_eq!(rows[0].tournament_id, tournament);
    assert_eq!(rows[0].game.as_ref(), "COD MW2");
    assert_eq!(rows[0].date.to_string(), "2026-02-02");

    let page = page_of(&api, &handle, "?page=2&pageSize=1").await;
    assert_eq!(page.matches.items.len(), 1);
    assert_eq!(page.matches.total, 2);
    assert_eq!(page.matches.total_pages, 2);
    assert_eq!(
        page.matches.items[0].round, 1,
        "page two holds the older row"
    );
    assert_eq!(
        (page.record.wins, page.record.losses),
        (2, 0),
        "the record covers every match, not the page"
    );
}

#[tokio::test]
async fn a_deleted_opponent_stays_in_the_log_and_is_never_the_nemesis() {
    let api = TestApi::with_database().await;
    let admin = api.signup_admin("root").await;
    let mallory = api.signup_player("mallory").await;
    let dave = api.signup_player("dave").await;

    duel(&api, &admin, "2026-01-01", &mallory, &dave).await;
    duel(&api, &admin, "2026-01-02", &mallory, &dave).await;
    assert_eq!(
        log_of(&api, "dave")
            .await
            .nemesis
            .unwrap()
            .opponent
            .handle
            .as_ref(),
        "mallory"
    );

    let removed = api
        .delete_as(&format!("/api/admin/players/{}", mallory.id), &admin)
        .await;
    assert_eq!(removed.status(), StatusCode::NO_CONTENT);

    let log = log_of(&api, "dave").await;
    assert_eq!(
        (log.record.wins, log.record.losses),
        (0, 2),
        "the matches were still played"
    );
    assert_eq!(opponents(&log), [None, None], "nobody to link to");
    assert!(log.nemesis.is_none(), "a deleted account holds no title");
}
