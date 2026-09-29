#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

//! The whole loop without a screen: a fake screen on the channels, a shell
//! script as the launcher, the real conductor in between.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::sync::mpsc::RecvTimeoutError;
use std::time::Duration;

use cabd_core::StartError;
use cabd_core::config::{Config, ConfigError};
use cabd_core::view::{Frame, Input, Orientation, OverscanPercent, Screen, TateTurn};

const WAIT: Duration = Duration::from_secs(5);

fn write_games(dir: &Path) -> std::path::PathBuf {
    let path = dir.join("games.toml");
    fs::write(
        &path,
        r#"
[[games]]
rom = "galaga"
title = "Galaga"
orientation = "tate"
decoder = "ascii_decimal"
"#,
    )
    .unwrap();
    path
}

fn write_launcher(dir: &Path) -> String {
    let templates = dir.join("templates");
    fs::create_dir_all(&templates).unwrap();
    fs::write(templates.join("galaga.hi"), "0000").unwrap();
    let path = dir.join("launch.sh");
    fs::write(&path, "#!/bin/sh\nset -eu\nprintf 4242 > \"$2\"\n").unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    path.to_string_lossy().into_owned()
}

fn config(dir: &Path) -> Config {
    Config {
        games: write_games(dir),
        rom_dir: dir.join("roms"),
        launcher: format!("{} {{rom}} {{hi}}", write_launcher(dir)),
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

fn next_frame(handle: &cabd_core::Handle) -> Frame {
    handle.frames.recv_timeout(WAIT).expect("a frame in time")
}

#[test]
fn a_full_run_goes_attract_select_suspended_postgame() {
    let dir = tempfile::tempdir().unwrap();
    let handle = cabd_core::start(config(dir.path())).unwrap();

    assert!(
        matches!(next_frame(&handle), Frame::Show(vm) if matches!(vm.screen, Screen::Attract { .. }))
    );

    handle.input(Input::Confirm).unwrap();
    assert!(
        matches!(next_frame(&handle), Frame::Show(vm) if matches!(vm.screen, Screen::Select { .. }))
    );

    handle.input(Input::Confirm).unwrap();
    assert_eq!(next_frame(&handle), Frame::Suspended);

    handle.display_released().unwrap();
    match next_frame(&handle) {
        Frame::Show(vm) => {
            assert_eq!(vm.command, "./galaga");
            match vm.screen {
                Screen::Postgame {
                    score_display,
                    status,
                    ..
                } => {
                    assert_eq!(score_display, "4,242");
                    assert_eq!(status, None);
                }
                other => panic!("expected postgame, got {other:?}"),
            }
        }
        Frame::Suspended => panic!("still suspended"),
    }
    assert_eq!(
        fs::read_to_string(dir.path().join("hi/galaga.hi")).unwrap(),
        "4242"
    );
}

#[test]
fn quit_stops_the_conductor_and_closes_the_frames_and_join_sees_no_panic() {
    let dir = tempfile::tempdir().unwrap();
    let handle = cabd_core::start(config(dir.path())).unwrap();
    next_frame(&handle);

    handle.quit().unwrap();
    match handle.frames.recv_timeout(WAIT) {
        Err(RecvTimeoutError::Disconnected) => {}
        other => panic!("expected the frames channel to close, got {other:?}"),
    }
    assert!(
        handle.input(Input::Confirm).is_err(),
        "nothing receives any more"
    );
    handle.join().unwrap();
}

#[test]
fn a_dropped_handle_stops_the_conductor() {
    let dir = tempfile::tempdir().unwrap();
    let handle = cabd_core::start(config(dir.path())).unwrap();
    next_frame(&handle);
    handle.join().unwrap();
}

#[test]
fn a_tick_sends_no_frame() {
    let dir = tempfile::tempdir().unwrap();
    let handle = cabd_core::start(config(dir.path())).unwrap();
    next_frame(&handle);
    match handle.frames.recv_timeout(Duration::from_millis(700)) {
        Err(RecvTimeoutError::Timeout) => {}
        other => panic!("expected no frame from the ticks, got {other:?}"),
    }
}

#[test]
fn a_missing_games_file_fails_at_start() {
    let dir = tempfile::tempdir().unwrap();
    let mut config = config(dir.path());
    config.games = dir.path().join("nowhere.toml");
    match cabd_core::start(config) {
        Err(StartError::Config(ConfigError::ReadGames { path, .. })) => {
            assert!(path.ends_with("nowhere.toml"));
        }
        other => panic!("expected a read error, got {other:?}"),
    }
}
