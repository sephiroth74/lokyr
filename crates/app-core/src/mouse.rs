//! The mouse over a pane: where the pointer is in its cells, the buttons
//! that the program learned were held, the wheel's scroll, which adds up to
//! whole steps or glides the view by pixels, and the cells that a drag
//! selects.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use lokyr_kernel::workspace::{Grid, PaneId};
use lokyr_platform_core::{MouseButton as Button, Point, ScrollDelta, ScrollPhase};
use lokyr_vt_core::input::MouseButton;
use lokyr_vt_core::terminal::Origin;

/// How long a wheel's step glides the view over its rows, scrolling by
/// pixels.
pub(crate) const GLIDE: Duration = Duration::from_millis(150);

/// Where the pointer is in a grid: its cell, and its pixel from the cells'
/// top left corner, of the rows as they are drawn, a share of a row up
/// while the view is scrolled by one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Pointer {
    pub(crate) col: u16,
    /// The view's row under the pointer: the row past the view's last, at
    /// the grid's bottom, while the view is scrolled a share of a row.
    pub(crate) row: u16,
    pub(crate) x: u32,
    pub(crate) y: u32,
    /// The row of the grid's band under the pointer, from the band's top,
    /// where what stays as the rows move a share of a row shows, as the
    /// prompt rows pinned at the view's top: `row`, at no share.
    pub(crate) band_row: u16,
}

/// The pointer at `position`, in points from the content's top left corner,
/// over `grid`, in a window of `scale`, whose cells start `padding` pixels
/// from its left and top edges, its rows drawn `up` pixels up, as a share
/// of a row of the view has them: a pointer over the padding, or past the
/// cells, takes the nearest cell's edge, of the rows that the grid's band
/// shows.
pub(crate) fn pointer(
    position: Point,
    scale: f64,
    padding: (u32, u32),
    grid: Grid,
    up: u32,
) -> Pointer {
    let within = |points: f64, padding: u32, cell: u16, cells: u16| {
        let cell = u32::from(cell.max(1));
        let last = (cell * u32::from(cells.max(1))).saturating_sub(1);
        // A cast from a float saturates, and takes NaN to 0.
        let pixel =
            ((points * scale).floor() as i64 - i64::from(padding)).clamp(0, i64::from(last));
        // Within the grid's pixels, which a `u32` holds.
        let pixel = pixel as u32;
        // Within the grid's cells, which a `u16` holds.
        (pixel, (pixel / cell) as u16)
    };
    let (x, col) = within(position.x, padding.0, grid.cell_width, grid.columns);
    let (band, band_row) = within(position.y, padding.1, grid.cell_height, grid.rows);
    // Up by a row at most, as a share rounds to its pixels: the row past
    // the view's last at most.
    let y = band + up.min(u32::from(grid.cell_height));
    // Within the grid's rows and the one below, which a `u16` holds.
    let row = (y / u32::from(grid.cell_height.max(1))) as u16;
    Pointer {
        col,
        row,
        x,
        y,
        band_row,
    }
}

/// `pointer`, over a view `offset` rows above the screen's top, of cells
/// `cell_height` pixels high, at its place on the screen, whose rows the
/// program counts: `None` above the screen, over the scrollback.
pub(crate) fn on_screen(pointer: Pointer, offset: usize, cell_height: u16) -> Option<Pointer> {
    let row = usize::from(pointer.row).checked_sub(offset)?;
    let above = u32::try_from(offset)
        .ok()?
        .checked_mul(u32::from(cell_height))?;
    Some(Pointer {
        // Below the pointer's row, which a `u16` holds.
        row: row as u16,
        y: pointer.y.checked_sub(above)?,
        ..pointer
    })
}

/// `pointer` moved to the row `row`, of cells `cell_height` pixels high, its
/// column and its place in its cell kept: `None` above the first row.
pub(crate) fn onto_row(pointer: Pointer, row: u16, cell_height: u16) -> Option<Pointer> {
    let rows = i64::from(row) - i64::from(pointer.row);
    let y = i64::from(pointer.y) + rows * i64::from(cell_height);
    Some(Pointer {
        row,
        y: u32::try_from(y).ok()?,
        ..pointer
    })
}

