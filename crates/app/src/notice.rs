//! The notice bar: a diagnostic that the user can act on, as a config that
//! does not read, across the top of a window's content, under its titlebar,
//! over the grid's first rows, until a fix or a click closes it.

use lokyr_render_core::{Color, Item, PlacedGlyph, Rect};
use lokyr_text_core::CellMetrics;
use lokyr_ui_core::pane::{AtlasGlyph, CellText};
use lokyr_vt_core::page::Style;
use lokyr_vt_core::unicode::Widths;

/// The most lines that a notice takes; the rest of its text, cut, ends
/// with an ellipsis.
const LINES: usize = 3;

/// The bar's background, a dark amber, or a dark red for a fault; and its
/// text.
const BACKGROUND: Color = Color {
    red: 0.29,
    green: 0.20,
    blue: 0.04,
    alpha: 0.95,
};
const FAULT: Color = Color {
    red: 0.36,
    green: 0.07,
    blue: 0.07,
    alpha: 0.95,
};
const TEXT: Color = Color {
    red: 0.96,
    green: 0.94,
    blue: 0.90,
    alpha: 1.0,
};

/// What closes the notice, at the end of its first line.
const CLOSE: char = '×';

/// `text` in lines of `columns` cells at most, broken between words, and a
/// word longer than a line within it; `LINES` at most, the last ending
/// with `…` where the text goes on.
fn lines(text: &str, columns: usize) -> Vec<String> {
    let widths = Widths::default();
    let cells = |character: char| usize::from(widths.char_width(character));
    let columns = columns.max(2);
    let mut lines = Vec::new();
    let (mut line, mut used) = (String::new(), 0);
    for word in text.split_whitespace() {
        let wide: usize = word.chars().map(cells).sum();
        // A word that fits goes on, after a space; another starts a line.
        if used > 0 && used + 1 + wide > columns {
            lines.push(std::mem::take(&mut line));
            used = 0;
        }
        if used > 0 {
            line.push(' ');
            used += 1;
        }
        // A word longer than a line goes on in the next ones.
        for character in word.chars() {
            if used > 0 && used + cells(character) > columns {
                lines.push(std::mem::take(&mut line));
                used = 0;
            }
            line.push(character);
            used += cells(character);
        }
    }
    if !line.is_empty() {
        lines.push(line);
    }
    if lines.len() > LINES {
        lines.truncate(LINES);
        let last = &mut lines[LINES - 1];
        let mut used: usize = last.chars().map(cells).sum();
        while used + 1 > columns {
            match last.pop() {
                Some(character) => used -= cells(character),
                None => break,
            }
        }
        last.push('…');
    }
    lines
}

/// The bar of the notice `text`, which `label` opens, a fault's when
/// `fault`, over a window of `width` pixels, whose content starts `top`
/// pixels down, under its titlebar: its rectangle, which a click closes,
/// and its drawings, a background and the glyphs of `cell`'s size, which
/// `glyph` gives, a cell a character.
pub(crate) fn bar(
    (label, text, fault): (&str, &str, bool),
    cell: &CellMetrics,
    (width, top): (u32, u32),
    mut glyph: impl FnMut(&CellText<'_>) -> Option<AtlasGlyph>,
) -> (Rect, Vec<Item>) {
    // The text's room from the bar's edges, half a cell, and the close's,
    // the last cell, after one left empty.
    let padding = cell.width / 2;
    let cells = width.saturating_sub(2 * padding) / cell.width.max(1);
    let columns = usize::try_from(cells.saturating_sub(2)).unwrap_or(0);
    let shown = lines(&format!("{label} {text}"), columns);
    let rows = u32::try_from(shown.len()).unwrap_or(u32::MAX);
    let rect = Rect {
        x: 0,
        y: i32::try_from(top).unwrap_or(i32::MAX),
        width,
        height: rows.saturating_mul(cell.height) + 2 * padding,
    };
    let style = Style::default();
    let widths = Widths::default();
    let mut glyphs = Vec::new();
    let mut place = |character: char, column: u32, row: u32| {
        let mut text = [0; 4];
        let wide = widths.char_width(character) == 2;
        let shown = CellText {
            text: character.encode_utf8(&mut text),
            style: &style,
            wide,
            dark: true,
        };
        if let Some(found) = glyph(&shown) {
            let pen = padding + column * cell.width;
            let baseline = top + padding + row * cell.height + cell.baseline;
            glyphs.push(PlacedGlyph {
                region: found.region,
                x: i32::try_from(pen).unwrap_or(i32::MAX) + found.left,
                y: i32::try_from(baseline).unwrap_or(i32::MAX) - found.top,
                color: TEXT,
            });
        }
    };
    for (row, line) in (0..).zip(&shown) {
        let mut column = 0;
        for character in line.chars() {
            if character != ' ' {
                place(character, column, row);
            }
            column += u32::from(widths.char_width(character));
        }
    }
    place(CLOSE, cells.saturating_sub(1), 0);
    let background = if fault { FAULT } else { BACKGROUND };
    (
        rect,
        vec![
            Item::Rect {
                rect,
                color: background,
            },
            Item::Glyphs(glyphs),
        ],
    )
}

/// Whether the point `(x, y)`, in pixels of the content, is in `rect`.
pub(crate) fn contains(rect: &Rect, (x, y): (f64, f64)) -> bool {
    let (left, top) = (f64::from(rect.x), f64::from(rect.y));
    (left..left + f64::from(rect.width)).contains(&x)
        && (top..top + f64::from(rect.height)).contains(&y)
}

#[cfg(test)]
mod tests {
    use lokyr_render_core::{AtlasRegion, Texture};

    use super::*;

    #[test]
    fn a_notice_s_text_breaks_between_words_and_ends_cut() {
        assert_eq!(lines("one two three", 9), ["one two", "three"]);
        // A word longer than a line goes on in the next.
        assert_eq!(lines("abcdefghij", 4), ["abcd", "efgh", "ij"]);
        // Wide characters take two cells each.
        assert_eq!(lines("日本語 x", 4), ["日本", "語 x"]);
        // Three lines at most, the last cut with an ellipsis.
        let long = "aa bb cc dd ee ff gg";
        assert_eq!(lines(long, 5), ["aa bb", "cc dd", "ee f…"]);
        assert_eq!(lines("", 10), [] as [String; 0]);
    }

    #[test]
    fn the_bar_spans_the_window_under_its_titlebar() {
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
            })
        };
        // A window of 21 cells, 20 within a padding of half a cell: 18
        // columns of text, past the close's two.
        let (rect, items) = bar(("Error:", "a b", false), &cell, (210, 56), glyph);
        assert_eq!((rect.x, rect.y, rect.width, rect.height), (0, 56, 210, 30));
        assert_eq!(asked, ["E", "r", "r", "o", "r", ":", "a", "b", "×"]);
        let [Item::Rect { color, .. }, Item::Glyphs(glyphs)] = &items[..] else {
            panic!("a background and glyphs: {items:?}");
        };
        assert_eq!(*color, BACKGROUND);
        // The first glyph, at the padding, on the first line's baseline.
        assert_eq!((glyphs[0].x, glyphs[0].y), (5 + 1, 56 + 5 + 16 - 10));
        // The close, in the last cell of the first line.
        assert_eq!(glyphs.last().unwrap().x, 5 + 190 + 1);
        assert!(contains(&rect, (0.0, 56.0)) && contains(&rect, (209.0, 85.0)));
        assert!(!contains(&rect, (10.0, 55.0)) && !contains(&rect, (10.0, 86.0)));
    }
}
