//! The weight of the interface's text against the system's: a line drawn
//! by the GPU, of its glyphs' coverage, which the shader brings to the
//! weight of macOS's text, and the same line as CoreText and Core Graphics
//! draw it, as AppKit's labels do, in the light appearance and the dark
//! one, at a scale of 1 and 2, each of about as much ink as the other.

#![cfg(target_os = "macos")]

use lokyr_font_coretext::{CoreText, system_text};
use lokyr_render_core::{Color, DisplayList, Item, Renderer};
use lokyr_render_metal::{Gpu, MetalRenderer};
use lokyr_text_core::FontConfig;
use lokyr_ui_core::font::Font;

/// The text of the measure: a path, as a tab's title or a field's.
const TEXT: &str = "~/Projects/lokyr";

/// The interface's text size, in points.
const POINTS: f64 = 13.0;

/// The share by which a line's ink can differ from the system's.
const TOLERANCE: f64 = 0.06;

/// The ink of `pixels`, RGBA, against `background`: how far each pixel's
/// green component is from the background's, summed.
fn ink(pixels: &[u8], background: [u8; 3]) -> f64 {
    pixels
        .as_chunks::<4>()
        .0
        .iter()
        .map(|pixel| f64::from(pixel[1].abs_diff(background[1])))
        .sum()
}

/// The ink of the line drawn by the GPU at `scale`, in `foreground` on
/// `background`, in an image of `width` by `height` pixels.
fn ours(scale: f32, (foreground, background): ([u8; 3], [u8; 3]), size: (u32, u32)) -> Option<f64> {
    let gpu = Gpu::new().ok()?;
    let mut font = Font::new(CoreText::new(), FontConfig::default(), scale).unwrap();
    let (coverage, color) = font.texture_sizes();
    let mut renderer = MetalRenderer::new(&gpu, coverage, color).unwrap();
    font.begin_frame();
    let ascent = font.interface().ascent() * scale;
    let [r, g, b] = foreground;
    let dark = background[1] < foreground[1];
    let margin = 4.0 * scale;
    let (glyphs, _) = font.interface_text(
        TEXT,
        (margin, (margin + ascent).round()),
        Color::from_rgb8(r, g, b),
        dark,
    );
    let [r, g, b] = background;
    let list = DisplayList {
        background: Color::from_rgb8(r, g, b),
        items: vec![Item::Glyphs(glyphs)],
        frame: font.atlas_frame(),
    };
    renderer.update_textures(&font.texture_updates()).unwrap();
    let pixels = renderer.draw_offscreen(&list, size.0, size.1).unwrap();
    Some(ink(&pixels, background))
}

#[test]
fn the_interface_s_text_weighs_as_the_system_s() {
    let light = ([0x1e, 0x1e, 0x20], [0xff, 0xff, 0xff]);
    let dark = ([0xe6, 0xe6, 0xe6], [0x1e, 0x1e, 0x20]);
    let mut report = Vec::new();
    for (name, colors) in [("light", light), ("dark", dark)] {
        for scale in [1.0, 2.0] {
            let system = system_text(TEXT, POINTS, f64::from(scale), colors.0, colors.1)
                .expect("Core Graphics draws the line");
            let Some(ours) = ours(scale, colors, (system.width, system.height)) else {
                eprintln!("no Metal device: the weight is not measured");
                return;
            };
            let theirs = ink(&system.pixels, colors.1);
            report.push((name, scale, ours / theirs));
        }
    }
    for &(name, scale, ratio) in &report {
        println!(
            "{name} at {scale}: {:+.1}% of the system's ink",
            (ratio - 1.0) * 100.0
        );
    }
    for (name, scale, ratio) in report {
        assert!(
            (ratio - 1.0).abs() <= TOLERANCE,
            "{name} at {scale}: {:+.1}% of the system's ink",
            (ratio - 1.0) * 100.0
        );
    }
}
