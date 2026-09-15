//! Text and QR textures, made one time and kept for the life of the window.
//! Press Start 2P is 8 by 8 pixels per glyph at size 8, so integer sizes stay
//! crisp at scale 1 on a 240-line display. The Pi 3 cannot rasterize every
//! string on every frame, so the `Cache` keeps each texture by its key.

use std::collections::HashMap;

use sdl2::pixels::{Color, PixelFormatEnum};
use sdl2::rect::Rect;
use sdl2::render::{Canvas, RenderTarget, Texture, TextureCreator};
use sdl2::rwops::RWops;
use sdl2::surface::Surface;
use sdl2::ttf::{Font, Sdl2TtfContext};

use super::ScreenError;
use super::canvas::to_i32;

const FONT_BYTES: &[u8] = include_bytes!("../../assets/PressStart2P-Regular.ttf");
const SMALL_PT: u16 = 8;
const BIG_PT: u16 = 16;
/// Above this many distinct strings the cache starts over. A screen shows a
/// few dozen, so this only guards against a runaway.
const TEXT_CACHE_MAX: usize = 512;
/// Light modules around the QR code, in modules. The standard asks for four.
const QR_QUIET: u32 = 2;
const QR_LIGHT: Color = Color::RGB(0xff, 0xff, 0xff);
const QR_DARK: Color = Color::RGB(0x05, 0x05, 0x05);

pub(crate) struct Fonts<'ttf, 'r> {
    small: Font<'ttf, 'r>,
    big: Font<'ttf, 'r>,
}

impl<'ttf> Fonts<'ttf, 'static> {
    pub(crate) fn load(ttf: &'ttf Sdl2TtfContext) -> Result<Self, ScreenError> {
        let small = ttf.load_font_from_rwops(RWops::from_bytes(FONT_BYTES)?, SMALL_PT)?;
        let big = ttf.load_font_from_rwops(RWops::from_bytes(FONT_BYTES)?, BIG_PT)?;
        Ok(Self { small, big })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Size {
    Small,
    Big,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Align {
    Left,
    Center,
    Right,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct TextKey {
    size: Size,
    color: (u8, u8, u8),
    text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct QrKey {
    payload: String,
    module_px: u32,
}

/// The textures of one window. Bound to the texture creator of that window,
/// so it is built after the window and dropped with it.
pub(crate) struct Cache<'d, C> {
    creator: &'d TextureCreator<C>,
    fonts: &'d Fonts<'d, 'd>,
    text: HashMap<TextKey, Texture<'d>>,
    qr: HashMap<QrKey, Texture<'d>>,
}

impl<'d, C> Cache<'d, C> {
    pub(crate) fn new(creator: &'d TextureCreator<C>, fonts: &'d Fonts<'d, 'd>) -> Self {
        Self {
            creator,
            fonts,
            text: HashMap::new(),
            qr: HashMap::new(),
        }
    }

    fn text(&mut self, size: Size, text: &str, color: Color) -> Result<&Texture<'d>, ScreenError> {
        let key = TextKey {
            size,
            color: (color.r, color.g, color.b),
            text: text.to_owned(),
        };
        if self.text.len() >= TEXT_CACHE_MAX {
            self.text.clear();
        }
        if !self.text.contains_key(&key) {
            let font = match size {
                Size::Small => &self.fonts.small,
                Size::Big => &self.fonts.big,
            };
            // The font is a pixel face at a multiple of its 8 pixel grid, so
            // the blended render lands on whole pixels with no gray fringe.
            // The paletted `solid` render loses its transparent background in
            // the texture conversion.
            let surface = font.render(text).blended(color)?;
            let texture = self.creator.create_texture_from_surface(&surface)?;
            self.text.insert(key.clone(), texture);
        }
        self.text.get(&key).ok_or(ScreenError::CacheMiss)
    }

    /// The QR code of `payload` as a square texture with `module_px` pixels
    /// per module and the quiet zone included, or nothing when the payload
    /// cannot be encoded.
    fn qr(&mut self, payload: &str, module_px: u32) -> Result<Option<&Texture<'d>>, ScreenError> {
        let key = QrKey {
            payload: payload.to_owned(),
            module_px,
        };
        if !self.qr.contains_key(&key) {
            let Some(surface) = qr_surface(payload, module_px)? else {
                return Ok(None);
            };
            let texture = self.creator.create_texture_from_surface(&surface)?;
            self.qr.insert(key.clone(), texture);
        }
        Ok(self.qr.get(&key))
    }
}

/// How many modules across a QR of this payload has, quiet zone included, so a
/// layout can pick whole pixels per module. Nothing if it cannot be encoded.
pub(crate) fn qr_modules(payload: &str) -> Option<u32> {
    let code = qrcode::QrCode::new(payload.as_bytes()).ok()?;
    let width = u32::try_from(code.width()).ok()?;
    Some(width + 2 * QR_QUIET)
}

fn qr_surface(payload: &str, module_px: u32) -> Result<Option<Surface<'static>>, ScreenError> {
    let Ok(code) = qrcode::QrCode::new(payload.as_bytes()) else {
        return Ok(None);
    };
    let modules = u32::try_from(code.width()).unwrap_or(1).max(1);
    let side = (modules + 2 * QR_QUIET) * module_px.max(1);
    let mut surface = Surface::new(side, side, PixelFormatEnum::RGB24)?;
    surface.fill_rect(None, QR_LIGHT)?;
    let origin = to_i32(QR_QUIET * module_px);
    for (i, color) in code.to_colors().iter().enumerate() {
        if *color != qrcode::Color::Dark {
            continue;
        }
        let i = u32::try_from(i).unwrap_or(0);
        let (col, row) = (i % modules, i / modules);
        surface.fill_rect(
            Rect::new(
                origin + to_i32(col * module_px),
                origin + to_i32(row * module_px),
                module_px,
                module_px,
            ),
            QR_DARK,
        )?;
    }
    Ok(Some(surface))
}

