//! The control sequences: `CSI`, parameters, intermediates, then a final
//! byte.

use super::{CursorShape, CursorStyle, Terminal};
use crate::parser::Params;

impl Terminal {
    /// Dispatches the control sequence with `params`, `intermediates` and
    /// `final_byte`. Those that the terminal does not know do nothing.
    pub(super) fn csi(&mut self, params: &Params, intermediates: &[u8], final_byte: u8) {
        // A count, or a position from 1, which an empty or zero parameter
        // makes 1.
        let n = |index| params.value_or(index, 1);
        match (intermediates, final_byte) {
            ([], b'A') => self.cursor_up(n(0)),
            // CUD, and VPR.
            ([], b'B' | b'e') => self.cursor_down(n(0)),
            // CUF, and HPR.
            ([], b'C' | b'a') => self.cursor_forward(n(0)),
            ([], b'D') => self.cursor_back(n(0)),
            // CNL and CPL: down or up, to the left margin.
            ([], b'E') => {
                self.cursor_down(n(0));
                self.carriage_return();
            }
            ([], b'F') => {
                self.cursor_up(n(0));
                self.carriage_return();
            }
            // CHA, and HPA.
            ([], b'G' | b'`') => self.goto_column(n(0) - 1),
            // CUP, and HVP.
            ([], b'H' | b'f') => self.goto(n(0) - 1, n(1) - 1),
            ([], b'I') => self.tab_forward(n(0)),
            ([], b'Z') => self.tab_back(n(0)),
            ([], b'd') => self.goto_row(n(0) - 1),
            ([], b'g') => self.clear_tab_stops(params.value(0)),
            ([b' '], b'q') => self.set_cursor_style(params.value(0)),
            // ED and EL, and their selective forms, DECSED and DECSEL.
            ([], b'J') => self.erase_in_display(params.value(0), false),
            ([b'?'], b'J') => self.erase_in_display(params.value(0), true),
            ([], b'K') => self.erase_in_line(params.value(0), false),
            ([b'?'], b'K') => self.erase_in_line(params.value(0), true),
            ([], b'X') => self.erase_characters(n(0)),
            ([], b'@') => self.insert_characters(n(0)),
            ([], b'P') => self.delete_characters(n(0)),
            ([], b'L') => self.insert_lines(n(0)),
            ([], b'M') => self.delete_lines(n(0)),
            ([], b'S') => self.scroll_up(n(0)),
            // With five parameters, CSI T starts xterm's highlight mouse
            // tracking instead.
            ([], b'T') if params.len() <= 1 => self.scroll_down(n(0)),
            ([], b'b') => self.repeat(n(0)),
            // DECSCA: 1 protects the text printed next, 0 and 2 do not.
            ([b'"'], b'q') => match params.value(0) {
                1 => self.protected = true,
                0 | 2 => self.protected = false,
                _ => {}
            },
            _ => {}
        }
    }

    /// DECSCUSR: 1 and 2 make the cursor a block, 3 and 4 an underline, 5
    /// and 6 a bar, blinking with the odd values; 0 gives it back to the
    /// configuration. Other values do nothing.
    fn set_cursor_style(&mut self, style: u16) {
        let shape = match style {
            0 => {
                self.cursor_style = None;
                return;
            }
            1 | 2 => CursorShape::Block,
            3 | 4 => CursorShape::Underline,
            5 | 6 => CursorShape::Bar,
            _ => return,
        };
        self.cursor_style = Some(CursorStyle {
            shape,
            blinking: style % 2 == 1,
        });
    }
}
