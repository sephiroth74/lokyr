//! A pane's grid of cells, as a display list draws it, from a snapshot of
//! its terminal.

use bolt_render_core::{AtlasRegion, CellBatch, CellGlyph, Color};
use bolt_text_core::CellMetrics;
use bolt_vt_core::page::{self, Content, Rgb, Style, Width};
use bolt_vt_core::terminal::{Colors, Snapshot};

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

/// The grid of `snapshot`, of cells of `cell`, with its top left corner at
/// `x` and `y`: each cell's background, and the glyph of each cell that
/// shows a character, which `glyph` finds, through the fonts and the atlas,
/// or not.
///
/// A character that takes two columns has its glyph in the first, which
/// spans the two; the second has only its background.
pub fn cells(
    snapshot: &Snapshot,
    cell: &CellMetrics,
    x: i32,
    y: i32,
    glyph: &mut dyn FnMut(&CellText<'_>) -> Option<AtlasGlyph>,
) -> CellBatch {
    let (columns, rows) = (snapshot.cols(), snapshot.rows());
    let colors = snapshot.colors();
    let mut batch = CellBatch {
        x,
        y,
        columns,
        rows,
        cell_width: cell.width,
        cell_height: cell.height,
        backgrounds: Vec::with_capacity(usize::from(columns) * usize::from(rows)),
        glyphs: Vec::new(),
        lines: Vec::new(),
    };
    let mut text = String::new();
    for row in 0..rows {
        let line = snapshot.line(row);
        for (column, &cell_of) in (0..columns).zip(line.cells()) {
            let style = line.style(cell_of.style());
            let (foreground, background) = style_colors(&style, colors);
            let character = match cell_of.content() {
                Content::Char(character) | Content::Grapheme(character) => character,
                Content::BgPalette(index) => {
                    batch
                        .backgrounds
                        .push(color(colors.palette[usize::from(index)]));
                    continue;
                }
                Content::BgRgb(rgb) => {
                    batch.backgrounds.push(color(rgb));
                    continue;
                }
            };
            batch.backgrounds.push(background);
            let width = cell_of.width();
            if character == '\0' || matches!(width, Width::SpacerTail | Width::SpacerHead) {
                continue;
            }
            text.clear();
            text.push(character);
            text.extend(line.grapheme(column));
            let shown = CellText {
                text: &text,
                style: &style,
                wide: width == Width::Wide,
            };
            if let Some(found) = glyph(&shown) {
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
        let missing = usize::from(columns).saturating_sub(line.cells().len());
        batch
            .backgrounds
            .extend(std::iter::repeat_n(color(colors.background), missing));
    }
    batch
}

/// The text's and the background's colors of `style`, from `colors`, and
/// swapped for inverse text.
fn style_colors(style: &Style, colors: &Colors) -> (Color, Color) {
    let resolve = |which: page::Color, default: Rgb| match which {
        page::Color::Default => color(default),
        page::Color::Palette(index) => color(colors.palette[usize::from(index)]),
        page::Color::Rgb(rgb) => color(rgb),
    };
    let foreground = resolve(style.fg, colors.foreground);
    let background = resolve(style.bg, colors.background);
    if style.inverse {
        (background, foreground)
    } else {
        (foreground, background)
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
    /// that records what it was asked for.
    fn atlas(
        asked: &mut Vec<(String, bool, bool)>,
    ) -> impl FnMut(&CellText<'_>) -> Option<AtlasGlyph> + '_ {
        move |shown: &CellText<'_>| {
            asked.push((shown.text.to_owned(), shown.style.bold, shown.wide));
            let first = shown.text.chars().next()? as u32;
            Some(AtlasGlyph {
                region: AtlasRegion {
                    texture: Texture::Coverage,
                    x: first,
                    y: 0,
                    width: 6,
                    height: 10,
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
        let batch = cells(&snapshot, &cell(), 4, 30, &mut atlas(&mut asked));
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
        let batch = cells(&snapshot, &cell(), 0, 0, &mut atlas(&mut asked));
        let columns: Vec<u16> = batch.glyphs.iter().map(|g| g.column).collect();
        assert_eq!(columns, [0, 1, 3]);
        assert_eq!(asked[1], ("日".to_owned(), false, true));
        assert_eq!(batch.backgrounds.len(), 6);
    }

    #[test]
    fn a_cluster_comes_whole() {
        let snapshot = snapshot(6, 1, "e\u{301}👍🏽");
        let mut asked = Vec::new();
        cells(&snapshot, &cell(), 0, 0, &mut atlas(&mut asked));
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
        let batch = cells(&snapshot, &cell(), 0, 0, &mut atlas(&mut asked));
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
        let batch = cells(&snapshot, &cell(), 0, 0, &mut atlas(&mut asked));
        let red = color(Colors::default().palette[1]);
        assert!(batch.backgrounds.iter().all(|&bg| bg == red));
        assert!(batch.glyphs.is_empty() && asked.is_empty());
    }

    #[test]
    fn a_glyph_that_the_fonts_lack_leaves_its_background() {
        let snapshot = snapshot(3, 1, "abc");
        let batch = cells(&snapshot, &cell(), 0, 0, &mut |_: &CellText<'_>| None);
        assert!(batch.glyphs.is_empty());
        assert_eq!(batch.backgrounds.len(), 3);
    }
}
