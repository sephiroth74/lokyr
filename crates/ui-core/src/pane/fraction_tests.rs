//! The rows of a view scrolled a share of a row past its first row: a
//! share of a row up, the row past the view's last below them, within the
//! band of the grid's rows.

use lokyr_render_core::{AtlasRegion, Item, Rect, Texture};
use lokyr_text_core::{FaceMetrics, FontConfig};
use lokyr_vt_core::parser::Parser;
use lokyr_vt_core::terminal::{Options, Scroll, Snapshot, Terminal};

use super::*;

/// A cell of 8 by 16 pixels.
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

/// A glyph of ink for every character but a space.
fn glyph(shown: &CellText<'_>) -> Option<AtlasGlyph> {
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
        smoothed: false,
    })
}

/// A terminal of 20 columns and 4 rows of the commands' blocks of `seq 1
/// 20`, then of `false`, which failed, then the prompt's row.
fn terminal() -> Terminal {
    let mut terminal = Terminal::new(20, 4, Options::default());
    let mut text = String::from("\x1b]133;A\x07$ \x1b]133;B\x07seq 1 20\r\n\x1b]133;C\x07");
    for at in 1..=20 {
        text.push_str(&format!("{at}\r\n"));
    }
    text.push_str("\x1b]133;D;0\x07\x1b]133;A\x07$ \x1b]133;B\x07false\r\n\x1b]133;C\x07");
    text.push_str("\x1b]133;D;1\x07\x1b]133;A\x07$ \x1b]133;B\x07");
    Parser::new().advance(&mut terminal, text.as_bytes());
    terminal
}

fn snapshot(terminal: &mut Terminal) -> Snapshot {
    let mut snapshot = Snapshot::default();
    terminal.snapshot(&mut snapshot);
    snapshot
}

/// The rows of the glyphs of `batch`, each once.
fn glyph_rows(batch: &CellBatch) -> Vec<u16> {
    let mut rows: Vec<u16> = batch.glyphs.iter().map(|glyph| glyph.row).collect();
    rows.dedup();
    rows
}

#[test]
fn of_no_share_the_rows_draw_in_their_place_and_nothing_is_cut() {
    let mut terminal = terminal();
    // At the screen, and scrolled by whole rows.
    for scroll in [None, Some(Scroll::Up(3))] {
        if let Some(scroll) = scroll {
            terminal.scroll_viewport(scroll);
        }
        let shown = snapshot(&mut terminal);
        let rows = rows_at(&shown, &cell(), (10, 20), (4, 168));
        assert_eq!(rows, Rows::default());
        assert_eq!(rows_up(&shown, 16), 0);
        assert_eq!(rows.corner((10, 20)), (10, 20));
        let items = vec![Item::Cells(CellBatch::default())];
        assert_eq!(rows.clipped(items.clone()), items);
        assert_eq!(rows.layer((10.0, 4.0), (8, 16)), Shows::All);
        let mut glyphs = glyph;
        let batch = cells_with(
            &shown,
            &cell(),
            (10, 20),
            None,
            None,
            TextLook::default(),
            &mut glyphs,
        );
        assert_eq!((batch.y, batch.rows), (20, 4));
    }
}

#[test]
fn a_share_of_a_row_draws_the_rows_up_and_the_row_below_within_the_grid_s_band() {
    let mut terminal = terminal();
    // The row of `10` first, a quarter of it past: four pixels of 16.
    terminal.scroll_viewport(Scroll::Row(10));
    terminal.scroll_viewport_by(-0.25);
    let shown = snapshot(&mut terminal);
    let rows = rows_at(&shown, &cell(), (10, 20), (4, 168));
    let band = Rect {
        x: 4,
        y: 20,
        width: 168,
        height: 4 * 16,
    };
    assert_eq!(
        rows,
        Rows {
            up: 4,
            clip: Some(band)
        }
    );
    assert_eq!(rows.corner((10, 20)), (10, 16));
    // What is drawn on them within the band; nothing of nothing.
    let cells = Item::Cells(CellBatch::default());
    assert_eq!(
        rows.clipped(vec![cells.clone()]),
        [Item::PushClip(band), cells, Item::PopClip]
    );
    assert!(rows.clipped(Vec::new()).is_empty());
    // The view's four rows and the row below, from the corner up.
    let mut glyphs = glyph;
    let batch = cells_with(
        &shown,
        &cell(),
        rows.corner((10, 20)),
        None,
        None,
        TextLook::default(),
        &mut glyphs,
    );
    assert_eq!((batch.y, batch.rows), (16, 5));
    assert_eq!(glyph_rows(&batch), [0, 1, 2, 3, 4]);
    // Half a row of 15 pixels rounds to whole pixels.
    terminal.scroll_viewport_by(-0.25);
    let shown = snapshot(&mut terminal);
    assert_eq!(rows_up(&shown, 15), 8);
}

