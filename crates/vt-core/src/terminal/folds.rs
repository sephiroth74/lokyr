//! Folds: the output of a command that ended, or its whole block, out of
//! the view of the primary screen, a placeholder row in its place, which
//! the snapshot, the view's scrolls and its jumps count as one row. A fold
//! holds two pins, of its first row and of its last, so that it keeps its
//! rows as output scrolls them into the scrollback and a resize reflows
//! them, and goes with them, as the scrollback's limit takes them; a clear
//! and a reset let go of every fold. None lies at or below the prompt in
//! force, so that the program's own grid, where it draws, stays whole.
//!
//! Of no fold, the view's rows are the list's from its top, and nothing
//! here costs a thing; of folds, the view's rows are found of the folds
//! that it meets, a binary search finding the first, and of the rows that
//! it shows.

use super::Terminal;
use crate::page::{PageList, PinId, Position};

/// The most folds that a terminal keeps, which a snapshot looks at each.
pub const MOST_FOLDS: usize = 4096;

/// A fold that a terminal keeps: its command's id, whether it folds the
/// whole block, and the pins of its first row and its last.
#[derive(Debug)]
pub(super) struct Fold {
    command: u64,
    whole: bool,
    first: PinId,
    last: PinId,
}

/// A row of the view: a row of the active screen's list, by its index, or
/// the placeholder of a fold, of its command's id, whether it folds the
/// whole block, and how many rows it folds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViewRow {
    /// A row of the list.
    Row(usize),
    /// A fold's placeholder.
    Folded {
        /// The command's id.
        command: u64,
        /// Whether it folds the whole block, the command's prompt and line
        /// too, as a block hidden.
        whole: bool,
        /// How many rows it folds.
        rows: usize,
    },
    /// An empty row past the list's last, as a view of folds whose list's
    /// rows are fewer than its own ends.
    Past,
}

/// A fold, as a terminal keeps it: its command's id, whether it folds the
/// whole block, and the numbers of its first row and its last.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FoldMark {
    /// The command's id.
    pub command: u64,
    /// Whether it folds the whole block.
    pub whole: bool,
    /// Its first row's number.
    pub first: i64,
    /// Its last row's number.
    pub last: i64,
}

/// A fold's rows, by their indexes in the list, from its first to its last.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Span {
    pub(super) first: usize,
    pub(super) last: usize,
    pub(super) command: u64,
    pub(super) whole: bool,
}

impl Span {
    /// How many rows it folds.
    fn rows(self) -> usize {
        self.last - self.first + 1
    }
}

/// The view of folds: the index of its first row, its rows, the rows that
/// the folds take out of the list, those of them above its first row,
/// whether it ends at the list's last, as a view that follows the screen,
/// and the folds' rows, in their order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct FoldedView {
    pub(super) top: usize,
    pub(super) rows: Vec<ViewRow>,
    pub(super) folded: usize,
    pub(super) folded_above: usize,
    pub(super) following: bool,
    pub(super) spans: Vec<Span>,
}

impl Terminal {
    /// Folds the output of the command `command`, or its whole block, as
    /// `whole` says, in place of a fold of it: the rows from its output's
    /// first, the row after its output's mark where the mark is past the
    /// row's first column, or from its block's first, to the last before
    /// its end. Whether it took it: the command ended, and the primary
    /// screen holds its rows, above the prompt in force's, the first of the
    /// last command's, of the primary screen shown, and the terminal keeps
    /// fewer than [`MOST_FOLDS`].
    pub fn fold(&mut self, command: u64, whole: bool) -> bool {
        if self.alternate_active {
            return false;
        }
        let Some(marks) = self.command(command) else {
            return false;
        };
        let Some((end_column, end_row)) = marks.end else {
            return false;
        };
        let last = if end_column == 0 {
            end_row - 1
        } else {
            end_row
        };
        let first = if whole {
            marks
                .prompt
                .or(marks.input)
                .or(marks.output)
                .map(|(_, row)| row)
        } else {
            marks
                .output
                .map(|(column, row)| if column == 0 { row } else { row + 1 })
        };
        let Some(first) = first.filter(|&first| first <= last) else {
            return false;
        };
        // Above the prompt in force, of the last command, whose rows the
        // program may draw on still.
        let in_force = self.last_command().and_then(|last| {
            last.prompt
                .or(last.input)
                .or(last.output)
                .or(last.end)
                .map(|(_, row)| row)
        });
        if last_command_is(self, command) || in_force.is_some_and(|row| last >= row) {
            return false;
        }
        self.unfold(command);
        if self.folds.len() >= MOST_FOLDS {
            return false;
        }
        let (Some(first_index), Some(last_index)) = (self.row_index(first), self.row_index(last))
        else {
            return false;
        };
        let rows = &mut self.primary.rows;
        let pin = |rows: &mut PageList, index: usize| {
            let (page, row) = rows.row_at(index)?;
            rows.pin(Position { page, row, col: 0 })
        };
        let Some(first_pin) = pin(rows, first_index) else {
            return false;
        };
        let Some(last_pin) = pin(rows, last_index) else {
            rows.unpin(first_pin);
            return false;
        };
        // In their rows' order, as the view walks them.
        let spans = self.fold_spans();
        let at = spans.partition_point(|span| span.first < first_index);
        self.folds.insert(
            at,
            Fold {
                command,
                whole,
                first: first_pin,
                last: last_pin,
            },
        );
        true
    }

