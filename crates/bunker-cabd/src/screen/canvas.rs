//! The window, the logical canvas and the safe area. The canvas rule: the short
//! side is 240 logical pixels, the long side follows the shape of the display,
//! and SDL scales by whole pixels. In TATE the scene is drawn on a tall canvas
//! and copied to the landscape output with a 90 degree turn.

use cabd_core::view::{CANVAS_SHORT_SIDE, DisplayAspect, Orientation, ScreenConfig, TateTurn};
use sdl2::VideoSubsystem;
use sdl2::rect::Rect;
use sdl2::render::{TextureCreator, WindowCanvas};
use sdl2::video::{WindowContext, WindowPos};
use tracing::info;

use super::ScreenError;

/// How many times to try to open the window, and the wait between tries. The
/// emulator can hold the display for a moment after it exits.
const OPEN_TRIES: u32 = 10;
const OPEN_RETRY_WAIT: std::time::Duration = std::time::Duration::from_millis(500);

/// The sizes every draw function works with.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Layout {
    /// The canvas the scene is drawn on. Wide in landscape, tall in TATE.
    pub(crate) width: u32,
    pub(crate) height: u32,
    /// The canvas minus the overscan inset on each side.
    pub(crate) safe: Rect,
    /// The canvas is tall and the layouts stack.
    pub(crate) tate: bool,
    /// The turn of the tall canvas onto the landscape output, in degrees, or
    /// nothing in landscape and in the upright development preview.
    pub(crate) rotation: Option<f64>,
    /// The scale from the landscape logical size to the output, per axis.
    /// Equal on both axes for square pixels.
    pub(crate) scale: (f32, f32),
}

impl Layout {
    /// From the output size of the display and the configuration.
    pub(crate) fn new(output: (u32, u32), config: &ScreenConfig) -> Self {
        let (out_w, out_h) = output;
        let tate = config.orientation == Orientation::Tate;
        let upright = tate && config.upright;
        // An upright preview has a portrait output, so its long side is
        // vertical. Every other output is landscape.
        let (shape_w, shape_h) = match config.display_aspect {
            Some(DisplayAspect { width, height }) => (width, height),
            None if upright => (out_h, out_w),
            None => (out_w, out_h),
        };
        let long = long_side(shape_w, shape_h);
        let (width, height) = if tate {
            (CANVAS_SHORT_SIDE, long)
        } else {
            (long, CANVAS_SHORT_SIDE)
        };
        let inset = CANVAS_SHORT_SIDE * u32::from(config.overscan_percent.into_inner()) / 100;
        let safe = Rect::new(
            to_i32(inset),
            to_i32(inset),
            width.saturating_sub(2 * inset).max(1),
            height.saturating_sub(2 * inset).max(1),
        );
        let rotation = match (tate, upright, config.tate_turn) {
            (false, _, _) | (true, true, _) => None,
            (true, false, TateTurn::Left) => Some(90.0),
            (true, false, TateTurn::Right) => Some(-90.0),
        };
        let logical = if rotation.is_some() {
            (height, width)
        } else {
            (width, height)
        };
        let scale = match config.display_aspect {
            // The pixel grid is not the shape, so the two axes scale apart:
            // whole lines vertically, and whatever the width needs.
            Some(_) => {
                let vertical = (out_h / logical.1.max(1)).max(1);
                let horizontal = out_w as f32 / logical.0.max(1) as f32;
                (horizontal, vertical as f32)
            }
            None => {
                let factor = (out_w / logical.0.max(1))
                    .min(out_h / logical.1.max(1))
                    .max(1);
                (factor as f32, factor as f32)
            }
        };
        Self {
            width,
            height,
            safe,
            tate,
            rotation,
            scale,
        }
    }

    /// The size the scene has before scaling. Landscape, because the signal
    /// is, unless the upright preview shows the tall canvas as is.
    pub(crate) fn logical(&self) -> (u32, u32) {
        if self.rotation.is_some() {
            (self.height, self.width)
        } else {
            (self.width, self.height)
        }
    }

