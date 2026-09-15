//! Sample view models for the `screenshots` command. One per screen and per
//! notable variant, so a reviewer sees every layout.

use cabd_core::view::{GameCard, Row, Screen, Status, Tone, ViewModel};

pub(crate) fn samples() -> Vec<(&'static str, ViewModel)> {
    let games = [
        "Bubble Bobble",
        "Ms. Pac-Man",
        "Galaga",
        "Donkey Kong",
        "Mr. Do!",
        "DoDonPachi",
    ]
    .into_iter()
    .map(|title| GameCard {
        title: title.to_owned(),
    })
    .collect::<Vec<_>>();

    // More rows than fit and one handle longer than the row, so the cut and
    // the cap show in the screenshots.
    let leaderboard = vec![
        row(1, "dave", "1,234,560", false),
        row(2, "mira", "998,000", true),
        row(3, "kt", "410,320", false),
        row(4, "guest", "120,000", false),
        row(5, "zed", "88,000", false),
        row(6, "a_very_long_handle_x", "70,000", false),
        row(7, "seven", "60,000", false),
        row(8, "eight", "50,000", false),
        row(9, "nine", "40,000", false),
        row(10, "ten", "30,000", false),
        row(11, "eleven", "20,000", false),
        row(12, "twelve", "10,000", false),
    ];

    vec![
        (
            "attract_empty",
            guest(
                "./attract",
                Screen::Attract {
                    qr_payload: None,
                    leaderboard: Vec::new(),
                },
            ),
        ),
        (
            "attract_scores",
            guest(
                "./attract",
                Screen::Attract {
                    qr_payload: Some("https://lanbunker.eu/checkin/abc123".to_owned()),
                    leaderboard: leaderboard.clone(),
                },
            ),
        ),
        (
            "select",
            player("./select", Screen::Select { games, selected: 2 }),
        ),
        (
            "postgame_score",
            player(
                "./galaga",
                Screen::Postgame {
                    score_display: "998,000".to_owned(),
                    status: Some(Status {
                        text: "rank 2   new best".to_owned(),
                        tone: Tone::Warn,
                    }),
                    leaderboard,
                },
            ),
        ),
        (
            "postgame_no_score",
            guest(
                "./bublbobl",
                Screen::Postgame {
                    score_display: "NO SCORE".to_owned(),
                    status: Some(Status {
                        text: "The game wrote no score file".to_owned(),
                        tone: Tone::Dim,
                    }),
                    leaderboard: Vec::new(),
                },
            ),
        ),
    ]
}

fn row(rank: u32, handle: &str, score: &str, highlight: bool) -> Row {
    Row {
        rank,
        handle: handle.to_owned(),
        score_display: score.to_owned(),
        highlight,
    }
}

fn guest(command: &str, screen: Screen) -> ViewModel {
    ViewModel {
        screen,
        session: None,
        command: command.to_owned(),
    }
}

fn player(command: &str, screen: Screen) -> ViewModel {
    ViewModel {
        screen,
        session: Some("mira".to_owned()),
        command: command.to_owned(),
    }
}
