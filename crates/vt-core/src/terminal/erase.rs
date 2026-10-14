//! Erasing: ED, EL and ECH, and the selective erases DECSED and DECSEL.
//!
//! An erased cell takes the pen's background color, as xterm's background
//! color erase does, and nothing else. ED, EL and ECH erase every cell; the
//! selective erases keep the cells that DECSCA protected. Erasing ends the
//! cluster being printed, and leaves no wrap pending.

use std::ops::Range;

use super::{Cursor, Terminal};
use crate::page::{Cell, Color, Content, RowFlag};

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
        match which {
            0 => {
                self.erase_in_row(cursor.y, cursor.x..cols, selective);
                for y in cursor.y + 1..rows {
                    self.erase_in_row(y, 0..cols, selective);
                }
            }
            1 => {
                for y in 0..cursor.y {
                    self.erase_in_row(y, 0..cols, selective);
                }
                self.erase_in_row(cursor.y, 0..cursor.x + 1, selective);
            }
            2 => {
                if !selective && !self.alternate_active {
                    self.scroll_into_scrollback();
                }
                for y in 0..rows {
                    self.erase_in_row(y, 0..cols, selective);
                }
            }
            3 if !selective => self.screen_mut().rows.clear_scrollback(),
            _ => {}
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
        self.erase_in_row(cursor.y, columns, selective);
    }

    /// ECH: erases `n` cells from the cursor, as far as the end of its row.
    pub(super) fn erase_characters(&mut self, n: u16) {
        let x = self.cursor().x;
        let end = x.saturating_add(n).min(self.cols);
        let cursor = self.erasing();
        self.erase_in_row(cursor.y, x..end, false);
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

    /// Erases `columns` of row `y`. An erase that reaches the end of the
    /// row ends its wrap: the row no longer goes on into the next.
    pub(super) fn erase_in_row(&mut self, y: u16, columns: Range<u16>, selective: bool) {
        let blank = self.blank();
        let reaches_end = columns.end == self.cols;
        let rows = &mut self.screen_mut().rows;
        let (page, row) = rows.screen_row_mut(y);
        page.erase(row, columns, blank, selective);
        if reaches_end && !selective {
            page.row_mut(row).set(RowFlag::SoftWrapped, false);
            if y + 1 < rows.rows() {
                let (page, row) = rows.screen_row_mut(y + 1);
                page.row_mut(row).set(RowFlag::Continuation, false);
            }
        }
    }

    /// Moves the rows of the screen, up to the last one that holds
    /// something, into the scrollback, and empty rows in at the bottom.
    fn scroll_into_scrollback(&mut self) {
        let rows = &mut self.screen_mut().rows;
        let holding = (0..rows.rows())
            .rev()
            .find(|&y| {
                let (page, row) = rows.screen_row(y);
                page.cells(row).iter().any(|cell| !cell.is_empty())
            })
            .map_or(0, |y| y + 1);
        for _ in 0..holding {
            rows.scroll_up();
        }
    }
}