    /// Takes the fold of the command `command` away, if the terminal keeps
    /// one. Whether it did.
    pub fn unfold(&mut self, command: u64) -> bool {
        let Some(at) = self.folds.iter().position(|fold| fold.command == command) else {
            return false;
        };
        let fold = self.folds.remove(at);
        self.primary.rows.unpin(fold.first);
        self.primary.rows.unpin(fold.last);
        true
    }

    /// The folds that the terminal keeps, in their rows' order, but those
    /// whose rows left.
    pub fn folds(&self) -> Vec<FoldMark> {
        let rows = &self.primary.rows;
        let first = rows.scrolled() as i64 - rows.scrollback_rows() as i64;
        let index = rows.row_indexes();
        let row_of = |pin: PinId| {
            let position = rows.pin_position(pin)?;
            Some(first.saturating_add(index(position.page, position.row)? as i64))
        };
        self.folds
            .iter()
            .filter_map(|fold| {
                Some(FoldMark {
                    command: fold.command,
                    whole: fold.whole,
                    first: row_of(fold.first)?,
                    last: row_of(fold.last)?,
                })
            })
            .collect()
    }

    /// Lets go of every fold, as a clear or a reset does.
    pub(super) fn forget_folds(&mut self) {
        for fold in std::mem::take(&mut self.folds) {
            self.primary.rows.unpin(fold.first);
            self.primary.rows.unpin(fold.last);
        }
    }

    /// The folds' rows, by their indexes, in their order, but those whose
    /// rows left.
    pub(super) fn fold_spans(&self) -> Vec<Span> {
        if self.folds.is_empty() {
            return Vec::new();
        }
        let rows = &self.primary.rows;
        let index = rows.row_indexes();
        let index_of = |pin: PinId| {
            let position = rows.pin_position(pin)?;
            index(position.page, position.row)
        };
        self.folds
            .iter()
            .filter_map(|fold| {
                let (first, last) = (index_of(fold.first)?, index_of(fold.last)?);
                (first <= last).then_some(Span {
                    first,
                    last,
                    command: fold.command,
                    whole: fold.whole,
                })
            })
            .collect()
    }

    /// Lets go of the folds whose rows left, as the scrollback's limit or a
    /// clearing took them.
    pub(super) fn drop_lost_folds(&mut self) {
        let rows = &self.primary.rows;
        let lost = |pin: PinId| rows.pin_position(pin).is_none();
        let gone: Vec<usize> = self
            .folds
            .iter()
            .enumerate()
            .filter(|(_, fold)| lost(fold.first) || lost(fold.last))
            .map(|(at, _)| at)
            .collect();
        for at in gone.into_iter().rev() {
            let fold = self.folds.remove(at);
            self.primary.rows.unpin(fold.first);
            self.primary.rows.unpin(fold.last);
        }
    }

    /// The view of the primary screen's folds, if it keeps any: its rows
    /// from its pinned first row, as far as they fill it, or else, as the
    /// view follows the screen or meets its end, the rows that end at the
    /// list's last; none of no fold, or on the alternate screen, whose view
    /// is the list's rows from its top.
    pub(super) fn folded_view(&self) -> Option<FoldedView> {
        if self.alternate_active || self.folds.is_empty() {
            return None;
        }
        let spans = self.fold_spans();
        if spans.is_empty() {
            return None;
        }
        let height = usize::from(self.rows);
        let total = self.primary.rows.total_rows();
        let pinned = self.viewport.and_then(|pin| {
            let rows = &self.primary.rows;
            let position = rows.pin_position(pin)?;
            rows.row_index(position.page, position.row)
        });
        let below = pinned
            .map(|top| down_from(starting(&spans, top), &spans, total, height))
            .filter(|(rows, _)| rows.len() == height);
        let (top, mut rows, following) = match below {
            Some((rows, _)) => (starting(&spans, pinned.unwrap_or(0)), rows, false),
            None => {
                let (top, rows) = up_from(&spans, total, height);
                (top, rows, true)
            }
        };
        // Of fewer rows than the view's, as a short list's, empty ones past
        // its last.
        rows.resize(height, ViewRow::Past);
        let folded = spans.iter().map(|span| span.rows() - 1).sum();
        let folded_above = spans
            .iter()
            .take_while(|span| span.last < top)
            .map(|span| span.rows() - 1)
            .sum();
        Some(FoldedView {
            top,
            rows,
            folded,
            folded_above,
            following,
            spans,
        })
    }

