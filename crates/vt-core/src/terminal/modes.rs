//! The modes that programs set, reset and query: ANSI's, with SM and RM,
//! and DEC's private ones, with DECSET and DECRST; DECRQM queries both.

use super::{Cursor, Margins, MouseEncoding, MouseTracking, Terminal};
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
            3 => self.set_column_mode(on),
            7 => self.autowrap = on,
            40 => self.column_switch = on,
            41 => self.tab_wraps = on,
            95 => self.keep_screen = on,
            4 => self.smooth_scroll = on,
            12 => self.cursor_blink = on,
            25 => self.cursor_visible = on,
            45 => self.reverse_wrap = on,
            1045 => self.reverse_wrap_extended = on,
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
            // The numeric keypad mode (DECNKM), as DECKPAM and DECKPNM.
            66 => self.input.keypad_application = on,
            67 => self.input.backspace_sends_bs = on,
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
            2026 => {
                // A start during an update goes on with it.
                self.synchronized_updates += u64::from(on && !self.synchronized_output);
                self.synchronized_output = on;
            }
            2027 => {
                self.widths.mode = if on { Mode::Unicode } else { Mode::Legacy };
                self.moved();
            }
            2031 => self.color_scheme_reports = on,
            _ => {}
        }
    }

    /// XTSAVE, `CSI ? Pm s`: saves the state of each DEC private mode of
    /// `params` that the terminal knows, for XTRESTORE.
    pub(super) fn save_private_modes(&mut self, params: &Params) {
        for mode in params.iter().map(|group| group[0]) {
            if let Some(on) = self.private_mode(mode) {
                self.saved_modes.retain(|&(saved, _)| saved != mode);
                self.saved_modes.push((mode, on));
            }
        }
    }

    /// XTRESTORE, `CSI ? Pm r`: sets each DEC private mode of `params` back
    /// as XTSAVE saved it, if it did.
    pub(super) fn restore_private_modes(&mut self, params: &Params) {
        for mode in params.iter().map(|group| group[0]) {
            let saved = self.saved_modes.iter().find(|&&(saved, _)| saved == mode);
            if let Some(&(_, on)) = saved {
                self.set_private_mode(mode, on);
            }
        }
    }

    /// DECCOLM, mode 3: 132 columns, or 80, if the configuration allows the
    /// window's operations and mode 40 the switch, where xterm needs mode
    /// 40 alone, which programs set themselves. The screen is erased,
    /// unless DECNCSM keeps it; the margins go, as DECLRMM does with 132
    /// columns, and the cursor goes home.
    fn set_column_mode(&mut self, on: bool) {
        if !self.window_ops || !self.column_switch {
            return;
        }
        self.columns_132 = on;
        self.take_size(if on { 132 } else { 80 }, self.rows);
        if !self.keep_screen {
            self.erase_screen();
        }
        if on {
            self.left_right_margins = false;
        }
        self.margins = Margins::screen(self.cols, self.rows);
        self.screen_mut().cursor = Cursor::default();
        self.moved();
    }

    /// DECRQM for the ANSI mode `mode`.
    pub(super) fn report_ansi_mode(&mut self, mode: u16) {
        let value = match mode {
            4 => value(Some(self.insert)),
            20 => value(Some(self.input.newline)),
            // The modes of ECMA-48 that xterm keeps reset for good: GATM,
            // SRTM, VEM, HEM, PUM, FEAM, FETM, MATM, TTM, SATM, TSM and EBM.
            1 | 5 | 7 | 10 | 11 | 13..=19 => PERMANENTLY_RESET,
            _ => value(None),
        };
        self.reply_mode("", mode, value);
    }

    /// DECRQM for the DEC private mode `mode`.
    pub(super) fn report_private_mode(&mut self, mode: u16) {
        let value = match mode {
            // Horizontal cursor coupling (DECHCCM), which xterm keeps reset
            // for good.
            60 => PERMANENTLY_RESET,
            _ => value(self.private_mode(mode)),
        };
        self.reply_mode("?", mode, value);
    }

    /// Whether the DEC private mode `mode` is set, if the terminal knows it.
    fn private_mode(&self, mode: u16) -> Option<bool> {
        let input = self.input;
        Some(match mode {
            1 => input.cursor_keys_application,
            4 => self.smooth_scroll,
            5 => self.reverse_video,
            6 => self.origin,
            3 => self.columns_132,
            7 => self.autowrap,
            40 => self.column_switch,
            41 => self.tab_wraps,
            95 => self.keep_screen,
            12 => self.cursor_blink,
            25 => self.cursor_visible,
            45 => self.reverse_wrap,
            1045 => self.reverse_wrap_extended,
            47 | 1047 | 1049 => self.alternate_active,
            1048 => self.screen().saved.is_some(),
            69 => self.left_right_margins,
            66 => input.keypad_application,
            67 => input.backspace_sends_bs,
            9 | 1000 | 1002 | 1003 => input.mouse_tracking == tracking(mode),
            1006 | 1016 => input.mouse_encoding == encoding(mode),
            1004 => input.focus_events,
            1007 => input.alternate_scroll,
            1036 => input.meta_sends_escape,
            2004 => input.bracketed_paste,
            2026 => self.synchronized_output,
            2027 => self.widths.mode == Mode::Unicode,
            2031 => self.color_scheme_reports,
            _ => return None,
        })
    }

    /// Queues the reply to DECRQM: `CSI` `marker` `mode` `;` the state's
    /// `value` `$y`.
    fn reply_mode(&mut self, marker: &str, mode: u16, value: u8) {
        let reply = format!("\x1b[{marker}{mode};{value}$y");
        self.reply(reply.as_bytes());
    }

    /// Makes the alternate screen, or the primary one, the active one, as
    /// the DEC modes do: the cursor goes along, as it does in xterm, which
    /// has one cursor for both.
    fn switch_screen(&mut self, alternate: bool) {
        if self.alternate_active == alternate {
            return;
        }
        let cursor = self.cursor();
        // The alternate screen's images go with the screen that its
        // program left.
        if !alternate {
            self.drop_alternate_images();
        }
        self.set_alternate_screen(alternate);
        self.screen_mut().cursor = cursor;
    }

    /// Erases every row of the active screen, and the alternate screen's
    /// images with them.
    fn erase_screen(&mut self) {
        if self.alternate_active {
            self.drop_alternate_images();
        }
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

/// What DECRQM tells of a mode that is permanently reset.
const PERMANENTLY_RESET: u8 = 4;

/// What DECRQM tells of a mode that is set, `Some(true)`, reset,
/// `Some(false)`, or that the terminal does not know, `None`: 1, 2 and 0.
fn value(state: Option<bool>) -> u8 {
    match state {
        None => 0,
        Some(true) => 1,
        Some(false) => 2,
    }
}
