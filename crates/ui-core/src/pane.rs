//! A pane's grid of cells, as a display list draws it, from a snapshot of
//! its terminal.

use bolt_render_core::{
    AtlasRegion, CellBatch, CellCursor, CellGlyph, CellLine, Color, CursorShape, LineStyle,
};
use bolt_text_core::CellMetrics;
use bolt_vt_core::page::{self, Content, Rgb, Style, Width};
use bolt_vt_core::terminal::{Colors, Snapshot};
use bolt_vt_core::unicode::Widths;

use crate::theme::SelectionColors;

/// A glyph as the atlas holds it: its pixels, and where they go from the pen
/// on the baseline.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AtlasGlyph {
    /// Its pixels.
    pub region: AtlasRegion,
    /// How far its left edge is from the pen, rightwards.
    pub left: i32,
    /// How far its top edge is from the baseline, upwards.
    pub top: i32,
}

/// What a cell shows, for the caller to find its glyph.
#[derive(Clone, Copy, Debug)]
pub struct CellText<'a> {
    /// The cell's grapheme cluster.
    pub text: &'a str,
    /// Its style: bold and italic choose the face.
    pub style: &'a Style,
    /// Whether it takes two columns.
    pub wide: bool,
}

/// How the cursor draws in a frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CursorLook {
    /// Its shape.
    pub shape: CursorShape,
    /// The pixels of a bar's width and of an underline's height; a hollow
    /// block's edges take the face's underline thickness.
    pub thickness: u32,
}

/// The text that an input method composes, which a pane shows at its
/// cursor and never sends.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Preedit {
    /// The text.
    pub text: String,
    /// Where the input method's cursor is in it, in bytes.
    pub cursor: usize,
}

/// The cells of a selection, and its colors.
#[derive(Clone, Copy)]
pub struct Selection<'a> {
    /// The selection's colors.
    pub colors: SelectionColors,
    /// Whether the cell at a column and a row is selected.
    pub contains: &'a dyn Fn(u16, u16) -> bool,
}

/// The color that the grid's empty cells show: the default background, or
/// the default foreground in reverse video.
pub fn background(snapshot: &Snapshot) -> Color {
    defaults(snapshot).1
}