    /// The index of the first row of the view of folds `view`, `rows`
    /// view's rows above or below its first, as `down` says, each fold's
    /// placeholder one row: below, as far as the view that ends at the
    /// list's last, whose first it then gives, of `None`, as the view
    /// follows the screen there.
    pub(super) fn folded_step(&self, view: &FoldedView, rows: usize, down: bool) -> Option<usize> {
        let spans = &view.spans;
        let height = usize::from(self.rows);
        let total = self.primary.rows.total_rows();
        let (bottom, _) = up_from(spans, total, height);
        let top = starting(spans, view.top);
        if down {
            let (_, after) = down_from(top, spans, total, rows);
            return (after < bottom).then_some(after);
        }
        let mut at = top;
        for _ in 0..rows {
            let Some(before) = at.checked_sub(1) else {
                break;
            };
            at = starting(spans, before);
        }
        Some(at).filter(|&at| at < bottom)
    }

    /// The index of the first row of a view of folds `view`'s that shows
    /// the row of the list at `index` at its top, the first of a fold that
    /// holds it, as far as the view that ends at the list's last, of
    /// `None` there.
    pub(super) fn folded_top_at(&self, view: &FoldedView, index: usize) -> Option<usize> {
        let height = usize::from(self.rows);
        let total = self.primary.rows.total_rows();
        let (bottom, _) = up_from(&view.spans, total, height);
        let top = starting(&view.spans, index);
        (top < bottom).then_some(top)
    }
}

impl Terminal {
    /// The indexes of the kept prompts' rows, oldest first, each in a fold
    /// of the view of folds `view` the fold's first row, its placeholder's.
    pub(super) fn folded_prompts(&self, view: &FoldedView) -> Vec<usize> {
        self.prompt_indexes()
            .into_iter()
            .map(|index| starting(&view.spans, index))
            .collect()
    }
}

/// Whether the command `command` is the last that `terminal` keeps, the
/// prompt in force's, which no fold takes.
fn last_command_is(terminal: &Terminal, command: u64) -> bool {
    terminal
        .last_command()
        .is_some_and(|last| last.id == command)
}

/// The index of the first row that a view from `index` shows: the first of
/// the fold of `spans` that holds it, or its own.
fn starting(spans: &[Span], index: usize) -> usize {
    let at = spans.partition_point(|span| span.last < index);
    spans
        .get(at)
        .filter(|span| span.first <= index)
        .map_or(index, |span| span.first)
}

/// The view's rows from the row at `top`, `height` at most, before the
/// list's `total`, each fold of `spans` one, and the index past them.
fn down_from(top: usize, spans: &[Span], total: usize, height: usize) -> (Vec<ViewRow>, usize) {
    let mut rows = Vec::with_capacity(height);
    let mut at = top;
    let mut next = spans.partition_point(|span| span.last < top);
    while rows.len() < height && at < total {
        match spans.get(next).filter(|span| span.first <= at) {
            Some(span) => {
                rows.push(ViewRow::Folded {
                    command: span.command,
                    whole: span.whole,
                    rows: span.rows(),
                });
                at = span.last + 1;
                next += 1;
            }
            None => {
                rows.push(ViewRow::Row(at));
                at += 1;
            }
        }
    }
    (rows, at)
}

/// The view's rows that end at the list's last, of its `total`, `height`
/// at most, each fold of `spans` one, and the index of its first.
fn up_from(spans: &[Span], total: usize, height: usize) -> (usize, Vec<ViewRow>) {
    let mut rows = Vec::with_capacity(height);
    let mut next = spans.len();
    let mut at = total;
    while rows.len() < height && at > 0 {
        let last = at - 1;
        match next
            .checked_sub(1)
            .and_then(|before| spans.get(before))
            .filter(|span| span.first <= last && last <= span.last)
        {
            Some(span) => {
                rows.push(ViewRow::Folded {
                    command: span.command,
                    whole: span.whole,
                    rows: span.rows(),
                });
                at = span.first;
                next -= 1;
            }
            None => {
                // Past the folds below it.
                while next > 0 && spans[next - 1].first > last {
                    next -= 1;
                }
                rows.push(ViewRow::Row(last));
                at = last;
            }
        }
    }
    rows.reverse();
    (at, rows)
}

#[cfg(test)]
mod tests;
