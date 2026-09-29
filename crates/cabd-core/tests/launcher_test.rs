#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use bunker_models::{GameTitle, RomName, Score};
use cabd_core::cabinet::Game;
use cabd_core::hiscore::Decoder;
use cabd_core::launcher::{Launcher, LauncherError};
use cabd_core::view::Orientation;

fn game() -> Game {
    Game {
        rom: RomName::try_new("galaga").unwrap(),
        title: GameTitle::try_new("Galaga").unwrap(),
        orientation: Orientation::Tate,
        decoder: Decoder::AsciiDecimal,
    }
}

/// A launcher script that records what it saw and writes a score. `$1` is the
/// ROM path, `$2` the live hiscore file.
fn script(dir: &Path, body: &str) -> String {
    let path = dir.join("launch.sh");
    fs::write(&path, format!("#!/bin/sh\nset -eu\n{body}\n")).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    path.to_string_lossy().into_owned()
}

/// A launcher with a template of four zeros for galaga, and the script as
/// command. A real template has the size the game writes, so the scripts
/// write four bytes when they mean a score.
fn launcher_with_template(dir: &Path, body: &str) -> Launcher {
    let templates = dir.join("templates");
    fs::create_dir_all(&templates).unwrap();
    fs::write(templates.join("galaga.hi"), "0000").unwrap();
    let script = script(dir, body);
    Launcher::try_new(
        &format!("{script} {{rom}} {{hi}}"),
        dir.join("roms"),
        dir.join("hi"),
        templates,
    )
    .unwrap()
}

#[test]
fn copies_the_template_then_runs_then_decodes() {
    let dir = tempfile::tempdir().unwrap();
    let witness = dir.path().join("witness");
    let launcher = launcher_with_template(
        dir.path(),
        &format!(
            "cat \"$2\" > {}; echo \"$1\" >> {}; printf 4242 > \"$2\"",
            witness.display(),
            witness.display()
        ),
    );
    let outcome = launcher.run(&game()).unwrap();

    assert_eq!(outcome.score, Some(Score::try_new(4242).unwrap()));
    assert_eq!(outcome.note, None);
    let seen = fs::read_to_string(witness).unwrap();
    assert!(
        seen.starts_with("0000"),
        "the template was in place before the game ran: {seen}"
    );
    assert!(
        seen.contains("roms/galaga.zip"),
        "the rom path was substituted: {seen}"
    );
    assert_eq!(
        fs::read_to_string(dir.path().join("hi/galaga.hi")).unwrap(),
        "4242"
    );
}

#[test]
fn without_a_template_the_run_gives_no_score_and_the_game_still_runs() {
    let dir = tempfile::tempdir().unwrap();
    let hiscores = dir.path().join("hi");
    fs::create_dir_all(&hiscores).unwrap();
    fs::write(hiscores.join("galaga.hi"), "999999").unwrap();
    let ran = dir.path().join("ran");
    let script = script(
        dir.path(),
        &format!("touch {}; printf 20000 > \"$2\"", ran.display()),
    );
    let launcher = Launcher::try_new(
        &format!("{script} {{rom}} {{hi}}"),
        dir.path().join("roms"),
        hiscores,
        dir.path().join("no-templates"),
    )
    .unwrap();
    let outcome = launcher.run(&game()).unwrap();

    assert!(ran.is_file(), "the game ran");
    assert_eq!(
        outcome.score, None,
        "a default table must not read as a score"
    );
    assert_eq!(
        outcome.note.as_deref(),
        Some("No score template for this game")
    );
}

#[test]
fn a_game_that_writes_garbage_gives_a_note_not_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let launcher = launcher_with_template(dir.path(), "printf 'xxxx' > \"$2\"");
    let outcome = launcher.run(&game()).unwrap();
    assert_eq!(outcome.score, None);
    assert_eq!(
        outcome.note.as_deref(),
        Some("The score file could not be decoded")
    );
}

#[test]
fn a_game_that_writes_no_file_gives_a_note() {
    let dir = tempfile::tempdir().unwrap();
    let launcher = launcher_with_template(dir.path(), "rm -f \"$2\"");
    let outcome = launcher.run(&game()).unwrap();
    assert_eq!(outcome.score, None);
    assert_eq!(
        outcome.note.as_deref(),
        Some("The game wrote no score file")
    );
}

#[test]
fn a_launcher_that_exits_non_zero_still_gives_the_score_it_wrote() {
    let dir = tempfile::tempdir().unwrap();
    let launcher = launcher_with_template(dir.path(), "printf 0007 > \"$2\"; exit 3");
    let outcome = launcher.run(&game()).unwrap();
    assert_eq!(outcome.score, Some(Score::try_new(7).unwrap()));
    assert_eq!(outcome.note, None);
}

#[test]
fn a_game_that_leaves_the_template_untouched_gives_no_score() {
    let dir = tempfile::tempdir().unwrap();
    let launcher = launcher_with_template(dir.path(), "true");
    let outcome = launcher.run(&game()).unwrap();
    assert_eq!(outcome.score, None);
    assert_eq!(outcome.note.as_deref(), Some("The game wrote no new score"));
}

#[test]
fn a_file_of_another_size_than_the_template_gives_no_score() {
    let dir = tempfile::tempdir().unwrap();
    let launcher = launcher_with_template(dir.path(), "printf 42 > \"$2\"");
    let outcome = launcher.run(&game()).unwrap();
    assert_eq!(
        outcome.score, None,
        "a cut write must not decode as a smaller score"
    );
    assert_eq!(
        outcome.note.as_deref(),
        Some("The score file has an unexpected size")
    );
}

#[test]
fn a_missing_program_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let launcher = Launcher::try_new(
        "/nonexistent/launcher {rom}",
        dir.path().join("roms"),
        dir.path().join("hi"),
        dir.path().join("templates"),
    )
    .unwrap();
    assert!(matches!(
        launcher.run(&game()),
        Err(LauncherError::Spawn { .. })
    ));
}

#[test]
fn an_empty_command_or_one_without_the_rom_placeholder_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let paths = || {
        (
            dir.path().to_path_buf(),
            dir.path().to_path_buf(),
            dir.path().to_path_buf(),
        )
    };
    let (a, b, c) = paths();
    assert!(matches!(
        Launcher::try_new("   ", a, b, c),
        Err(LauncherError::EmptyCommand)
    ));
    let (a, b, c) = paths();
    assert!(matches!(
        Launcher::try_new("retroarch {hi}", a, b, c),
        Err(LauncherError::NoRomPlaceholder)
    ));
}
