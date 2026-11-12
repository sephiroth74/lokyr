//! Erasing: ED, EL and ECH, and the selective erases DECSED and DECSEL.
//!
//! An erased cell takes the pen's background color, as xterm's background
//! color erase does, and nothing else. ED, EL and ECH erase every cell but
//! those that SPA protected, in ISO's protection; the selective erases keep
//! those that DECSCA protected, in DEC's, and erase those that SPA did, as
//! esctest asks, where xterm keeps them. Erasing ends the cluster being
//! printed, and leaves no wrap pending.

use std::ops::Range;

use super::{Cursor, Terminal};
use crate::page::{Cell, Color, Content};

impl Terminal {
    /// ED, or DECSED if `selective`: 0 erases from the cursor to the end of
    /// the screen, 1 from its start to the cursor, 2 all of it, and 3, for
    /// ED only, the scrollback.
    ///
    /// ED 2 on the primary screen first moves the screen's rows, up to the
    /// last one that holds something, into the scrollback, as
    /// `alacritty_terminal` does, so that clearing the screen loses nothing.
    pub(super) fn erase_in_display(&mut self, which: u16, selective: bool) {
        let cursor = self.erasing();
        let (rows, cols) = (self.rows, self.cols);
        let keep = self.keeps_protected(selective);
        match which {
            0 => {
                self.erase_in_row(cursor.y, cursor.x..cols, keep);
                for y in cursor.y + 1..rows {
                    self.erase_in_row(y, 0..cols, keep);
                }
            }
            1 => {
                for y in 0..cursor.y {
                    self.erase_in_row(y, 0..cols, keep);
                }
                self.erase_in_row(cursor.y, 0..cursor.x + 1, keep);
            }
            2 => {
                // The screen goes to the scrollback but when ISO's
                // protection keeps some of its cells there.
                let into_scrollback =
                    !selective && !self.alternate_active && !(keep && self.screen_protects());
                if into_scrollback {
                    self.scroll_into_scrollback();
                }
                // The alternate screen, which keeps nothing, loses its
                // images; the primary's went to the scrollback, unless the
                // erase takes them in place.
                if self.alternate_active {
                    self.drop_alternate_images();
                } else if !into_scrollback {
                    self.drop_screen_images();
                }
                for y in 0..rows {
                    self.erase_in_row(y, 0..cols, keep);
                }
            }
            3 if !selective => self.clear_scrollback(),
            _ => {}
        }
    }

    /// Clears the active screen's scrollback, as ED 3 does.
    pub fn clear_scrollback(&mut self) {
        self.screen_mut().rows.clear_scrollback();
        // The view's rows went: it follows the screen at once, so that
        // output does not leave it on the scrollback's next rows.
        if !self.alternate_active {
            self.set_viewport_offset(0);
        }
    }

    /// EL, or DECSEL if `selective`: 0 erases from the cursor to the end of
    /// its row, 1 from the start of the row to the cursor, and 2 the whole
    /// row.
    pub(super) fn erase_in_line(&mut self, which: u16, selective: bool) {
        let columns = match which {
            0 => self.cursor().x..self.cols,
            1 => 0..self.cursor().x + 1,
            2 => 0..self.cols,
            _ => return,
        };
        let cursor = self.erasing();
        let keep = self.keeps_protected(selective);
        self.erase_in_row(cursor.y, columns, keep);
    }

    /// ECH: erases `n` cells from the cursor, as far as the end of its row.
    pub(super) fn erase_characters(&mut self, n: u16) {
        let x = self.cursor().x;
        let end = x.saturating_add(n).min(self.cols);
        let cursor = self.erasing();
        let keep = self.keeps_protected(false);
        self.erase_in_row(cursor.y, x..end, keep);
    }

    /// Whether an erase keeps the protected cells: ED, EL and ECH in ISO's
    /// protection, and the `selective` ones in DEC's.
    pub(super) fn keeps_protected(&self, selective: bool) -> bool {
        selective != self.iso_protection
    }

    /// The cell that erasing leaves: empty, with the pen's background color.
    pub(super) fn blank(&self) -> Cell {
        match self.pen.bg {
            Color::Default => Cell::EMPTY,
            Color::Palette(index) => Cell::EMPTY.with_content(Content::BgPalette(index)),
            Color::Rgb(rgb) => Cell::EMPTY.with_content(Content::BgRgb(rgb)),
        }
    }

    /// Ends the cluster and the pending wrap before an erase, and returns
    /// the cursor.
    fn erasing(&mut self) -> Cursor {
        self.moved();
        self.screen_mut().cursor.pending_wrap = false;
        self.cursor()
    }

    /// Erases `columns` of row `y`, but for the protected cells if `keep`.
    /// An erase of them all that reaches the end of the row ends its wrap:
    /// the row no longer goes on into the next.
    pub(super) fn erase_in_row(&mut self, y: u16, columns: Range<u16>, keep: bool) {
        let blank = self.blank();
        let reaches_end = columns.end == self.cols;
        let (page, row) = self.screen_mut().rows.screen_row_mut(y);
        page.erase(row, columns, blank, keep);
        if reaches_end && !keep {
            self.end_soft_wrap(y);
        }
    }

    /// Whether a cell of the screen is protected.
    fn screen_protects(&self) -> bool {
        let rows = &self.screen().rows;
        (0..self.rows).any(|y| {
            let (page, row) = rows.screen_row(y);
            page.cells(row).iter().any(|cell| cell.is_protected())
        })
    }

    /// Moves the rows of the screen, up to the last one that holds
    /// something, text or an image, into the scrollback, and empty rows in
    /// at the bottom.
    fn scroll_into_scrollback(&mut self) {
        let imaged = self.rows_under_images();
        let rows = &mut self.screen_mut().rows;
        let holding = (0..rows.rows())
            .rev()
            .find(|&y| {
                let (page, row) = rows.screen_row(y);
                page.cells(row).iter().any(|cell| !cell.is_empty())
            })
            .map_or(0, |y| y + 1)
            .max(imaged);
        for _ in 0..holding {
            rows.scroll_up();
        }
    }
}
