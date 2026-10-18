//! The window's operations of XTWINOPS, when the configuration allows them:
//! programs ask the window to iconify, move, resize, be maximized or fill
//! the display, and ask where it is and how large the display is.
//!
//! The terminal keeps what it knows of the window, as the embedder told it
//! and programs asked, and answers from it at once, so that a program's next
//! query reads what it asked for: a request changes it, and queues
//! `WindowChanged`, or `SizeChanged`, for the embedder to make the window
//! so, as it can, and to tell the terminal how the window ended.

use super::{Event, Terminal};
use crate::parser::Params;

/// What the terminal knows of its window, which programs ask about and ask
/// to change.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Window {
    /// The place of its top left on the display, in pixels: across.
    pub x: u16,
    /// Down.
    pub y: u16,
    /// Whether it is iconified.
    pub iconified: bool,
    /// Whether it is maximized across the display.
    pub maximized_horizontally: bool,
    /// Whether it is maximized down it.
    pub maximized_vertically: bool,
    /// Whether it fills the display.
    pub full_screen: bool,
}

impl Terminal {
    /// What the terminal knows of its window.
    pub fn window(&self) -> Window {
        self.window
    }

    /// Tells the terminal how its window is, as the embedder made it.
    pub fn set_window(&mut self, window: Window) {
        self.window = window;
    }

    /// Sets the size of the display in pixels, which CSI 15 t reports and,
    /// in cells, once the size of a cell is set, CSI 19 t, and which a
    /// window maximized fills; until then, they get no reply, and maximizing
    /// does nothing.
    pub fn set_display_size(&mut self, width: u16, height: u16) {
        self.display_size = Some((width, height));
    }

    /// The XTWINOPS operations on the window, `Ps` and its parameters, when
    /// the configuration allows them: iconify (2) and back (1), move (3),
    /// resize in pixels (4), maximize (9), fill the display (10), and the
    /// reports of the window's state (11), of its place (13) and of the
    /// display's size in pixels (15) and in cells (19). Raising, lowering
    /// and refreshing the window (5, 6 and 7) do nothing.
    pub(super) fn window_operation(&mut self, params: &Params) {
        match params.value(0) {
            which @ (1 | 2) => {
                self.window.iconified = which == 2;
                self.events.push(Event::WindowChanged);
            }
            // As xterm, an omitted place is 0.
            3 => {
                (self.window.x, self.window.y) = (params.value(1), params.value(2));
                self.events.push(Event::WindowChanged);
            }
            4 => {
                let Some((width, height)) = self.cell_size else {
                    return;
                };
                let display = self.display_size;
                // The cells that a side in pixels holds, the current ones
                // where it is omitted.
                let cells = |index: usize, now: u16, cell: u16, display: Option<u16>| -> u16 {
                    let pixels = match (params.has_value(index), params.value(index)) {
                        (false, _) => return now,
                        (true, 0) => match display {
                            Some(display) => display,
                            None => return now,
                        },
                        (true, value) => value,
                    };
                    pixels / cell.max(1)
                };
                let rows = cells(1, self.rows, height, display.map(|d| d.1));
                let cols = cells(2, self.cols, width, display.map(|d| d.0));
                self.take_size(cols, rows);
            }
            9 => self.maximize(params.value(1)),
            10 => self.fill_display(params.value(1)),
            11 => {
                let state = if self.window.iconified { 2 } else { 1 };
                self.reply(format!("\x1b[{state}t").as_bytes());
            }
            13 => {
                let Window { x, y, .. } = self.window;
                self.reply(format!("\x1b[3;{x};{y}t").as_bytes());
            }
            15 => {
                if let Some((width, height)) = self.display_size {
                    self.reply(format!("\x1b[5;{height};{width}t").as_bytes());
                }
            }
            19 => {
                if let Some((cols, rows)) = self.display_cells() {
                    self.reply(format!("\x1b[9;{rows};{cols}t").as_bytes());
                }
            }
            _ => {}
        }
    }

    /// XTWINOPS 8 t: the text area's rows and columns, the current ones
    /// where they are omitted, and the display's where they are 0, as xterm
    /// takes them.
    pub(super) fn resize_in_cells(&mut self, params: &Params) {
        let display = self.display_cells();
        let cells = |index: usize, now: u16, display: Option<u16>| -> u16 {
            match (params.has_value(index), params.value(index)) {
                (false, _) => now,
                (true, 0) => display.unwrap_or(now),
                (true, value) => value,
            }
        };
        let rows = cells(1, self.rows, display.map(|d| d.1));
        let cols = cells(2, self.cols, display.map(|d| d.0));
        self.take_size(cols, rows);
    }

    /// The display's size in cells, once both it and a cell's are known.
    fn display_cells(&self) -> Option<(u16, u16)> {
        let ((width, height), (cell_width, cell_height)) = (self.display_size?, self.cell_size?);
        Some((width / cell_width.max(1), height / cell_height.max(1)))
    }

    /// XTWINOPS 9 t: maximizes the window across the display and down it
    /// (1), down it (2) or across it (3), or brings back the size before
    /// (0), keeping it the first time.
    fn maximize(&mut self, how: u16) {
        let Some((cols, rows)) = self.display_cells() else {
            return;
        };
        let (across, down) = match how {
            0 => {
                self.restore_size();
                self.window.maximized_horizontally = false;
                self.window.maximized_vertically = false;
                self.events.push(Event::WindowChanged);
                return;
            }
            1 => (true, true),
            2 => (false, true),
            3 => (true, false),
            _ => return,
        };
        self.keep_size();
        self.window.maximized_horizontally |= across;
        self.window.maximized_vertically |= down;
        let cols = if across { cols } else { self.cols };
        let rows = if down { rows } else { self.rows };
        self.take_size(cols, rows);
        self.events.push(Event::WindowChanged);
    }

    /// XTWINOPS 10 t: makes the window fill the display (1), stop (0), or
    /// toggle (2), bringing back the size before when it stops.
    fn fill_display(&mut self, how: u16) {
        let Some((cols, rows)) = self.display_cells() else {
            return;
        };
        let fill = match how {
            0 => false,
            1 => true,
            2 => !self.window.full_screen,
            _ => return,
        };
        if fill {
            self.keep_size();
            self.take_size(cols, rows);
        } else if self.window.full_screen {
            self.restore_size();
            self.window.maximized_horizontally = false;
            self.window.maximized_vertically = false;
        }
        self.window.full_screen = fill;
        self.events.push(Event::WindowChanged);
    }

    /// Keeps the size before the window was maximized or filled the
    /// display, the first time.
    fn keep_size(&mut self) {
        if self.restored_size.is_none() {
            self.restored_size = Some((self.cols, self.rows));
        }
    }

    /// Brings back the size kept before the window was maximized or filled
    /// the display, if it was.
    fn restore_size(&mut self) {
        if let Some((cols, rows)) = self.restored_size.take() {
            self.take_size(cols, rows);
        }
    }
}
