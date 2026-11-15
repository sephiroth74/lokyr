use lokyr_render_core::{Color, CursorShape};
use lokyr_text_core::FaceMetrics;
use lokyr_vt_core::terminal::GutterKind;

use super::*;
use crate::pane::CellText;
use crate::theme::Theme;

/// A cell of 8 by 16 pixels.
fn cell() -> CellMetrics {
    CellMetrics::new(
        &FaceMetrics {
            ascent: 12.07,
            descent: 3.07,
            advance: 7.83,
            ..FaceMetrics::default()
        },
        &lokyr_text_core::FontConfig::default(),
    )
}

/// The specimen of the built-in dark theme, its cursor guide of `guide`.
fn specimen(guide: Option<Color>) -> Specimen {
    let theme = Theme::builtin();
    let variant = &theme.dark;
    Specimen {
        colors: variant.colors.clone(),
        selection: variant.selection,
        matches: variant.match_colors(),
        text: TextLook {
            cursor_guide: guide,
            ..TextLook::default()
        },
        cursor: CursorLook {
            shape: CursorShape::Block,
            thickness: 2,
            length: 1.0,
        },
    }
}

#[test]
fn the_sample_marks_its_commands_of_each_state_and_fits_its_grid() {
    let snapshot = snapshot(&specimen(None).colors);
    let marks: Vec<(u16, GutterKind)> = snapshot
        .gutter()
        .iter()
        .map(|mark| (mark.row, mark.kind))
        .collect();
    assert_eq!(
        marks,
        [
            (0, GutterKind::Succeeded),
            (2, GutterKind::Succeeded),
            (8, GutterKind::Failed),
            (10, GutterKind::Running),
        ]
    );
    // Its last row the cursor's, none scrolled away.
    let cursor = snapshot.cursor();
    assert_eq!(cursor.position.y, ROWS - 1);
    assert!(cursor.visible);
}

#[test]
fn the_cells_show_the_selection_matches_backgrounds_cursor_guide_and_bars() {
    let tokens = Theme::builtin().dark.tokens.clone();
    let band = Color::from_rgb8(255, 0, 0).with_alpha(0.25);
    let spec = specimen(Some(band));
    let mut none = |_: &CellText<'_>| None;
    let (batch, bars) = cells(&spec, &cell(), (12, 12), (12, 2.0), &tokens, &mut none);
    let at = |column: u16, row: u16| {
        batch.backgrounds[usize::from(row) * usize::from(batch.columns) + usize::from(column)]
    };
    // The selected text of the selection's background.
    assert_eq!(at(0, SELECTED.0), spec.selection.background);
    assert_eq!(at(12, SELECTED.0), spec.selection.background);
    // The find's matches over their cells.
    assert_eq!(
        at(MATCHED.start, SELECTED.0),
        paint::over(spec.matches.matched, at(30, SELECTED.0))
    );
    assert_eq!(
        at(CURRENT.start, SELECTED.0),
        paint::over(spec.matches.current, at(30, SELECTED.0))
    );
    // The palette's backgrounds: red's third cell of its row.
    let red = spec.colors.palette[1];
    assert_eq!(at(4, 5), Color::from_rgb8(red.r, red.g, red.b));
    // The cursor on the last row, on a character, the cursor guide's band
    // across its row.
    let cursor = batch.cursor.expect("its cursor");
    assert_eq!(cursor.row, ROWS - 1);
    assert_eq!(cursor.shape, CursorShape::Block);
    assert!(at(0, ROWS - 1).alpha > 0.0);
    assert_eq!(at(0, ROWS - 2).alpha, 0.0);
    // An underline, a curly one and a strikethrough among its lines.
    assert!(batch.lines.len() >= 3);
    // The gutter's bars of the commands' states, of the theme's tokens.
    let fills: Vec<Color> = bars.iter().map(|bar| bar.fill).collect();
    assert_eq!(
        fills,
        [
            tokens.get("gutter.succeeded").unwrap(),
            tokens.get("gutter.succeeded").unwrap(),
            tokens.get("gutter.failed").unwrap(),
            tokens.get("gutter.running").unwrap(),
        ]
    );
    assert!(bars.iter().all(|bar| bar.rect.x < 12));
}

#[test]
fn a_specimen_s_natural_size_is_its_grid_s_within_its_padding() {
    assert_eq!(
        natural((8.0, 16.0)),
        (
            f32::from(COLUMNS) * 8.0 + 2.0 * PADDING,
            f32::from(ROWS) * 16.0 + 2.0 * PADDING
        )
    );
}
