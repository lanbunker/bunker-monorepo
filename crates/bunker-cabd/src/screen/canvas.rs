//! The window, the logical canvas and the safe area. The canvas rule: the short
//! side is 240 logical pixels, the long side follows the aspect of the display,
//! and SDL scales by an integer factor. In TATE the scene is drawn on a tall
//! canvas and copied to the landscape output with a 90 degree turn.

use cabd_core::view::{CANVAS_SHORT_SIDE, Orientation, ScreenConfig};
use sdl2::VideoSubsystem;
use sdl2::rect::Rect;
use sdl2::render::{TextureCreator, WindowCanvas};
use sdl2::video::{WindowContext, WindowPos};
use tracing::info;

use super::ScreenError;

/// The sizes every draw function works with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Layout {
    /// The canvas the scene is drawn on. Wide in landscape, tall in TATE.
    pub(crate) width: u32,
    pub(crate) height: u32,
    /// The canvas minus the overscan inset on each side.
    pub(crate) safe: Rect,
    /// The canvas is tall and the layouts stack.
    pub(crate) tate: bool,
    /// The tall canvas is turned by 90 degrees onto a landscape output. Off
    /// in the upright development preview.
    pub(crate) rotate: bool,
}

impl Layout {
    /// From the output size of the display and the configuration.
    pub(crate) fn new(output: (u32, u32), config: &ScreenConfig) -> Self {
        let (out_w, out_h) = output;
        let tate = config.orientation == Orientation::Tate;
        let upright = tate && config.upright;
        // An upright preview has a portrait output, so its long side is
        // vertical. Every other output is landscape.
        let long = if upright {
            long_side(out_h, out_w)
        } else {
            long_side(out_w, out_h)
        };
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
        Self {
            width,
            height,
            safe,
            tate,
            rotate: tate && !upright,
        }
    }

    /// The logical size SDL scales to the output. Landscape, because the
    /// signal is, unless the upright preview shows the tall canvas as is.
    pub(crate) fn logical(&self) -> (u32, u32) {
        if self.rotate {
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
}

impl Display {
    pub(crate) fn open(video: &VideoSubsystem, config: &ScreenConfig) -> Result<Self, ScreenError> {
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
        canvas.set_logical_size(logical.0, logical.1)?;
        canvas.set_integer_scale(true)?;
        let creator = canvas.texture_creator();
        info!(
            output_w = output.0,
            output_h = output.1,
            logical_w = logical.0,
            logical_h = logical.1,
            canvas_w = layout.width,
            canvas_h = layout.height,
            scale = output.1 / logical.1.max(1),
            tate = layout.tate,
            rotate = layout.rotate,
            safe = ?layout.safe,
            renderer = ?canvas.info().name,
            "window created"
        );
        Ok(Self {
            canvas,
            creator,
            layout,
        })
    }
}

pub(crate) fn to_i32(v: u32) -> i32 {
    i32::try_from(v).unwrap_or(i32::MAX)
}

/// The long side of the canvas: 240 scaled by the aspect of the output, rounded
/// to the nearest pixel, never shorter than the short side. Integer arithmetic,
/// so no cast can lose a value.
fn long_side(out_w: u32, out_h: u32) -> u32 {
    let out_h = u64::from(out_h.max(1));
    let scaled = (u64::from(CANVAS_SHORT_SIDE) * u64::from(out_w) + out_h / 2) / out_h;
    u32::try_from(scaled)
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
            overscan_percent: OverscanPercent::try_new(overscan).unwrap(),
            window: None,
            upright: false,
        }
    }

    #[test]
    fn the_long_side_follows_the_aspect_and_rounds() {
        assert_eq!(long_side(640, 480), 320);
        assert_eq!(long_side(720, 240), 720);
        assert_eq!(long_side(1920, 1080), 427);
        assert_eq!(long_side(1280, 720), 427);
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
    fn landscape_and_tate_swap_the_canvas_but_not_the_logical_size() {
        let landscape = Layout::new((960, 720), &config(Orientation::Landscape, 0));
        assert_eq!((landscape.width, landscape.height), (320, 240));
        assert_eq!(landscape.logical(), (320, 240));
        assert_eq!(landscape.safe, Rect::new(0, 0, 320, 240));

        let tate = Layout::new((960, 720), &config(Orientation::Tate, 0));
        assert_eq!((tate.width, tate.height), (240, 320));
        assert!(tate.rotate);
        assert_eq!(tate.logical(), (320, 240));
        assert_eq!(tate.tate_destination(), Rect::new(40, -40, 240, 320));
    }

    #[test]
    fn the_upright_preview_keeps_the_tall_canvas_unturned_in_a_portrait_window() {
        let mut config = config(Orientation::Tate, 0);
        config.upright = true;
        let layout = Layout::new((540, 720), &config);
        assert_eq!((layout.width, layout.height), (240, 320));
        assert!(layout.tate, "the layouts still stack");
        assert!(!layout.rotate);
        assert_eq!(layout.logical(), (240, 320));

        config.orientation = Orientation::Landscape;
        let layout = Layout::new((960, 720), &config);
        assert_eq!(
            (layout.width, layout.height),
            (320, 240),
            "upright means nothing in landscape"
        );
        assert!(!layout.rotate);
    }

    #[test]
    fn the_overscan_inset_shrinks_the_safe_area_on_every_side() {
        let layout = Layout::new((320, 240), &config(Orientation::Landscape, 10));
        assert_eq!(layout.safe, Rect::new(24, 24, 272, 192));
        let layout = Layout::new((320, 240), &config(Orientation::Landscape, 25));
        assert_eq!(layout.safe, Rect::new(60, 60, 200, 120));
    }
}
