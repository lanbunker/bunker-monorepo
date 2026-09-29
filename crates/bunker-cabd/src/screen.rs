//! The SDL2 screen. It imports `cabd_core::view`, calls `cabd_core::start` and
//! talks through the `Handle`. Nothing else from the library. It draws frames
//! and maps input. It decides nothing.

mod canvas;
mod draw;
mod input;
mod samples;
mod shots;
mod text;

use std::sync::mpsc::{RecvTimeoutError, TryRecvError};
use std::thread;
use std::time::{Duration, Instant};

use cabd_core::Handle;
use cabd_core::view::{Frame, ScreenConfig, ViewModel};
use sdl2::EventPump;
use sdl2::IntegerOrSdlError;
use sdl2::render::{TargetRenderError, TextureValueError};
use sdl2::ttf::FontError;
use sdl2::video::WindowBuildError;
use tracing::{info, warn};

pub(crate) use shots::screenshots;

use self::canvas::Display;
use self::draw::draw_view;
use self::input::{InputMap, Mapped};
use self::text::{Cache, Fonts};

/// How long the screen waits for a frame between event pumps while the display
/// is released.
const SUSPENDED_POLL: Duration = Duration::from_millis(50);
const STATS_EVERY: Duration = Duration::from_secs(10);
/// The cursor is on for this long, then off for as long.
const BLINK: Duration = Duration::from_millis(500);
/// The frame period when the renderer gives no vsync, so the loop does not
/// spin on one core.
const FRAME_WITHOUT_VSYNC: Duration = Duration::from_millis(16);

