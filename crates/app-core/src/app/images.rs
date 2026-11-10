//! The images of cells that plugins ask for: laid out as a window lays out
//! a pane, in the theme's colors and the terminal's font, drawn offscreen
//! by the windows' renderer, and written as a PNG.

use lokyr_kernel::config::Padding;
use lokyr_kernel::plugins::image::{MOST_PIXELS, MOST_SIDE, Png};
use lokyr_kernel::plugins::{self, Image, ImageSpec};
use lokyr_render_core::Renderer;
use lokyr_ui_core::image::{compose, terminal};
use lokyr_ui_core::pane::TextLook;
use lokyr_vt_core::terminal::{Colors, Snapshot};
use lokyr_vt_core::unicode::Widths;

use crate::drawing::{Backend, Drawing};

/// What renders the plugins' images during their calls: the app's drawing,
/// if it draws, and what of the config and the theme its windows take.
pub struct ImagePainter<'a, B: Backend> {
    /// The app's drawing, if it draws.
    pub drawing: Option<&'a mut Drawing<B>>,
    /// The terminal's colors of the appearance in force.
    pub colors: &'a Colors,
    /// How the terminal counts the cells of text.
    pub widths: Widths,
    /// How the text looks.
    pub look: TextLook,
    /// The room between a window's cells and its edges, across and down.
    pub padding: (Padding, Padding),
    /// The scale of an image that asks for none: the focused window's
    /// display's.
    pub scale: f32,
}

impl<B: Backend> plugins::Images for ImagePainter<'_, B> {
    fn render(&mut self, image: &ImageSpec) -> Result<Image, String> {
        let drawing = self
            .drawing
            .as_deref_mut()
            .ok_or("the app draws no image, as it has no GPU")?;
        let scale = image.scale.unwrap_or(self.scale);
        let mut snapshot = Snapshot::default();
        terminal(&image.rows, self.widths, self.colors.clone()).snapshot(&mut snapshot);
        // Its cells alone past the most, refused before they are laid out.
        let before = drawing.font.scale();
        drawing.font.set_scale(scale);
        let cell = *drawing.font.cell();
        drawing.font.set_scale(before);
        within(
            u64::from(snapshot.cols()) * u64::from(cell.width),
            u64::from(snapshot.rows()) * u64::from(cell.height),
        )?;
        // The glyphs of the frames that the GPU still draws stay in the
        // atlas, as a window's frame keeps them.
        let oldest = drawing.renderer.drawing_from();
        drawing.font.set_drawing(oldest);
        drawing.font.begin_frame();
        drawing.font.set_text(self.look.config);
        let padding = (
            self.padding.0.pixels(f64::from(scale)),
            self.padding.1.pixels(f64::from(scale)),
        );
        let composed = compose(
            image,
            &snapshot,
            self.look,
            padding,
            scale,
            &mut drawing.font,
        );
        let (width, height) = (composed.width, composed.height);
        within(u64::from(width), u64::from(height))?;
        let updates = drawing.font.texture_updates();
        let renderer = &mut drawing.renderer;
        renderer
            .update_textures(&updates)
            .map_err(|error| error.to_string())?;
        let pixels = renderer
            .read_back(&composed.list, width, height)
            .map_err(|error| error.to_string())?
            .ok_or("the renderer reads no image back")?;
        Ok(Image {
            png: Png(png(pixels, (width, height), scale)?),
            width,
            height,
            scale,
        })
    }
}

/// Whether an image of `width` by `height` pixels is within the most that
/// the app renders, of a GPU's texture and of its memory; the error that
/// tells it is not.
fn within(width: u64, height: u64) -> Result<(), String> {
    if width > u64::from(MOST_SIDE) || height > u64::from(MOST_SIDE) || width * height > MOST_PIXELS
    {
        return Err(format!(
            "an image of {width} by {height} pixels is past the most, {MOST_SIDE} a side and \
             {MOST_PIXELS} pixels"
        ));
    }
    Ok(())
}

/// The PNG of `pixels`, RGBA of their opacity premultiplied, `size` wide
/// and high, in sRGB, of the resolution of `scale` pixels a point, 72
/// points an inch, as a picture's viewer shows it at its points' size.
fn png(mut pixels: Vec<u8>, (width, height): (u32, u32), scale: f32) -> Result<Vec<u8>, String> {
    for pixel in pixels.as_chunks_mut::<4>().0 {
        let alpha = pixel[3];
        if alpha != 0 && alpha != u8::MAX {
            for component in &mut pixel[..3] {
                let straight = u32::from(*component) * 255 + u32::from(alpha) / 2;
                *component = (straight / u32::from(alpha)).min(255) as u8;
            }
        }
    }
    let mut bytes = Vec::new();
    let mut encoder = png::Encoder::new(&mut bytes, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_compression(png::Compression::Fast);
    encoder.set_source_srgb(png::SrgbRenderingIntent::Perceptual);
    // Pixels a meter, of 72 points an inch.
    let per_meter = (f64::from(scale) * 72.0 / 0.0254).round() as u32;
    encoder.set_pixel_dims(Some(png::PixelDimensions {
        xppu: per_meter,
        yppu: per_meter,
        unit: png::Unit::Meter,
    }));
    let mut writer = encoder.write_header().map_err(|error| error.to_string())?;
    writer
        .write_image_data(&pixels)
        .map_err(|error| error.to_string())?;
    writer.finish().map_err(|error| error.to_string())?;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The pixels, the size and the resolution of `bytes`, a PNG.
    fn read(bytes: &[u8]) -> (Vec<u8>, (u32, u32), Option<png::PixelDimensions>) {
        let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes))
            .read_info()
            .unwrap();
        let dims = decoder.info().pixel_dims;
        let mut pixels = vec![0; decoder.output_buffer_size().unwrap()];
        let info = decoder.next_frame(&mut pixels).unwrap();
        pixels.truncate(info.buffer_size());
        (pixels, (info.width, info.height), dims)
    }

    #[test]
    fn a_png_holds_its_pixels_of_their_opacity_unmultiplied_and_its_resolution() {
        let pixels = vec![
            255, 0, 0, 255, // opaque red
            0, 0, 0, 0, // clear
            64, 32, 0, 128, // a half of orange
            0, 0, 0, 64, // a quarter of black, as a shadow's
        ];
        let bytes = png(pixels, (2, 2), 2.0).unwrap();
        let (read, size, dims) = read(&bytes);
        assert_eq!(size, (2, 2));
        assert_eq!(
            read,
            [255, 0, 0, 255, 0, 0, 0, 0, 128, 64, 0, 128, 0, 0, 0, 64]
        );
        let dims = dims.unwrap();
        // 144 pixels an inch.
        assert_eq!((dims.xppu, dims.yppu), (5669, 5669));
        assert_eq!(dims.unit, png::Unit::Meter);
    }

    #[test]
    fn an_image_is_within_its_sides_and_its_pixels() {
        assert_eq!(within(16_384, 2441), Ok(()));
        assert_eq!(within(1, 16_384), Ok(()));
        assert!(within(16_385, 1).unwrap_err().contains("16385 by 1"));
        assert!(within(1, 16_385).is_err());
        assert!(within(10_000, 4001).is_err());
    }

    #[test]
    fn the_fake_host_s_blank_image_is_a_png_of_a_clear_pixel() {
        let blank = Image::blank(2.0);
        let (pixels, size, _) = read(&blank.png.0);
        assert_eq!(size, (blank.width, blank.height));
        assert_eq!(pixels, [0, 0, 0, 0]);
    }
}
