//! The escape sequences: `ESC`, intermediates, then a final byte.

use super::{Charset, Shift, Terminal};

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
            // SS2 and SS3: the next character from G2 or G3.
            ([], b'N') => self.charsets.set_single_shift(Shift::G2),
            ([], b'O') => self.charsets.set_single_shift(Shift::G3),
            // LS2 and LS3: G2 or G3 in use.
            ([], b'n') => self.charsets.lock(Shift::G2),
            ([], b'o') => self.charsets.lock(Shift::G3),
            // Designations of sets of 94 characters to G0–G3. Sets the
            // terminal does not know leave the slot as it was.
            ([slot @ (b'(' | b')' | b'*' | b'+')], _) => {
                let slot = match slot {
                    b'(' => Shift::G0,
                    b')' => Shift::G1,
                    b'*' => Shift::G2,
                    _ => Shift::G3,
                };
                if let Some(set) = Charset::designated_by(final_byte) {
                    self.charsets.designate(slot, set);
                }
            }
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
