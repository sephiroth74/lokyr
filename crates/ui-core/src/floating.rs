//! A floating pane's room over its tab's split panes, and its frame: a
//! rounded box of the terminal's background, of a border and a shadow,
//! under a thin header of the pane's title, its grid within the box below
//! the header.

use lokyr_render_core::{Color, Item, PlacedGlyph, Rect, Shadow, Shape};
use lokyr_text_core::CellMetrics;
use lokyr_vt_core::page::Style;

use crate::pane::{AtlasGlyph, CellText};

/// The radius of the box's corners, in points.
const RADIUS: f64 = 8.0;

/// The room between the box's edges and the grid, at its sides and its
/// bottom, in points, past its corners' curve.
const INSET: f64 = 6.0;

/// The room above and below the header's title, in points.
const HEADER_MARGIN: f64 = 3.0;

/// A floating pane's room in its window, in pixels from the content's top
/// left corner: its box, its header across the box's top, and its grid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Room {
    /// Its box, the border's outer edges.
    pub outer: Rect,
    /// Its header, across the box's top.
    pub header: Rect,
    /// Its grid's room, of whole cells from its top left corner.
    pub grid: Rect,
}

/// A floating pane's frame over its tab's room: its top left corner and its
/// size, each a share of the room, from 0 to 1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shares {
    /// Its corner's share across.
    pub x: f64,
    /// Its corner's share down.
    pub y: f64,
    /// Its share across.
    pub width: f64,
    /// Its share down.
    pub height: f64,
}

/// The room of a floating pane of the frame `frame` over `content`, the
/// tab's room, of cells of `cell` pixels, on a display of `scale`: its box
/// at its shares of the room, of a cell at least each way past its header
/// and insets, within the room where it fits.
pub fn room(frame: Shares, content: Rect, cell: (u32, u32), scale: f64) -> Room {
    let inset = (INSET * scale).round() as u32;
    let header_height = cell.1 + 2 * (HEADER_MARGIN * scale).round() as u32;
    let (room_width, room_height) = (f64::from(content.width), f64::from(content.height));
    // Of a cell's room at least past its frame, within the tab's room.
    let width = ((frame.width * room_width).round() as u32)
        .max(cell.0 + 2 * inset)
        .min(content.width);
    let height = ((frame.height * room_height).round() as u32)
        .max(cell.1 + header_height + inset)
        .min(content.height);
    let place = |at: f64, room: f64, size: u32, edge: i32| {
        let most = (room - f64::from(size)).max(0.0);
        edge + (at * room).round().clamp(0.0, most) as i32
    };
    let outer = Rect {
        x: place(frame.x, room_width, width, content.x),
        y: place(frame.y, room_height, height, content.y),
        width,
        height,
    };
    let side = i32::try_from(inset).unwrap_or(0);
    let header = Rect {
        height: header_height.min(height),
        ..outer
    };
    let grid = Rect {
        x: outer.x + side,
        y: outer.y + i32::try_from(header.height).unwrap_or(0),
        width: width.saturating_sub(2 * inset),
        height: height.saturating_sub(header.height + inset),
    };
    Room {
        outer,
        header,
        grid,
    }
}

/// The colors of a floating pane's frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Look {
    /// The box's background, the terminal's.
    pub fill: Color,
    /// Its border.
    pub border: Color,
    /// The line under its header.
    pub separator: Color,
    /// Its shadow's, if it casts one.
    pub shadow: Option<Color>,
    /// Its title's.
    pub title: Color,
}

/// The drawings of `room`'s frame, under its grid's cells: its box, the
/// line under its header, and its `title`, of glyphs of `cell`'s size that
/// `glyph` gives, a cell a character, cut with an ellipsis where it would
/// pass the header, on a display of `scale`.
pub fn frame(
    room: &Room,
    title: &str,
    look: Look,
    cell: &CellMetrics,
    scale: f64,
    mut glyph: impl FnMut(&CellText<'_>) -> Option<AtlasGlyph>,
) -> Vec<Item> {
    let border = scale.round().max(1.0) as f32;
    let shape = Shape {
        rect: room.outer,
        radius: (RADIUS * scale) as f32,
        fill: look.fill,
        border,
        border_color: look.border,
        shadow: look.shadow.map(|color| Shadow {
            x: 0.0,
            y: (4.0 * scale) as f32,
            blur: (16.0 * scale) as f32,
            color,
        }),
        arc: None,
        knockout: false,
        mark: None,
    };
    let line = Rect {
        x: room.outer.x + border as i32,
        y: room.header.y + i32::try_from(room.header.height).unwrap_or(0) - border as i32,
        width: room.outer.width.saturating_sub(2 * border as u32),
        height: border as u32,
    };
    // The title, from the grid's left edge, of the columns that the header
    // holds.
    let left = room.grid.x;
    let columns = (room.grid.width / cell.width.max(1)) as usize;
    let mut shown: Vec<char> = title.chars().collect();
    if shown.len() > columns {
        shown.truncate(columns.saturating_sub(1));
        shown.push('…');
    }
    let style = Style::default();
    let mut buffer = [0; 4];
    let margin = (room.header.height.saturating_sub(cell.height)) / 2;
    let baseline = room.header.y + i32::try_from(margin + cell.baseline).unwrap_or(0);
    let mut glyphs = Vec::new();
    for (column, character) in (0..).zip(shown) {
        if character == ' ' {
            continue;
        }
        let text = CellText {
            text: character.encode_utf8(&mut buffer),
            style: &style,
            wide: false,
            dark: true,
        };
        let Some(found) = glyph(&text) else {
            continue;
        };
        let pen = left + i32::try_from(column * cell.width).unwrap_or(i32::MAX);
        glyphs.push(PlacedGlyph {
            region: found.region,
            x: pen + found.left,
            y: baseline - found.top,
            color: look.title,
            weight: found.smoothed,
        });
    }
    vec![
        Item::Shapes(vec![shape]),
        Item::Rect {
            rect: line,
            color: look.separator,
        },
        Item::Glyphs(glyphs),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shares(x: f64, y: f64, width: f64, height: f64) -> Shares {
        Shares {
            x,
            y,
            width,
            height,
        }
    }

    #[test]
    fn a_floating_pane_s_room_takes_its_shares_of_the_tab_within_it() {
        let content = Rect {
            x: 10,
            y: 20,
            width: 1000,
            height: 600,
        };
        // Half and half, in the middle, at a scale of 2, of cells of 16 by
        // 32: an inset of 12 and a header of a cell and two margins of 6.
        let room = room(shares(0.25, 0.25, 0.5, 0.5), content, (16, 32), 2.0);
        assert_eq!(
            room.outer,
            Rect {
                x: 260,
                y: 170,
                width: 500,
                height: 300
            }
        );
        assert_eq!(room.header.height, 44);
        assert_eq!(
            room.grid,
            Rect {
                x: 272,
                y: 214,
                width: 476,
                height: 244
            }
        );
        // The least frame: a cell past its header and insets, within the
        // room.
        let least = super::room(shares(0.95, 0.95, 0.05, 0.05), content, (16, 32), 2.0);
        assert!(least.grid.width >= 16, "{least:?}");
        assert_eq!(least.grid.height, 32);
        assert!(least.outer.x + least.outer.width as i32 <= 1010);
        assert!(least.outer.y + least.outer.height as i32 <= 620);
    }
}