/// The grid of `snapshot`, of cells of `cell`, with its top left corner at
/// `x` and `y`: each cell's background, and the glyph of each cell that
/// shows a character, which `glyph` finds, through the fonts and the atlas,
/// or not; the cells of `selection` in its colors; and the cursor, as
/// `cursor` has it look, unless the program hid it or the blinking does.
///
/// A character that takes two columns has its glyph in the first, which
/// spans the two; the second has only its background, as has a glyph
/// without ink. Reverse video swaps the default colors; faint text mixes
/// its color halfway to its background; hidden text shows its background
/// alone. The cursor covers both cells of a wide character, and the text
/// under a block takes its cell's background.
pub fn cells(
    snapshot: &Snapshot,
    cell: &CellMetrics,
    x: i32,
    y: i32,
    selection: Option<&Selection<'_>>,
    cursor: Option<CursorLook>,
    glyph: &mut dyn FnMut(&CellText<'_>) -> Option<AtlasGlyph>,
) -> CellBatch {
    let (columns, rows) = (snapshot.cols(), snapshot.rows());
    let colors = snapshot.colors();
    let defaults = defaults(snapshot);
    let area = usize::from(columns) * usize::from(rows);
    let mut batch = CellBatch {
        x,
        y,
        columns,
        rows,
        cell_width: cell.width,
        cell_height: cell.height,
        backgrounds: Vec::with_capacity(area),
        glyphs: Vec::with_capacity(area),
        lines: Vec::new(),
        cursor: cursor.and_then(|look| place_cursor(snapshot, look, cell)),
    };
    let selected = |column: u16, row: u16| {
        selection
            .filter(|selection| (selection.contains)(column, row))
            .map(|selection| selection.colors)
    };
    let mut text = String::new();
    for row in 0..rows {
        let line = snapshot.line(row);
        for (column, &cell_of) in (0..columns).zip(line.cells()) {
            let chosen = selected(column, row);
            let style = line.style(cell_of.style());
            let (mut foreground, mut background) = style_colors(&style, colors, defaults);
            let character = match cell_of.content() {
                Content::Char(character) | Content::Grapheme(character) => character,
                Content::BgPalette(index) => {
                    let erased = color(colors.palette[usize::from(index)]);
                    batch
                        .backgrounds
                        .push(chosen.map_or(erased, |chosen| chosen.background));
                    continue;
                }
                Content::BgRgb(rgb) => {
                    batch
                        .backgrounds
                        .push(chosen.map_or(color(rgb), |chosen| chosen.background));
                    continue;
                }
            };
            if let Some(chosen) = chosen {
                background = chosen.background;
                foreground = chosen.foreground.unwrap_or(foreground);
            }
            batch.backgrounds.push(background);
            let width = cell_of.width();
            if character == '\0'
                || style.invisible
                || matches!(width, Width::SpacerTail | Width::SpacerHead)
            {
                continue;
            }
            if style.faint {
                foreground = mix(foreground, background);
            }
            text.clear();
            text.push(character);
            text.extend(line.grapheme(column));
            let shown = CellText {
                text: &text,
                style: &style,
                wide: width == Width::Wide,
            };
            // A glyph without ink, a space's, draws nothing.
            if let Some(found) = glyph(&shown).filter(|found| found.region.width > 0) {
                batch.glyphs.push(CellGlyph {
                    column,
                    row,
                    region: found.region,
                    x: found.left,
                    // The baseline's row, in the cell, less the rows of the
                    // glyph above it.
                    y: i32::try_from(cell.baseline).unwrap_or(i32::MAX) - found.top,
                    color: foreground,
                });
            }
        }
        // A line shorter than the screen, as after a resize, has the
        // default background past its end.
        for column in u16::try_from(line.cells().len()).unwrap_or(u16::MAX)..columns {
            let chosen = selected(column, row);
            batch
                .backgrounds
                .push(chosen.map_or(defaults.1, |chosen| chosen.background));
        }
    }
    settle_cursor(&mut batch, snapshot);
    batch
}

/// The least contrast, as WCAG measures it, of a cursor on its cell's
/// background.
const CURSOR_CONTRAST: f64 = 1.5;

/// Gives `batch`'s cursor a color apart from its cell's background, as
/// xterm keeps the cursor's color apart from the background's: the
/// cursor's own color, or, when that is too near, as on inverse text or in
/// reverse video, the color of the cell's text, or else black or white,
/// whichever stands farther; a block's text takes the cell's background.
fn settle_cursor(batch: &mut CellBatch, snapshot: &Snapshot) {
    let Some(cursor) = &mut batch.cursor else {
        return;
    };
    let (column, row) = (cursor.column, cursor.row);
    let index = usize::from(row) * usize::from(batch.columns) + usize::from(column);
    let Some(&background) = batch.backgrounds.get(index) else {
        return;
    };
    let glyph = batch
        .glyphs
        .iter_mut()
        .find(|glyph| glyph.row == row && glyph.column == column);
    let text = glyph
        .as_ref()
        .map_or(defaults(snapshot).0, |glyph| glyph.color);
    let own = color(snapshot.colors().cursor);
    cursor.color = if contrast(own, background) >= CURSOR_CONTRAST {
        own
    } else if contrast(text, background) >= CURSOR_CONTRAST {
        text
    } else if luminance(background) > 0.18 {
        Color::from_rgb8(0, 0, 0)
    } else {
        Color::from_rgb8(255, 255, 255)
    };
    if cursor.shape == CursorShape::Block
        && let Some(glyph) = glyph
    {
        glyph.color = background;
    }
}

/// The relative luminance of `color`, of sRGB's encoded components, as
/// WCAG 2 defines it.
fn luminance(color: Color) -> f64 {
    let linear = |channel: f32| {
        let value = f64::from(channel);
        if value <= 0.040_45 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * linear(color.red) + 0.7152 * linear(color.green) + 0.0722 * linear(color.blue)
}

/// The contrast ratio of two colors, as WCAG 2 defines it: from 1 to 21.
fn contrast(a: Color, b: Color) -> f64 {
    let (a, b) = (luminance(a), luminance(b));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

/// Draws `preedit` over `batch`, the grid of `snapshot`, from its cursor's
/// cell: each character in the cells that its width takes, on the default
/// background and in the default text's color, in place of what they show,
/// underlined, with the cursor where the input method has its own. What
/// goes past the right edge is left out, as is a character that takes no
/// cell.
pub fn preedit(
    batch: &mut CellBatch,
    snapshot: &Snapshot,
    cell: &CellMetrics,
    preedit: &Preedit,
    glyph: &mut dyn FnMut(&CellText<'_>) -> Option<AtlasGlyph>,
) {
    let (foreground, background) = defaults(snapshot);
    let position = snapshot.cursor().position;
    let (mut start, row, columns) = (position.x, position.y, batch.columns);
    if row >= batch.rows || start >= columns {
        return;
    }
    // From a wide character's second cell, its first.
    let line = snapshot.line(row);
    let width_at = |column: u16| {
        line.cells()
            .get(usize::from(column))
            .map(|cell| cell.width())
    };
    if start > 0 && width_at(start) == Some(Width::SpacerTail) {
        start -= 1;
    }
    let widths = Widths::default();
    let mut column = start;
    let mut cursor = None;
    let mut placed = Vec::new();
    let mut text = String::new();
    for (at, character) in preedit.text.char_indices() {
        if at == preedit.cursor {
            cursor = Some(column);
        }
        let width = u16::from(widths.char_width(character));
        if width == 0 {
            continue;
        }
        if column + width > columns {
            break;
        }
        let covered = column..column + width;
        batch
            .glyphs
            .retain(|glyph| glyph.row != row || !covered.contains(&glyph.column));
        let first = usize::from(row) * usize::from(columns) + usize::from(column);
        for drawn in &mut batch.backgrounds[first..first + usize::from(width)] {
            *drawn = background;
        }
        text.clear();
        text.push(character);
        let shown = CellText {
            text: &text,
            style: &Style::DEFAULT,
            wide: width == 2,
        };
        if let Some(found) = glyph(&shown).filter(|found| found.region.width > 0) {
            batch.glyphs.push(CellGlyph {
                column,
                row,
                region: found.region,
                x: found.left,
                y: i32::try_from(cell.baseline).unwrap_or(i32::MAX) - found.top,
                color: foreground,
            });
        }
        placed.push((column, width));
        column += width;
    }
    batch.lines.push(CellLine {
        column: start,
        row,
        columns: column - start,
        style: LineStyle::Single,
        top: cell.underline_top,
        thickness: cell.underline_thickness,
        color: foreground,
    });
    if let Some(drawn) = &mut batch.cursor {
        let at = cursor.unwrap_or(column).min(columns - 1);
        // A block over a wide composed character covers it whole.
        let width = placed
            .iter()
            .find(|&&(column, _)| column == at)
            .map_or(1, |&(_, width)| width);
        drawn.column = at;
        drawn.columns = width.min(columns - at);
    }
    settle_cursor(batch, snapshot);
}

/// The cursor of `snapshot`, as `look` has it, where it shows: on a wide
/// character, over its two cells, from either.
fn place_cursor(snapshot: &Snapshot, look: CursorLook, cell: &CellMetrics) -> Option<CellCursor> {
    let cursor = snapshot.cursor();
    let (mut column, row) = (cursor.position.x, cursor.position.y);
    let (columns, rows) = (snapshot.cols(), snapshot.rows());
    if !cursor.visible || row >= rows || column >= columns {
        return None;
    }
    let cells = snapshot.line(row).cells();
    let width = |column: u16| cells.get(usize::from(column)).map(|cell| cell.width());
    if column > 0 && width(column) == Some(Width::SpacerTail) {
        column -= 1;
    }
    let wide = width(column) == Some(Width::Wide);
    Some(CellCursor {
        column,
        row,
        columns: if wide { 2 } else { 1 }.min(columns - column),
        shape: look.shape,
        thickness: match look.shape {
            CursorShape::Bar | CursorShape::Underline => look.thickness,
            CursorShape::Block | CursorShape::Hollow => cell.underline_thickness,
        },
        color: color(snapshot.colors().cursor),
    })
}

/// The default text and background colors of `snapshot`, swapped in reverse
/// video.
fn defaults(snapshot: &Snapshot) -> (Color, Color) {
    let colors = snapshot.colors();
    let (foreground, background) = (color(colors.foreground), color(colors.background));
    if snapshot.reverse_video() {
        (background, foreground)
    } else {
        (foreground, background)
    }
}

/// The text's and the background's colors of `style`, from `colors` and the
/// `defaults`, and swapped for inverse text.
fn style_colors(style: &Style, colors: &Colors, (fg, bg): (Color, Color)) -> (Color, Color) {
    let resolve = |which: page::Color, default: Color| match which {
        page::Color::Default => default,
        page::Color::Palette(index) => color(colors.palette[usize::from(index)]),
        page::Color::Rgb(rgb) => color(rgb),
    };
    let foreground = resolve(style.fg, fg);
    let background = resolve(style.bg, bg);
    if style.inverse {
        (background, foreground)
    } else {
        (foreground, background)
    }
}

/// The color halfway between `a` and `b`, in the encoded space, where the
/// text blends.
fn mix(a: Color, b: Color) -> Color {
    Color {
        red: (a.red + b.red) / 2.0,
        green: (a.green + b.green) / 2.0,
        blue: (a.blue + b.blue) / 2.0,
    }
}

fn color(rgb: Rgb) -> Color {
    Color::from_rgb8(rgb.r, rgb.g, rgb.b)
}

#[cfg(test)]
mod tests {
    use bolt_render_core::Texture;
    use bolt_text_core::{FaceMetrics, FontConfig};
    use bolt_vt_core::parser::Parser;
    use bolt_vt_core::terminal::{Options, Terminal};

    use super::*;

    /// The snapshot of a screen of `cols` by `rows` after `text`.
    fn snapshot(cols: u16, rows: u16, text: &str) -> Snapshot {
        let mut terminal = Terminal::new(cols, rows, Options::default());
        Parser::new().advance(&mut terminal, text.as_bytes());
        let mut snapshot = Snapshot::default();
        terminal.snapshot(&mut snapshot);
        snapshot
    }

    /// A cell of 8 by 16 pixels, its baseline at 13.
    fn cell() -> CellMetrics {
        CellMetrics::new(
            &FaceMetrics {
                ascent: 12.07,
                descent: 3.07,
                advance: 7.83,
                ..FaceMetrics::default()
            },
            &FontConfig::default(),
        )
    }

    /// A glyph of the atlas for each character, at a column of its own,
    /// that records what it was asked for; a space's has no ink, as the
    /// atlas gives it no room.
    fn atlas(
        asked: &mut Vec<(String, bool, bool)>,
    ) -> impl FnMut(&CellText<'_>) -> Option<AtlasGlyph> + '_ {
        move |shown: &CellText<'_>| {
            asked.push((shown.text.to_owned(), shown.style.bold, shown.wide));
            let first = shown.text.chars().next()?;
            let (width, height) = if first == ' ' { (0, 0) } else { (6, 10) };
            Some(AtlasGlyph {
                region: AtlasRegion {
                    texture: Texture::Coverage,
                    x: first as u32,
                    y: 0,
                    width,
                    height,
                },
                left: 1,
                top: 10,
            })
        }
    }

    fn rgb(r: u8, g: u8, b: u8) -> Color {
        Color::from_rgb8(r, g, b)
    }

    #[test]
    fn a_snapshot_becomes_cells() {
        let snapshot = snapshot(6, 2, "AB\x1b[41;1mC\x1b[0m");
        let mut asked = Vec::new();
        let batch = cells(
            &snapshot,
            &cell(),
            4,
            30,
            None,
            None,
            &mut atlas(&mut asked),
        );
        assert_eq!((batch.x, batch.y, batch.columns, batch.rows), (4, 30, 6, 2));
        assert_eq!((batch.cell_width, batch.cell_height), (8, 16));
        assert_eq!(batch.backgrounds.len(), 12);
        let colors = Colors::default();
        let background = color(colors.background);
        assert_eq!(batch.backgrounds[0], background);
        // SGR 41 is the palette's red.
        assert_eq!(batch.backgrounds[2], color(colors.palette[1]));
        assert!(batch.backgrounds[3..].iter().all(|&bg| bg == background));
        // The three characters, and nothing for the empty cells.
        assert_eq!(
            asked,
            [
                ("A".to_owned(), false, false),
                ("B".to_owned(), false, false),
                ("C".to_owned(), true, false),
            ]
        );
        let glyphs: Vec<(u16, u16, i32, i32)> = batch
            .glyphs
            .iter()
            .map(|g| (g.column, g.row, g.x, g.y))
            .collect();
        // From the baseline at 13, a glyph whose top is 10 above it.
        assert_eq!(glyphs, [(0, 0, 1, 3), (1, 0, 1, 3), (2, 0, 1, 3)]);
        assert!(
            batch
                .glyphs
                .iter()
                .all(|g| g.color == color(colors.foreground))
        );
    }

    #[test]
    fn a_wide_character_takes_its_first_cell() {
        let snapshot = snapshot(6, 1, "a日b");
        let mut asked = Vec::new();
        let batch = cells(&snapshot, &cell(), 0, 0, None, None, &mut atlas(&mut asked));
        let columns: Vec<u16> = batch.glyphs.iter().map(|g| g.column).collect();
        assert_eq!(columns, [0, 1, 3]);
        assert_eq!(asked[1], ("日".to_owned(), false, true));
        assert_eq!(batch.backgrounds.len(), 6);
    }

    #[test]
    fn a_cluster_comes_whole() {
        let snapshot = snapshot(6, 1, "e\u{301}👍🏽");
        let mut asked = Vec::new();
        cells(&snapshot, &cell(), 0, 0, None, None, &mut atlas(&mut asked));
        let texts: Vec<&str> = asked.iter().map(|(text, ..)| text.as_str()).collect();
        assert_eq!(texts, ["e\u{301}", "👍🏽"]);
    }

    #[test]
    fn colors_resolve_from_the_palette_rgb_and_inverse() {
        let snapshot = snapshot(
            4,
            1,
            "\x1b[38;5;196ma\x1b[38;2;1;2;3;48;2;4;5;6mb\x1b[0;7mc",
        );
        let mut asked = Vec::new();
        let batch = cells(&snapshot, &cell(), 0, 0, None, None, &mut atlas(&mut asked));
        let colors = Colors::default();
        assert_eq!(batch.glyphs[0].color, color(colors.palette[196]));
        assert_eq!(batch.glyphs[1].color, rgb(1, 2, 3));
        assert_eq!(batch.backgrounds[1], rgb(4, 5, 6));
        // Inverse: the default background's color on the default
        // foreground's.
        assert_eq!(batch.glyphs[2].color, color(colors.background));
        assert_eq!(batch.backgrounds[2], color(colors.foreground));
    }

    #[test]
    fn an_erase_with_a_background_takes_its_color() {
        // Background color erase: the cleared cells keep the red.
        let snapshot = snapshot(4, 2, "\x1b[41m\x1b[2J\x1b[0m");
        let mut asked = Vec::new();
        let batch = cells(&snapshot, &cell(), 0, 0, None, None, &mut atlas(&mut asked));
        let red = color(Colors::default().palette[1]);
        assert!(batch.backgrounds.iter().all(|&bg| bg == red));
        assert!(batch.glyphs.is_empty() && asked.is_empty());
    }

    #[test]
    fn a_glyph_without_ink_takes_no_instance() {
        // Spaces that a program writes, as full-screen programs do, over a
        // background of their own.
        let snapshot = snapshot(4, 2, "\x1b[44m    \r\n a  ");
        let mut asked = Vec::new();
        let batch = cells(&snapshot, &cell(), 0, 0, None, None, &mut atlas(&mut asked));
        assert_eq!(asked.len(), 8);
        assert_eq!(batch.glyphs.len(), 1);
        assert_eq!((batch.glyphs[0].column, batch.glyphs[0].row), (1, 1));
        let blue = color(Colors::default().palette[4]);
        assert_eq!(batch.backgrounds, [blue; 8]);
    }

    #[test]
    fn a_glyph_that_the_fonts_lack_leaves_its_background() {
        let snapshot = snapshot(3, 1, "abc");
        let batch = cells(&snapshot, &cell(), 0, 0, None, None, &mut |_: &CellText<
            '_,
        >| None);
        assert!(batch.glyphs.is_empty());
        assert_eq!(batch.backgrounds.len(), 3);
    }

    /// The snapshot of a screen of `cols` by `rows` in the colors of
    /// `theme`, after `text`.
    fn themed(cols: u16, rows: u16, colors: Colors, text: &str) -> Snapshot {
        let mut terminal = Terminal::new(cols, rows, Options::default());
        terminal.set_default_colors(colors);
        Parser::new().advance(&mut terminal, text.as_bytes());
        let mut snapshot = Snapshot::default();
        terminal.snapshot(&mut snapshot);
        snapshot
    }

    #[test]
    fn a_test_pattern_takes_the_theme_s_values() {
        let theme = crate::theme::Theme::builtin();
        for variant in [&theme.light, &theme.dark] {
            let mut pattern = String::new();
            for index in 0..8 {
                pattern.push_str(&format!("\x1b[{}m#\x1b[{}m#", 30 + index, 90 + index));
            }
            pattern.push_str("\x1b[38;5;123m#\x1b[38;2;10;20;30m#\x1b[0m#");
            let snapshot = themed(40, 1, variant.colors.clone(), &pattern);
            let mut asked = Vec::new();
            let batch = cells(&snapshot, &cell(), 0, 0, None, None, &mut atlas(&mut asked));
            let palette = &variant.colors.palette;
            let mut expected: Vec<Color> = (0..8)
                .flat_map(|index| [color(palette[index]), color(palette[index + 8])])
                .collect();
            expected.extend([
                color(palette[123]),
                rgb(10, 20, 30),
                color(variant.colors.foreground),
            ]);
            let found: Vec<Color> = batch.glyphs.iter().map(|glyph| glyph.color).collect();
            assert_eq!(found, expected);
            assert!(
                batch
                    .backgrounds
                    .iter()
                    .all(|&bg| bg == color(variant.colors.background))
            );
            assert_eq!(background(&snapshot), color(variant.colors.background));
        }
    }

    #[test]
    fn a_program_s_colors_take_the_place_of_the_theme_s() {
        // OSC 4 sets the palette's red, OSC 10 and 11 the default text and
        // background.
        let snapshot = snapshot(
            4,
            1,
            "\x1b]4;1;rgb:11/22/33\x07\x1b]10;rgb:44/55/66\x07\x1b]11;#778899\x07\x1b[31ma\x1b[0mb",
        );
        let mut asked = Vec::new();
        let batch = cells(&snapshot, &cell(), 0, 0, None, None, &mut atlas(&mut asked));
        assert_eq!(batch.glyphs[0].color, rgb(0x11, 0x22, 0x33));
        assert_eq!(batch.glyphs[1].color, rgb(0x44, 0x55, 0x66));
        assert_eq!(batch.backgrounds[3], rgb(0x77, 0x88, 0x99));
        assert_eq!(background(&snapshot), rgb(0x77, 0x88, 0x99));
    }

    #[test]
    fn reverse_video_swaps_the_default_colors() {
        // DECSCNM: the default colors swap; a color that a program sets
        // stays.
        let snapshot = snapshot(4, 1, "\x1b[?5ha\x1b[31mb");
        let mut asked = Vec::new();
        let batch = cells(&snapshot, &cell(), 0, 0, None, None, &mut atlas(&mut asked));
        let colors = Colors::default();
        assert_eq!(batch.glyphs[0].color, color(colors.background));
        assert_eq!(batch.backgrounds[0], color(colors.foreground));
        assert_eq!(batch.glyphs[1].color, color(colors.palette[1]));
        assert_eq!(batch.backgrounds[3], color(colors.foreground));
        assert_eq!(background(&snapshot), color(colors.foreground));
    }

    #[test]
    fn faint_text_mixes_halfway_and_hidden_text_shows_nothing() {
        let colors = Colors {
            foreground: Rgb {
                r: 200,
                g: 100,
                b: 0,
            },
            background: Rgb { r: 0, g: 0, b: 100 },
            ..Colors::default()
        };
        let snapshot = themed(4, 1, colors, "\x1b[2ma\x1b[0;8mb\x1b[0mc");
        let mut asked = Vec::new();
        let batch = cells(&snapshot, &cell(), 0, 0, None, None, &mut atlas(&mut asked));
        let texts: Vec<&str> = asked.iter().map(|(text, ..)| text.as_str()).collect();
        assert_eq!(texts, ["a", "c"]);
        assert_eq!(batch.glyphs[0].color, Color::from_rgb8(100, 50, 50));
        // The hidden cell keeps its background.
        assert_eq!(batch.backgrounds[1], Color::from_rgb8(0, 0, 100));
    }

    /// The cursor that `cells` gives `snapshot` in `shape`, and the color
    /// of each glyph by its column.
    fn cursor_of(
        snapshot: &Snapshot,
        shape: CursorShape,
    ) -> (Option<CellCursor>, Vec<(u16, Color)>) {
        let mut asked = Vec::new();
        let look = CursorLook {
            shape,
            thickness: 3,
        };
        let batch = cells(
            snapshot,
            &cell(),
            0,
            0,
            None,
            Some(look),
            &mut atlas(&mut asked),
        );
        let colors = batch
            .glyphs
            .iter()
            .map(|glyph| (glyph.column, glyph.color))
            .collect();
        (batch.cursor, colors)
    }

    #[test]
    fn the_cursor_takes_its_shape_and_the_theme_s_color() {
        let colors = Colors::default();
        let face = cell().underline_thickness;
        assert_ne!(face, 3);
        // A bar and an underline take the look's thickness, a hollow block
        // the face's.
        for (shape, thickness) in [
            (CursorShape::Block, face),
            (CursorShape::Bar, 3),
            (CursorShape::Underline, 3),
            (CursorShape::Hollow, face),
        ] {
            let (cursor, _) = cursor_of(&snapshot(4, 2, "ab"), shape);
            assert_eq!(
                cursor,
                Some(CellCursor {
                    column: 2,
                    row: 0,
                    columns: 1,
                    shape,
                    thickness,
                    color: color(colors.cursor),
                })
            );
        }
        // OSC 12 sets its color.
        let (cursor, _) = cursor_of(&snapshot(4, 2, "\x1b]12;#ff0000\x07"), CursorShape::Bar);
        assert_eq!(cursor.unwrap().color, rgb(255, 0, 0));
    }

    #[test]
    fn the_text_under_a_block_takes_its_cell_s_background() {
        // The cursor back on `b`, whose background is red.
        let text = "a\x1b[41mb\x1b[0mc\x1b[2D";
        let (cursor, glyphs) = cursor_of(&snapshot(4, 1, text), CursorShape::Block);
        assert_eq!(cursor.map(|cursor| cursor.column), Some(1));
        let colors = Colors::default();
        let (ink, red) = (color(colors.foreground), color(colors.palette[1]));
        assert_eq!(glyphs, [(0, ink), (1, red), (2, ink)]);
        // The other shapes leave the text's colors.
        let (_, glyphs) = cursor_of(&snapshot(4, 1, text), CursorShape::Hollow);
        assert_eq!(glyphs, [(0, ink), (1, ink), (2, ink)]);
    }

    #[test]
    fn the_cursor_covers_a_wide_character_from_either_cell() {
        // On the character, and on its second cell.
        for text in ["\u{4E2D}\x1b[2D", "\u{4E2D}\x1b[D"] {
            let (cursor, _) = cursor_of(&snapshot(4, 1, text), CursorShape::Block);
            let cursor = cursor.unwrap();
            assert_eq!((cursor.column, cursor.columns), (0, 2), "{text:?}");
        }
    }

    #[test]
    fn a_hidden_cursor_draws_nothing() {
        let (cursor, _) = cursor_of(&snapshot(4, 1, "a\x1b[?25l"), CursorShape::Block);
        assert_eq!(cursor, None);
        // Nor does one that the blinking hides.
        let mut asked = Vec::new();
        let batch = cells(
            &snapshot(4, 1, "a"),
            &cell(),
            0,
            0,
            None,
            None,
            &mut atlas(&mut asked),
        );
        assert_eq!(batch.cursor, None);
    }

    #[test]
    fn the_composed_text_shows_at_the_cursor_underlined() {
        // The cursor back on `c`, in a grid of 8 columns.
        let snapshot = snapshot(8, 2, "abcdef\x1b[4D");
        let mut asked = Vec::new();
        let look = CursorLook {
            shape: CursorShape::Bar,
            thickness: 2,
        };
        let mut batch = cells(
            &snapshot,
            &cell(),
            0,
            0,
            None,
            Some(look),
            &mut atlas(&mut asked),
        );
        let composed = Preedit {
            text: "にほ".to_owned(),
            cursor: 3,
        };
        preedit(
            &mut batch,
            &snapshot,
            &cell(),
            &composed,
            &mut atlas(&mut asked),
        );
        let row: Vec<(u16, bool)> = batch
            .glyphs
            .iter()
            .filter(|glyph| glyph.row == 0)
            .map(|glyph| {
                (
                    glyph.column,
                    glyph.region.x == u32::from('に') || glyph.region.x == u32::from('ほ'),
                )
            })
            .collect();
        // `a` and `b`, then the two wide characters over `c` to `f`.
        assert_eq!(row, [(0, false), (1, false), (2, true), (4, true)]);
        let line = batch.lines.last().unwrap();
        assert_eq!((line.column, line.row, line.columns), (2, 0, 4));
        assert_eq!(line.top, cell().underline_top);
        // The input method's cursor, after `に`.
        assert_eq!(batch.cursor.map(|cursor| cursor.column), Some(4));
        let colors = Colors::default();
        assert_eq!(batch.backgrounds[2], color(colors.background));
    }

    /// The batch of `text` on a grid of `cols` by one row, with the cursor
    /// of `shape`.
    fn with_cursor(cols: u16, text: &str, shape: CursorShape) -> CellBatch {
        let mut asked = Vec::new();
        let look = CursorLook {
            shape,
            thickness: 2,
        };
        cells(
            &snapshot(cols, 1, text),
            &cell(),
            0,
            0,
            None,
            Some(look),
            &mut atlas(&mut asked),
        )
    }

    fn glyph_color(batch: &CellBatch, column: u16) -> Option<Color> {
        batch
            .glyphs
            .iter()
            .find(|glyph| glyph.column == column)
            .map(|glyph| glyph.color)
    }

    #[test]
    fn a_block_on_inverse_text_stays_apart_from_its_cell() {
        // `b` in inverse, whose background is the cursor's color.
        let batch = with_cursor(4, "a\x1b[7mb\x1b[0mc\x1b[2D", CursorShape::Block);
        let colors = Colors::default();
        let (text, background) = (color(colors.foreground), color(colors.background));
        let cursor = batch.cursor.unwrap();
        assert_eq!((cursor.column, batch.backgrounds[1]), (1, text));
        // The block takes the cell's text color, and its text the cell's
        // background.
        assert_eq!(cursor.color, background);
        assert_eq!(glyph_color(&batch, 1), Some(text));
        // On a plain cell, the cursor's own color.
        let batch = with_cursor(4, "ab\x1b[D", CursorShape::Block);
        assert_eq!(batch.cursor.unwrap().color, color(colors.cursor));
        assert_eq!(glyph_color(&batch, 1), Some(background));
    }

    #[test]
    fn every_shape_shows_in_reverse_video() {
        let colors = Colors::default();
        let dark = color(colors.background);
        for shape in [
            CursorShape::Bar,
            CursorShape::Underline,
            CursorShape::Hollow,
        ] {
            // On a character, and past the text.
            for text in ["\x1b[?5hab\x1b[D", "\x1b[?5hab"] {
                let batch = with_cursor(4, text, shape);
                let cursor = batch.cursor.unwrap();
                let under = batch.backgrounds[usize::from(cursor.column)];
                assert_eq!(cursor.color, dark, "{shape:?} {text:?}");
                assert_ne!(cursor.color, under);
            }
        }
    }

    #[test]
    fn a_cell_of_one_color_takes_black_or_white() {
        // Text and background both of the cursor's light gray.
        let gray = "\x1b[38;2;229;229;229;48;2;229;229;229mx\x1b[D";
        let batch = with_cursor(4, gray, CursorShape::Block);
        assert_eq!(batch.cursor.unwrap().color, Color::from_rgb8(0, 0, 0));
    }

    #[test]
    fn a_block_under_a_composition_leaves_its_text_readable() {
        let look = CursorLook {
            shape: CursorShape::Block,
            thickness: 2,
        };
        let compose = |text: &str, composed: &str, cursor: usize| {
            let snapshot = snapshot(8, 1, text);
            let mut asked = Vec::new();
            let mut batch = cells(
                &snapshot,
                &cell(),
                0,
                0,
                None,
                Some(look),
                &mut atlas(&mut asked),
            );
            let composed = Preedit {
                text: composed.to_owned(),
                cursor,
            };
            preedit(
                &mut batch,
                &snapshot,
                &cell(),
                &composed,
                &mut atlas(&mut asked),
            );
            batch
        };
        let background = color(Colors::default().background);
        // The cursor after `x`, on the `d` that follows the composition.
        let batch = compose("abcdef\x1b[4D", "x", 1);
        let cursor = batch.cursor.unwrap();
        assert_eq!((cursor.column, cursor.columns), (3, 1));
        assert_eq!(glyph_color(&batch, 3), Some(background));
        // On a wide composed character, which it covers whole.
        let batch = compose("ab", "日本", 3);
        let cursor = batch.cursor.unwrap();
        assert_eq!((cursor.column, cursor.columns), (4, 2));
        assert_eq!(glyph_color(&batch, 4), Some(background));
        assert_eq!(
            glyph_color(&batch, 2),
            Some(color(Colors::default().foreground))
        );
    }

    #[test]
    fn a_composition_from_a_wide_character_s_second_cell_starts_on_its_first() {
        // `中`, and the cursor on its second cell.
        let snapshot = snapshot(4, 1, "\u{4E2D}\x1b[2G");
        assert_eq!(snapshot.cursor().position.x, 1);
        let mut asked = Vec::new();
        let mut batch = cells(&snapshot, &cell(), 0, 0, None, None, &mut atlas(&mut asked));
        let composed = Preedit {
            text: "x".to_owned(),
            cursor: 1,
        };
        preedit(
            &mut batch,
            &snapshot,
            &cell(),
            &composed,
            &mut atlas(&mut asked),
        );
        let row: Vec<u16> = batch.glyphs.iter().map(|glyph| glyph.column).collect();
        assert_eq!(row, [0]);
        assert_eq!(batch.glyphs[0].region.x, u32::from('x'));
        let line = batch.lines.last().unwrap();
        assert_eq!((line.column, line.columns), (0, 1));
    }

    #[test]
    fn composed_text_past_the_right_edge_is_left_out() {
        let snapshot = snapshot(4, 1, "ab");
        let mut asked = Vec::new();
        let mut batch = cells(&snapshot, &cell(), 0, 0, None, None, &mut atlas(&mut asked));
        let composed = Preedit {
            text: "日本語".to_owned(),
            cursor: 9,
        };
        preedit(
            &mut batch,
            &snapshot,
            &cell(),
            &composed,
            &mut atlas(&mut asked),
        );
        let line = batch.lines.last().unwrap();
        assert_eq!((line.column, line.columns), (2, 2));
        assert_eq!(batch.glyphs.len(), 3);
    }

    #[test]
    fn the_selection_takes_its_colors() {
        let snapshot = snapshot(6, 2, "\x1b[41mab\x1b[0mcd\r\nef");
        let colors = SelectionColors {
            background: rgb(1, 2, 3),
            foreground: Some(rgb(4, 5, 6)),
        };
        // The second and third cells of the first row, and the whole second
        // row, past its text too.
        let contains = |column: u16, row: u16| (row == 0 && (1..3).contains(&column)) || row == 1;
        let selection = Selection {
            colors,
            contains: &contains,
        };
        let mut asked = Vec::new();
        let batch = cells(
            &snapshot,
            &cell(),
            0,
            0,
            Some(&selection),
            None,
            &mut atlas(&mut asked),
        );
        let red = color(Colors::default().palette[1]);
        assert_eq!(
            batch.backgrounds[..4],
            [
                red,
                rgb(1, 2, 3),
                rgb(1, 2, 3),
                color(Colors::default().background)
            ]
        );
        assert!(batch.backgrounds[6..].iter().all(|&bg| bg == rgb(1, 2, 3)));
        let colored: Vec<(u16, u16, Color)> = batch
            .glyphs
            .iter()
            .map(|glyph| (glyph.column, glyph.row, glyph.color))
            .collect();
        let text = color(Colors::default().foreground);
        assert_eq!(
            colored,
            [
                (0, 0, text),
                (1, 0, rgb(4, 5, 6)),
                (2, 0, rgb(4, 5, 6)),
                (3, 0, text),
                (0, 1, rgb(4, 5, 6)),
                (1, 1, rgb(4, 5, 6)),
            ]
        );
    }
}
