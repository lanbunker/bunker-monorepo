//! The loop that owns the state machine. It receives every `Event` on one
//! channel, applies it, sends the new `Frame` to the screen and executes the
//! effects. It runs on its own thread, so the screen thread never blocks.

use std::collections::VecDeque;
use std::error::Error;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use tracing::{debug, error, info, trace};

use crate::cabinet::{Cabinet, Effect, Event, Outcome};
use crate::config::{Config, ConfigError};
use crate::launcher::Launcher;
use crate::view::{DevCommand, Frame, Input};

/// The loop sends itself a `Tick` at this interval, whatever else arrives. The
/// idle timeout runs on ticks, so this is also its resolution.
pub const TICK: Duration = Duration::from_millis(250);

/// The screen's side of the two channels, and the thread behind them. The
/// screen sends through the methods and never builds an `Event` itself.
#[derive(Debug)]
pub struct Handle {
    events: Sender<Event>,
    pub frames: Receiver<Frame>,
    thread: Option<JoinHandle<()>>,
}

/// The conductor thread has stopped, so nothing receives events any more.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("the conductor is gone")]
pub struct ConductorGone;

/// The conductor thread ended with a panic instead of a quit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("the conductor thread panicked")]
pub struct ConductorPanicked;

#[derive(Debug, thiserror::Error)]
pub enum StartError {
    #[error("configuration error")]
    Config(#[source] ConfigError),
    #[error("cannot start the conductor thread")]
    Thread(#[source] std::io::Error),
}

impl Handle {
    pub fn input(&self, input: Input) -> Result<(), ConductorGone> {
        self.send(Event::Input(input))
    }

    /// The window is destroyed. The launcher may take the display.
    pub fn display_released(&self) -> Result<(), ConductorGone> {
        self.send(Event::DisplayReleased)
    }

    pub fn command(&self, command: DevCommand) -> Result<(), ConductorGone> {
        self.send(Event::Command(command))
    }

    pub fn quit(&self) -> Result<(), ConductorGone> {
        self.send(Event::Quit)
    }

    /// Closes the event channel and waits for the thread. A thread that
    /// stopped on its own has already released the frames channel, so the
    /// screen sees that first and calls this after.
    pub fn join(mut self) -> Result<(), ConductorPanicked> {
        drop(self.frames);
        let thread = self.thread.take();
        drop(self.events);
        match thread {
            Some(thread) => thread.join().map_err(|_| ConductorPanicked),
            None => Ok(()),
        }
    }

    fn send(&self, event: Event) -> Result<(), ConductorGone> {
        self.events.send(event).map_err(|_| ConductorGone)
    }
}

/// Loads the games, starts the conductor thread and returns its two channels.
/// The first frame is on the channel before this returns.
pub fn start(config: Config) -> Result<Handle, StartError> {
    info!(
        games = %config.games.display(),
        rom_dir = %config.rom_dir.display(),
        launcher = %config.launcher,
        hiscore_dir = %config.hiscore_dir.display(),
        template_dir = %config.template_dir.display(),
        idle_seconds = config.idle_seconds,
        orientation = %config.orientation,
        tate_turn = %config.tate_turn,
        display_aspect = ?config.display_aspect,
        overscan_percent = %config.overscan_percent,
        window = ?config.window,
        upright = config.upright,
        "configuration"
    );
    let games = config.load_games().map_err(StartError::Config)?;
    for game in &games {
        info!(rom = %game.rom, title = %game.title, orientation = %game.orientation, decoder = ?game.decoder, "game");
    }
    let launcher = config.launcher().map_err(StartError::Config)?;
    let cabinet = Cabinet::new(config.cabinet(games), Instant::now());

    let (events_tx, events_rx) = mpsc::channel();
    let (frames_tx, frames_rx) = mpsc::channel();
    let mut conductor = Conductor {
        cabinet,
        launcher,
        frames: frames_tx,
    };
    conductor.send_frame(conductor.cabinet.frame());

    let thread = thread::Builder::new()
        .name("conductor".to_owned())
        .spawn(move || conductor.run(&events_rx))
        .map_err(StartError::Thread)?;

    Ok(Handle {
        events: events_tx,
        frames: frames_rx,
        thread: Some(thread),
    })
}

/// Every cause of an error, outermost first, for one log line.
pub fn error_chain(error: &dyn Error) -> String {
    let mut parts = vec![error.to_string()];
    let mut cause = error.source();
    while let Some(next) = cause {
        parts.push(next.to_string());
        cause = next.source();
    }
    parts.join(": ")
}

struct Conductor {
    cabinet: Cabinet,
    launcher: Launcher,
    frames: Sender<Frame>,
}

impl Conductor {
    fn run(&mut self, events: &Receiver<Event>) {
        info!("conductor started");
        let mut next_tick = Instant::now() + TICK;
        loop {
            let wait = next_tick.saturating_duration_since(Instant::now());
            let event = match events.recv_timeout(wait) {
                Ok(event) => event,
                Err(RecvTimeoutError::Timeout) => {
                    next_tick += TICK;
                    Event::Tick
                }
                Err(RecvTimeoutError::Disconnected) => {
                    info!("every event sender is gone, conductor stops");
                    return;
                }
            };
            if !self.handle(event) {
                info!("conductor stops");
                return;
            }
        }
    }

    /// Applies one event and every event its effects produce. Returns false
    /// when the cabinet wants to quit or the screen is gone. The effects of a
    /// step run even when its frame cannot be delivered, because an effect
    /// such as a queued score must not depend on the screen.
    fn handle(&mut self, first: Event) -> bool {
        let mut queue = VecDeque::from([first]);
        let mut screen_alive = true;
        while let Some(event) = queue.pop_front() {
            let from = self.cabinet.state_name();
            let is_tick = matches!(event, Event::Tick);
            let step = self.cabinet.apply(event.clone(), Instant::now());
            let to = self.cabinet.state_name();
            if from != to || !step.effects.is_empty() {
                info!(event = ?event, from, to, effects = ?step.effects, "transition");
            } else if is_tick {
                trace!("tick");
            } else {
                debug!(event = ?event, state = to, "event without transition");
            }
            if let Some(frame) = step.frame
                && !self.send_frame(frame)
            {
                screen_alive = false;
            }
            for effect in step.effects {
                match effect {
                    Effect::Launch(game) => {
                        let started = Instant::now();
                        let outcome = match self.launcher.run(&game) {
                            Ok(outcome) => outcome,
                            Err(e) => {
                                error!(rom = %game.rom, error = %error_chain(&e), "the game did not start");
                                Outcome {
                                    rom: game.rom.clone(),
                                    score: None,
                                    note: Some("The game could not be started".to_owned()),
                                }
                            }
                        };
                        debug!(rom = %game.rom, seconds = started.elapsed().as_secs_f64(), "launch effect done");
                        queue.push_back(Event::GameEnded(outcome));
                    }
                    Effect::Quit => return false,
                }
            }
        }
        screen_alive
    }

    fn send_frame(&self, frame: Frame) -> bool {
        match &frame {
            Frame::Show(vm) => debug!(screen = ?vm.screen, "frame"),
            Frame::Suspended => info!("frame: suspended, the screen must release the display"),
        }
        if self.frames.send(frame).is_err() {
            info!("the screen is gone");
            return false;
        }
        true
    }
}