#[derive(Debug, thiserror::Error)]
pub(crate) enum ScreenError {
    #[error("SDL: {0}")]
    Sdl(String),
    #[error("cannot create the window")]
    Window(#[from] WindowBuildError),
    #[error("cannot create the renderer")]
    Renderer(#[from] IntegerOrSdlError),
    #[error("cannot create a texture")]
    Texture(#[from] TextureValueError),
    #[error("cannot render into a texture")]
    Target(#[from] TargetRenderError),
    #[error("font error")]
    Font(#[from] FontError),
    #[error("cannot write the PNG file")]
    Png(#[from] png::EncodingError),
    #[error("file error at {path}")]
    Io {
        path: std::path::PathBuf,
        #[source]
        source: std::io::Error,
    },
}

impl From<String> for ScreenError {
    fn from(message: String) -> Self {
        Self::Sdl(message)
    }
}

/// Runs on the main thread until the conductor quits or the window closes.
/// Returns the handle so the caller can join the conductor thread.
pub(crate) fn run(config: ScreenConfig, handle: Handle) -> Result<Handle, ScreenError> {
    let sdl = sdl2::init()?;
    let ttf = sdl2::ttf::init()?;
    let fonts = Fonts::load(&ttf)?;
    let mut pump = sdl.event_pump()?;
    let started = Instant::now();
    info!(
        sdl_version = %sdl2::version::version(),
        sdl_revision = %sdl2::version::revision(),
        orientation = %config.orientation,
        tate_turn = %config.tate_turn,
        display_aspect = ?config.display_aspect,
        overscan_percent = %config.overscan_percent,
        window = ?config.window,
        upright = config.upright,
        "screen started"
    );

    loop {
        let Some(first) = wait_for_show(&handle, &mut pump)? else {
            info!("the conductor is gone, screen stops");
            return Ok(handle);
        };
        // The video and joystick subsystems live with the window. Closing
        // them gives the display and the devices to the emulator, the way
        // EmulationStation does before a launch.
        let video = sdl.video()?;
        let mut input = InputMap::new(sdl.joystick()?);
        info!(video_driver = %video.current_video_driver(), "video opened");
        let mut display = Display::open(&video, &config)?;
        sdl.mouse().show_cursor(false);
        if !discard_player_input(&mut pump, &mut input, &handle) {
            info!("the conductor is gone, screen stops");
            return Ok(handle);
        }

        let mut window = Window {
            display: &mut display,
            handle: &handle,
            input: &mut input,
            pump: &mut pump,
            stats: FrameStats::new(),
            started,
        };
        let exit = window.run(first, &fonts)?;
        drop(display);
        drop(input);
        drop(video);
        match exit {
            WindowExit::Suspended => {
                info!("window destroyed, display released");
                if handle.display_released().is_err() {
                    info!("the conductor is gone, screen stops");
                    return Ok(handle);
                }
            }
            WindowExit::Stop => {
                info!("the conductor is gone, screen stops");
                return Ok(handle);
            }
        }
    }
}

/// Why a window closed.
enum WindowExit {
    /// The conductor asked for the display. The caller answers once the
    /// window is gone.
    Suspended,
    /// The conductor is gone. The screen stops.
    Stop,
}

/// Blocks until the conductor sends a frame to show. Nothing while the display
/// is released. Returns nothing when the conductor is gone. Frames that queued
/// up are drained to the newest, so an old `Show` never opens a window over a
/// newer `Suspended`.
fn wait_for_show(handle: &Handle, pump: &mut EventPump) -> Result<Option<ViewModel>, ScreenError> {
    loop {
        let first = match handle.frames.recv_timeout(SUSPENDED_POLL) {
            Ok(frame) => frame,
            Err(RecvTimeoutError::Timeout) => {
                pump.pump_events();
                continue;
            }
            Err(RecvTimeoutError::Disconnected) => return Ok(None),
        };
        match newest(first, handle) {
            Some(Frame::Show(vm)) => return Ok(Some(vm)),
            Some(Frame::Suspended) => {
                if handle.display_released().is_err() {
                    return Ok(None);
                }
            }
            None => return Ok(None),
        }
    }
}

/// The newest frame on the channel, starting from one already received.
/// Nothing when the channel is closed.
fn newest(mut frame: Frame, handle: &Handle) -> Option<Frame> {
    loop {
        match handle.frames.try_recv() {
            Ok(next) => frame = next,
            Err(TryRecvError::Empty) => return Some(frame),
            Err(TryRecvError::Disconnected) => return None,
        }
    }
}

/// Input from the time without a window belongs to the game, not to the
/// screen that comes back. A quit or a device event in the same queue still
/// counts, and SDL cannot re-queue an event, so those are handled here.
/// Returns false when the conductor is gone.
fn discard_player_input(pump: &mut EventPump, input: &mut InputMap, handle: &Handle) -> bool {
    let mut dropped = 0;
    for event in pump.poll_iter() {
        if InputMap::is_player_input(&event) {
            dropped += 1;
            continue;
        }
        if let Some(Mapped::Quit) = input.map(event)
            && handle.quit().is_err()
        {
            return false;
        }
    }
    if dropped > 0 {
        info!(dropped, "input events from the suspended time discarded");
    }
    true
}

/// Everything the frame loop of one window needs.
struct Window<'a> {
    display: &'a mut Display,
    handle: &'a Handle,
    input: &'a mut InputMap,
    pump: &'a mut EventPump,
    stats: FrameStats,
    started: Instant,
}

impl Window<'_> {
    /// Draws frames and maps input for the life of the window.
    fn run(
        &mut self,
        first: ViewModel,
        fonts: &Fonts<'_, 'static>,
    ) -> Result<WindowExit, ScreenError> {
        let Display {
            canvas,
            creator,
            layout,
            vsync,
        } = &mut *self.display;
        let mut cache = Cache::new(creator, fonts);
        let mut tate_target = match layout.rotation {
            Some(_) => Some(creator.create_texture_target(None, layout.width, layout.height)?),
            None => None,
        };
        let mut current = first;

        loop {
            match self.handle.frames.try_recv() {
                Ok(frame) => match newest(frame, self.handle) {
                    Some(Frame::Show(vm)) => current = vm,
                    Some(Frame::Suspended) => return Ok(WindowExit::Suspended),
                    None => return Ok(WindowExit::Stop),
                },
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => return Ok(WindowExit::Stop),
            }

            for sdl_event in self.pump.poll_iter() {
                let sent = match self.input.map(sdl_event) {
                    Some(Mapped::Input(i)) => self.handle.input(i),
                    Some(Mapped::Command(c)) => self.handle.command(c),
                    Some(Mapped::Quit) => self.handle.quit(),
                    None => Ok(()),
                };
                if sent.is_err() {
                    return Ok(WindowExit::Stop);
                }
            }

            let frame_started = Instant::now();
            let blink_on =
                (self.started.elapsed().as_millis() / BLINK.as_millis()).is_multiple_of(2);
            match (tate_target.as_mut(), layout.rotation) {
                (Some(target), Some(angle)) => {
                    let mut inner: Result<(), ScreenError> = Ok(());
                    canvas.with_texture_canvas(target, |c| {
                        inner = draw_view(c, &mut cache, layout, &current, blink_on);
                    })?;
                    inner?;
                    canvas.set_draw_color(draw::BG);
                    canvas.clear();
                    canvas.copy_ex(
                        target,
                        None,
                        layout.tate_destination(),
                        angle,
                        None,
                        false,
                        false,
                    )?;
                }
                _ => draw_view(canvas, &mut cache, layout, &current, blink_on)?,
            }
            canvas.present();
            if !*vsync {
                thread::sleep(FRAME_WITHOUT_VSYNC.saturating_sub(frame_started.elapsed()));
            }
            self.stats.frame();
        }
    }
}

/// Average and worst frame time, logged every few seconds. This is the
/// performance measure on the Pi. One per window, so a game does not count
/// as a frame.
struct FrameStats {
    since: Instant,
    last: Instant,
    frames: u32,
    total: Duration,
    worst: Duration,
}

impl FrameStats {
    fn new() -> Self {
        let now = Instant::now();
        Self {
            since: now,
            last: now,
            frames: 0,
            total: Duration::ZERO,
            worst: Duration::ZERO,
        }
    }

    fn frame(&mut self) {
        let now = Instant::now();
        let dt = now - self.last;
        self.last = now;
        self.frames += 1;
        self.total += dt;
        self.worst = self.worst.max(dt);
        if now - self.since >= STATS_EVERY {
            let avg_ms = self.total.as_secs_f64() * 1000.0 / f64::from(self.frames.max(1));
            let worst_ms = self.worst.as_secs_f64() * 1000.0;
            if worst_ms > 50.0 {
                warn!(frames = self.frames, avg_ms, worst_ms, "frame time");
            } else {
                info!(frames = self.frames, avg_ms, worst_ms, "frame time");
            }
            self.since = now;
            self.frames = 0;
            self.total = Duration::ZERO;
            self.worst = Duration::ZERO;
        }
    }
}
