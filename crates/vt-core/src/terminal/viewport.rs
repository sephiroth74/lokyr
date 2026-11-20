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
use super::folds::{ShownRow, shown_index, starting};
use crate::page::Position;

/// The least share of a row that the view keeps past its first row: a
/// smaller one is none, as it draws nothing that a frame would show.
const LEAST_FRACTION: f64 = 1.0 / 1024.0;

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
    /// To the row of this number, at the view's top, as far as the
    /// scrollback's first row and the screen let it: a row that the list
    /// does not hold, to its nearer end.
    Row(i64),
    /// To the view's row of this place from the list's first, each fold's
    /// placeholder one row, at the view's top, as a scroll bar's thumb puts
    /// it: the list's row of this index of no fold.
    Shown(usize),
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
        // Of folds, the view's first row is the folded view's.
        if let Some(view) = self.folded_view() {
            return self.primary.rows.scrollback_rows().saturating_sub(view.top);
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
        if let Some(view) = self.settle_folded() {
            return self.primary.rows.scrollback_rows().saturating_sub(view.top);
        }
        let offset = self.viewport_offset();
        if offset == 0 && self.viewport.is_some() && !self.holds_view() {
            self.set_viewport_offset(0);
        }
        offset
    }

    /// The row of the view `y` now, by the numbers of the rows that it
    /// shows: the list's row, or a fold's placeholder; none past the list's
    /// last or the view's.
    pub fn shown_row(&self, y: u16) -> Option<ShownRow> {
        let (top, rows) = self.view_rows();
        self.view_map(top, &rows).at(y)
    }

    /// The row of the view now that shows the row numbered `row`: its own,
    /// or its fold's placeholder; none of a row that the view does not
    /// show.
    pub fn view_y(&self, row: i64) -> Option<u16> {
        let (top, rows) = self.view_rows();
        self.view_map(top, &rows).showing(row)
    }

    /// Scrolls the view, through the scrollback, so that the row numbered
    /// `row` shows: at the view's edge that it comes past, as it does
    /// already, its fold's placeholder for a row of a fold, or not at all,
    /// of a row that the list does not hold.
    pub fn show_row(&mut self, row: i64) {
        let Some(index) = self.row_index(row) else {
            return;
        };
        if let Some(view) = self.settle_folded() {
            if self.view_map(view.top, &view.rows).showing(row).is_some() {
                return;
            }
            let top = if index < view.top {
                index
            } else {
                // The view whose last row shows it.
                let mut at = starting(&view.spans, index);
                for _ in 1..self.rows {
                    let Some(before) = at.checked_sub(1) else {
                        break;
                    };
                    at = starting(&view.spans, before);
                }
                at
            };
            self.scroll_folded(Scroll::Row(self.row_number(top)), &view);
            return;
        }
        let top = self.origin().top;
        let bottom = top.saturating_add(i64::from(self.rows));
        let rows = |n: i64| usize::try_from(n).unwrap_or(usize::MAX);
        if row < top {
            self.scroll_viewport(Scroll::Up(rows(top - row)));
        } else if row >= bottom {
            self.scroll_viewport(Scroll::Down(rows(row - bottom + 1)));
        }
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
        self.fraction = None;
        if self.alternate_active {
            return;
        }
        if let Some(view) = self.settle_folded() {
            self.scroll_folded(scroll, &view);
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
            Scroll::Row(row) => {
                let scrollback = self.primary.rows.scrollback_rows();
                scrollback.saturating_sub(self.nearest_index(row))
            }
            Scroll::Shown(index) => self.primary.rows.scrollback_rows().saturating_sub(index),
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

    /// Scrolls the view of the primary screen by `rows`, up into the
    /// scrollback where positive, of a fraction of a row too, as a
    /// trackpad's points scroll it: whole rows move the view as
    /// [`Scroll::Up`] and [`Scroll::Down`] do, and what is left is the
    /// share of a row that the view is scrolled past its first row,
    /// [`Terminal::viewport_fraction`], which the snapshots draw of the row
    /// past the view's last; as far as the scrollback's first row, of no
    /// share, and the screen, which the view then follows, of none either.
    /// Any other scroll of the view, output that brings it back to the
    /// screen, a resize, the scrollback's limit that takes its first row,
    /// and the alternate screen let the share go. A view of folds scrolls
    /// by whole rows alone, rounded toward none; the alternate screen's
    /// never moves.
    pub fn scroll_viewport_by(&mut self, rows: f64) {
        if self.alternate_active || !rows.is_finite() {
            return;
        }
        if self.settle_folded().is_some() {
            let whole = rows.trunc();
            // Far fewer rows than a `usize` holds, as the list's are.
            let count = whole.abs().min(usize::MAX as f64) as usize;
            self.scroll_viewport(if whole > 0.0 {
                Scroll::Up(count)
            } else {
                Scroll::Down(count)
            });
            return;
        }
        let offset = self.settle_viewport();
        let scrollback = self.primary.rows.scrollback_rows();
        // The view's place, in rows down from the list's first: its first
        // row's index and the share past it.
        // Far fewer rows than an `f64` holds exactly.
        let place = (scrollback - offset) as f64 + f64::from(self.viewport_fraction());
        let place = (place - rows).clamp(0.0, scrollback as f64);
        let top = place.floor();
        let share = place - top;
        // Within the list, as the clamp keeps it.
        let top = (top as usize).min(scrollback);
        self.fraction = None;
        self.set_viewport_offset(scrollback - top);
        if top < scrollback && share >= LEAST_FRACTION {
            // Below 1, as `floor` leaves it.
            self.fraction = Some((self.origin(), share as f32));
        }
    }

    /// Lets the share of a row that the view is scrolled past its first row
    /// go, its rows where they are, as scrolling by pixels turns off.
    pub fn drop_viewport_fraction(&mut self) {
        self.fraction = None;
    }

    /// The share of a row, from 0 to under 1, that the view is scrolled
    /// past its first row, as [`Terminal::scroll_viewport_by`] left it, so
    /// that the view shows its first row a share of a row up and the row
    /// past its last at its bottom: 0 while the view's rows are no longer
    /// where that scroll left them, at the screen, and on the alternate
    /// screen.
    pub fn viewport_fraction(&self) -> f32 {
        match self.fraction {
            Some((origin, share))
                if !self.alternate_active
                    && origin == self.origin()
                    && self.viewport_offset() > 0 =>
            {
                share
            }
            _ => 0.0,
        }
    }

    /// The view of folds, as [`Terminal::folded_view`] finds it, its folds
    /// whose rows left let go of first, and its pin let go of where it
    /// ends at the list's last, which it then follows, but while a search
    /// holds it at its current match; none of no fold.
    #[inline]
    pub(super) fn settle_folded(&mut self) -> Option<super::folds::FoldedView> {
        if self.folds.is_empty() || self.alternate_active {
            return None;
        }
        self.settle_folds()
    }

    /// The view of folds of [`Terminal::settle_folded`], of at least a fold
    /// on the primary screen.
    #[cold]
    fn settle_folds(&mut self) -> Option<super::folds::FoldedView> {
        self.drop_lost_folds();
        let view = self.folded_view()?;
        if view.following && self.viewport.is_some() && !self.holds_view() {
            self.set_viewport_offset(0);
        }
        Some(view)
    }

    /// Scrolls the view of folds `view` by its rows,
    /// each fold's placeholder one: as far as the view that ends at the
    /// list's last, which then follows it. A jump to a prompt goes to the
    /// prompt before or after the view's first row, or to the placeholder
    /// of a fold that holds it.
    fn scroll_folded(&mut self, scroll: Scroll, view: &super::folds::FoldedView) {
        let (page, top) = (usize::from(self.rows), view.top);
        let target = match scroll {
            Scroll::Up(rows) => self.folded_step(view, rows, false),
            Scroll::Down(rows) => self.folded_step(view, rows, true),
            Scroll::PageUp => self.folded_step(view, page, false),
            Scroll::PageDown => self.folded_step(view, page, true),
            Scroll::Top => self.folded_top_at(view, 0),
            Scroll::Bottom => None,
            Scroll::Row(row) => self.folded_top_at(view, self.nearest_index(row)),
            Scroll::Shown(place) => self.folded_top_at(view, shown_index(&view.spans, place)),
            Scroll::PreviousPrompt | Scroll::NextPrompt => {
                let prompts = self.folded_prompts(view);
                let found = if scroll == Scroll::PreviousPrompt {
                    prompts.into_iter().filter(|&index| index < top).max()
                } else {
                    prompts.into_iter().filter(|&index| index > top).min()
                };
                match found {
                    Some(index) => self.folded_top_at(view, index),
                    // No prompt above: the view stays.
                    None if scroll == Scroll::PreviousPrompt => Some(top),
                    None => None,
                }
            }
        };
        let scrollback = self.primary.rows.scrollback_rows();
        match target {
            Some(top) => self.set_viewport_offset(scrollback.saturating_sub(top).max(1)),
            None => self.set_viewport_offset(0),
        }
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
