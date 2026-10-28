//! Golden images of the screen: a terminal's snapshot laid out in cells, as
//! a window's frame lays it out, drawn by the GPU into an offscreen
//! texture, and compared with the image in `tests/golden/`, within a
//! tolerance for the GPU's and the rasterizer's rounding.
//!
//! `LOKYR_GOLDEN_UPDATE=1 cargo test -p lokyr-app --test golden` writes the
//! images afresh, to look at before they are committed. An image that
//! differs is written next to the target's other files, with a map of the
//! pixels that differ, as the failure tells.

#![cfg(target_os = "macos")]

use std::fs::{self, File};
use std::io::BufWriter;
use std::path::{Path, PathBuf};

use lokyr_font_coretext::CoreText;
use lokyr_render_core::{CellBatch, CursorShape, DisplayList, Item, Renderer};
use lokyr_render_metal::{Gpu, MetalRenderer};
use lokyr_text_core::{FontConfig, Size};
use lokyr_ui_core::font::Font;
use lokyr_ui_core::pane::{self as cells, CursorLook, Selection, TextLook};
use lokyr_ui_core::theme::Theme;
use lokyr_vt_core::parser::Parser;
use lokyr_vt_core::terminal::{Options, SelectionShape, SelectionUnit, Snapshot, Terminal};

/// The pixels around the cells, as a window's padding.
const MARGIN: u32 = 4;

/// The font's size, in points.
const POINTS: f64 = 13.0;

/// A pixel differs when one of its components differs by more than this,
/// past the GPU's rounding and the rasterizer's small changes between
/// releases of macOS.
const CHANNEL: u8 = 24;

/// The share of an image's pixels that can differ, in a scene of text: a
/// glyph moved by a pixel changes many more.
const SHARE: f64 = 0.002;

/// The pixels that can differ, at a scale of 1, in a scene of the shapes
/// that the renderer draws itself, without text, as a cursor or a dotted
/// line: as many times the scale's square at another.
const SHAPES: usize = 2;

/// What a scene draws: a screen's columns and rows, what a program writes
/// to it, the cursor's shape where it shows, and the cells that are
/// selected, from one to another.
struct Scene {
    name: &'static str,
    columns: u16,
    rows: u16,
    text: String,
    cursor: Option<CursorShape>,
    selection: Option<((u16, i64), (u16, i64))>,
    /// Whether it draws shapes alone, without text, which the GPU draws
    /// the same wherever it runs.
    shapes: bool,
}

/// The GPU, its renderer and a font of `POINTS` at `scale`, as a window of
/// that scale has them; `None` without a Metal device.
struct Drawing {
    _gpu: Gpu,
    renderer: MetalRenderer,
    font: Font<CoreText>,
}

impl Drawing {
    fn new(scale: f32) -> Option<Self> {
        let Ok(gpu) = Gpu::new() else {
            eprintln!("no Metal device: the golden images are not compared");
            return None;
        };
        let config = FontConfig {
            size: Size::try_from(POINTS).unwrap(),
            ..FontConfig::default()
        };
        // The system's default, which smooths fonts.
        let font = Font::new(CoreText::with_font_smoothing(true), config, scale).unwrap();
        let (coverage, color) = font.texture_sizes();
        let renderer = MetalRenderer::new(&gpu, coverage, color).unwrap();
        Some(Self {
            _gpu: gpu,
            renderer,
            font,
        })
    }

    /// The cells of `scene`'s screen, from `(x, y)`, in the dark variant of
    /// the built-in theme, as a frame lays them out.
    fn batch(
        &mut self,
        scene: &Scene,
        (x, y): (i32, i32),
    ) -> (CellBatch, lokyr_render_core::Color) {
        let theme = Theme::builtin();
        let variant = theme.variant(true);
        let mut terminal = Terminal::new(scene.columns, scene.rows, Options::default());
        terminal.set_default_colors(variant.colors.clone());
        Parser::new().advance(&mut terminal, scene.text.as_bytes());
        if let Some((from, to)) = scene.selection {
            terminal.select(from, to, SelectionShape::Cells, SelectionUnit::Cell, "");
        }
        let mut snapshot = Snapshot::default();
        terminal.snapshot(&mut snapshot);
        self.font.begin_frame();
        let cell = *self.font.cell();
        let cursor = scene.cursor.map(|shape| CursorLook {
            shape,
            thickness: cell.underline_thickness.max(1),
        });
        let selected = snapshot.selection();
        let contains =
            |column: u16, row: u16| selected.is_some_and(|shown| shown.contains(column, row));
        let selection = selected.map(|_| Selection {
            colors: variant.selection,
            contains: &contains,
        });
        let batch = cells::cells_with(
            &snapshot,
            &cell,
            (x, y),
            selection.as_ref(),
            cursor,
            TextLook::default(),
            &mut self.font,
        );
        (batch, cells::background(&snapshot))
    }

