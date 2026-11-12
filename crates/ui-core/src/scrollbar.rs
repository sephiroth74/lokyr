//! A pane's scroll bar, as macOS's overlay scroll bars look: a knob over the
//! pane's right edge, whose length and place tell how much of the rows the
//! view shows and where; thin as it shows while the view scrolls, wider
//! over a track as the pointer comes over it or drags it; and the marks of
//! a search's matches along it. Of the theme's `scrollbar` tokens, it takes
//! no column of the grid.

use lokyr_render_core::{Color, Rect, Shape};

use crate::theme::Tokens;

/// The bar's width, of its track and of the area that takes the pointer,
/// in points.
pub const WIDTH: f32 = 15.0;

/// The knob's width, thin and wide, in points.
const KNOB: (f32, f32) = (6.0, 9.0);

/// The room between the knob and the bar's right edge, its top and its
/// bottom, in points.
const INSET: f32 = 3.0;

/// The knob's least length, in points.
const LEAST: f32 = 24.0;

/// A mark's height, in points.
pub const MARK: f32 = 2.0;

/// What the view shows of a pane's rows: how many the pane holds, of its
/// scrollback and its screen, the view's first by its place from the
/// oldest, and how many the view shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rows {
    /// The rows that the pane holds.
    pub total: u64,
    /// The view's first row, from the oldest, 0.
    pub top: u64,
    /// The rows that the view shows.
    pub shown: u64,
}

impl Rows {
    /// Whether the rows are more than the view shows, which a bar then
    /// moves through.
    pub fn scrolls(&self) -> bool {
        self.total > self.shown
    }

    /// The most that the view's first row can be.
    fn last_top(&self) -> u64 {
        self.total.saturating_sub(self.shown)
    }
}

/// How the bar shows.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Look {
    /// Its opacity, from 0, hidden, to 1.
    pub opacity: f32,
    /// Whether its knob is wide, over a track, as the pointer is over it.
    pub wide: bool,
    /// Whether its knob is pressed, as a drag holds it, which darkens it.
    pub pressed: bool,
}

/// A mark of a match along the bar: where its row is among the pane's, from
/// 0, the oldest, to 1, past the last, and whether it is the current one.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mark {
    /// Its place along the rows.
    pub at: f32,
    /// Whether it is the current match's.
    pub current: bool,
}

/// The bar's area in a pane of `pane`'s rectangle, in pixels, on a display
/// of `scale`: at its right edge, of its height, [`WIDTH`] points wide.
pub fn area(pane: Rect, scale: f32) -> Rect {
    let width = ((WIDTH * scale).round() as u32).min(pane.width);
    Rect {
        x: pane.x + pane.width as i32 - width as i32,
        y: pane.y,
        width,
        height: pane.height,
    }
}

/// The run of `area`'s height that the knob moves along, its top and its
/// length, in pixels, within the insets at its ends.
fn track(area: Rect, scale: f32) -> (f32, f32) {
    let inset = INSET * scale;
    (
        area.y as f32 + inset,
        (area.height as f32 - 2.0 * inset).max(0.0),
    )
}

/// The knob's length along `area`, in pixels, of the share of the rows that
/// the view shows, [`LEAST`] points at least.
fn length(area: Rect, rows: Rows, scale: f32) -> f32 {
    let (_, run) = track(area, scale);
    let share = rows.shown as f32 / rows.total.max(1) as f32;
    (run * share).max(LEAST * scale).min(run)
}

/// The knob's rectangle in `area`, in pixels, on a display of `scale`, wide
/// or thin: its length of the share of the rows that the view shows, its
/// place of the view's first row.
pub fn knob(area: Rect, rows: Rows, wide: bool, scale: f32) -> Rect {
    let (start, run) = track(area, scale);
    let length = length(area, rows, scale);
    let share = match rows.last_top() {
        0 => 0.0,
        last => rows.top.min(last) as f32 / last as f32,
    };
    let top = start + (run - length) * share;
    let width = if wide { KNOB.1 } else { KNOB.0 } * scale;
    let right = (area.x + area.width as i32) as f32 - INSET * scale;
    Rect {
        x: (right - width).round() as i32,
        y: top.round() as i32,
        width: width.round() as u32,
        height: length.round() as u32,
    }
}

/// The view's first row, from the oldest, that the knob's top at `y`, in
/// pixels, gives, as a drag puts it, within the rows.
pub fn top_at(area: Rect, rows: Rows, scale: f32, y: f32) -> u64 {
    let (start, run) = track(area, scale);
    let room = run - length(area, rows, scale);
    if room <= 0.0 {
        return 0;
    }
    let share = ((y - start) / room).clamp(0.0, 1.0);
    (share * rows.last_top() as f32).round() as u64
}

/// The knob's top, in pixels, that puts its middle at `y`, as a click on
/// the track that jumps there does.
pub fn centered_at(area: Rect, rows: Rows, scale: f32, y: f32) -> f32 {
    y - length(area, rows, scale) / 2.0
}

/// How many marks fit along `area`'s track, each [`MARK`] points high, on a
/// display of `scale`: the runs of rows that a search's matches are told
/// by.
pub fn mark_slots(area: Rect, scale: f32) -> usize {
    let (_, run) = track(area, scale);
    (run / (MARK * scale)).floor().max(0.0) as usize
}

/// The shapes of the bar in `area`, on a display of `scale`, of `rows` and
/// `look`, its marks of `marks`, in `tokens`' colors: its track, while its
/// knob is wide; the marks, across the wide knob's width, under the knob;
/// the knob. None while it is hidden, or of rows that the view shows
/// whole.
pub fn shapes(
    area: Rect,
    rows: Rows,
    look: Look,
    marks: &[Mark],
    tokens: &Tokens,
    scale: f32,
) -> Vec<Shape> {
    if look.opacity <= 0.0 || !rows.scrolls() {
        return Vec::new();
    }
    let faded = |color: Color| color.with_alpha(color.alpha * look.opacity.min(1.0));
    let mut shapes = Vec::new();
    if look.wide
        && let Some(color) = tokens.get("scrollbar.track")
    {
        shapes.push(Shape::filled(area, faded(color)));
    }
    let wide = knob(area, rows, true, scale);
    let (start, run) = track(area, scale);
    let height = (MARK * scale).round().max(1.0) as u32;
    let (matched, current) = (
        tokens.get("scrollbar.mark"),
        tokens.get("scrollbar.mark.current"),
    );
    for mark in marks {
        let color = if mark.current { current } else { matched };
        let Some(color) = color else {
            continue;
        };
        let middle = start + run * mark.at.clamp(0.0, 1.0);
        let top =
            (middle - height as f32 / 2.0).clamp(start, (start + run - height as f32).max(start));
        shapes.push(Shape::filled(
            Rect {
                x: wide.x,
                y: top.round() as i32,
                width: wide.width,
                height,
            },
            faded(color),
        ));
    }
    let name = if look.pressed {
        "scrollbar.hover"
    } else {
        "scrollbar"
    };
    if let Some(color) = tokens.get(name) {
        let rect = knob(area, rows, look.wide, scale);
        shapes.push(Shape {
            radius: rect.width as f32 / 2.0,
            ..Shape::filled(rect, faded(color))
        });
    }
    shapes
}

#[cfg(test)]
mod tests;
