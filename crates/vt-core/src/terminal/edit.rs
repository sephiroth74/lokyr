//! Inserting and deleting: cells within the cursor's row, and rows within
//! the scroll region, which scrolling moves as well.
//!
//! What comes in is erased as the erases leave it, with the pen's
//! background color. Each of these ends the cluster being printed.

use super::{Cursor, Terminal};
use crate::page::Cell;

impl Terminal {
    /// ICH: inserts `n` erased cells at the cursor; the cells from there
    /// move right, and those that pass the end of the row are dropped.
    pub(super) fn insert_characters(&mut self, n: u16) {
        let Cursor { x, y, .. } = self.editing();
        let (blank, cols) = (self.blank(), self.cols);
        let (page, row) = self.screen_mut().rows.screen_row_mut(y);
        page.insert_blanks(row, x, n);
        page.erase(row, x..x.saturating_add(n).min(cols), blank, false);
    }

    /// DCH: deletes `n` cells at the cursor; the cells after them move left,
    /// and erased cells come in at the end of the row.
    pub(super) fn delete_characters(&mut self, n: u16) {
        let Cursor { x, y, .. } = self.editing();
        let (blank, cols) = (self.blank(), self.cols);
        let n = n.min(cols - x);
        let (page, row) = self.screen_mut().rows.screen_row_mut(y);
        page.delete_cells(row, x, n);
        page.erase(row, cols - n..cols, blank, false);
    }

    /// IL: inserts `n` erased rows at the cursor's row, which move down
    /// with the rows below them, as far as the bottom margin. The cursor
    /// goes to the left margin. Outside the scroll region, it does nothing.
    pub(super) fn insert_lines(&mut self, n: u16) {
        let Some(y) = self.row_in_region() else {
            return;
        };
        self.shift_rows_down(y, self.margins.bottom, n);
        self.move_to_left_margin();
    }

    /// DL: deletes `n` rows from the cursor's row: the rows below them move
    /// up, and erased rows come in above the bottom margin. The cursor goes
    /// to the left margin. Outside the scroll region, it does nothing.
    pub(super) fn delete_lines(&mut self, n: u16) {
        let Some(y) = self.row_in_region() else {
            return;
        };
        self.shift_rows_up(y, self.margins.bottom, n);
        self.move_to_left_margin();
    }

    /// SU: scrolls the scroll region up `n` rows. When the region is the
    /// whole screen, the rows that leave it go into the scrollback.
    pub(super) fn scroll_up(&mut self, n: u16) {
        self.moved();
        let (top, bottom) = (self.margins.top, self.margins.bottom);
        if top == 0 && bottom == self.rows - 1 {
            let n = n.min(self.rows);
            for _ in 0..n {
                self.screen_mut().rows.scroll_up();
            }
            for y in self.rows - n..self.rows {
                self.blank_row(y);
            }
        } else {
            self.shift_rows_up(top, bottom, n);
        }
    }

    /// SD: scrolls the scroll region down `n` rows.
    pub(super) fn scroll_down(&mut self, n: u16) {
        self.moved();
        self.shift_rows_down(self.margins.top, self.margins.bottom, n);
    }

    /// REP: prints the last character printed `n` more times.
    pub(super) fn repeat(&mut self, n: u16) {
        if let Some(c) = self.last_printed {
            for _ in 0..n {
                self.print_translated(c);
            }
        }
    }

    /// Moves the rows from `top` to `bottom` down `n` rows: those that pass
    /// `bottom` are dropped, and erased rows come in at `top`.
    pub(super) fn shift_rows_down(&mut self, top: u16, bottom: u16, n: u16) {
        let n = n.min(bottom - top + 1);
        let rows = &mut self.screen_mut().rows;
        for y in (top..bottom + 1 - n).rev() {
            rows.move_row(y, y + n);
        }
        for y in top..top + n {
            self.blank_row(y);
        }
    }

    /// Moves the rows from `top` to `bottom` up `n` rows: those that pass
    /// `top` are dropped, and erased rows come in at `bottom`.
    fn shift_rows_up(&mut self, top: u16, bottom: u16, n: u16) {
        let n = n.min(bottom - top + 1);
        let rows = &mut self.screen_mut().rows;
        for y in top + n..=bottom {
            rows.move_row(y, y - n);
        }
        for y in bottom + 1 - n..=bottom {
            self.blank_row(y);
        }
    }

    /// Makes row `y` a new row, erased: no cells but erased ones, and no
    /// wrap.
    fn blank_row(&mut self, y: u16) {
        let (blank, cols) = (self.blank(), self.cols);
        let (page, row) = self.screen_mut().rows.screen_row_mut(y);
        page.clear_row(row);
        if blank != Cell::EMPTY {
            page.erase(row, 0..cols, blank, false);
        }
    }

    /// Ends the cluster and the pending wrap before editing the cursor's
    /// row, and returns the cursor.
    fn editing(&mut self) -> Cursor {
        self.moved();
        self.screen_mut().cursor.pending_wrap = false;
        self.cursor()
    }

    /// The cursor's row, if it is within the scroll region, after ending
    /// the cluster and the pending wrap.
    fn row_in_region(&mut self) -> Option<u16> {
        let y = self.editing().y;
        (self.margins.top..=self.margins.bottom)
            .contains(&y)
            .then_some(y)
    }

    /// Puts the cursor at the left margin of its row.
    fn move_to_left_margin(&mut self) {
        let left = self.margins.left;
        self.screen_mut().cursor.x = left;
    }
}
