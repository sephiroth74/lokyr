//! The mouse over a pane: where the pointer is in its cells, the buttons
//! that the program learned were held, and the wheel's scroll, which adds up
//! to whole steps.

use lokyr_kernel::workspace::Grid;
use lokyr_platform_core::{MouseButton as Button, Point, ScrollDelta, ScrollPhase};
use lokyr_vt_core::input::MouseButton;

/// Where the pointer is in a grid: its cell, and its pixel from the cells'
/// top left corner.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Pointer {
    pub(crate) col: u16,
    pub(crate) row: u16,
    pub(crate) x: u32,
    pub(crate) y: u32,
}

/// The pointer at `position`, in points from the content's top left corner,
/// over `grid`, in a window of `scale`, whose cells start `padding` pixels
/// from its left and top edges: a pointer over the padding, or past the
/// cells, takes the nearest cell's edge.
pub(crate) fn pointer(position: Point, scale: f64, padding: (u32, u32), grid: Grid) -> Pointer {
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
    let (y, row) = within(position.y, padding.1, grid.cell_height, grid.rows);
    Pointer { col, row, x, y }
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

/// `pointer` at the screen's top row, where an event that must reach the
/// program goes from above the screen.
pub(crate) fn at_top(pointer: Pointer) -> Pointer {
    Pointer {
        row: 0,
        y: 0,
        ..pointer
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

/// A window's mouse: the buttons pressed that the program learned of,
/// where it last learned the pointer was, and the scroll that adds up to the
/// wheel's next steps.
#[derive(Debug, Default)]
pub(crate) struct Mouse {
    held: Vec<MouseButton>,
    last: Option<Pointer>,
    /// The scroll that makes no whole step yet, across and up, in rows.
    rest: (f64, f64),
}

impl Mouse {
    /// Notes that `button` went down.
    pub(crate) fn press(&mut self, button: MouseButton) {
        if !self.held.contains(&button) {
            self.held.push(button);
        }
    }

    /// Notes that `button` went up, and returns whether it was down: a
    /// release whose press the program did not learn of goes nowhere.
    pub(crate) fn release(&mut self, button: MouseButton) -> bool {
        let held = self.held.contains(&button);
        self.held.retain(|&other| other != button);
        held
    }

    /// The lowest-numbered of the buttons down, which a motion reports.
    pub(crate) fn held(&self) -> Option<MouseButton> {
        self.held
            .iter()
            .copied()
            .min_by_key(|&button| number(button))
    }

    /// Notes that the program learns the pointer is at `pointer`, and
    /// returns whether that is news: another cell, or with `pixels`, as
    /// reports in pixels have it, another pixel.
    pub(crate) fn moved(&mut self, pointer: Pointer, pixels: bool) -> bool {
        let new = match self.last {
            Some(last) if pixels => last != pointer,
            Some(last) => (last.col, last.row) != (pointer.col, pointer.row),
            None => true,
        };
        self.last = Some(pointer);
        new
    }

    /// The whole steps, across and up, that `delta` makes with the scroll
    /// that made none yet, in rows of `row_points` points: a wheel's lines,
    /// or a trackpad's points. A gesture that starts, and a scroll the other
    /// way, start the sum again. With `sideways`, a scroll across alone, as
    /// Shift makes a mouse's wheel, goes up and down. A step across goes
    /// toward the content's left edge, and a step up toward its top, when
    /// positive.
    pub(crate) fn scroll(
        &mut self,
        delta: ScrollDelta,
        phase: Option<ScrollPhase>,
        row_points: f64,
        sideways: bool,
    ) -> (i32, i32) {
        let (mut across, mut up) = match delta {
            ScrollDelta::Lines { x, y } => (x, y),
            ScrollDelta::Points { x, y } => (x / row_points, y / row_points),
        };
        if sideways && up == 0.0 {
            (across, up) = (0.0, across);
        }
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
        let at = |x, y| pointer(Point { x, y }, 2.0, (8, 8), grid());
        // 8 pixels of padding, at a scale of 2: the first cell from 4 points.
        assert_eq!(
            at(4.0, 4.0),
            Pointer {
                col: 0,
                row: 0,
                x: 0,
                y: 0
            }
        );
        assert_eq!(
            at(12.5, 21.0),
            Pointer {
                col: 1,
                row: 1,
                x: 17,
                y: 34
            }
        );
        // Over the padding, and past the last cell.
        assert_eq!(
            at(0.0, 0.0),
            Pointer {
                col: 0,
                row: 0,
                x: 0,
                y: 0
            }
        );
        let far = at(5000.0, 5000.0);
        assert_eq!(
            (far.col, far.row, far.x, far.y),
            (79, 23, 80 * 16 - 1, 24 * 32 - 1)
        );
    }

    #[test]
    fn a_pointer_in_a_scrolled_view_takes_the_screen_s_row() {
        let at = |row, y| Pointer {
            col: 3,
            row,
            x: 50,
            y,
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
    }

    #[test]
    fn buttons_take_x11_s_numbers_and_the_lowest_held_reports() {
        assert_eq!(button(Button::Other(5)), Some(MouseButton::Button10));
        assert_eq!(button(Button::Other(7)), None);
        let mut mouse = Mouse::default();
        assert!(!mouse.release(MouseButton::Left), "never pressed");
        mouse.press(MouseButton::Right);
        mouse.press(MouseButton::Left);
        assert_eq!(mouse.held(), Some(MouseButton::Left));
        assert!(mouse.release(MouseButton::Left));
        assert_eq!(mouse.held(), Some(MouseButton::Right));
    }

    #[test]
    fn a_move_is_news_in_another_cell_or_pixel() {
        let mut mouse = Mouse::default();
        let at = |col, x| Pointer {
            col,
            row: 0,
            x,
            y: 0,
        };
        assert!(mouse.moved(at(0, 1), false));
        assert!(!mouse.moved(at(0, 5), false), "the same cell");
        assert!(mouse.moved(at(0, 6), true), "another pixel");
        assert!(mouse.moved(at(1, 17), false));
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
    }
}
