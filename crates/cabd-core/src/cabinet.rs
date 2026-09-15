//! The state machine. Every decision the cabinet takes is here, and nothing
//! else is: no I/O, no threads, no clock. The conductor feeds it an `Event` and
//! the time, and executes the `Effect` values it returns.

use std::time::{Duration, Instant};

use bunker_models::{RomName, Score};

use crate::hiscore::Decoder;
use crate::view::{
    DevCommand, Frame, GameCard, Input, Orientation, Scenario, Screen, Status, Tone, ViewModel,
    format_score,
};

/// What the attract QR code encodes until the check-in flow exists. The site
/// root is true today and the nonce URL replaces it.
const ATTRACT_QR_PAYLOAD: &str = "https://lanbunker.eu";

/// One entry of `games.toml`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Game {
    pub rom: RomName,
    pub title: String,
    pub orientation: Orientation,
    pub decoder: Decoder,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CabinetConfig {
    pub games: Vec<Game>,
    /// Without input for this long, the select and postgame screens go back to
    /// attract. A game in progress never times out.
    pub idle: Duration,
}

/// What happened, from the outside in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    Input(Input),
    /// The conductor sends one when nothing else arrives for a while. The
    /// idle timeout runs on these.
    Tick,
    /// The screen has destroyed its window. The launcher may start.
    DisplayReleased,
    GameEnded(Outcome),
    Command(DevCommand),
    /// The window was closed, or a debug key asked to stop.
    Quit,
}

/// The result of one run of a game. A missing or unreadable `.hi` file is an
/// outcome with no score and a note, not an error: the player still gets a
/// postgame screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome {
    pub rom: RomName,
    pub score: Option<Score>,
    pub note: Option<String>,
}

/// What the conductor must do. The state machine never does it itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    /// Start the launcher for this game and wait for it. The display is
    /// already released.
    Launch(Game),
    Quit,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    /// A new frame, or nothing when the screen has nothing new to draw.
    pub frame: Option<Frame>,
    pub effects: Vec<Effect>,
}

#[derive(Debug)]
pub struct Cabinet {
    config: CabinetConfig,
    state: State,
    last_input: Instant,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum State {
    Attract,
    Select {
        selected: usize,
    },
    /// Confirm was pressed. The screen is giving the display away.
    Releasing {
        game: Game,
    },
    InGame {
        game: Game,
    },
    Postgame {
        outcome: Outcome,
    },
}

impl Cabinet {
    pub fn new(config: CabinetConfig, now: Instant) -> Self {
        Self {
            config,
            state: State::Attract,
            last_input: now,
        }
    }

    /// The frame for the current state. The conductor sends it one time at
    /// startup, before any event.
    pub fn frame(&self) -> Frame {
        match &self.state {
            State::Attract => show(
                "./attract",
                Screen::Attract {
                    qr_payload: Some(ATTRACT_QR_PAYLOAD.to_owned()),
                    leaderboard: Vec::new(),
                },
            ),
            State::Select { selected } => show(
                "./select",
                Screen::Select {
                    games: self
                        .config
                        .games
                        .iter()
                        .map(|game| GameCard {
                            title: game.title.clone(),
                        })
                        .collect(),
                    selected: *selected,
                },
            ),
            State::Releasing { .. } | State::InGame { .. } => Frame::Suspended,
            State::Postgame { outcome } => show(
                &format!("./{}", outcome.rom),
                Screen::Postgame {
                    score_display: outcome
                        .score
                        .map_or_else(|| "NO SCORE".to_owned(), format_score),
                    status: outcome.note.clone().map(|text| Status {
                        text,
                        tone: Tone::Dim,
                    }),
                    leaderboard: Vec::new(),
                },
            ),
        }
    }

    /// The name of the state, for the log line of each transition.
    pub fn state_name(&self) -> &'static str {
        match self.state {
            State::Attract => "attract",
            State::Select { .. } => "select",
            State::Releasing { .. } => "releasing",
            State::InGame { .. } => "in_game",
            State::Postgame { .. } => "postgame",
        }
    }