    /// `scenes`, one under the other, drawn offscreen: the image's width,
    /// its height, and its pixels.
    fn draw(&mut self, scenes: &[Scene]) -> (u32, u32, Vec<u8>) {
        let mut items = Vec::new();
        let mut background = None;
        let mut y = MARGIN;
        let mut width = 0;
        for scene in scenes {
            let (batch, color) = self.batch(scene, (MARGIN as i32, y as i32));
            width = width.max(u32::from(scene.columns) * batch.cell_width);
            y += u32::from(scene.rows) * batch.cell_height;
            background.get_or_insert(color);
            items.push(Item::Cells(batch));
        }
        let (width, height) = (width + 2 * MARGIN, y + MARGIN);
        let list = DisplayList {
            background: background.unwrap_or_default(),
            items,
            frame: self.font.atlas_frame(),
        };
        let updates = self.font.texture_updates();
        self.renderer.update_textures(&updates).unwrap();
        let pixels = self.renderer.draw_offscreen(&list, width, height).unwrap();
        (width, height, pixels)
    }
}

/// The scenes of the text: Latin, its styles, CJK, emoji, combining marks,
/// box drawing, blocks and powerline symbols, which the terminal draws
/// itself.
fn text() -> Vec<Scene> {
    vec![Scene {
        name: "text",
        columns: 40,
        rows: 8,
        text: concat!(
            "The quick brown fox, 0O 1lI| {}[]()\r\n",
            "\x1b[1mbold\x1b[0m \x1b[3mitalic\x1b[0m \x1b[1;3mbold italic\x1b[0m \x1b[2mdim\x1b[0m\r\n",
            "日本語のテキスト|한국어|中文\r\n",
            "😀|👍🏽|🇮🇹|❤️|1️⃣ e\u{301} n\u{303} o\u{308}\u{301}\r\n",
            "┌─┬─┐ ╔═╗ ╭─╮ ░▒▓█ ▀▄▌▐ ▁▂▃▄▅▆▇█\r\n",
            "└─┴─┘ ╚═╝ ╰─╯ ⣿⡇⠁ \u{e0b0}\u{e0b1}\u{e0b2}\u{e0b3}\u{e0b4}\u{e0b6}\r\n",
            "\x1b[30;44m ~/lokyr \x1b[34;42m\u{e0b0}\x1b[30m main \x1b[32;49m\u{e0b0}\x1b[0m",
        )
        .to_owned(),
        cursor: None,
        selection: None,
        shapes: false,
    }]
}

/// The palette, 256 colors, true colors, reverse video and dim text.
fn colors() -> Vec<Scene> {
    let mut text = String::new();
    for base in [40, 100] {
        for n in 0..8 {
            text.push_str(&format!("\x1b[{}m   ", base + n));
        }
        text.push_str("\x1b[0m\r\n");
    }
    for n in 0..8 {
        text.push_str(&format!("\x1b[{}mAa \x1b[1mAa\x1b[0m ", 30 + n));
    }
    text.push_str("\r\n");
    for n in (16..232).step_by(6) {
        text.push_str(&format!("\x1b[48;5;{n}m "));
    }
    text.push_str("\x1b[0m\r\n");
    for n in 0..36_u32 {
        let (r, g, b) = (255 - n * 7, n * 7, 128);
        text.push_str(&format!("\x1b[48;2;{r};{g};{b}m "));
    }
    text.push_str("\x1b[0m\r\n");
    text.push_str(
        "\x1b[7m reverse \x1b[0m \x1b[31;7m red reverse \x1b[0m \x1b[2;32mdim green\x1b[0m",
    );
    vec![Scene {
        name: "colors",
        columns: 48,
        rows: 6,
        text,
        cursor: None,
        selection: None,
        shapes: false,
    }]
}

