//! The label of a window's grid as the window resizes: its columns and its
//! rows, as `100 × 33`, at the centre of its content, until a moment after
//! the last change.

use std::time::Duration;

use lokyr_render_core::{Color, Item, PlacedGlyph, Rect};
use lokyr_text_core::CellMetrics;
use lokyr_ui_core::pane::{AtlasGlyph, CellText};
use lokyr_vt_core::page::Style;

/// How long the label stays after the grid's last change, or after the end
/// of a live resize.
pub(crate) const HOLD: Duration = Duration::from_secs(1);

/// The label's background, a dark gray that lets a little of the window
/// through, and its text.
const BACKGROUND: Color = Color {
    red: 0.08,
    green: 0.08,
    blue: 0.09,
    alpha: 0.85,
};
const TEXT: Color = Color {
    red: 0.95,
    green: 0.95,
    blue: 0.95,
    alpha: 1.0,
};

/// The label's text of a grid of `columns` and `rows`.
pub(crate) fn text(columns: u16, rows: u16) -> String {
    format!("{columns} × {rows}")
}

/// The drawings of the label `text` at the centre of a window's content of
/// `width` by `height` pixels, whose part under the titlebar starts `top`
/// pixels down: a background, then the glyphs of `cell`'s size, which
/// `glyph` gives, a cell a character.
pub(crate) fn label(
    text: &str,
    cell: &CellMetrics,
    (width, height, top): (u32, u32, u32),
    mut glyph: impl FnMut(&CellText<'_>) -> Option<AtlasGlyph>,
) -> Vec<Item> {
    let columns = u32::try_from(text.chars().count()).unwrap_or(u32::MAX);
    // The text's room from the label's edges: a cell across, half of one
    // down.
    let (across, down) = (cell.width, cell.height / 2);
    let label_width = columns.saturating_mul(cell.width) + 2 * across;
    let label_height = cell.height + 2 * down;
    let x = width.saturating_sub(label_width) / 2;
    let below = height.saturating_sub(top);
    let y = top + below.saturating_sub(label_height) / 2;
    let style = Style::default();
    let mut glyphs = Vec::new();
    let mut buffer = [0; 4];
    for (column, character) in (0..).zip(text.chars()) {
        if character == ' ' {
            continue;
        }
        let shown = CellText {
            text: character.encode_utf8(&mut buffer),
            style: &style,
            wide: false,
            dark: true,
        };
        let Some(found) = glyph(&shown) else {
            continue;
        };
        let pen = x + across + column * cell.width;
        let baseline = y + down + cell.baseline;
        glyphs.push(PlacedGlyph {
            region: found.region,
            x: i32::try_from(pen).unwrap_or(i32::MAX) + found.left,
            y: i32::try_from(baseline).unwrap_or(i32::MAX) - found.top,
            color: TEXT,
            weight: found.smoothed,
        });
    }
    vec![
        Item::Rect {
            rect: Rect {
                x: i32::try_from(x).unwrap_or(i32::MAX),
                y: i32::try_from(y).unwrap_or(i32::MAX),
                width: label_width,
                height: label_height,
            },
            color: BACKGROUND,
        },
        Item::Glyphs(glyphs),
    ]
}

#[cfg(test)]
mod tests {
    use lokyr_render_core::{AtlasRegion, Texture};

    use super::*;

    #[test]
    fn the_label_tells_the_grid_at_the_content_s_centre() {
        assert_eq!(text(100, 33), "100 × 33");
        let cell = CellMetrics {
            width: 10,
            height: 20,
            baseline: 16,
            underline_top: 18,
            underline_thickness: 1,
            strikeout_top: 10,
            strikeout_thickness: 1,
        };
        let mut asked = Vec::new();
        let glyph = |shown: &CellText<'_>| {
            asked.push(shown.text.to_owned());
            Some(AtlasGlyph {
                region: AtlasRegion {
                    texture: Texture::Coverage,
                    x: 0,
                    y: 0,
                    width: 6,
                    height: 10,
                },
                left: 1,
                top: 10,
                smoothed: false,
            })
        };
        // A window of 400 by 300 pixels, its content 40 down: seven
        // columns of text, and a cell's room on each side, 90 wide; a line
        // and half a cell above and below, 40 high.
        let items = label("80 × 24", &cell, (400, 300, 40), glyph);
        assert_eq!(asked, ["8", "0", "×", "2", "4"]);
        let [Item::Rect { rect, color }, Item::Glyphs(glyphs)] = &items[..] else {
            panic!("a background and glyphs: {items:?}");
        };
        assert_eq!(*color, BACKGROUND);
        assert_eq!(
            (rect.x, rect.y, rect.width, rect.height),
            (155, 40 + 110, 90, 40)
        );
        // The first glyph, a cell in, on the line's baseline.
        assert_eq!(
            (glyphs[0].x, glyphs[0].y),
            (155 + 10 + 1, 150 + 10 + 16 - 10)
        );
    }
}
