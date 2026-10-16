//! Moving the cursor, and the tab stops.
//!
//! The relative moves stop at the margins when they start within them, and
//! at the edges of the screen otherwise; the absolute ones count from the
//! top left of the screen, or of the margins in origin mode, and stay
//! within them. All of them end the cluster being printed, and leave no
//! wrap pending.

use super::{Cursor, Terminal};
use crate::page::RowFlag;

impl Terminal {
    /// CUU: up `n` rows.
    pub(super) fn cursor_up(&mut self, n: u16) {
        let Cursor { x, y, .. } = self.cursor();
        let top = if y >= self.margins.top {
            self.margins.top
        } else {
            0
        };
        self.move_to(x, y.saturating_sub(n).max(top));
    }

    /// CUD: down `n` rows.
    pub(super) fn cursor_down(&mut self, n: u16) {
        let Cursor { x, y, .. } = self.cursor();
        let bottom = if y <= self.margins.bottom {
            self.margins.bottom
        } else {
            self.rows - 1
        };
        self.move_to(x, y.saturating_add(n).min(bottom));
    }

    /// CUF: right `n` columns.
    pub(super) fn cursor_forward(&mut self, n: u16) {
        let Cursor { x, y, .. } = self.cursor();
        let right = if x <= self.margins.right {
            self.margins.right
        } else {
            self.cols - 1
        };
        self.move_to(x.saturating_add(n).min(right), y);
    }

    /// CUB, and BS: left `n` columns.
    ///
    /// With reverse wraparound and autowrap on, a move past the left margin
    /// goes on from the right margin of the row above, as long as that row
    /// wraps into this one, as far as the top margin. A pending wrap takes
    /// one of the moves, as in xterm.
    pub(super) fn cursor_back(&mut self, n: u16) {
        let Cursor {
            mut x,
            mut y,
            pending_wrap,
        } = self.cursor();
        let left = if x >= self.margins.left {
            self.margins.left
        } else {
            0
        };
        if !(self.reverse_wrap && self.autowrap) {
            self.move_to(x.saturating_sub(n).max(left), y);
            return;
        }
        let mut count = if pending_wrap { n - 1 } else { n };
        loop {
            let step = count.min(x - left);
            x -= step;
            count -= step;
            if count == 0 || y <= self.margins.top {
                break;
            }
            // The row above wraps into this one only when this one is its
            // continuation too: rows that edits moved can keep the flag.
            let rows = &self.screen().rows;
            let (above, row_above) = rows.screen_row(y - 1);
            let (page, row) = rows.screen_row(y);
            if !(above.row(row_above).has(RowFlag::SoftWrapped)
                && page.row(row).has(RowFlag::Continuation))
            {
                break;
            }
            (x, y) = (self.margins.right, y - 1);
            count -= 1;
        }
        self.move_to(x, y);
    }

    /// CUP and HVP: to row `row` and column `col`, both from 0.
    pub(super) fn goto(&mut self, row: u16, col: u16) {
        let (top, bottom, left, right) = self.bounds();
        self.move_to(
            left.saturating_add(col).min(right),
            top.saturating_add(row).min(bottom),
        );
    }

    /// CHA and HPA: to column `col`, from 0, in the same row.
    pub(super) fn goto_column(&mut self, col: u16) {
        let (_, _, left, right) = self.bounds();
        let y = self.cursor().y;
        self.move_to(left.saturating_add(col).min(right), y);
    }

    /// VPA: to row `row`, from 0, in the same column.
    pub(super) fn goto_row(&mut self, row: u16) {
        let (top, bottom, _, _) = self.bounds();
        let x = self.cursor().x;
        self.move_to(x, top.saturating_add(row).min(bottom));
    }

    /// The top, bottom, left and right that absolute positions count from
    /// and stay within: the margins in origin mode, the screen otherwise.
    fn bounds(&self) -> (u16, u16, u16, u16) {
        if self.origin {
            let margins = self.margins;
            (margins.top, margins.bottom, margins.left, margins.right)
        } else {
            (0, self.rows - 1, 0, self.cols - 1)
        }
    }

    /// Puts the cursor at column `x` of row `y`, which are on the screen.
    fn move_to(&mut self, x: u16, y: u16) {
        self.moved();
        self.screen_mut().cursor = Cursor {
            x,
            y,
            pending_wrap: false,
        };
    }

    /// DECSTBM: makes rows `top` to `bottom`, from 1, the top and bottom
    /// margins; 0 for `bottom` is the last row. A region of fewer than two
    /// rows is ignored. The cursor goes home.
    pub(super) fn set_top_bottom_margins(&mut self, top: u16, bottom: u16) {
        let top = top.max(1) - 1;
        let bottom = match bottom {
            0 => self.rows,
            bottom => bottom.min(self.rows),
        } - 1;
        if top >= bottom {
            return;
        }
        self.margins.top = top;
        self.margins.bottom = bottom;
        self.goto(0, 0);
    }

    /// DECSLRM: makes columns `left` to `right`, from 1, the left and right
    /// margins; 0 for `right` is the last column. Margins of fewer than two
    /// columns are ignored. The cursor goes home.
    pub(super) fn set_left_right_margins(&mut self, left: u16, right: u16) {
        let left = left.max(1) - 1;
        let right = match right {
            0 => self.cols,
            right => right.min(self.cols),
        } - 1;
        if left >= right {
            return;
        }
        self.margins.left = left;
        self.margins.right = right;
        self.goto(0, 0);
    }

    /// CHT: forward `n` tab stops.
    pub(super) fn tab_forward(&mut self, n: u16) {
        for _ in 0..n {
            let x = self.cursor().x;
            self.horizontal_tab();
            if self.cursor().x == x {
                break;
            }
        }
    }

    /// CBT: back `n` tab stops, or, when none is left, to the left margin
    /// from within the margins and to the first column from left of them.
    pub(super) fn tab_back(&mut self, n: u16) {
        let Cursor { mut x, y, .. } = self.cursor();
        let first = if x >= self.margins.left {
            self.margins.left
        } else {
            0
        };
        for _ in 0..n {
            if x <= first {
                break;
            }
            x = (first..x)
                .rev()
                .find(|&column| self.tab_stops[usize::from(column)])
                .unwrap_or(first);
        }
        self.move_to(x, y);
    }

    /// TBC: clears the tab stop at the cursor with 0, and every tab stop
    /// with 3. Other values do nothing.
    pub(super) fn clear_tab_stops(&mut self, which: u16) {
        match which {
            0 => {
                let x = self.cursor().x;
                self.tab_stops[usize::from(x)] = false;
            }
            3 => self.tab_stops.fill(false),
            _ => {}
        }
    }
}
