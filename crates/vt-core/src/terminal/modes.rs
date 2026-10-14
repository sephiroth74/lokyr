//! The modes that programs set, reset and query: ANSI's, with SM and RM,
//! and DEC's private ones, with DECSET and DECRST; DECRQM queries both.

use std::io::Write as _;

use super::{MouseEncoding, MouseTracking, Terminal};
use crate::parser::Params;
use crate::unicode::Mode;

impl Terminal {
    /// SM and RM: sets, if `on`, or resets the ANSI modes in `params`:
    /// insert mode (IRM, 4) and new-line mode (LNM, 20). Those that the
    /// terminal does not know do nothing.
    pub(super) fn set_ansi_modes(&mut self, params: &Params, on: bool) {
        for mode in params.iter().map(|group| group[0]) {
            match mode {
                4 => self.insert = on,
                20 => self.input.newline = on,
                _ => {}
            }
        }
    }

    /// DECSET and DECRST: sets, if `on`, or resets the DEC private modes in
    /// `params`. Those that the terminal does not know do nothing.
    pub(super) fn set_private_modes(&mut self, params: &Params, on: bool) {
        for mode in params.iter().map(|group| group[0]) {
            self.set_private_mode(mode, on);
        }
    }

    fn set_private_mode(&mut self, mode: u16, on: bool) {
        match mode {
            1 => self.input.cursor_keys_application = on,
            5 => self.reverse_video = on,
            6 => self.set_origin_mode(on),
            7 => self.autowrap = on,
            12 => self.cursor_blink = on,
            25 => self.cursor_visible = on,
            45 => self.reverse_wrap = on,
            // The alternate screen: 47 switches, 1047 also erases it on the
            // way out, and 1049 saves the cursor and erases it on the way
            // in, and brings the cursor back on the way out.
            47 => self.switch_screen(on),
            1047 => {
                if !on && self.alternate_active {
                    self.erase_screen();
                }
                self.switch_screen(on);
            }
            1048 if on => self.save_cursor(),
            1048 => self.restore_cursor(),
            1049 if on && !self.alternate_active => {
                self.save_cursor();
                self.switch_screen(true);
                self.erase_screen();
            }
            1049 if !on && self.alternate_active => {
                self.switch_screen(false);
                self.restore_cursor();
            }
            69 => self.set_left_right_margin_mode(on),
            9 | 1000 | 1002 | 1003 => {
                self.input.mouse_tracking = if on {
                    tracking(mode)
                } else {
                    MouseTracking::None
                };
            }
            1006 | 1016 => {
                self.input.mouse_encoding = if on {
                    encoding(mode)
                } else {
                    MouseEncoding::X10
                };
            }
            1004 => self.input.focus_events = on,
            1007 => self.input.alternate_scroll = on,
            1036 => self.input.meta_sends_escape = on,
            2004 => self.input.bracketed_paste = on,
            2026 => self.synchronized_output = on,
            2027 => {
                self.widths.mode = if on { Mode::Unicode } else { Mode::Legacy };
                self.moved();
            }
            _ => {}
        }
    }

    /// DECRQM for the ANSI mode `mode`.
    pub(super) fn report_ansi_mode(&mut self, mode: u16) {
        let state = match mode {
            4 => Some(self.insert),
            20 => Some(self.input.newline),
            _ => None,
        };
        self.reply_mode("", mode, state);
    }

    /// DECRQM for the DEC private mode `mode`.
    pub(super) fn report_private_mode(&mut self, mode: u16) {
        let state = self.private_mode(mode);
        self.reply_mode("?", mode, state);
    }

    /// Whether the DEC private mode `mode` is set, if the terminal knows it.
    fn private_mode(&self, mode: u16) -> Option<bool> {
        let input = self.input;
        Some(match mode {
            1 => input.cursor_keys_application,
            5 => self.reverse_video,
            6 => self.origin,
            7 => self.autowrap,
            12 => self.cursor_blink,
            25 => self.cursor_visible,
            45 => self.reverse_wrap,
            47 | 1047 | 1049 => self.alternate_active,
            1048 => self.screen().saved.is_some(),
            69 => self.left_right_margins,
            9 | 1000 | 1002 | 1003 => input.mouse_tracking == tracking(mode),
            1006 | 1016 => input.mouse_encoding == encoding(mode),
            1004 => input.focus_events,
            1007 => input.alternate_scroll,
            1036 => input.meta_sends_escape,
            2004 => input.bracketed_paste,
            2026 => self.synchronized_output,
            2027 => self.widths.mode == Mode::Unicode,
            _ => return None,
        })
    }

    /// Queues the reply to DECRQM: `CSI` `marker` `mode` `;` the state `$y`,
    /// where the state is 0 for a mode the terminal does not know, 1 for
    /// set and 2 for reset.
    fn reply_mode(&mut self, marker: &str, mode: u16, state: Option<bool>) {
        let value = match state {
            None => 0,
            Some(true) => 1,
            Some(false) => 2,
        };
        write!(self.replies, "\x1b[{marker}{mode};{value}$y").expect("writing to a Vec");
    }

    /// Makes the alternate screen, or the primary one, the active one, as
    /// the DEC modes do: the cursor goes along, as it does in xterm, which
    /// has one cursor for both.
    fn switch_screen(&mut self, alternate: bool) {
        if self.alternate_active == alternate {
            return;
        }
        let cursor = self.cursor();
        self.set_alternate_screen(alternate);
        self.screen_mut().cursor = cursor;
    }

    /// Erases every row of the active screen.
    fn erase_screen(&mut self) {
        for y in 0..self.rows {
            self.erase_in_row(y, 0..self.cols, false);
        }
    }
}

/// The mouse tracking that mode 9, 1000, 1002 or 1003 asks for.
fn tracking(mode: u16) -> MouseTracking {
    match mode {
        9 => MouseTracking::X10,
        1000 => MouseTracking::Normal,
        1002 => MouseTracking::ButtonEvent,
        _ => MouseTracking::AnyEvent,
    }
}

/// The mouse encoding that mode 1006 or 1016 asks for.
fn encoding(mode: u16) -> MouseEncoding {
    match mode {
        1006 => MouseEncoding::Sgr,
        _ => MouseEncoding::SgrPixels,
    }
}
