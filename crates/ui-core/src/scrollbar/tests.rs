//! The bar's knob, its drag's rows, and its shapes.

use super::*;
use crate::theme::Theme;

/// A pane's rectangle, 400 by 600 pixels, from (10, 20).
const PANE: Rect = Rect {
    x: 10,
    y: 20,
    width: 400,
    height: 600,
};

/// 1000 rows, of which the view shows 100 from the 450th.
const ROWS: Rows = Rows {
    total: 1000,
    top: 450,
    share: 0.0,
    shown: 100,
};

fn tokens() -> Tokens {
    Theme::builtin().light.tokens.clone()
}

#[test]
fn the_area_is_at_the_pane_s_right_edge_of_its_height() {
    assert_eq!(
        area(PANE, 2.0),
        Rect {
            x: 380,
            y: 20,
            width: 30,
            height: 600,
        }
    );
}

#[test]
fn the_knob_is_of_the_view_s_share_at_the_view_s_place() {
    let area = area(PANE, 1.0);
    // A run of 594 pixels, between insets of 3; a tenth of it, 59.4 long,
    // at 450 of 900, half the way along the rest.
    let thin = knob(area, ROWS, false, 1.0);
    assert_eq!((thin.height, thin.width), (59, 6));
    assert_eq!(thin.y, (23.0_f32 + (594.0 - 59.4) / 2.0).round() as i32);
    assert_eq!(thin.x + thin.width as i32, 410 - 3);
    let wide = knob(area, ROWS, true, 1.0);
    assert_eq!((wide.y, wide.width), (thin.y, 9));
    // At the top, and at the screen.
    let top = Rows { top: 0, ..ROWS };
    assert_eq!(knob(area, top, false, 1.0).y, 23);
    let bottom = Rows { top: 900, ..ROWS };
    let at = knob(area, bottom, false, 1.0);
    assert_eq!(at.y + at.height as i32, 20 + 600 - 3);
    // Of many rows, its least length.
    let many = Rows {
        total: 1_000_000,
        ..ROWS
    };
    assert_eq!(knob(area, many, false, 1.0).height, 24);
}

#[test]
fn a_drag_s_knob_top_gives_the_view_s_first_row() {
    let area = area(PANE, 2.0);
    // Of the knob's top rounded to a pixel, a row or so; the ends whole.
    for top in [0, 1, 450, 899, 900] {
        let rows = Rows { top, ..ROWS };
        let knob = knob(area, rows, true, 2.0);
        let found = top_at(area, rows, 2.0, knob.y as f32);
        assert!(found.abs_diff(top) <= 1, "{top}: {found}");
        if top == 0 || top == 900 {
            assert_eq!(found, top);
        }
    }
    // Past the ends, the ends.
    assert_eq!(top_at(area, ROWS, 2.0, -100.0), 0);
    assert_eq!(top_at(area, ROWS, 2.0, 5000.0), 900);
    // A click's middle.
    let middle = (area.y + area.height as i32 / 2) as f32;
    let top = top_at(area, ROWS, 2.0, centered_at(area, ROWS, 2.0, middle));
    assert_eq!(top, 450);
}

#[test]
fn by_pixels_the_knob_takes_the_view_s_share_and_a_drag_gives_a_place_of_one() {
    let area = area(PANE, 1.0);
    // 200 rows, of which the view shows 100: a knob of half the run, 297
    // pixels, over a room of 297 for 100 places, 2.97 pixels a row.
    let rows = Rows {
        total: 200,
        top: 50,
        share: 0.0,
        shown: 100,
    };
    let at = |top, share| knob(area, Rows { top, share, ..rows }, false, 1.0).y;
    assert_eq!((at(50, 0.0), at(51, 0.0)), (172, 174));
    // Half a row past the 50th, between the two.
    assert_eq!(at(50, 0.5), 173);
    // A drag's knob top gives the place of a share of a row, which the
    // whole rows' drag rounds.
    let place = place_at(area, rows, 1.0, 23.0 + 297.0 * 0.505);
    assert!((place - 50.5).abs() < 1e-3, "{place}");
    let place = place_at(area, rows, 1.0, 23.0 + 297.0 * 0.2025);
    assert!((place - 20.25).abs() < 1e-3, "{place}");
    assert_eq!(top_at(area, rows, 1.0, 23.0 + 297.0 * 0.2025), 20);
    // Past the ends, the ends, whole.
    assert_eq!(place_at(area, rows, 1.0, -100.0), 0.0);
    assert_eq!(place_at(area, rows, 1.0, 5000.0), 100.0);
}

#[test]
fn rows_that_the_view_shows_whole_and_a_hidden_bar_have_no_shapes() {
    let area = area(PANE, 1.0);
    let look = Look {
        opacity: 1.0,
        wide: true,
        pressed: false,
    };
    let fit = Rows {
        total: 100,
        top: 0,
        share: 0.0,
        shown: 100,
    };
    assert!(shapes(area, fit, look, &[], &tokens(), 1.0).is_empty());
    let hidden = Look {
        opacity: 0.0,
        ..look
    };
    assert!(shapes(area, ROWS, hidden, &[], &tokens(), 1.0).is_empty());
}

#[test]
fn the_shapes_are_the_track_the_marks_then_the_knob_faded_by_the_opacity() {
    let tokens = tokens();
    let token = |name: &str| tokens.get(name).unwrap();
    let area = area(PANE, 1.0);
    let marks = [
        Mark {
            at: 0.0,
            current: false,
        },
        Mark {
            at: 0.5,
            current: true,
        },
        Mark {
            at: 1.0,
            current: false,
        },
    ];
    let wide = Look {
        opacity: 1.0,
        wide: true,
        pressed: true,
    };
    let shapes = shapes(area, ROWS, wide, &marks, &tokens, 1.0);
    let fills: Vec<Color> = shapes.iter().map(|shape| shape.fill).collect();
    assert_eq!(
        fills,
        [
            token("scrollbar.track"),
            token("scrollbar.mark"),
            token("scrollbar.mark.current"),
            token("scrollbar.mark"),
            token("scrollbar.hover"),
        ]
    );
    assert_eq!(shapes[0].rect, area);
    // The marks within the track, across the wide knob, 2 pixels high.
    let knob = knob(area, ROWS, true, 1.0);
    assert_eq!(
        (shapes[1].rect.x, shapes[1].rect.width, shapes[1].rect.y),
        (knob.x, knob.width, 23)
    );
    assert_eq!(shapes[3].rect.y + 2, 20 + 600 - 3);
    assert_eq!(shapes[2].rect.y, 23 + 297 - 1);
    assert_eq!(shapes[4].radius, 4.5);
    // Thin and half faded: no track, the knob's own color at half.
    let thin = Look {
        opacity: 0.5,
        wide: false,
        pressed: false,
    };
    let shapes = super::shapes(area, ROWS, thin, &[], &tokens, 1.0);
    let [knob] = &shapes[..] else {
        panic!("{shapes:?}");
    };
    let color = token("scrollbar");
    assert_eq!(knob.fill, color.with_alpha(color.alpha * 0.5));
    assert_eq!(knob.rect.width, 6);
}

#[test]
fn the_marks_fit_the_track_two_points_each() {
    assert_eq!(mark_slots(area(PANE, 1.0), 1.0), 297);
    assert_eq!(mark_slots(area(PANE, 2.0), 2.0), 147);
}
