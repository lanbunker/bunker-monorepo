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
use std::time::{Duration, Instant};

use cabd_core::view::{Frame, ScreenConfig, ViewModel};
use cabd_core::{ConductorGone, Handle};
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
    #[error("a texture was not in the cache after it was put there")]
    CacheMiss,
    #[error("cannot write the PNG file")]
    Png(#[from] png::EncodingError),
    #[error("file error at {path}")]
    Io {
        path: std::path::PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("the conductor is gone")]
    ConductorGone(#[source] ConductorGone),
}

impl From<String> for ScreenError {
    fn from(message: String) -> Self {
        Self::Sdl(message)
    }
}

impl From<ConductorGone> for ScreenError {
    fn from(gone: ConductorGone) -> Self {
        Self::ConductorGone(gone)
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

/// Runs on the main thread until the conductor quits or the window closes.
pub(crate) fn run(config: ScreenConfig, handle: Handle) -> Result<(), ScreenError> {
    let sdl = sdl2::init()?;
    let video = sdl.video()?;
    let joysticks = sdl.joystick()?;
    let ttf = sdl2::ttf::init()?;
    let fonts = Fonts::load(&ttf)?;
    let mut pump = sdl.event_pump()?;
    let mut input = InputMap::new(joysticks);
    let mut stats = FrameStats::new();
    let started = Instant::now();
    info!(
        sdl_version = %sdl2::version::version(),
        sdl_revision = %sdl2::version::revision(),
        video_driver = %video.current_video_driver(),
        orientation = %config.orientation,
        overscan_percent = %config.overscan_percent,
        window = ?config.window,
        "screen started"
    );

    loop {
        let Some(first) = wait_for_show(&handle, &mut pump)? else {
            info!("the conductor is gone, screen stops");
            return Ok(());
        };
        let mut display = Display::open(&video, &config)?;
        input.open_joystick();
        // Input from the time without a window belongs to the game, not to
        // the screen that comes back.
        let dropped = pump.poll_iter().count();
        if dropped > 0 {
            info!(dropped, "input events from the suspended time discarded");
        }

        let mut window = Window {
            display: &mut display,
            handle: &handle,
            input: &mut input,
            pump: &mut pump,
            stats: &mut stats,
            started,
        };
        let exit = window.run(first, &fonts)?;
        drop(display);
        input.close_joystick();
        match exit {
            WindowExit::Suspended => {
                info!("window destroyed, display released");
                handle.display_released()?;
            }
            WindowExit::Stop => {
                info!("the conductor is gone, screen stops");
                return Ok(());
            }
        }
    }
}

/// Blocks until the conductor sends a frame to show. Nothing while the display
/// is released. Returns nothing when the conductor is gone.
fn wait_for_show(handle: &Handle, pump: &mut EventPump) -> Result<Option<ViewModel>, ScreenError> {
    loop {
        match handle.frames.recv_timeout(SUSPENDED_POLL) {
            Ok(Frame::Show(vm)) => return Ok(Some(vm)),
            Ok(Frame::Suspended) => handle.display_released()?,
            Err(RecvTimeoutError::Timeout) => pump.pump_events(),
            Err(RecvTimeoutError::Disconnected) => return Ok(None),
        }
    }
}

/// Everything the frame loop of one window needs.
struct Window<'a> {
    display: &'a mut Display,
    handle: &'a Handle,
    input: &'a mut InputMap,
    pump: &'a mut EventPump,
    stats: &'a mut FrameStats,
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
        } = &mut *self.display;
        let mut cache = Cache::new(creator, fonts);
        let mut tate_target = if layout.rotate {
            Some(creator.create_texture_target(None, layout.width, layout.height)?)
        } else {
            None
        };
        let mut current = first;

        loop {
            loop {
                match self.handle.frames.try_recv() {
                    Ok(Frame::Show(vm)) => current = vm,
                    Ok(Frame::Suspended) => return Ok(WindowExit::Suspended),
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => return Ok(WindowExit::Stop),
                }
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

            let blink_on =
                (self.started.elapsed().as_millis() / BLINK.as_millis()).is_multiple_of(2);
            match tate_target.as_mut() {
                Some(target) => {
                    let mut inner: Result<(), ScreenError> = Ok(());
                    canvas.with_texture_canvas(target, |c| {
                        inner = draw_view(c, &mut cache, layout, &current, blink_on);
                    })?;
                    inner?;
                    canvas.copy_ex(
                        target,
                        None,
                        layout.tate_destination(),
                        90.0,
                        None,
                        false,
                        false,
                    )?;
                }
                None => draw_view(canvas, &mut cache, layout, &current, blink_on)?,
            }
            canvas.present();
            self.stats.frame();
        }
    }
}

/// Average and worst frame time, logged every few seconds. This is the
/// performance measure on the Pi.
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
