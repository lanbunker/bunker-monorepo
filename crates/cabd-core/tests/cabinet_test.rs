#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::time::{Duration, Instant};

use bunker_models::{GameTitle, RomName, Score};
use cabd_core::cabinet::{Cabinet, CabinetConfig, Effect, Event, Game, Outcome, Step};
use cabd_core::hiscore::Decoder;
use cabd_core::view::{DevCommand, Frame, Input, Orientation, Scenario, Screen, Tone};

const IDLE: Duration = Duration::from_secs(60);

fn rom(name: &str) -> RomName {
    RomName::try_new(name).unwrap()
}

fn game(name: &str, title: &str, orientation: Orientation) -> Game {
    Game {
        rom: rom(name),
        title: GameTitle::try_new(title).unwrap(),
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
        idle: IDLE,
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

fn outcome(name: &str, score: Option<u64>, note: Option<&str>) -> Outcome {
    Outcome {
        rom: rom(name),
        score: score.map(|s| Score::try_new(s).unwrap()),
        note: note.map(str::to_owned),
    }
}

/// Selects the game at `index` and releases the display, so the cabinet is in
/// the game. Returns the time of the last press.
fn play(cabinet: &mut Cabinet, now: Instant, index: usize) -> Instant {
    cabinet.apply(Event::Input(Input::Confirm), now);
    for _ in 0..index {
        cabinet.apply(Event::Input(Input::Down), now);
    }
    let step = cabinet.apply(Event::Input(Input::Confirm), now);
    assert_eq!(step.frame, Some(Frame::Suspended));
    cabinet.apply(Event::DisplayReleased, now);
    assert_eq!(cabinet.state_name(), "in_game");
    now
}

#[test]
fn starts_on_attract_and_any_button_opens_select() {
    let (mut cabinet, now) = new_cabinet();
    assert!(matches!(
        cabinet.frame(),
        Frame::Show(vm) if matches!(vm.screen, Screen::Attract { qr_payload: Some(_), .. })
    ));
    let step = cabinet.apply(Event::Input(Input::Back), now);
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
fn select_wraps_in_both_directions_and_back_returns_to_attract() {
    let (mut cabinet, now) = new_cabinet();
    cabinet.apply(Event::Input(Input::Confirm), now);
    let step = cabinet.apply(Event::Input(Input::Up), now);
    assert!(matches!(screen(&step), Screen::Select { selected: 1, .. }));
    let step = cabinet.apply(Event::Input(Input::Down), now);
    assert!(matches!(screen(&step), Screen::Select { selected: 0, .. }));
    let step = cabinet.apply(Event::Input(Input::Left), now);
    assert_eq!(step.frame, None, "left and right mean nothing in the list");
    let step = cabinet.apply(Event::Input(Input::Back), now);
    assert!(matches!(screen(&step), Screen::Attract { .. }));
}

#[test]
fn confirm_suspends_then_display_released_launches() {
    let (mut cabinet, now) = new_cabinet();
    cabinet.apply(Event::Input(Input::Confirm), now);
    cabinet.apply(Event::Input(Input::Down), now);
    let step = cabinet.apply(Event::Input(Input::Confirm), now);
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

    let step = cabinet.apply(Event::DisplayReleased, now);
    assert!(step.effects.is_empty(), "a second release launches nothing");
    let step = cabinet.apply(Event::Input(Input::Confirm), now);
    assert_eq!(step.frame, None, "input during a game is ignored");
    assert_eq!(cabinet.state_name(), "in_game");
}

#[test]
fn display_released_and_game_ended_outside_a_launch_do_nothing() {
    let (mut cabinet, now) = new_cabinet();
    let step = cabinet.apply(Event::DisplayReleased, now);
    assert_eq!(step.frame, None);
    assert!(step.effects.is_empty());
    let step = cabinet.apply(Event::GameEnded(outcome("galaga", Some(1), None)), now);
    assert_eq!(step.frame, None);
    assert_eq!(cabinet.state_name(), "attract");
}

#[test]
fn game_ended_shows_postgame_with_a_formatted_score_and_the_rom_as_command() {
    let (mut cabinet, now) = new_cabinet();
    play(&mut cabinet, now, 0);
    let step = cabinet.apply(
        Event::GameEnded(outcome("bublbobl", Some(1_234_560), None)),
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
fn game_ended_without_a_score_shows_the_note_as_a_dim_status() {
    let (mut cabinet, now) = new_cabinet();
    play(&mut cabinet, now, 0);
    let step = cabinet.apply(
        Event::GameEnded(outcome(
            "bublbobl",
            None,
            Some("The game wrote no score file"),
        )),
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
fn input_right_after_game_over_is_ignored_for_a_moment() {
    let (mut cabinet, now) = new_cabinet();
    play(&mut cabinet, now, 1);
    let ended = now + Duration::from_secs(300);
    cabinet.apply(Event::GameEnded(outcome("galaga", Some(10), None)), ended);

    let step = cabinet.apply(
        Event::Input(Input::Confirm),
        ended + Duration::from_millis(500),
    );
    assert_eq!(
        step.frame, None,
        "a mashed button must not skip the postgame"
    );
    assert_eq!(cabinet.state_name(), "postgame");

    let step = cabinet.apply(Event::Input(Input::Confirm), ended + Duration::from_secs(2));
    assert_eq!(
        step.frame,
        Some(Frame::Suspended),
        "confirm plays the same game again"
    );
    let step = cabinet.apply(Event::DisplayReleased, ended + Duration::from_secs(2));
    assert_eq!(
        step.effects,
        vec![Effect::Launch(game("galaga", "Galaga", Orientation::Tate))]
    );
}

#[test]
fn postgame_back_returns_to_select_on_the_same_game() {
    let (mut cabinet, now) = new_cabinet();
    play(&mut cabinet, now, 1);
    let ended = now + Duration::from_secs(300);
    cabinet.apply(Event::GameEnded(outcome("galaga", Some(10), None)), ended);
    let later = ended + Duration::from_secs(2);
    let step = cabinet.apply(Event::Input(Input::Up), later);
    assert_eq!(step.frame, None, "directions mean nothing on the postgame");
    let step = cabinet.apply(Event::Input(Input::Back), later);
    assert!(matches!(screen(&step), Screen::Select { selected: 1, .. }));
}

#[test]
fn select_goes_back_to_attract_after_the_idle_time_from_the_last_press() {
    let (mut cabinet, now) = new_cabinet();
    let pressed = now + Duration::from_secs(10);
    cabinet.apply(Event::Input(Input::Confirm), pressed);
    let step = cabinet.apply(Event::Tick, pressed + IDLE - Duration::from_secs(1));
    assert_eq!(step.frame, None, "still inside the idle window");
    let step = cabinet.apply(Event::Tick, pressed + IDLE);
    assert!(matches!(screen(&step), Screen::Attract { .. }));
}

#[test]
fn postgame_goes_back_to_attract_after_the_idle_time_from_game_over() {
    let (mut cabinet, now) = new_cabinet();
    play(&mut cabinet, now, 0);
    let ended = now + Duration::from_secs(3600);
    cabinet.apply(Event::GameEnded(outcome("bublbobl", Some(5), None)), ended);
    let step = cabinet.apply(Event::Tick, ended + IDLE - Duration::from_secs(1));
    assert_eq!(step.frame, None, "the game time does not count as idle");
    assert_eq!(cabinet.state_name(), "postgame");
    let step = cabinet.apply(Event::Tick, ended + IDLE);
    assert!(matches!(screen(&step), Screen::Attract { .. }));
}

#[test]
fn a_game_in_progress_never_times_out() {
    let (mut cabinet, now) = new_cabinet();
    play(&mut cabinet, now, 0);
    let step = cabinet.apply(Event::Tick, now + Duration::from_secs(36_000));
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
fn a_scenario_command_jumps_to_that_screen_and_resets_the_idle_time() {
    let (mut cabinet, now) = new_cabinet();
    let later = now + Duration::from_secs(3600);
    let step = cabinet.apply(
        Event::Command(DevCommand::Scenario(Scenario::Postgame)),
        later,
    );
    assert!(matches!(screen(&step), Screen::Postgame { .. }));
    let step = cabinet.apply(Event::Tick, later + Duration::from_secs(1));
    assert_eq!(step.frame, None, "the scenario counts as input");
    let step = cabinet.apply(
        Event::Command(DevCommand::Scenario(Scenario::Select)),
        later,
    );
    assert!(matches!(screen(&step), Screen::Select { selected: 0, .. }));
}

#[test]
fn a_cabinet_without_games_never_launches_and_shows_attract_for_a_postgame_scenario() {
    let now = Instant::now();
    let mut cabinet = Cabinet::new(
        CabinetConfig {
            games: Vec::new(),
            idle: IDLE,
        },
        now,
    );
    cabinet.apply(Event::Input(Input::Confirm), now);
    let step = cabinet.apply(Event::Input(Input::Confirm), now);
    assert_eq!(step.frame, None);
    assert!(step.effects.is_empty());
    assert_eq!(cabinet.state_name(), "select");
    let step = cabinet.apply(
        Event::Command(DevCommand::Scenario(Scenario::Postgame)),
        now,
    );
    assert!(matches!(screen(&step), Screen::Attract { .. }));
}
