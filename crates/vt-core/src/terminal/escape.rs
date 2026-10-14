//! The escape sequences: `ESC`, intermediates, then a final byte.

use super::Terminal;

impl Terminal {
    /// Dispatches the escape sequence with `intermediates` and
    /// `final_byte`. Those that the terminal does not know do nothing.
    pub(super) fn escape(&mut self, intermediates: &[u8], final_byte: u8) {
        match (intermediates, final_byte) {
            // IND: down a row, as LF.
            ([], b'D') => {
                self.moved();
                self.index();
            }
            // NEL: to the start of the next row.
            ([], b'E') => {
                self.carriage_return();
                self.index();
            }
            ([], b'H') => self.set_tab_stop(),
            ([], b'M') => self.reverse_index(),
            _ => {}
        }
    }

    /// HTS: makes the cursor's column a tab stop.
    fn set_tab_stop(&mut self) {
        let x = self.cursor().x;
        self.tab_stops[usize::from(x)] = true;
    }

    /// RI: up a row, scrolling the screen down at the top. The column
    /// stays.
    fn reverse_index(&mut self) {
        self.moved();
        let screen = self.screen_mut();
        screen.cursor.pending_wrap = false;
        if screen.cursor.y == 0 {
            self.scroll_down();
        } else {
            screen.cursor.y -= 1;
        }
    }

    /// Scrolls the screen down a row: the bottom row is dropped, the others
    /// move down, and an empty row appears at the top.
    fn scroll_down(&mut self) {
        let bottom = self.rows - 1;
        let rows = &mut self.screen_mut().rows;
        for y in (0..bottom).rev() {
            rows.move_row(y, y + 1);
        }
        // With a single row, nothing moved out of it.
        let (page, row) = rows.screen_row_mut(0);
        page.clear_row(row);
    }
}
