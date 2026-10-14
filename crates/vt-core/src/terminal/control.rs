//! The C0 controls.

use super::{Cursor, Shift, Terminal};

impl Terminal {
    /// Executes a C0 control. Those that a terminal has no use for, such as
    /// NUL, do nothing.
    pub(super) fn control(&mut self, byte: u8) {
        match byte {
            0x07 => self.bells += 1,
            0x08 => self.backspace(),
            0x09 => self.horizontal_tab(),
            // LF, VT and FF all feed a line.
            0x0A..=0x0C => self.line_feed(),
            0x0D => self.carriage_return(),
            0x0E => self.charsets.lock(Shift::G1),
            0x0F => self.charsets.lock(Shift::G0),
            _ => {}
        }
    }

    /// BS: one column to the left, as CUB 1 moves.
    fn backspace(&mut self) {
        self.cursor_back(1);
    }

    /// HT: to the next tab stop, or to the right margin from within the
    /// margins and to the last column from right of them.
    pub(super) fn horizontal_tab(&mut self) {
        self.moved();
        let x = self.cursor().x;
        let last = if x <= self.margins.right {
            self.margins.right
        } else {
            self.cols - 1
        };
        let next = (x + 1..last)
            .find(|&column| self.tab_stops[usize::from(column)])
            .unwrap_or(last);
        let cursor = &mut self.screen_mut().cursor;
        cursor.x = next.max(x);
        cursor.pending_wrap = false;
    }

    /// LF: one row down, scrolling the scroll region up at its bottom
    /// margin. The column stays, unless LNM makes it a new line.
    fn line_feed(&mut self) {
        self.moved();
        self.index();
        if self.input.newline {
            self.carriage_return();
        }
    }

    /// One row down, scrolling the scroll region up at its bottom margin,
    /// without ending the cluster being printed, as wrapping needs. At the
    /// bottom margin but outside the left and right ones, the cursor stays;
    /// below the region, the last row stops it.
    pub(super) fn index(&mut self) {
        self.screen_mut().cursor.pending_wrap = false;
        let Cursor { x, y, .. } = self.cursor();
        if y == self.margins.bottom {
            if self.margins.has_column(x) {
                self.scroll_region_up(1);
            }
        } else if y + 1 < self.rows {
            self.screen_mut().cursor.y += 1;
        }
    }

    /// CR: to the left margin, or to the first column from left of it
    /// outside origin mode.
    pub(super) fn carriage_return(&mut self) {
        self.moved();
        let (left, origin) = (self.margins.left, self.origin);
        let cursor = &mut self.screen_mut().cursor;
        cursor.x = if origin || cursor.x >= left { left } else { 0 };
        cursor.pending_wrap = false;
    }
}
