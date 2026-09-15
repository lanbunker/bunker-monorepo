#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::time::{Duration, Instant};

use bunker_models::{RomName, Score};
use cabd_core::cabinet::{Cabinet, CabinetConfig, Effect, Event, Game, Outcome, Step};
use cabd_core::hiscore::Decoder;
use cabd_core::view::{DevCommand, Frame, Input, Orientation, Scenario, Screen, Tone};

fn rom(name: &str) -> RomName {
    RomName::try_new(name).unwrap()
}

fn game(name: &str, title: &str, orientation: Orientation) -> Game {
    Game {
        rom: rom(name),
        title: title.to_owned(),
        orientation,
        decoder: Decoder::AsciiDecimal,
    }
}

fn config() -> CabinetConfig {
    CabinetConfig {
        games: vec![
            game("bublbobl", "Bubble Bobble", Orientation::Landscape),
            game("galaga", "Galaga", Orientation::Tate),
        ],
        idle: Duration::from_secs(60),
    }
}

fn new_cabinet() -> (Cabinet, Instant) {
    let now = Instant::now();
    (Cabinet::new(config(), now), now)
}

fn screen(step: &Step) -> &Screen {
    match step.frame.as_ref().expect("a frame") {
        Frame::Show(vm) => &vm.screen,
        Frame::Suspended => panic!("suspended"),
    }
}

fn confirm_game(cabinet: &mut Cabinet, now: Instant, index: usize) -> Step {
    cabinet.apply(Event::Input(Input::Confirm), now);
    for _ in 0..index {
        cabinet.apply(Event::Input(Input::Down), now);
    }
    cabinet.apply(Event::Input(Input::Confirm), now)
}

#[test]
fn starts_on_attract_and_any_button_opens_select() {
    let (mut cabinet, now) = new_cabinet();
    assert!(matches!(
        cabinet.frame(),
        Frame::Show(vm) if matches!(vm.screen, Screen::Attract { .. })
    ));
    let step = cabinet.apply(Event::Input(Input::Confirm), now);
    match screen(&step) {
        Screen::Select { games, selected } => {
            assert_eq!(*selected, 0);
            assert_eq!(games.len(), 2);
            assert_eq!(games[1].title, "Galaga");
        }
        other => panic!("expected select, got {other:?}"),
    }
}

#[test]
fn select_wraps_in_both_directions() {
    let (mut cabinet, now) = new_cabinet();
    cabinet.apply(Event::Input(Input::Confirm), now);
    let step = cabinet.apply(Event::Input(Input::Up), now);
    assert!(matches!(screen(&step), Screen::Select { selected: 1, .. }));
    let step = cabinet.apply(Event::Input(Input::Down), now);
    assert!(matches!(screen(&step), Screen::Select { selected: 0, .. }));
}

#[test]
fn confirm_suspends_then_display_released_launches() {
    let (mut cabinet, now) = new_cabinet();
    let step = confirm_game(&mut cabinet, now, 1);
    assert_eq!(step.frame, Some(Frame::Suspended));
    assert!(
        step.effects.is_empty(),
        "nothing launches before the display is released"
    );

    let step = cabinet.apply(Event::DisplayReleased, now);
    assert_eq!(step.frame, None);
    assert_eq!(
        step.effects,
        vec![Effect::Launch(game("galaga", "Galaga", Orientation::Tate))]
    );
    assert_eq!(cabinet.state_name(), "in_game");
}

#[test]
fn display_released_outside_a_launch_does_nothing() {
    let (mut cabinet, now) = new_cabinet();
    let step = cabinet.apply(Event::DisplayReleased, now);
    assert_eq!(step.frame, None);
    assert!(step.effects.is_empty());
    assert_eq!(cabinet.state_name(), "attract");
}