/// `pointer` at the screen's top row, where an event that must reach the
/// program goes from above the screen.
pub(crate) fn at_top(pointer: Pointer) -> Pointer {
    Pointer {
        row: 0,
        y: 0,
        ..pointer
    }
}

/// Where a selection by the mouse starts, or where the pointer is as it
/// drags: its row's number, which output and the view's scrolling leave
/// it; the cell under the pointer; and the boundary between cells nearest
/// to it, from 0, before the row's first cell, to the grid's columns, after
/// its last.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Spot {
    pub(crate) row: i64,
    pub(crate) col: u16,
    pub(crate) boundary: u16,
}

/// The spot of `pointer`, over the row numbered `row`, in cells
/// `cell_width` pixels wide, of a grid of `columns`.
pub(crate) fn spot(pointer: Pointer, row: i64, cell_width: u16, columns: u16) -> Spot {
    let width = u32::from(cell_width.max(1));
    // Within the grid's columns, which a `u16` holds.
    let boundary = ((pointer.x + width / 2) / width).min(u32::from(columns)) as u16;
    Spot {
        row,
        col: pointer.col,
        boundary,
    }
}

/// The cells, by column and row's number, that a drag from
/// `anchor` to `to` selects, the first and the last in reading order: those
/// between the two spots' boundaries, as text's selection takes them; `None`
/// when no cell is between them, as after a click.
pub(crate) fn dragged_cells(
    anchor: Spot,
    to: Spot,
    columns: u16,
) -> Option<((u16, i64), (u16, i64))> {
    let (first, last) = if (anchor.row, anchor.boundary) <= (to.row, to.boundary) {
        (anchor, to)
    } else {
        (to, anchor)
    };
    // A boundary after a row's last cell starts at the next row's first;
    // one before a row's first ends at the row before's last.
    let from = if first.boundary >= columns {
        (0, first.row + 1)
    } else {
        (first.boundary, first.row)
    };
    let to = if last.boundary == 0 {
        (columns.saturating_sub(1), last.row - 1)
    } else {
        (last.boundary - 1, last.row)
    };
    ((from.1, from.0) <= (to.1, to.0)).then_some((from, to))
}

/// The corners, top left and bottom right, of the rectangle that a drag from
/// `anchor` to `to` selects: the columns between the two spots' boundaries,
/// on the rows from one's to the other's; `None` when no column is between
/// them.
pub(crate) fn rectangle_cells(anchor: Spot, to: Spot) -> Option<((u16, i64), (u16, i64))> {
    let (left, right) = (
        anchor.boundary.min(to.boundary),
        anchor.boundary.max(to.boundary),
    );
    let (top, bottom) = (anchor.row.min(to.row), anchor.row.max(to.row));
    (left < right).then_some(((left, top), (right - 1, bottom)))
}

/// How many rows past the grid's top, negative, or its bottom, positive,
/// the pointer at `y` points is, in a window of `scale`, whose cells start
/// `padding` pixels from its top edge, of `grid`: 0 within the grid, and a
/// row as soon as it leaves it.
pub(crate) fn beyond(y: f64, scale: f64, padding: u32, grid: Grid) -> i64 {
    let height = f64::from(grid.cell_height.max(1));
    let pixel = y * scale - f64::from(padding);
    let bottom = height * f64::from(grid.rows);
    // A cast from a float saturates, and takes NaN to 0.
    if pixel < 0.0 {
        -((-pixel / height).floor() as i64) - 1
    } else if pixel >= bottom {
        ((pixel - bottom) / height).floor() as i64 + 1
    } else {
        0
    }
}

/// The terminal's button for the platform's `button`, numbered as X11
/// numbers them; `None` for a button past the eleventh.
pub(crate) fn button(button: Button) -> Option<MouseButton> {
    Some(match button {
        Button::Left => MouseButton::Left,
        Button::Middle => MouseButton::Middle,
        Button::Right => MouseButton::Right,
        Button::Back => MouseButton::Back,
        Button::Forward => MouseButton::Forward,
        // The platform's numbers from 0, the first five being named.
        Button::Other(5) => MouseButton::Button10,
        Button::Other(6) => MouseButton::Button11,
        Button::Other(_) => return None,
    })
}

