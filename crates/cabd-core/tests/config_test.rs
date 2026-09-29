#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

//! The games list and the parsers behind the `CABD_*` variables.

use std::fs;
use std::path::Path;

use cabd_core::config::{Config, ConfigError};
use cabd_core::view::{DisplayAspect, Orientation, OverscanPercent, TateTurn, WindowSize};

fn config_with_games(dir: &Path, games: &str) -> Config {
    let path = dir.join("games.toml");
    fs::write(&path, games).unwrap();
    Config {
        games: path,
        rom_dir: dir.join("roms"),
        launcher: "true {rom} {hi}".to_owned(),
        hiscore_dir: dir.join("hi"),
        template_dir: dir.join("templates"),
        idle_seconds: 180,
        orientation: Orientation::Landscape,
        tate_turn: TateTurn::Left,
        display_aspect: None,
        overscan_percent: OverscanPercent::try_new(0).unwrap(),
        window: None,
        upright: false,
    }
}

const ONE_GAME: &str = r#"
[[games]]
rom = "galaga"
title = "  Galaga "
orientation = "tate"
decoder = "ascii_decimal"
"#;

#[test]
fn a_games_list_loads_in_file_order_with_trimmed_titles() {
    let dir = tempfile::tempdir().unwrap();
    let games = config_with_games(
        dir.path(),
        &format!(
            "{ONE_GAME}\n[[games]]\nrom = \"bublbobl\"\ntitle = \"Bubble Bobble\"\norientation = \"landscape\"\ndecoder = \"ascii_decimal\"\n"
        ),
    )
    .load_games()
    .unwrap();
    assert_eq!(games.len(), 2);
    assert_eq!(games[0].rom.as_ref(), "galaga");
    assert_eq!(games[0].title.as_ref(), "Galaga");
    assert_eq!(games[0].orientation, Orientation::Tate);
    assert_eq!(games[1].rom.as_ref(), "bublbobl");
}

#[test]
fn a_missing_file_an_empty_list_and_a_bad_entry_are_distinct_errors() {
    let dir = tempfile::tempdir().unwrap();
    let mut config = config_with_games(dir.path(), "games = []\n");
    assert!(matches!(
        config.load_games(),
        Err(ConfigError::NoGames { .. })
    ));

    config.games = dir.path().join("nowhere.toml");
    assert!(matches!(
        config.load_games(),
        Err(ConfigError::ReadGames { .. })
    ));

    for bad in [
        "[[games]]\nrom = \"galaga\"\ntitle = \"Galaga\"\norientation = \"tate\"\ndecoder = \"ascii_decimal\"\npicture = \"x.png\"\n",
        "[[games]]\nrom = \"Galaga\"\ntitle = \"Galaga\"\norientation = \"tate\"\ndecoder = \"ascii_decimal\"\n",
        "[[games]]\nrom = \"galaga\"\ntitle = \"   \"\norientation = \"tate\"\ndecoder = \"ascii_decimal\"\n",
        "[[games]]\nrom = \"galaga\"\ntitle = \"Galaga\"\norientation = \"sideways\"\ndecoder = \"ascii_decimal\"\n",
        "[[games]]\nrom = \"galaga\"\ntitle = \"Galaga\"\norientation = \"tate\"\ndecoder = \"magic\"\n",
    ] {
        let config = config_with_games(dir.path(), bad);
        assert!(
            matches!(config.load_games(), Err(ConfigError::ParseGames { .. })),
            "expected a parse error for {bad}"
        );
    }
}

#[test]
fn a_rom_named_two_times_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let config = config_with_games(dir.path(), &format!("{ONE_GAME}{ONE_GAME}"));
    match config.load_games() {
        Err(ConfigError::DuplicateRom { rom, .. }) => assert_eq!(rom.as_ref(), "galaga"),
        other => panic!("expected a duplicate error, got {other:?}"),
    }
}

#[test]
fn a_launcher_without_the_rom_placeholder_is_a_config_error() {
    let dir = tempfile::tempdir().unwrap();
    let mut config = config_with_games(dir.path(), ONE_GAME);
    config.launcher = "retroarch".to_owned();
    assert!(matches!(config.launcher(), Err(ConfigError::Launcher(_))));
}

#[test]
fn the_variable_parsers_accept_their_forms_and_refuse_the_rest() {
    assert_eq!("tate".parse::<Orientation>().unwrap(), Orientation::Tate);
    assert_eq!(
        " Landscape ".parse::<Orientation>().unwrap(),
        Orientation::Landscape
    );
    assert!("portrait".parse::<Orientation>().is_err());

    assert_eq!("right".parse::<TateTurn>().unwrap(), TateTurn::Right);
    assert!("up".parse::<TateTurn>().is_err());

    assert_eq!(
        "4:3".parse::<DisplayAspect>().unwrap(),
        DisplayAspect {
            width: 4,
            height: 3
        }
    );
    assert!("4x3".parse::<DisplayAspect>().is_err());
    assert!("4:0".parse::<DisplayAspect>().is_err());

    assert_eq!(
        "960x720".parse::<WindowSize>().unwrap(),
        WindowSize {
            width: 960,
            height: 720
        }
    );
    assert!("960".parse::<WindowSize>().is_err());
    assert!("0x720".parse::<WindowSize>().is_err());

    assert_eq!(
        "25".parse::<OverscanPercent>().unwrap(),
        OverscanPercent::try_new(25).unwrap()
    );
    assert!("26".parse::<OverscanPercent>().is_err());
    assert!("-1".parse::<OverscanPercent>().is_err());
}
