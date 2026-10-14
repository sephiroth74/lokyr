//! The C0 controls.

use super::{Shift, Terminal};

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

    /// BS: one column to the left, if there is one. With the wrap pending,
    /// the cursor leaves the last column.
    fn backspace(&mut self) {
        self.moved();
        let cursor = &mut self.screen_mut().cursor;
        cursor.pending_wrap = false;
        cursor.x = cursor.x.saturating_sub(1);
    }

    /// HT: to the next tab stop, or to the last column.
    fn horizontal_tab(&mut self) {
        self.moved();
        let last = self.cols - 1;
        let x = self.cursor().x;
        let next = (x + 1..last)
            .find(|&column| self.tab_stops[usize::from(column)])
            .unwrap_or(last);
        let cursor = &mut self.screen_mut().cursor;
        cursor.x = next.max(x);
        cursor.pending_wrap = false;
    }

    /// LF: one row down, scrolling the screen up at the bottom. The column
    /// stays.
    fn line_feed(&mut self) {
        self.moved();
        self.index();
    }

    /// One row down, scrolling the screen up at the bottom, without ending
    /// the cluster being printed, as wrapping needs.
    pub(super) fn index(&mut self) {
        let rows = self.rows;
        let screen = self.screen_mut();
        screen.cursor.pending_wrap = false;
        if screen.cursor.y + 1 == rows {
            screen.rows.scroll_up();
        } else {
            screen.cursor.y += 1;
        }
    }

    /// CR: to the first column.
    pub(super) fn carriage_return(&mut self) {
        self.moved();
        let cursor = &mut self.screen_mut().cursor;
        cursor.x = 0;
        cursor.pending_wrap = false;
    }
}