/// X11's number of `button`.
fn number(button: MouseButton) -> u8 {
    match button {
        MouseButton::Left => 1,
        MouseButton::Middle => 2,
        MouseButton::Right => 3,
        MouseButton::Back => 8,
        MouseButton::Forward => 9,
        MouseButton::Button10 => 10,
        MouseButton::Button11 => 11,
    }
}

/// Where a view is while it is above the screen, as only a scroll moves it
/// there: its rows' origin and its share of a row; none at the screen,
/// where output moves it.
pub(crate) type Place = Option<(Origin, f32)>;

/// A wheel's steps gliding a pane's view over their rows, by pixels: from
/// the time of its last step, it eases out over [`GLIDE`], the view moving
/// fast at first and slower as it comes to its rows. It puts the view at
/// each step's place from where it started, by the rows' numbers, so that
/// no step's rounding adds up, and its last puts it where it goes.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Glide {
    /// The pane whose view glides.
    pub(crate) pane: PaneId,
    /// When its last step came.
    start: Instant,
    /// The view's place then: its first row's number and the share of a
    /// row past it.
    from: f64,
    /// The rows that it moves the view from `from`, up where positive.
    rows: f64,
    /// Those of them that it moved.
    moved: f64,
    /// Where it left the view, which a move of the view of another kind,
    /// as a key's, changes, and which then ends it.
    pub(crate) left: Place,
}

impl Glide {
    /// A glide of `pane`'s view by `rows`, up where positive, of a step at
    /// `now`, the view at `place`, from its place `from`, of its first
    /// row's number and the share past it.
    pub(crate) fn new(pane: PaneId, rows: f64, now: Instant, (place, from): (Place, f64)) -> Glide {
        Glide {
            pane,
            start: now,
            from,
            rows,
            moved: 0.0,
            left: place,
        }
    }

    /// Adds `rows`, of another step at `now`, the view at `place` and its
    /// place `from`: the glide eases out again from where it is, over the
    /// rows that it has left and these, or from `from` over these alone
    /// where the view moved another way since its last step.
    pub(crate) fn add(&mut self, rows: f64, now: Instant, (place, from): (Place, f64)) {
        *self = if place == self.left {
            Glide::new(
                self.pane,
                self.left() + rows,
                now,
                (place, self.from - self.moved),
            )
        } else {
            Glide::new(self.pane, rows, now, (place, from))
        };
    }

    /// The rows that it has yet to move the view.
    pub(crate) fn left(&self) -> f64 {
        self.rows - self.moved
    }

    /// The place where it takes the view, its first row's number and the
    /// share of a row past it.
    pub(crate) fn end(&self) -> f64 {
        self.from - self.rows
    }

    /// The view's place at `now`, of its first row's number and the share
    /// past it, and whether the glide goes on after: the last step's is
    /// where it goes.
    pub(crate) fn step(&mut self, now: Instant) -> (f64, bool) {
        let elapsed = now.saturating_duration_since(self.start);
        let time = (elapsed.as_secs_f64() / GLIDE.as_secs_f64()).min(1.0);
        // An ease-out of the cube: fast at first, still at its end.
        self.moved = if time >= 1.0 {
            self.rows
        } else {
            self.rows * (1.0 - (1.0 - time).powi(3))
        };
        (self.from - self.moved, time < 1.0)
    }
}

/// A window's mouse: the buttons pressed that a pane's program learned of,
/// and that pane, which takes the moves and the releases until the last
/// goes up; where each pane's program last learned the pointer was; the
/// scroll that adds up to the wheel's next steps; and the wheel's glide of
/// a view, scrolling by pixels.
#[derive(Debug, Default)]
pub(crate) struct Mouse {
    held: Vec<MouseButton>,
    holder: Option<PaneId>,
    last: BTreeMap<PaneId, Pointer>,
    /// The scroll that makes no whole step yet, across and up, in rows.
    rest: (f64, f64),
    /// The wheel's glide of a pane's view under way.
    pub(crate) glide: Option<Glide>,
}

impl Mouse {
    /// Notes that `button` went down, which `pane`'s program learned of:
    /// the pane of the first button holds the others.
    pub(crate) fn press(&mut self, pane: PaneId, button: MouseButton) {
        if self.held.is_empty() {
            self.holder = Some(pane);
        }
        if !self.held.contains(&button) {
            self.held.push(button);
        }
    }

