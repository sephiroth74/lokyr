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
            // DA1, DA2 and DA3; DSR; XTVERSION; XTWINOPS.
            ([], b'c') if params.value(0) == 0 => self.reply(super::DA1),
            ([b'>'], b'c') if params.value(0) == 0 => self.reply(b"\x1b[>1;10;0c"),
            ([b'='], b'c') if params.value(0) == 0 => self.reply(b"\x1bP!|00000000\x1b\\"),
            ([], b'n') => self.device_status(params.value(0)),
            ([b'?'], b'n') => self.dec_device_status(params.value(0), params.value(1)),
            ([b'>'], b'q') if params.value(0) == 0 => self.report_version(),
            ([], b't') => self.window_ops(params),
            ([b'!'], b'p') => self.soft_reset(),
            ([b'"'], b'p') => self.set_conformance_level(params.value(0)),
            ([b'*'], b'x') if params.value(0) <= 2 => self.attribute_extent = params.value(0),
            ([b'*'], b'y') if self.rectangle_checksums => self.report_checksum(params),
            ([], b'm') => self.select_graphic_rendition(params),
            // XTMODKEYS for modifyOtherKeys, its reset (XTMODKEYS without a
            // value, or without parameters, which resets every resource and
            // reads as `CSI > 0 m`, and the older `CSI > 4 n`), and its
            // query.
            ([b'>'], b'm') if params.value(0) == 4 => {
                self.input.modify_other_keys = params.value(1).min(2) as u8;
            }
            ([b'>'], b'm') if params.len() == 1 && params.value(0) == 0 => {
                self.input.modify_other_keys = 0;
            }
            ([b'>'], b'n') if params.value(0) == 4 => self.input.modify_other_keys = 0,
            ([b'?'], b'm') if params.value(0) == 4 => {
                let reply = format!("\x1b[>4;{}m", self.input.modify_other_keys);
                self.reply(reply.as_bytes());
            }
            // SM and RM, DECSET and DECRST, and DECRQM for both kinds.
            ([], b'h') => self.set_ansi_modes(params, true),
            ([], b'l') => self.set_ansi_modes(params, false),
            ([b'?'], b'h') => self.set_private_modes(params, true),
            ([b'?'], b'l') => self.set_private_modes(params, false),
            ([b'$'], b'p') => self.report_ansi_mode(params.value(0)),
            ([b'?', b'$'], b'p') => self.report_private_mode(params.value(0)),
            ([], b'r') => self.set_top_bottom_margins(params.value(0), params.value(1)),
            // DECSLRM in left and right margin mode; otherwise SCOSC and
            // SCORC, which save and restore the cursor as DECSC and DECRC
            // do. With parameters or a private marker, CSI u belongs to the
            // kitty keyboard protocol.
            ([], b's') if self.left_right_margins => {
                self.set_left_right_margins(params.value(0), params.value(1));
            }
            ([], b's') => self.save_cursor(),
            ([], b'u') if params.len() <= 1 && params.value(0) == 0 => self.restore_cursor(),
            ([b'>'], b'u') => self.push_keyboard_flags(params.value(0)),
            ([b'<'], b'u') => self.pop_keyboard_flags(n(0)),
            ([b'='], b'u') => self.set_keyboard_flags(params.value(0), n(1)),
            ([b'?'], b'u') => self.report_keyboard_flags(),
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
