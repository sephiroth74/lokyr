//! Draws lines of text in a family's faces into a BMP, a cell a character,
//! dark on light and light on dark, to set beside the same text in another
//! terminal at the same size: Preview shows it a pixel a pixel at Actual
//! Size, on a display of a scale of 1.
//!
//! ```text
//! cargo run -p lokyr-font-coretext --example glyphs -- "CaskaydiaCove NFM" 22 SemiBold 1 glyphs.bmp
//! ```

use std::fs;
use std::process::ExitCode;

#[cfg(target_os = "macos")]
fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(family) = args.first() else {
        eprintln!("usage: glyphs <family> [points] [style] [scale] [output.bmp]");
        return ExitCode::FAILURE;
    };
    let points: f32 = args
        .get(1)
        .and_then(|points| points.parse().ok())
        .unwrap_or(13.0);
    let style = args.get(2).filter(|style| !style.is_empty());
    let scale: f32 = args
        .get(3)
        .and_then(|scale| scale.parse().ok())
        .unwrap_or(1.0);
    let output = args.get(4).map_or("glyphs.bmp", String::as_str);
    match draw(family, points, style.map(String::as_str), scale) {
        Ok(image) => {
            if let Err(error) = fs::write(output, image.bmp()) {
                eprintln!("glyphs: {output}: {error}");
                return ExitCode::FAILURE;
            }
            println!("{output}: {}×{} pixels", image.width, image.height);
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("glyphs: {error}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(not(target_os = "macos"))]
fn main() -> ExitCode {
    eprintln!("glyphs: CoreText is macOS's");
    ExitCode::FAILURE
}

/// The lines of text, and the style of each.
const LINES: [(&str, Style); 6] = [
    (
        "The quick brown fox jumps over the lazy dog.",
        Style::Regular,
    ),
    (
        "0123456789 !@#$%^&*()[]{}<>?/\\|~`'\";:,._-+=",
        Style::Regular,
    ),
    (
        "fn main() { let x = a != b && c >= d; } // ok",
        Style::Regular,
    ),
    ("fn main() { let x = a != b && c >= d; } // ok", Style::Bold),
    (
        "fn main() { let x = a != b && c >= d; } // ok",
        Style::Italic,
    ),
    (
        "ls -la ~/Documents | grep -i 'notes' > out.txt",
        Style::BoldItalic,
    ),
];

#[derive(Clone, Copy)]
enum Style {
    Regular,
    Bold,
    Italic,
    BoldItalic,
}

/// The colors of the two bands, background and text, in sRGB.
const BANDS: [([u8; 3], [u8; 3]); 2] = [
    ([0xfa, 0xfa, 0xfa], [0x1e, 0x1e, 0x20]),
    ([0x1e, 0x1e, 0x20], [0xe6, 0xe6, 0xe6]),
];

/// The cells of blank around the text.
const MARGIN: u32 = 1;

/// An image of RGB pixels, rows from the top.
struct Image {
    width: u32,
    height: u32,
    pixels: Vec<[u8; 3]>,
}

impl Image {
    /// The image, as a BMP of 24 bits a pixel.
    fn bmp(&self) -> Vec<u8> {
        let row = (self.width * 3).div_ceil(4) * 4;
        let size = 54 + row * self.height;
        let mut out = Vec::with_capacity(size as usize);
        out.extend_from_slice(b"BM");
        out.extend_from_slice(&size.to_le_bytes());
        out.extend_from_slice(&0_u32.to_le_bytes());
        out.extend_from_slice(&54_u32.to_le_bytes());
        out.extend_from_slice(&40_u32.to_le_bytes());
        out.extend_from_slice(&self.width.to_le_bytes());
        out.extend_from_slice(&self.height.to_le_bytes());
        out.extend_from_slice(&1_u16.to_le_bytes());
        out.extend_from_slice(&24_u16.to_le_bytes());
        out.extend_from_slice(&[0; 24]);
        // BMP's rows go from the bottom, each pixel blue first.
        for y in (0..self.height).rev() {
            let start = (y * self.width) as usize;
            for &[red, green, blue] in &self.pixels[start..start + self.width as usize] {
                out.extend_from_slice(&[blue, green, red]);
            }
            out.resize(out.len() + (row - self.width * 3) as usize, 0);
        }
        out
    }
}

#[cfg(target_os = "macos")]
fn draw(family: &str, points: f32, style: Option<&str>, scale: f32) -> Result<Image, String> {
    use lokyr_font_coretext::CoreText;
    use lokyr_text_core::{
        BitmapFormat, CellMetrics, FontBackend, FontConfig, Name, resolve_faces,
    };

    let mut backend = CoreText::new();
    let name = |name: &str| Name::try_from(name.to_owned());
    let config = FontConfig {
        family: Some(name(family)?),
        style: style.map(name).transpose()?,
        ..FontConfig::default()
    };
    let resolved = resolve_faces(&mut backend, &config).map_err(|error| error.to_string())?;
    if let Some(missing) = &resolved.missing_family {
        return Err(format!("the family `{missing}` is not installed"));
    }
    let pixels = points * scale;
    let cell = CellMetrics::new(&backend.metrics(resolved.faces.regular.id, pixels), &config);
    let columns = LINES
        .iter()
        .map(|(text, _)| text.chars().count())
        .max()
        .unwrap_or(0) as u32;
    let rows = LINES.len() as u32;
    let band = (rows + 2 * MARGIN) * cell.height;
    let width = (columns + 2 * MARGIN) * cell.width;
    let height = band * BANDS.len() as u32;
    let mut image = Image {
        width,
        height,
        pixels: vec![[0; 3]; (width * height) as usize],
    };

    for (index, &(background, foreground)) in BANDS.iter().enumerate() {
        let top = index as u32 * band;
        for y in top..top + band {
            for x in 0..width {
                image.pixels[(y * width + x) as usize] = background;
            }
        }
        for (row, &(text, style)) in LINES.iter().enumerate() {
            let face = match style {
                Style::Regular => resolved.faces.regular,
                Style::Bold => resolved.faces.bold,
                Style::Italic => resolved.faces.italic,
                Style::BoldItalic => resolved.faces.bold_italic,
            };
            let baseline = top + (row as u32 + MARGIN) * cell.height + cell.baseline;
            for (column, character) in text.chars().enumerate() {
                let Some(glyph) = backend.glyph(face.id, character) else {
                    continue;
                };
                let bitmap = backend.rasterize(face.id, glyph, pixels, face.synthesis);
                if bitmap.format != BitmapFormat::Gray {
                    continue;
                }
                let pen = (column as u32 + MARGIN) * cell.width;
                for (y, coverages) in bitmap.data.chunks(bitmap.width.max(1) as usize).enumerate() {
                    for (x, &coverage) in coverages.iter().enumerate() {
                        let px = pen as i64 + i64::from(bitmap.left) + x as i64;
                        let py = i64::from(baseline) - i64::from(bitmap.top) + y as i64;
                        if coverage == 0
                            || px < 0
                            || py < 0
                            || px >= i64::from(width)
                            || py >= i64::from(height)
                        {
                            continue;
                        }
                        // Blended in the encoded space, as macOS blends text.
                        let pixel =
                            &mut image.pixels[(py as u64 * u64::from(width) + px as u64) as usize];
                        for channel in 0..3 {
                            let (from, to) =
                                (i32::from(pixel[channel]), i32::from(foreground[channel]));
                            let blended = from + (to - from) * i32::from(coverage) / 255;
                            pixel[channel] = blended.clamp(0, 255) as u8;
                        }
                    }
                }
            }
        }
    }
    Ok(image)
}
