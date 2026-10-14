//! The escape sequences: `ESC`, intermediates, then a final byte.

use super::{Charset, Cursor, Options, SavedCursor, Shift, Terminal};
use crate::page::{Cell, Content};

impl Terminal {
    /// Dispatches the escape sequence with `intermediates` and
    /// `final_byte`. Those that the terminal does not know do nothing.
    pub(super) fn escape(&mut self, intermediates: &[u8], final_byte: u8) {
        match (intermediates, final_byte) {
            ([], b'7') => self.save_cursor(),
            ([], b'8') => self.restore_cursor(),
            // DECKPAM and DECKPNM.
            ([], b'=') => self.keypad_application = true,
            ([], b'>') => self.keypad_application = false,
            ([], b'c') => self.full_reset(),
            ([b'#'], b'8') => self.screen_alignment(),
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

    /// DECSC: saves the cursor, with its pending wrap, the pen, the
    /// character sets and origin mode, for the active screen.
    pub(super) fn save_cursor(&mut self) {
        let saved = SavedCursor {
            cursor: self.cursor(),
            pen: self.pen,
            charsets: self.charsets,
            origin: self.origin,
        };
        self.screen_mut().saved = Some(saved);
    }

    /// DECRC: brings back what DECSC saved for the active screen, or, with
    /// nothing saved, puts the cursor at the top left with the default pen
    /// and character sets, and origin mode off.
    pub(super) fn restore_cursor(&mut self) {
        self.moved();
        let saved = self.screen().saved.unwrap_or_default();
        self.screen_mut().cursor = saved.cursor;
        self.pen = saved.pen;
        self.charsets = saved.charsets;
        self.origin = saved.origin;
    }

    /// RIS: puts the terminal back as it was made, with its screens and
    /// scrollback empty. It keeps its configuration: the size, the
    /// scrollback limits, the widths and the normalization. The bells that
    /// rang stay counted.
    fn full_reset(&mut self) {
        let options = Options {
            scrollback: self.primary.rows.scrollback().clone(),
            widths: self.widths,
            normalization: self.normalization,
        };
        let bells = self.bells;
        *self = Self::new(self.cols, self.rows, options);
        self.bells = bells;
    }

    /// DECALN: fills the screen with `E`, in the default style, and puts the
    /// cursor at the top left.
    fn screen_alignment(&mut self) {
        self.moved();
        let fill = Cell::EMPTY.with_content(Content::Char('E'));
        let cols = self.cols;
        let screen = self.screen_mut();
        for y in 0..screen.rows.rows() {
            let (page, row) = screen.rows.screen_row_mut(y);
            page.clear_row(row);
            for x in 0..cols {
                page.set_cell(row, x, fill);
            }
        }
        screen.cursor = Cursor::default();
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