#[test]
fn a_layer_over_the_rows_shows_its_part_within_their_band() {
    let rows = Rows {
        up: 4,
        clip: Some(Rect {
            x: 4,
            y: 20,
            width: 168,
            height: 64,
        }),
    };
    // A cell within the band, whole.
    assert_eq!(rows.layer((10.0, 30.0), (8, 16)), Shows::All);
    // A cell half past its top, then half past its bottom: the half in it,
    // in the layer's own pixels.
    let half = |y: i32| {
        Shows::Part(Rect {
            x: 0,
            y,
            width: 8,
            height: 8,
        })
    };
    assert_eq!(rows.layer((10.0, 12.0), (8, 16)), half(8));
    assert_eq!(rows.layer((10.0, 76.0), (8, 16)), half(0));
    // Past it, nothing.
    assert_eq!(rows.layer((10.0, 84.0), (8, 16)), Shows::Nothing);
    assert_eq!(rows.layer((10.0, -12.0), (8, 16)), Shows::Nothing);
    // Blinking text's layer of a cell's margin around its rows, across its
    // whole width.
    assert_eq!(
        rows.layer((2.0, 4.0), (24, 48)),
        Shows::Part(Rect {
            x: 0,
            y: 16,
            width: 24,
            height: 32,
        })
    );
}

#[test]
fn the_cursor_the_blocks_and_the_gutter_s_marks_of_the_row_below_draw_there() {
    let mut terminal = terminal();
    // Half a row up from the screen: the prompt's row, the cursor's, below
    // the view's last, as `false`'s rows above it.
    terminal.scroll_viewport_by(0.5);
    let shown = snapshot(&mut terminal);
    assert_eq!(shown.drawn_rows(), 5);
    let look = CursorLook {
        shape: CursorShape::Block,
        thickness: 1,
        length: 1.0,
    };
    let mut glyphs = glyph;
    let batch = cells_with(
        &shown,
        &cell(),
        (0, 0),
        None,
        Some(look),
        TextLook::default(),
        &mut glyphs,
    );
    let cursor = batch.cursor.expect("the cursor on the row below");
    assert_eq!((cursor.column, cursor.row), (2, 4));
    let (column, row, _) = cursor_cells(&batch, &shown, look, &cell(), None).unwrap();
    assert_eq!((column, row), (2, 4));
    // `false`'s failed tint over its row, the view's last, then the
    // dividers at its top and at the prompt's block's, on the row below.
    let tokens = crate::theme::Theme::builtin().dark.tokens.clone();
    let looks = lokyr_plugin_sdk::host::BlockLooks {
        bars: true,
        dividers: true,
        failed_tint: true,
        prompt_rows: false,
        spacing: false,
    };
    let shapes: Vec<(i32, u32)> =
        block_shapes(&shown, 0, 16, (0, 176), 1.0, &tokens, (looks, None))
            .iter()
            .map(|shape| (shape.rect.y, shape.rect.height))
            .collect();
    assert_eq!(shapes, [(48, 16), (48, 1), (64, 1)]);
    // A row further up, `false` on the row below: its tint, its divider and
    // its mark there, a point short at its ends; the cursor past it.
    terminal.scroll_viewport_by(1.0);
    let shown = snapshot(&mut terminal);
    let shapes: Vec<(i32, u32)> =
        block_shapes(&shown, 0, 16, (0, 176), 1.0, &tokens, (looks, None))
            .iter()
            .map(|shape| (shape.rect.y, shape.rect.height))
            .collect();
    assert_eq!(shapes, [(64, 16), (64, 1)]);
    let marks: Vec<(i32, u32)> = gutter_marks(&shown, (8, 0), (8, 16), 1.0, &tokens, false)
        .map(|mark| (mark.rect.y, mark.rect.height))
        .collect();
    assert_eq!(marks.last(), Some(&(65, 14)));
    let batch = cells_with(
        &shown,
        &cell(),
        (0, 0),
        None,
        Some(look),
        TextLook::default(),
        &mut glyphs,
    );
    assert!(batch.cursor.is_none());
}