    /// Where the tall canvas lands on the landscape output in TATE: centered,
    /// so a 90 degree turn about its own center covers the whole output.
    pub(crate) fn tate_destination(&self) -> Rect {
        let (lw, lh) = self.logical();
        Rect::new(
            to_i32(lw / 2) - to_i32(self.width / 2),
            to_i32(lh / 2) - to_i32(self.height / 2),
            self.width,
            self.height,
        )
    }
}

pub(crate) struct Display {
    pub(crate) canvas: WindowCanvas,
    pub(crate) creator: TextureCreator<WindowContext>,
    pub(crate) layout: Layout,
    pub(crate) vsync: bool,
}

impl Display {
    /// Opens the window, with a few tries, because the emulator can still
    /// hold the display for a moment after it exits.
    pub(crate) fn open(video: &VideoSubsystem, config: &ScreenConfig) -> Result<Self, ScreenError> {
        let mut last = None;
        for attempt in 1..=OPEN_TRIES {
            match Self::open_once(video, config) {
                Ok(display) => return Ok(display),
                Err(e) => {
                    tracing::warn!(attempt, tries = OPEN_TRIES, error = %e, "cannot open the window yet");
                    last = Some(e);
                    std::thread::sleep(OPEN_RETRY_WAIT);
                }
            }
        }
        Err(last.unwrap_or(ScreenError::Sdl("no window".to_owned())))
    }

    fn open_once(video: &VideoSubsystem, config: &ScreenConfig) -> Result<Self, ScreenError> {
        let mut builder = match config.window {
            Some(size) => video.window("LAN BUNKER", size.width, size.height),
            None => video.window("LAN BUNKER", 1, 1),
        };
        if config.window.is_none() {
            builder.fullscreen_desktop();
        }
        let mut window = builder.build()?;
        if config.window.is_some() {
            window.set_position(WindowPos::Centered, WindowPos::Centered);
        }
        let mut canvas = window
            .into_canvas()
            .present_vsync()
            .target_texture()
            .build()?;
        let output = canvas.output_size()?;
        let layout = Layout::new(output, config);
        let logical = layout.logical();
        canvas.set_scale(layout.scale.0, layout.scale.1)?;
        let creator = canvas.texture_creator();
        let info = canvas.info();
        let vsync =
            info.flags & sdl2::sys::SDL_RendererFlags::SDL_RENDERER_PRESENTVSYNC as u32 != 0;
        info!(
            output_w = output.0,
            output_h = output.1,
            logical_w = logical.0,
            logical_h = logical.1,
            canvas_w = layout.width,
            canvas_h = layout.height,
            scale_x = layout.scale.0,
            scale_y = layout.scale.1,
            tate = layout.tate,
            rotation = ?layout.rotation,
            safe = ?layout.safe,
            renderer = ?info.name,
            vsync,
            "window created"
        );
        Ok(Self {
            canvas,
            creator,
            layout,
            vsync,
        })
    }
}

pub(crate) fn to_i32(v: u32) -> i32 {
    i32::try_from(v).unwrap_or(i32::MAX)
}