/// Every underline's style, colored underlines, the strike and the
/// overline, under descenders and wide characters too.
fn decorations() -> Vec<Scene> {
    vec![Scene {
        name: "decorations",
        columns: 40,
        rows: 4,
        text: concat!(
            "\x1b[4msingle\x1b[0m \x1b[4:2mdouble\x1b[0m \x1b[4:3mcurly\x1b[0m \x1b[4:4mdotted\x1b[0m\r\n",
            "\x1b[4:5mdashed\x1b[0m \x1b[9mstrike\x1b[0m \x1b[53moverline\x1b[0m \x1b[4;9;53mall\x1b[0m\r\n",
            "\x1b[4:3;58;5;196mred curly\x1b[0m \x1b[4;58;2;0;160;255mblue line\x1b[0m\r\n",
            "\x1b[4mgjpqy\x1b[0m \x1b[4m日本\x1b[0m \x1b[9;31mred strike\x1b[0m",
        )
        .to_owned(),
        cursor: None,
        selection: None,
        shapes: false,
    }]
}

/// The cursor's shapes, each on a screen of its own: a block over a
/// letter, a bar, an underline, and the hollow block of a window without
/// the focus, over a wide character.
fn cursors() -> Vec<Scene> {
    [
        ("block", CursorShape::Block, "abc\x1b[D"),
        ("bar", CursorShape::Bar, "abc\x1b[D"),
        ("underline", CursorShape::Underline, "abc\x1b[D"),
        ("hollow", CursorShape::Hollow, "a日\x1b[2D"),
    ]
    .into_iter()
    .map(|(_, shape, text)| Scene {
        name: "cursor",
        columns: 8,
        rows: 1,
        text: text.to_owned(),
        cursor: Some(shape),
        selection: None,
        shapes: false,
    })
    .collect()
}

/// A selection across two rows, over colored text and a wide character.
fn selection() -> Vec<Scene> {
    vec![Scene {
        name: "selection",
        columns: 30,
        rows: 3,
        text: "plain \x1b[31mred\x1b[0m \x1b[42mon green\x1b[0m\r\n日本語 and more\r\nafter"
            .to_owned(),
        cursor: None,
        selection: Some(((2, 0), (5, 1))),
        shapes: false,
    }]
}

/// The shapes that the renderer draws itself, without text: the cursor's
/// four shapes on empty cells, and every line on spaces, each style of
/// underline, the strike and the overline.
fn shapes() -> Vec<Scene> {
    let mut scenes: Vec<Scene> = [
        CursorShape::Block,
        CursorShape::Bar,
        CursorShape::Underline,
        CursorShape::Hollow,
    ]
    .into_iter()
    .map(|shape| Scene {
        name: "shapes",
        columns: 8,
        // A row of its own, and an empty one after it, which keeps it apart
        // from the next.
        rows: 2,
        text: "  \x1b[D".to_owned(),
        cursor: Some(shape),
        selection: None,
        shapes: true,
    })
    .collect();
    let lines = ["4", "4:2", "4:3", "4:4", "4:5", "9", "53"]
        .map(|style| format!("\x1b[{style}m{}\x1b[0m\r\n", " ".repeat(12)))
        .concat();
    scenes.push(Scene {
        name: "shapes",
        columns: 12,
        rows: 7,
        text: lines.trim_end_matches("\r\n").to_owned(),
        cursor: None,
        selection: None,
        shapes: true,
    });
    scenes
}

/// The directory of the golden images.
fn golden() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden")
}

/// Writes `pixels`, RGBA, `width` by `height`, as a PNG at `path`.
fn write_png(path: &Path, width: u32, height: u32, pixels: &[u8]) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut encoder = png::Encoder::new(BufWriter::new(File::create(path).unwrap()), width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_compression(png::Compression::High);
    encoder
        .write_header()
        .unwrap()
        .write_image_data(pixels)
        .unwrap();
}

/// The PNG at `path`: its width, its height and its RGBA pixels.
fn read_png(path: &Path) -> Option<(u32, u32, Vec<u8>)> {
    let decoder = png::Decoder::new(std::io::BufReader::new(File::open(path).ok()?));
    let mut reader = decoder.read_info().ok()?;
    let mut pixels = vec![0; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut pixels).ok()?;
    if info.color_type != png::ColorType::Rgba || info.bit_depth != png::BitDepth::Eight {
        return None;
    }
    pixels.truncate(info.buffer_size());
    Some((info.width, info.height, pixels))
}

