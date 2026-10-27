//! The view of the primary screen: the rows that a renderer shows, the
//! screen's or, scrolled up, rows of the scrollback above it. A view above
//! the screen keeps its rows, pinned by its first, as output scrolls the
//! screen under it.

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
}

impl Terminal {
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
    /// which it then follows.
    pub(super) fn settle_viewport(&mut self) -> usize {
        let offset = self.viewport_offset();
        if offset == 0 && self.viewport.is_some() {
            self.set_viewport_offset(0);
        }
        offset
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