    /// Notes that `button` went up, and returns whether it was down: a
    /// release whose press the program did not learn of goes nowhere.
    pub(crate) fn release(&mut self, button: MouseButton) -> bool {
        let held = self.held.contains(&button);
        self.held.retain(|&other| other != button);
        if self.held.is_empty() {
            self.holder = None;
        }
        held
    }

    /// The pane whose program learned of the buttons held, while one is.
    pub(crate) fn holder(&self) -> Option<PaneId> {
        self.holder
    }

    /// Forgets `pane`, which closed, and its buttons, which no program
    /// holds any more.
    pub(crate) fn forget(&mut self, pane: PaneId) {
        self.last.remove(&pane);
        if self.glide.is_some_and(|glide| glide.pane == pane) {
            self.glide = None;
        }
        if self.holder == Some(pane) {
            self.holder = None;
            self.held.clear();
        }
    }

    /// The lowest-numbered of the buttons down, which a motion reports.
    pub(crate) fn held(&self) -> Option<MouseButton> {
        self.held
            .iter()
            .copied()
            .min_by_key(|&button| number(button))
    }

    /// Notes that `pane`'s program learns the pointer is at `pointer`, and
    /// returns whether that is news to it: another cell than the one that
    /// it learned last, or with `pixels`, as reports in pixels have it,
    /// another pixel.
    pub(crate) fn moved(&mut self, pane: PaneId, pointer: Pointer, pixels: bool) -> bool {
        match self.last.insert(pane, pointer) {
            Some(last) if pixels => last != pointer,
            Some(last) => (last.col, last.row) != (pointer.col, pointer.row),
            None => true,
        }
    }

    /// The whole steps, across and up, that `delta` makes with the scroll
    /// that made none yet, in rows of `row_points` points, as [`rows`]
    /// finds them. A gesture that starts, and a scroll the other way, start
    /// the sum again. A step across goes toward the content's left edge,
    /// and a step up toward its top, when positive.
    pub(crate) fn scroll(
        &mut self,
        delta: ScrollDelta,
        phase: Option<ScrollPhase>,
        row_points: f64,
        sideways: bool,
    ) -> (i32, i32) {
        let (across, up) = rows(delta, row_points, sideways);
        if phase == Some(ScrollPhase::Started) {
            self.rest = (0.0, 0.0);
        }
        let step = |rest: &mut f64, amount: f64| {
            // A scroll the other way drops what the last one left.
            *rest = if *rest * amount < 0.0 {
                amount
            } else {
                *rest + amount
            };
            let steps = rest.trunc();
            *rest -= steps;
            // Far fewer steps than an `i32` holds, from one event.
            steps as i32
        };
        let across = step(&mut self.rest.0, across);
        let up = step(&mut self.rest.1, up);
        (across, up)
    }

    /// Lets the scroll up and down that made no whole step yet go, as the
    /// view takes it by pixels.
    pub(crate) fn settle_up(&mut self) {
        self.rest.1 = 0.0;
    }
}