#[test]
fn game_ended_shows_postgame_with_a_formatted_score() {
    let (mut cabinet, now) = new_cabinet();
    confirm_game(&mut cabinet, now, 0);
    cabinet.apply(Event::DisplayReleased, now);
    let step = cabinet.apply(
        Event::GameEnded(Outcome {
            rom: rom("bublbobl"),
            score: Some(Score(1_234_560)),
            note: None,
        }),
        now,
    );
    match step.frame.as_ref().expect("a frame") {
        Frame::Show(vm) => {
            assert_eq!(vm.command, "./bublbobl");
            match &vm.screen {
                Screen::Postgame {
                    score_display,
                    status,
                    ..
                } => {
                    assert_eq!(score_display, "1,234,560");
                    assert_eq!(*status, None);
                }
                other => panic!("expected postgame, got {other:?}"),
            }
        }
        Frame::Suspended => panic!("suspended"),
    }
}

#[test]
fn game_ended_without_a_score_shows_the_note() {
    let (mut cabinet, now) = new_cabinet();
    confirm_game(&mut cabinet, now, 0);
    cabinet.apply(Event::DisplayReleased, now);
    let step = cabinet.apply(
        Event::GameEnded(Outcome {
            rom: rom("bublbobl"),
            score: None,
            note: Some("The game wrote no score file".to_owned()),
        }),
        now,
    );
    match screen(&step) {
        Screen::Postgame {
            score_display,
            status,
            ..
        } => {
            assert_eq!(score_display, "NO SCORE");
            let status = status.as_ref().expect("a status line");
            assert_eq!(status.text, "The game wrote no score file");
            assert_eq!(status.tone, Tone::Dim);
        }
        other => panic!("expected postgame, got {other:?}"),
    }
}

#[test]
fn postgame_confirm_returns_to_select_on_the_same_game() {
    let (mut cabinet, now) = new_cabinet();
    confirm_game(&mut cabinet, now, 1);
    cabinet.apply(Event::DisplayReleased, now);
    cabinet.apply(
        Event::GameEnded(Outcome {
            rom: rom("galaga"),
            score: Some(Score(10)),
            note: None,
        }),
        now,
    );
    let step = cabinet.apply(Event::Input(Input::Confirm), now);
    assert!(matches!(screen(&step), Screen::Select { selected: 1, .. }));
}

#[test]
fn idle_returns_select_and_postgame_to_attract_but_not_a_game() {
    let (mut cabinet, now) = new_cabinet();
    cabinet.apply(Event::Input(Input::Confirm), now);
    let step = cabinet.apply(Event::Tick, now + Duration::from_secs(59));
    assert_eq!(step.frame, None, "still inside the idle window");
    let step = cabinet.apply(Event::Tick, now + Duration::from_secs(60));
    assert!(matches!(screen(&step), Screen::Attract { .. }));

    let (mut cabinet, now) = new_cabinet();
    confirm_game(&mut cabinet, now, 0);
    cabinet.apply(Event::DisplayReleased, now);
    let step = cabinet.apply(Event::Tick, now + Duration::from_secs(3600));
    assert_eq!(step.frame, None);
    assert_eq!(cabinet.state_name(), "in_game");
}

#[test]
fn quit_is_an_effect_and_touches_no_state() {
    let (mut cabinet, now) = new_cabinet();
    let step = cabinet.apply(Event::Quit, now);
    assert_eq!(step.effects, vec![Effect::Quit]);
    assert_eq!(step.frame, None);
    assert_eq!(cabinet.state_name(), "attract");
}

#[test]
fn a_scenario_command_jumps_to_that_screen() {
    let (mut cabinet, now) = new_cabinet();
    let step = cabinet.apply(
        Event::Command(DevCommand::Scenario(Scenario::Postgame)),
        now,
    );
    assert!(matches!(screen(&step), Screen::Postgame { .. }));
    let step = cabinet.apply(Event::Command(DevCommand::Scenario(Scenario::Select)), now);
    assert!(matches!(screen(&step), Screen::Select { selected: 0, .. }));
}

#[test]
fn a_cabinet_without_games_never_launches() {
    let now = Instant::now();
    let mut cabinet = Cabinet::new(
        CabinetConfig {
            games: Vec::new(),
            idle: Duration::from_secs(60),
        },
        now,
    );
    cabinet.apply(Event::Input(Input::Confirm), now);
    let step = cabinet.apply(Event::Input(Input::Confirm), now);
    assert_eq!(step.frame, None);
    assert!(step.effects.is_empty());
    assert_eq!(cabinet.state_name(), "select");
}