    pub fn apply(&mut self, event: Event, now: Instant) -> Step {
        match event {
            Event::Quit => Step {
                frame: None,
                effects: vec![Effect::Quit],
            },
            Event::Input(input) => {
                self.last_input = now;
                self.on_input(input)
            }
            Event::Tick => self.on_tick(now),
            Event::DisplayReleased => match &self.state {
                State::Releasing { game } => {
                    let game = game.clone();
                    self.state = State::InGame { game: game.clone() };
                    Step {
                        frame: None,
                        effects: vec![Effect::Launch(game)],
                    }
                }
                _ => Step::nothing(),
            },
            Event::GameEnded(outcome) => match &self.state {
                State::InGame { .. } => {
                    self.last_input = now;
                    self.goto(State::Postgame { outcome })
                }
                _ => Step::nothing(),
            },
            Event::Command(DevCommand::Scenario(scenario)) => {
                self.last_input = now;
                self.goto(self.scenario_state(scenario))
            }
        }
    }

    fn on_input(&mut self, input: Input) -> Step {
        match (&self.state, input) {
            (State::Attract, Input::Back) => Step::nothing(),
            (State::Attract, _) => self.goto(State::Select { selected: 0 }),
            (State::Select { selected }, Input::Up) => {
                let selected = selected
                    .checked_sub(1)
                    .unwrap_or(self.config.games.len().saturating_sub(1));
                self.goto(State::Select { selected })
            }
            (State::Select { selected }, Input::Down) => {
                let selected = match self.config.games.len() {
                    0 => 0,
                    n => (selected + 1) % n,
                };
                self.goto(State::Select { selected })
            }
            (State::Select { selected }, Input::Confirm) => {
                match self.config.games.get(*selected) {
                    Some(game) => {
                        let game = game.clone();
                        self.goto(State::Releasing { game })
                    }
                    None => Step::nothing(),
                }
            }
            (State::Select { .. }, Input::Back) => self.goto(State::Attract),
            (State::Select { .. }, Input::Left | Input::Right) => Step::nothing(),
            (State::Releasing { .. } | State::InGame { .. }, _) => Step::nothing(),
            (State::Postgame { outcome }, Input::Confirm) => {
                let selected = self
                    .config
                    .games
                    .iter()
                    .position(|game| game.rom == outcome.rom)
                    .unwrap_or(0);
                self.goto(State::Select { selected })
            }
            (State::Postgame { .. }, Input::Back) => self.goto(State::Attract),
            (State::Postgame { .. }, Input::Up | Input::Down | Input::Left | Input::Right) => {
                Step::nothing()
            }
        }
    }

    fn on_tick(&mut self, now: Instant) -> Step {
        let idle_expired = now.saturating_duration_since(self.last_input) >= self.config.idle;
        match self.state {
            State::Select { .. } | State::Postgame { .. } if idle_expired => {
                self.goto(State::Attract)
            }
            _ => Step::nothing(),
        }
    }

    fn scenario_state(&self, scenario: Scenario) -> State {
        match scenario {
            Scenario::Attract => State::Attract,
            Scenario::Select => State::Select { selected: 0 },
            // A postgame needs a game. A cabinet with none shows attract.
            Scenario::Postgame => match self.config.games.first() {
                Some(game) => State::Postgame {
                    outcome: Outcome {
                        rom: game.rom.clone(),
                        score: Some(Score(123_450)),
                        note: Some("Scenario, not a real run".to_owned()),
                    },
                },
                None => State::Attract,
            },
        }
    }

    fn goto(&mut self, state: State) -> Step {
        self.state = state;
        Step {
            frame: Some(self.frame()),
            effects: Vec::new(),
        }
    }
}

impl Step {
    fn nothing() -> Self {
        Self {
            frame: None,
            effects: Vec::new(),
        }
    }
}

fn show(command: &str, screen: Screen) -> Frame {
    Frame::Show(ViewModel {
        screen,
        session: None,
        command: command.to_owned(),
    })
}