/// The long side of the canvas: 240 scaled by the shape, rounded to the
/// nearest even pixel, never shorter than the short side. Even, so a turn
/// about the center lands on whole pixels.
fn long_side(shape_w: u32, shape_h: u32) -> u32 {
    let shape_h = u64::from(shape_h.max(1));
    let scaled = (u64::from(CANVAS_SHORT_SIDE) * u64::from(shape_w) + shape_h / 2) / shape_h;
    let even = scaled + scaled % 2;
    u32::try_from(even)
        .unwrap_or(u32::MAX)
        .max(CANVAS_SHORT_SIDE)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use cabd_core::view::OverscanPercent;

    use super::*;

    fn config(orientation: Orientation, overscan: u8) -> ScreenConfig {
        ScreenConfig {
            orientation,
            tate_turn: TateTurn::Left,
            display_aspect: None,
            overscan_percent: OverscanPercent::try_new(overscan).unwrap(),
            window: None,
            upright: false,
        }
    }

    #[test]
    fn the_long_side_follows_the_shape_and_rounds_to_even() {
        assert_eq!(long_side(4, 3), 320);
        assert_eq!(long_side(640, 480), 320);
        assert_eq!(long_side(1920, 1080), 428, "427 rounds up to even");
        assert_eq!(
            long_side(100, 1000),
            240,
            "never shorter than the short side"
        );
        assert_eq!(
            long_side(320, 0),
            320 * 240,
            "a zero height does not divide by zero"
        );
    }

    #[test]
    fn square_pixels_scale_by_one_whole_factor() {
        let layout = Layout::new((960, 720), &config(Orientation::Landscape, 0));
        assert_eq!((layout.width, layout.height), (320, 240));
        assert_eq!(layout.logical(), (320, 240));
        assert_eq!(layout.scale, (3.0, 3.0));
        assert_eq!(layout.safe, Rect::new(0, 0, 320, 240));

        let layout = Layout::new((1920, 1080), &config(Orientation::Landscape, 0));
        assert_eq!((layout.width, layout.height), (428, 240));
        assert_eq!(layout.scale, (4.0, 4.0), "the width limits the factor");

        let layout = Layout::new((853, 480), &config(Orientation::Landscape, 0));
        assert_eq!(layout.scale, (1.0, 1.0), "853 is under two canvases wide");
    }

    #[test]
    fn a_composite_mode_scales_the_two_axes_apart() {
        let mut config = config(Orientation::Landscape, 0);
        config.display_aspect = Some(DisplayAspect {
            width: 4,
            height: 3,
        });
        let layout = Layout::new((720, 240), &config);
        assert_eq!((layout.width, layout.height), (320, 240));
        assert_eq!(layout.scale, (2.25, 1.0));

        let layout = Layout::new((720, 480), &config);
        assert_eq!((layout.width, layout.height), (320, 240));
        assert_eq!(layout.scale, (2.25, 2.0));

        let layout = Layout::new((720, 576), &config);
        assert_eq!(
            layout.scale,
            (2.25, 2.0),
            "PAL keeps whole lines and leaves a bar"
        );

        config.orientation = Orientation::Tate;
        let layout = Layout::new((720, 240), &config);
        assert_eq!((layout.width, layout.height), (240, 320));
        assert_eq!(layout.logical(), (320, 240));
        assert_eq!(layout.scale, (2.25, 1.0));
    }

    #[test]
    fn tate_turns_the_tall_canvas_the_way_the_tube_was_turned() {
        let mut config = config(Orientation::Tate, 0);
        let left = Layout::new((960, 720), &config);
        assert_eq!((left.width, left.height), (240, 320));
        assert_eq!(left.rotation, Some(90.0));
        assert_eq!(left.logical(), (320, 240));
        assert_eq!(left.tate_destination(), Rect::new(40, -40, 240, 320));

        config.tate_turn = TateTurn::Right;
        let right = Layout::new((960, 720), &config);
        assert_eq!(right.rotation, Some(-90.0));
    }

    #[test]
    fn the_upright_preview_keeps_the_tall_canvas_unturned_in_a_portrait_window() {
        let mut config = config(Orientation::Tate, 0);
        config.upright = true;
        let layout = Layout::new((540, 720), &config);
        assert_eq!((layout.width, layout.height), (240, 320));
        assert!(layout.tate, "the layouts still stack");
        assert_eq!(layout.rotation, None);
        assert_eq!(layout.logical(), (240, 320));
        assert_eq!(layout.scale, (2.0, 2.0));

        config.orientation = Orientation::Landscape;
        let layout = Layout::new((960, 720), &config);
        assert_eq!(
            (layout.width, layout.height),
            (320, 240),
            "upright means nothing in landscape"
        );
        assert_eq!(layout.rotation, None);
    }

    #[test]
    fn the_overscan_inset_shrinks_the_safe_area_on_every_side() {
        let layout = Layout::new((320, 240), &config(Orientation::Landscape, 10));
        assert_eq!(layout.safe, Rect::new(24, 24, 272, 192));
        let layout = Layout::new((320, 240), &config(Orientation::Landscape, 25));
        assert_eq!(layout.safe, Rect::new(60, 60, 200, 120));
    }
}