/// The rows, across and up, that `delta` scrolls, of rows of `row_points`
/// points: a wheel's lines, or a trackpad's points. With `sideways`, a
/// scroll across alone, as Shift makes a mouse's wheel, goes up and down.
/// Across goes toward the content's left edge, and up toward its top, when
/// positive.
pub(crate) fn rows(delta: ScrollDelta, row_points: f64, sideways: bool) -> (f64, f64) {
    let (across, up) = match delta {
        ScrollDelta::Lines { x, y } => (x, y),
        ScrollDelta::Points { x, y } => (x / row_points, y / row_points),
    };
    if sideways && up == 0.0 {
        (0.0, across)
    } else {
        (across, up)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grid() -> Grid {
        Grid {
            columns: 80,
            rows: 24,
            cell_width: 16,
            cell_height: 32,
        }
    }

    #[test]
    fn the_pointer_takes_its_cell_and_pixel_within_the_grid() {
        let at = |x, y| pointer(Point { x, y }, 2.0, (8, 8), grid(), 0);
        // 8 pixels of padding, at a scale of 2: the first cell from 4 points.
        assert_eq!(
            at(4.0, 4.0),
            Pointer {
                col: 0,
                row: 0,
                x: 0,
                y: 0,
                band_row: 0,
            }
        );
        assert_eq!(
            at(12.5, 21.0),
            Pointer {
                col: 1,
                row: 1,
                x: 17,
                y: 34,
                band_row: 1,
            }
        );
        // Over the padding, and past the last cell.
        assert_eq!(
            at(0.0, 0.0),
            Pointer {
                col: 0,
                row: 0,
                x: 0,
                y: 0,
                band_row: 0,
            }
        );
        let far = at(5000.0, 5000.0);
        assert_eq!(
            (far.col, far.row, far.x, far.y),
            (79, 23, 80 * 16 - 1, 24 * 32 - 1)
        );
    }

    #[test]
    fn over_rows_a_share_up_the_pointer_takes_the_drawn_row() {
        // Rows of 32 pixels drawn 12 up, as a view scrolled a share of a
        // row has them; 8 pixels of padding, at a scale of 2.
        let at = |y| pointer(Point { x: 4.0, y }, 2.0, (8, 8), grid(), 12);
        // The band's top shows the first row's lower part.
        let top = at(4.0);
        assert_eq!((top.row, top.y, top.band_row), (0, 12, 0));
        // 20 pixels down the band, the second row, the band's first still,
        // where what stays shows.
        let second = at(14.0);
        assert_eq!((second.row, second.y, second.band_row), (1, 32, 0));
        // At the band's bottom, the row past the view's last, and past it
        // or over the padding, the nearest of the drawn rows.
        let bottom = (8.0 + 24.0 * 32.0 - 1.0) / 2.0;
        let below = at(bottom);
        assert_eq!((below.row, below.y, below.band_row), (24, 24 * 32 + 11, 23));
        assert_eq!(at(5000.0), below);
        assert_eq!(at(0.0), top);
        // Up a whole row, as a share near a row rounds, the row below at
        // most.
        let whole = pointer(Point { x: 4.0, y: 5000.0 }, 2.0, (8, 8), grid(), 99);
        assert_eq!((whole.row, whole.y), (24, 25 * 32 - 1));
    }

    fn at(row: i64, col: u16, boundary: u16) -> Spot {
        Spot { row, col, boundary }
    }

    #[test]
    fn a_spot_takes_the_nearest_boundary_and_its_row_s_number() {
        let pointer = |col: u16, x: u32, row: u16| Pointer {
            col,
            row,
            x,
            y: 0,
            band_row: row,
        };
        // Cells of 16 pixels: the boundary after the first cell from 8.
        assert_eq!(spot(pointer(0, 7, 3), 3, 16, 80), at(3, 0, 0));
        assert_eq!(spot(pointer(0, 8, 3), 3, 16, 80), at(3, 0, 1));
        assert_eq!(spot(pointer(79, 80 * 16 - 1, 0), 0, 16, 80), at(0, 79, 80));
        // The row's number that it is given, whatever the view's row.
        assert_eq!(spot(pointer(5, 80, 1), 41, 16, 80).row, 41);
    }

    #[test]
    fn a_drag_selects_the_cells_between_its_boundaries() {
        // Within a row, either way; nothing between the same boundaries.
        assert_eq!(
            dragged_cells(at(0, 2, 2), at(0, 5, 5), 10),
            Some(((2, 0), (4, 0)))
        );
        assert_eq!(
            dragged_cells(at(0, 5, 5), at(0, 2, 2), 10),
            Some(((2, 0), (4, 0)))
        );
        assert_eq!(dragged_cells(at(0, 3, 3), at(0, 3, 3), 10), None);
        // Across rows, in reading order.
        assert_eq!(
            dragged_cells(at(2, 7, 7), at(0, 3, 3), 10),
            Some(((3, 0), (6, 2)))
        );
        // From after a row's last cell, and to before a row's first.
        assert_eq!(
            dragged_cells(at(0, 9, 10), at(2, 0, 0), 10),
            Some(((0, 1), (9, 1)))
        );
        assert_eq!(dragged_cells(at(0, 9, 10), at(1, 0, 0), 10), None);
        // From the scrollback, above the screen.
        assert_eq!(
            dragged_cells(at(-3, 0, 0), at(1, 4, 4), 10),
            Some(((0, -3), (3, 1)))
        );
    }

    #[test]
    fn a_rectangle_takes_the_columns_between_its_boundaries() {
        assert_eq!(
            rectangle_cells(at(4, 6, 6), at(1, 2, 2)),
            Some(((2, 1), (5, 4)))
        );
        assert_eq!(rectangle_cells(at(1, 2, 2), at(4, 2, 2)), None);
    }

    #[test]
    fn beyond_the_grid_the_pointer_counts_rows() {
        let grid = grid();
        // 8 pixels of padding at a scale of 2, rows of 32 pixels, 24 rows.
        assert_eq!(beyond(4.0, 2.0, 8, grid), 0);
        assert_eq!(beyond(3.9, 2.0, 8, grid), -1);
        assert_eq!(beyond(-30.0, 2.0, 8, grid), -3);
        let bottom = (8.0 + 24.0 * 32.0) / 2.0;
        assert_eq!(beyond(bottom - 0.1, 2.0, 8, grid), 0);
        assert_eq!(beyond(bottom, 2.0, 8, grid), 1);
        assert_eq!(beyond(bottom + 40.0, 2.0, 8, grid), 3);
    }

    #[test]
    fn a_pointer_in_a_scrolled_view_takes_the_screen_s_row() {
        let at = |row, y| Pointer {
            col: 3,
            row,
            x: 50,
            y,
            band_row: 0,
        };
        // Two rows of 32 pixels above the screen: the view's fifth row is
        // the screen's third.
        assert_eq!(on_screen(at(4, 4 * 32 + 5), 2, 32), Some(at(2, 2 * 32 + 5)));
        assert_eq!(on_screen(at(2, 2 * 32), 2, 32), Some(at(0, 0)));
        // Above the screen, nowhere, but at its top for what must reach it.
        assert_eq!(on_screen(at(1, 32 + 31), 2, 32), None);
        assert_eq!(at_top(at(1, 63)), at(0, 0));
        // A view on the screen leaves it as it is.
        assert_eq!(on_screen(at(1, 40), 0, 32), Some(at(1, 40)));
        assert_eq!(on_screen(at(1, 40), usize::MAX, 32), None);
        // Moved to another row, as a view of folds shows the screen's rows.
        assert_eq!(onto_row(at(2, 2 * 32 + 5), 6, 32), Some(at(6, 6 * 32 + 5)));
        assert_eq!(onto_row(at(6, 6 * 32 + 5), 2, 32), Some(at(2, 2 * 32 + 5)));
    }

    #[test]
    fn buttons_take_x11_s_numbers_and_the_lowest_held_reports() {
        assert_eq!(button(Button::Other(5)), Some(MouseButton::Button10));
        assert_eq!(button(Button::Other(7)), None);
        let mut mouse = Mouse::default();
        let (first, second) = (PaneId::of(1), PaneId::of(2));
        assert!(!mouse.release(MouseButton::Left), "never pressed");
        mouse.press(first, MouseButton::Right);
        mouse.press(second, MouseButton::Left);
        assert_eq!(mouse.held(), Some(MouseButton::Left));
        // The pane of the first press holds them all, until the last goes
        // up.
        assert_eq!(mouse.holder(), Some(first));
        assert!(mouse.release(MouseButton::Left));
        assert_eq!(mouse.held(), Some(MouseButton::Right));
        assert_eq!(mouse.holder(), Some(first));
        assert!(mouse.release(MouseButton::Right));
        assert_eq!(mouse.holder(), None);
        // A pane that closes takes its buttons with it.
        mouse.press(second, MouseButton::Left);
        mouse.forget(second);
        assert_eq!((mouse.holder(), mouse.held()), (None, None));
    }

    #[test]
    fn a_move_is_news_in_another_cell_or_pixel() {
        let mut mouse = Mouse::default();
        let at = |col, x| Pointer {
            col,
            row: 0,
            x,
            y: 0,
            band_row: 0,
        };
        let (first, second) = (PaneId::of(1), PaneId::of(2));
        assert!(mouse.moved(first, at(0, 1), false));
        assert!(!mouse.moved(first, at(0, 5), false), "the same cell");
        assert!(mouse.moved(first, at(0, 6), true), "another pixel");
        assert!(mouse.moved(first, at(1, 17), false));
        // Each pane's program learns of its own cell.
        assert!(mouse.moved(second, at(1, 17), false), "news to the second");
        assert!(!mouse.moved(first, at(1, 18), false), "the first's cell");
    }

    #[test]
    fn a_scroll_adds_up_to_whole_steps() {
        let mut mouse = Mouse::default();
        let points = |y| ScrollDelta::Points { x: 0.0, y };
        // Rows of 16 points: 40 points make two steps, and 8 more a third.
        assert_eq!(
            mouse.scroll(points(40.0), Some(ScrollPhase::Started), 16.0, false),
            (0, 2)
        );
        assert_eq!(
            mouse.scroll(points(8.0), Some(ScrollPhase::Moved), 16.0, false),
            (0, 1)
        );
        // The other way drops the rest.
        assert_eq!(mouse.scroll(points(-8.0), None, 16.0, false), (0, 0));
        assert_eq!(mouse.scroll(points(-8.0), None, 16.0, false), (0, -1));
        // A new gesture starts from nothing.
        mouse.scroll(points(10.0), None, 16.0, false);
        assert_eq!(
            mouse.scroll(points(10.0), Some(ScrollPhase::Started), 16.0, false),
            (0, 0)
        );
        // A wheel's lines, and Shift's sideways wheel.
        let lines = ScrollDelta::Lines { x: 0.0, y: -3.0 };
        assert_eq!(mouse.scroll(lines, None, 16.0, false), (0, -3));
        let sideways = ScrollDelta::Lines { x: 2.0, y: 0.0 };
        assert_eq!(mouse.scroll(sideways, None, 16.0, true), (0, 2));
        assert_eq!(mouse.scroll(sideways, None, 16.0, false), (2, 0));
        // The rows of a scroll, which the view takes by pixels.
        assert_eq!(rows(points(-24.0), 16.0, false), (0.0, -1.5));
        assert_eq!(rows(sideways, 16.0, true), (0.0, 2.0));
        mouse.scroll(points(8.0), None, 16.0, false);
        mouse.settle_up();
        assert_eq!(mouse.scroll(points(8.0), None, 16.0, false), (0, 0));
    }

    #[test]
    fn a_wheel_s_glide_eases_out_to_its_rows() {
        let pane = PaneId::of(1);
        let start = Instant::now();
        let at = |ms| start + Duration::from_millis(ms);
        // Three rows up from the row numbered 40.
        let mut glide = Glide::new(pane, 3.0, start, (None, 40.0));
        assert_eq!(glide.end(), 37.0);
        // Fast at first: past half its rows in a quarter of its time.
        let (first, on) = glide.step(at(37));
        assert!(on);
        assert!(first < 38.5 && first > 37.0, "{first}");
        let (second, on) = glide.step(at(75));
        assert!(on);
        assert!(second < first && first - second < 40.0 - first, "{second}");
        // At its end, its rows whole, and no further.
        assert_eq!(glide.step(at(150)), (37.0, false));
        assert_eq!(glide.left(), 0.0);
        assert_eq!(glide.step(at(300)), (37.0, false));
        // A step during a glide adds to where it goes, from where it is.
        let mut glide = Glide::new(pane, 3.0, start, (None, 40.0));
        let (there, _) = glide.step(at(50));
        glide.add(-1.0, at(50), (None, 0.0));
        assert_eq!(glide.end(), 38.0);
        assert!((glide.left() - (there - 38.0)).abs() < 1e-9);
        assert_eq!(glide.step(at(200)), (38.0, false));
        // A view moved another way since starts it anew, from where the
        // view is, of the step's rows alone.
        let origin = Origin { epoch: 0, top: 10 };
        let mut glide = Glide::new(pane, 3.0, start, (Some((origin, 0.5)), 10.5));
        glide.add(1.0, at(10), (None, 60.0));
        assert_eq!((glide.left(), glide.end()), (1.0, 59.0));
        // A clock that goes back moves nothing.
        let mut glide = Glide::new(pane, 3.0, at(10), (None, 40.0));
        assert_eq!(glide.step(start), (40.0, true));
    }
}
