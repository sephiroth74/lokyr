//! The escape sequences: `ESC`, intermediates, then a final byte.

use super::{Charset, Charsets, Cursor, Event, Margins, Options, SavedCursor, Shift, Terminal};
use crate::page::{Cell, Content, Scrollback, Style};

impl Terminal {
    /// Dispatches the escape sequence with `intermediates` and
    /// `final_byte`. Those that the terminal does not know do nothing.
    pub(super) fn escape(&mut self, intermediates: &[u8], final_byte: u8) {
        match (intermediates, final_byte) {
            ([], b'7') => self.save_cursor(),
            ([], b'8') => self.restore_cursor(),
            // SPA and EPA: ISO's protection of the text printed between.
            ([], b'V') => {
                self.protected = true;
                self.iso_protection = true;
            }
            ([], b'W') => self.protected = false,
            // DECBI and DECFI.
            ([], b'6') => self.back_index(),
            ([], b'9') => self.forward_index(),
            // DECKPAM and DECKPNM.
            ([], b'=') => self.input.keypad_application = true,
            ([], b'>') => self.input.keypad_application = false,
            ([], b'c') => self.full_reset(),
            // DECID, which DA took the place of, and answers as DA does.
            ([], b'Z') => self.reply(super::DA1),
            ([b'#'], b'8') => self.screen_alignment(),
            // IND: down a row, as LF.
            ([], b'D') => {
                self.moved();
                self.index();
            }
            // NEL: to the start of the next row, indexing first as xterm
            // does, which matters right of the right margin.
            ([], b'E') => {
                self.index();
                self.carriage_return();
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

    /// DECSC: saves the cursor, with its pending wrap, the pen and its
    /// protection, the character sets and origin mode, for the active
    /// screen.
    pub(super) fn save_cursor(&mut self) {
        let saved = SavedCursor {
            cursor: self.cursor(),
            pen: self.pen,
            charsets: self.charsets,
            origin: self.origin,
            protected: self.protected,
        };
        self.screen_mut().saved = Some(saved);
    }

    /// DECRC: brings back what DECSC saved for the active screen, or, with
    /// nothing saved, puts the cursor at the top left with the default pen
    /// and character sets, and origin mode off. In origin mode the cursor
    /// goes no lower than the scroll region's bottom margin, nor past the
    /// right margin, as xterm sets it; above the region, or left of the
    /// left margin, it stays, as there.
    pub(super) fn restore_cursor(&mut self) {
        self.moved();
        let saved = self.screen().saved.unwrap_or_default();
        let mut cursor = saved.cursor;
        if saved.origin {
            cursor.y = cursor.y.min(self.margins.bottom);
            cursor.x = cursor.x.min(self.margins.right);
        }
        self.screen_mut().cursor = cursor;
        self.pen = saved.pen;
        self.pen_changed();
        self.charsets = saved.charsets;
        self.origin = saved.origin;
        self.protected = saved.protected;
    }

    /// DECSTR: the soft reset of the VT220, as xterm does it. The cursor
    /// shows, and stays where it is, its shape and blinking back to the
    /// configuration's; insert mode, origin mode and both reverse
    /// wraparounds go off, and autowrap on, xterm's default; the cursor keys
    /// and the keypad send their normal codes, and modifyOtherKeys goes
    /// off; the margins become the screen's edges, with left and right
    /// margin mode off; the character sets, the style and the protection of
    /// printed text go back to their defaults, and the palette and the
    /// special colors to the theme's; and the cursor that DECSC saved goes,
    /// so that DECRC homes the cursor with the defaults. The screens' text,
    /// the modes of the mouse, bracketed paste, the dynamic colors and the
    /// title stay.
    pub(super) fn soft_reset(&mut self) {
        self.cursor_visible = true;
        self.insert = false;
        self.origin = false;
        self.reverse_wrap = false;
        self.reverse_wrap_extended = false;
        self.autowrap = true;
        self.cursor_style = None;
        self.cursor_blink = false;
        self.input.cursor_keys_application = false;
        self.input.keypad_application = false;
        self.input.modify_other_keys = 0;
        // The palette and the special colors, which xterm's soft reset
        // resets as OSC 104 does.
        let colors = &self.default_colors;
        if self.colors.palette != colors.palette || self.colors.special != colors.special {
            self.colors.palette = colors.palette;
            self.colors.special = colors.special;
            self.events.push(Event::ColorsChanged);
        }
        self.left_right_margins = false;
        self.margins = Margins::screen(self.cols, self.rows);
        self.charsets = Charsets::default();
        self.pen = Style::DEFAULT;
        self.pen_changed();
        self.protected = false;
        self.iso_protection = false;
        self.screen_mut().saved = None;
    }

    /// DECSCL: the conformance level, 61 for the VT100's or 62 for the
    /// VT220's that DA tells, which DECRQSS tells, with a soft reset, as the
    /// VT220 and xterm do; the VT510 resets in full. The terminal does what
    /// it does at either level; a level above the VT220's, as xterm refuses
    /// one above its own, or another value, does nothing.
    pub(super) fn set_conformance_level(&mut self, level: u16) {
        if (61..=62).contains(&level) {
            self.conformance = level;
            self.soft_reset();
        }
    }

    /// RIS: puts the terminal back as it was made, with its screens and
    /// scrollback empty, no title and the theme's colors. It keeps its
    /// configuration: the size, though 132 columns that DECCOLM set go back
    /// to 80; the width that the embedder gave, which the sizes that
    /// programs set are measured against; the scrollback limits, the widths
    /// it was made with, the normalization, the `TERM`, the title and
    /// clipboard reports, the link schemes, the window's operations,
    /// whether output scrolls the view to the screen, the cell size and the
    /// theme. It keeps the working directory too, which
    /// describes the shell rather than the terminal, and what it knows of
    /// the window and the display.
    /// The bells that rang and the synchronized updates stay counted, and
    /// the replies and events not yet taken stay queued, with events for
    /// the titles, the tab color, the colors and the size that RIS
    /// changed.
    fn full_reset(&mut self) {
        let options = Options {
            // The limit as it was configured, rather than as a program's
            // width made it.
            scrollback: Scrollback {
                limit: self.scrollback_limit,
                ..self.primary.rows.scrollback().clone()
            },
            widths: self.configured_widths,
            normalization: self.normalization,
            term: std::mem::take(&mut self.term),
            title_reports: self.title_reports,
            clipboard_reads: self.clipboard_reads,
            rectangle_checksums: self.rectangle_checksums,
            link_schemes: std::mem::take(&mut self.link_schemes),
            window_ops: self.window_ops,
            scroll_on_output: self.scroll_on_output,
        };
        let (bells, updates, replies, reply_bytes, cell_size) = (
            self.bells,
            self.synchronized_updates,
            std::mem::take(&mut self.replies),
            self.reply_bytes,
            self.cell_size,
        );
        let (window, display_size) = (self.window, self.display_size);
        let given_cols = self.given_cols;
        let working_directory = self.working_directory.take();
        let mut events = std::mem::take(&mut self.events);
        if !self.title.is_empty() || !self.icon_title.is_empty() {
            events.push(Event::TitleChanged);
        }
        if self.tab_color.is_some() {
            events.push(Event::TabColor(None));
        }
        if self.colors != self.default_colors {
            events.push(Event::ColorsChanged);
        }
        let colors = std::mem::take(&mut self.default_colors);
        // 132 columns that DECCOLM set go back to 80, as in xterm.
        let cols = if self.columns_132 { 80 } else { self.cols };
        if cols != self.cols {
            events.push(Event::SizeChanged);
        }
        *self = Self::new(cols, self.rows, options);
        (self.bells, self.replies, self.reply_bytes, self.cell_size) =
            (bells, replies, reply_bytes, cell_size);
        (self.window, self.display_size) = (window, display_size);
        self.given_cols = given_cols;
        self.synchronized_updates = updates;
        self.working_directory = working_directory;
        self.events = events;
        self.set_default_colors(colors);
    }

    /// DECALN: fills the screen with `E`, in the default style, makes the
    /// margins those of the whole screen, and puts the cursor at the top
    /// left.
    fn screen_alignment(&mut self) {
        self.moved();
        self.margins = Margins::screen(self.cols, self.rows);
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

    /// RI: up a row, scrolling the scroll region down at its top margin,
    /// when the cursor is between the left and right margins. The column
    /// stays.
    fn reverse_index(&mut self) {
        let Cursor { x, y, .. } = self.cursor();
        if y == self.margins.top && self.margins.has_column(x) {
            self.moved();
            self.screen_mut().cursor.pending_wrap = false;
            self.scroll_region_down(1);
        } else {
            self.cursor_up(1);
        }
    }
}
