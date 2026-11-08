//! The view of the primary screen: the rows that a renderer shows, the
//! screen's or, scrolled up, rows of the scrollback above it. A view above
//! the screen keeps its rows, pinned by its first, as output scrolls the
//! screen under it.
//!
//! The rows of the active screen and of its scrollback have numbers, which
//! output, the view's scrolling and the scrollback's limit leave them: the
//! screen's top row is numbered by the times that the screen scrolled up
//! in its epoch, the rows below it on from there, and the scrollback's
//! rows above it down from there. A resize, which lays the rows out again,
//! starts an epoch of other numbers; a pin keeps its cell through it.

use super::Terminal;
use crate::page::Position;

/// How the view scrolls.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scroll {
    /// Up into the scrollback, by rows.
    Up(usize),
    /// Down toward the screen, by rows.
    Down(usize),
    /// Up by the screen's rows.
    PageUp,
    /// Down by the screen's rows.
    PageDown,
    /// To the scrollback's first row.
    Top,
    /// To the screen, which the view then follows.
    Bottom,
    /// To the prompt above the view's first row, of the commands that the
    /// shell marked, at the view's top; none moves it.
    PreviousPrompt,
    /// To the prompt below the view's first row, at the view's top, or to
    /// the screen past the scrollback's last.
    NextPrompt,
}

/// Where the active screen's rows are, at a moment: the epoch of their
/// numbers, and the number of the view's first row.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Origin {
    /// The epoch of the rows' numbers, which a resize changes, and which
    /// no other screen's rows share.
    pub epoch: u64,
    /// The number of the view's first row.
    pub top: i64,
}

impl Origin {
    /// The number of the view's row `row`.
    pub fn row(self, row: u16) -> i64 {
        self.top.saturating_add(i64::from(row))
    }
}

impl Terminal {
    /// Where the active screen's rows are now: their epoch, and the number
    /// of the view's first row.
    pub fn origin(&self) -> Origin {
        Origin {
            epoch: self.screen().rows.epoch(),
            top: self.row_number(self.view_top()),
        }
    }

    /// The number of the active screen's row at `index` of its list.
    pub fn row_number(&self, index: usize) -> i64 {
        let rows = &self.screen().rows;
        // Fewer rows, and scrolls, than an `i64` holds.
        let first = rows.scrolled() as i64 - rows.scrollback_rows() as i64;
        first.saturating_add(index as i64)
    }

    /// The index in the active screen's list of the row numbered `row`, if
    /// the list holds it.
    pub fn row_index(&self, row: i64) -> Option<usize> {
        let index = usize::try_from(row.checked_sub(self.row_number(0))?).ok()?;
        (index < self.screen().rows.total_rows()).then_some(index)
    }

    /// The index in the active screen's list of the row numbered `row`, or
    /// of the list's end nearest to it.
    pub(super) fn nearest_index(&self, row: i64) -> usize {
        let last = self.screen().rows.total_rows() - 1;
        let index = row.saturating_sub(self.row_number(0)).max(0);
        usize::try_from(index).map_or(last, |index| index.min(last))
    }

    /// How many rows the active screen's scrollback holds, above the
    /// screen: none on the alternate screen.
    pub fn scrollback_rows(&self) -> usize {
        self.screen().rows.scrollback_rows()
    }

    /// The list's index of the view's first row.
    pub(super) fn view_top(&self) -> usize {
        self.screen().rows.scrollback_rows() - self.viewport_offset()
    }

    /// How many rows the view is above the screen's top: 0 while it shows
    /// the screen, and while the alternate screen, which has no
    /// scrollback, is active. A view whose first row left the scrollback,
    /// as its oldest rows went, is at the scrollback's top; one whose first
    /// row a reflow brought into the screen is at the screen.
    pub fn viewport_offset(&self) -> usize {
        if self.alternate_active {
            return 0;
        }
        let Some(pin) = self.viewport else {
            return 0;
        };
        let rows = &self.primary.rows;
        let scrollback = rows.scrollback_rows();
        match rows
            .pin_position(pin)
            .and_then(|position| rows.row_index(position.page, position.row))
        {
            Some(index) => scrollback.saturating_sub(index),
            None => scrollback,
        }
    }

    /// The view's offset, as [`Terminal::viewport_offset`] tells it, with
    /// its pin let go when a reflow brought the view down to the screen,
    /// which it then follows, but while a search holds it at its current
    /// match.
    pub(super) fn settle_viewport(&mut self) -> usize {
        let offset = self.viewport_offset();
        if offset == 0 && self.viewport.is_some() && !self.holds_view() {
            self.set_viewport_offset(0);
        }
        offset
    }

    /// Pins the view by its first row where it is, at the screen too, so
    /// that output scrolls the screen under it rather than taking it along.
    pub(super) fn pin_viewport(&mut self) {
        if self.alternate_active || self.viewport.is_some() {
            return;
        }
        let rows = &mut self.primary.rows;
        if let Some((page, row)) = rows.row_at(rows.scrollback_rows()) {
            self.viewport = rows.pin(Position { page, row, col: 0 });
        }
    }

    /// Scrolls the view of the primary screen, within its scrollback and
    /// the screen; the alternate screen's never moves.
    pub fn scroll_viewport(&mut self, scroll: Scroll) {
        if self.alternate_active {
            return;
        }
        let offset = self.settle_viewport();
        let page = usize::from(self.rows);
        let target = match scroll {
            Scroll::Up(rows) => offset.saturating_add(rows),
            Scroll::Down(rows) => offset.saturating_sub(rows),
            Scroll::PageUp => offset.saturating_add(page),
            Scroll::PageDown => offset.saturating_sub(page),
            Scroll::Top => usize::MAX,
            Scroll::Bottom => 0,
            Scroll::PreviousPrompt | Scroll::NextPrompt => {
                let scrollback = self.primary.rows.scrollback_rows();
                let top = scrollback - offset;
                let prompts = self.prompt_indexes();
                let found = if scroll == Scroll::PreviousPrompt {
                    prompts.into_iter().filter(|&index| index < top).max()
                } else {
                    prompts.into_iter().filter(|&index| index > top).min()
                };
                match found {
                    Some(index) => scrollback.saturating_sub(index),
                    None if scroll == Scroll::NextPrompt => 0,
                    None => offset,
                }
            }
        };
        self.set_viewport_offset(target);
    }

    /// Puts the view `offset` rows above the screen's top, within the
    /// scrollback, pinned by its first row; at the screen, it follows it.
    pub(super) fn set_viewport_offset(&mut self, offset: usize) {
        let rows = &mut self.primary.rows;
        if let Some(pin) = self.viewport.take() {
            rows.unpin(pin);
        }
        let scrollback = rows.scrollback_rows();
        let offset = offset.min(scrollback);
        if offset == 0 {
            return;
        }
        if let Some((page, row)) = rows.row_at(scrollback - offset) {
            self.viewport = rows.pin(Position { page, row, col: 0 });
        }
    }

    /// Whether output brings the view back to the screen.
    pub fn scroll_on_output(&self) -> bool {
        self.scroll_on_output
    }

    /// Sets whether output brings the view back to the screen, from the
    /// next output on.
    pub fn set_scroll_on_output(&mut self, scroll: bool) {
        self.scroll_on_output = scroll;
    }
}
