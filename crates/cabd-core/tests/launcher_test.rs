#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use bunker_models::{RomName, Score};
use cabd_core::cabinet::Game;
use cabd_core::hiscore::Decoder;
use cabd_core::launcher::{Launcher, LauncherError};
use cabd_core::view::Orientation;

fn game() -> Game {
    Game {
        rom: RomName::try_new("galaga").unwrap(),
        title: "Galaga".to_owned(),
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

#[test]
fn copies_the_template_then_runs_then_decodes() {
    let dir = tempfile::tempdir().unwrap();
    let templates = dir.path().join("templates");
    let hiscores = dir.path().join("hi");
    fs::create_dir_all(&templates).unwrap();
    fs::write(templates.join("galaga.hi"), "0").unwrap();
    let witness = dir.path().join("witness");
    let script = script(
        dir.path(),
        &format!(
            "cat \"$2\" > {}; echo \"$1\" >> {}; printf 4242 > \"$2\"",
            witness.display(),
            witness.display()
        ),
    );

    let launcher = Launcher::new(
        &format!("{script} {{rom}} {{hi}}"),
        dir.path().join("roms"),
        hiscores.clone(),
        templates,
    )
    .unwrap();
    let outcome = launcher.run(&game()).unwrap();

    assert_eq!(outcome.score, Some(Score(4242)));
    assert_eq!(outcome.note, None);
    let seen = fs::read_to_string(witness).unwrap();
    assert!(
        seen.starts_with('0'),
        "the template was in place before the game ran: {seen}"
    );
    assert!(
        seen.contains("roms/galaga.zip"),
        "the rom path was substituted: {seen}"
    );
    assert_eq!(
        fs::read_to_string(hiscores.join("galaga.hi")).unwrap(),
        "4242"
    );
}

#[test]
fn without_a_template_a_stale_file_is_removed_first() {
    let dir = tempfile::tempdir().unwrap();
    let hiscores = dir.path().join("hi");
    fs::create_dir_all(&hiscores).unwrap();
    fs::write(hiscores.join("galaga.hi"), "999999").unwrap();
    let script = script(dir.path(), "test ! -e \"$2\"");

    let launcher = Launcher::new(
        &format!("{script} {{rom}} {{hi}}"),
        dir.path().join("roms"),
        hiscores,
        dir.path().join("no-templates"),
    )
    .unwrap();
    let outcome = launcher.run(&game()).unwrap();

    assert_eq!(outcome.score, None, "the stale score must not come back");
    assert!(outcome.note.is_some());
}

#[test]
fn a_game_that_writes_garbage_gives_a_note_not_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let script = script(dir.path(), "printf 'xx' > \"$2\"");
    let launcher = Launcher::new(
        &format!("{script} {{rom}} {{hi}}"),
        dir.path().join("roms"),
        dir.path().join("hi"),
        dir.path().join("templates"),
    )
    .unwrap();
    let outcome = launcher.run(&game()).unwrap();
    assert_eq!(outcome.score, None);
    assert_eq!(
        outcome.note.as_deref(),
        Some("The score file could not be decoded")
    );
}

#[test]
fn a_missing_program_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let launcher = Launcher::new(
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
fn an_empty_command_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    assert!(matches!(
        Launcher::new(
            "   ",
            dir.path().into(),
            dir.path().into(),
            dir.path().into()
        ),
        Err(LauncherError::EmptyCommand)
    ));
}

#[test]
fn a_launcher_that_exits_non_zero_gives_no_score_even_with_a_template() {
    let dir = tempfile::tempdir().unwrap();
    let templates = dir.path().join("templates");
    fs::create_dir_all(&templates).unwrap();
    fs::write(templates.join("galaga.hi"), "0").unwrap();
    let script = script(dir.path(), "exit 3");
    let launcher = Launcher::new(
        &format!("{script} {{rom}} {{hi}}"),
        dir.path().join("roms"),
        dir.path().join("hi"),
        templates,
    )
    .unwrap();
    let outcome = launcher.run(&game()).unwrap();
    assert_eq!(
        outcome.score, None,
        "the template must not read as a score of zero"
    );
    assert_eq!(outcome.note.as_deref(), Some("The game did not run"));
}

#[test]
fn a_game_that_leaves_the_template_untouched_gives_no_score() {
    let dir = tempfile::tempdir().unwrap();
    let templates = dir.path().join("templates");
    fs::create_dir_all(&templates).unwrap();
    fs::write(templates.join("galaga.hi"), "0").unwrap();
    let script = script(dir.path(), "true");
    let launcher = Launcher::new(
        &format!("{script} {{rom}} {{hi}}"),
        dir.path().join("roms"),
        dir.path().join("hi"),
        templates,
    )
    .unwrap();
    let outcome = launcher.run(&game()).unwrap();
    assert_eq!(outcome.score, None);
    assert_eq!(outcome.note.as_deref(), Some("The game wrote no new score"));
}