/// A canvas and its cache, so a draw function passes one value around.
pub(crate) struct Painter<'a, 'd, T: RenderTarget> {
    pub(crate) canvas: &'a mut Canvas<T>,
    cache: &'a mut Cache<'d, T::Context>,
}

impl<'a, 'd, T: RenderTarget> Painter<'a, 'd, T> {
    pub(crate) fn new(canvas: &'a mut Canvas<T>, cache: &'a mut Cache<'d, T::Context>) -> Self {
        Self { canvas, cache }
    }

    pub(crate) fn fill(&mut self, rect: Rect, color: Color) -> Result<(), ScreenError> {
        self.canvas.set_draw_color(color);
        self.canvas.fill_rect(rect)?;
        Ok(())
    }

    pub(crate) fn outline(&mut self, rect: Rect, color: Color) -> Result<(), ScreenError> {
        self.canvas.set_draw_color(color);
        self.canvas.draw_rect(rect)?;
        Ok(())
    }

    /// Draws one line. `x` is the left edge, the center or the right edge, by
    /// `align`. Returns the size drawn.
    pub(crate) fn text(
        &mut self,
        size: Size,
        text: &str,
        (x, y): (i32, i32),
        color: Color,
        align: Align,
    ) -> Result<(u32, u32), ScreenError> {
        if text.is_empty() {
            return Ok((0, 0));
        }
        let texture = self.cache.text(size, text, color)?;
        let query = texture.query();
        let (w, h) = (query.width, query.height);
        let left = match align {
            Align::Left => x,
            Align::Center => x - to_i32(w / 2),
            Align::Right => x - to_i32(w),
        };
        self.canvas.copy(texture, None, Rect::new(left, y, w, h))?;
        Ok((w, h))
    }

    /// The QR code centered in `area`, as large as whole pixels per module
    /// allow. Returns false when the payload cannot be encoded.
    pub(crate) fn qr(&mut self, area: Rect, payload: &str) -> Result<bool, ScreenError> {
        let Some(modules) = qr_modules(payload) else {
            return Ok(false);
        };
        let module_px = (area.width().min(area.height()) / modules).max(1);
        let Some(texture) = self.cache.qr(payload, module_px)? else {
            return Ok(false);
        };
        let side = texture.query().width;
        self.canvas
            .copy(texture, None, Rect::from_center(area.center(), side, side))?;
        Ok(true)
    }
}