/// How many of the pixels of two images of the same size differ by more
/// than `CHANNEL` in one of their components, and a map of them, red on
/// the expected image faded.
fn differences(expected: &[u8], actual: &[u8]) -> (usize, Vec<u8>) {
    let mut count = 0;
    let mut map = Vec::with_capacity(expected.len());
    for (want, got) in expected
        .as_chunks::<4>()
        .0
        .iter()
        .zip(actual.as_chunks::<4>().0)
    {
        let differs = want.iter().zip(got).any(|(a, b)| a.abs_diff(*b) > CHANNEL);
        if differs {
            count += 1;
            map.extend_from_slice(&[255, 0, 0, 255]);
        } else {
            map.extend(want[..3].iter().map(|c| c / 4));
            map.push(255);
        }
    }
    (count, map)
}

/// Draws `scenes` at `scale`, and compares the image with its golden one.
fn check(scenes: &[Scene], scale: u32) {
    let Some(mut drawing) = Drawing::new(scale as f32) else {
        return;
    };
    let (width, height, pixels) = drawing.draw(scenes);
    let name = format!("{}-{scale}x", scenes[0].name);
    let path = golden().join(format!("{name}.png"));
    if std::env::var_os("LOKYR_GOLDEN_UPDATE").is_some() {
        write_png(&path, width, height, &pixels);
        return;
    }
    let failed = Path::new(env!("CARGO_TARGET_TMPDIR")).join("golden");
    let actual = failed.join(format!("{name}.png"));
    let Some((want_width, want_height, expected)) = read_png(&path) else {
        write_png(&actual, width, height, &pixels);
        panic!(
            "{} is not there, or is not an RGBA image: the image drawn is {}; \
             LOKYR_GOLDEN_UPDATE=1 writes the golden images",
            path.display(),
            actual.display()
        );
    };
    if (want_width, want_height) != (width, height) {
        write_png(&actual, width, height, &pixels);
        panic!(
            "{name} is {width} by {height} pixels, where {} is {want_width} by {want_height}: \
             the image drawn is {}",
            path.display(),
            actual.display()
        );
    }
    let (count, map) = differences(&expected, &pixels);
    let most = if scenes[0].shapes {
        SHAPES * (scale * scale) as usize
    } else {
        (f64::from(width * height) * SHARE) as usize
    };
    if count > most {
        write_png(&actual, width, height, &pixels);
        let map_path = failed.join(format!("{name}-differences.png"));
        write_png(&map_path, width, height, &map);
        panic!(
            "{name}: {count} pixels differ from {}, more than {most}: the image drawn is {}, \
             and the pixels that differ are red in {}",
            path.display(),
            actual.display(),
            map_path.display()
        );
    }
}

#[test]
fn text_draws_as_its_golden_image() {
    for scale in [1, 2] {
        check(&text(), scale);
    }
}

#[test]
fn colors_draw_as_their_golden_image() {
    for scale in [1, 2] {
        check(&colors(), scale);
    }
}

#[test]
fn decorations_draw_as_their_golden_image() {
    for scale in [1, 2] {
        check(&decorations(), scale);
    }
}

#[test]
fn the_cursor_s_shapes_draw_as_their_golden_image() {
    for scale in [1, 2] {
        check(&cursors(), scale);
    }
}

#[test]
fn the_shapes_draw_as_their_golden_image() {
    for scale in [1, 2] {
        check(&shapes(), scale);
    }
}

#[test]
fn a_selection_draws_as_its_golden_image() {
    for scale in [1, 2] {
        check(&selection(), scale);
    }
}

#[test]
fn a_pixel_differs_past_the_tolerance_alone() {
    let expected = [10, 20, 30, 255, 0, 0, 0, 255];
    let close = [10 + CHANNEL, 20, 30 - CHANNEL, 255, 0, 0, 0, 255];
    assert_eq!(differences(&expected, &close).0, 0);
    let far = [10, 20, 31 + CHANNEL, 255, 0, 0, 0, 255];
    let (count, map) = differences(&expected, &far);
    assert_eq!(count, 1);
    assert_eq!(map, [255, 0, 0, 255, 0, 0, 0, 255]);
}
