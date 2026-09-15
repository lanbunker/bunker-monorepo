//! Renders every sample screen into a PNG file with the software renderer. No
//! window and no display are needed, so this runs on a build machine and on the
//! Pi over ssh.

use std::fs::File;
use std::io::BufWriter;
use std::path::Path;

use cabd_core::view::ScreenConfig;
use sdl2::pixels::PixelFormatEnum;
use sdl2::surface::Surface;
use tracing::info;

use super::ScreenError;
use super::canvas::Layout;
use super::draw::draw_view;
use super::samples::samples;
use super::text::{Cache, Fonts};

/// A 4:3 output, the shape of the CRT.
const OUTPUT: (u32, u32) = (320, 240);

pub(crate) fn screenshots(out: &Path, config: ScreenConfig) -> Result<(), ScreenError> {
    std::fs::create_dir_all(out).map_err(|source| ScreenError::Io {
        path: out.to_owned(),
        source,
    })?;
    let ttf = sdl2::ttf::init()?;
    let fonts = Fonts::load(&ttf)?;
    let layout = Layout::new(OUTPUT, &config);
    info!(canvas_w = layout.width, canvas_h = layout.height, safe = ?layout.safe, "rendering samples");

    for (name, vm) in samples() {
        let surface = Surface::new(layout.width, layout.height, PixelFormatEnum::RGB24)?;
        let mut canvas = surface.into_canvas()?;
        let creator = canvas.texture_creator();
        let mut cache = Cache::new(&creator, &fonts);
        draw_view(&mut canvas, &mut cache, &layout, &vm, true)?;
        drop(cache);

        let path = out.join(format!("{name}_{}.png", config.orientation));
        let surface = canvas.into_surface();
        let pitch = usize::try_from(surface.pitch()).unwrap_or(0);
        let row_bytes = usize::try_from(surface.width()).unwrap_or(0) * 3;
        let mut packed =
            Vec::with_capacity(row_bytes * usize::try_from(surface.height()).unwrap_or(0));
        surface.with_lock(|pixels| {
            for row in pixels.chunks(pitch.max(1)) {
                packed.extend_from_slice(row.get(..row_bytes).unwrap_or(row));
            }
        });
        write_png(&path, surface.width(), surface.height(), &packed)?;
        info!(path = %path.display(), "screenshot");
    }
    Ok(())
}

fn write_png(path: &Path, width: u32, height: u32, rgb: &[u8]) -> Result<(), ScreenError> {
    let file = File::create(path).map_err(|source| ScreenError::Io {
        path: path.to_owned(),
        source,
    })?;
    let mut encoder = png::Encoder::new(BufWriter::new(file), width, height);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header()?;
    writer.write_image_data(rgb)?;
    Ok(())
}
