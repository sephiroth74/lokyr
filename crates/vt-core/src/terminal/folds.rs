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

/// The command's id of a search's filter's spans, the runs of rows between
/// its matches', which no command takes, and which no fold's mark tells.
pub(super) const FILTERED: u64 = u64::MAX;

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
/// whole block, the index of its first row, and how many rows it folds.
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
        /// The index of its first row in the list.
        first: usize,
        /// How many rows it folds.
        rows: usize,
    },
    /// An empty row past the list's last, as a view of folds whose list's
    /// rows are fewer than its own ends.
    Past,
}

impl ViewRow {
    /// The index of the list's first row that it shows, past every row of
    /// the list of [`ViewRow::Past`].
    fn first(self) -> usize {
        match self {
            Self::Row(index) | Self::Folded { first: index, .. } => index,
            Self::Past => usize::MAX,
        }
    }

    /// The index of the list's last row that it shows, past every row of
    /// the list of [`ViewRow::Past`].
    fn last(self) -> usize {
        match self {
            Self::Row(index) => index,
            Self::Folded { first, rows, .. } => first + rows - 1,
            Self::Past => usize::MAX,
        }
    }
}

/// The rows of the list that a view shows between two rows, each of its
/// view's row, as [`ViewMap::shown`] gives them: of no fold, a range of the
/// rows' numbers from the view's first, as the parts of a snapshot took
/// them before folds; of folds, the view's rows of the list's that meet
/// them.
pub(super) enum ShownRows<'a> {
    /// The rows from the first to the last, the view's first numbered
    /// `top`.
    Plain {
        top: i64,
        rows: std::ops::RangeInclusive<i64>,
    },
    /// The view's rows `ys` of `map`, its placeholders left out.
    Folded {
        map: ViewMap<'a>,
        ys: std::ops::Range<usize>,
    },
}

impl Iterator for ShownRows<'_> {
    type Item = (u16, i64);

    fn next(&mut self) -> Option<(u16, i64)> {
        match self {
            // Within the view's rows, which a `u16` holds.
            Self::Plain { top, rows } => rows.next().map(|row| ((row - *top) as u16, row)),
            Self::Folded { map, ys } => loop {
                let y = ys.next()?;
                if let ViewRow::Row(index) = map.rows[y] {
                    return Some((y as u16, map.number(index)));
                }
            },
        }
    }
}

/// A row of the view, by the numbers of the rows that it shows: a row of
/// the list, or a fold's placeholder, of its command's id, whether it
/// folds the whole block, and the numbers of its first row and its last.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShownRow {
    /// A row of the list, by its number.
    Row(i64),
    /// A fold's placeholder.
    Folded {
        /// The command's id.
        command: u64,
        /// Whether it folds the whole block.
        whole: bool,
        /// Its first row's number.
        first: i64,
        /// Its last row's number.
        last: i64,
    },
}

impl ShownRow {
    /// The number of the row that it shows, a fold's first.
    pub fn row(self) -> i64 {
        match self {
            Self::Row(row) | Self::Folded { first: row, .. } => row,
        }
    }
}

/// Where the view's rows are in the active screen's list, by the rows'
/// numbers: the number of the list's first row, the index of the view's
/// first, its height, and, of folds, its rows; of none, the list's rows
/// from its first, each at its place, of no cost.
#[derive(Clone, Copy, Debug)]
pub(super) struct ViewMap<'a> {
    first: i64,
    top: usize,
    height: usize,
    rows: &'a [ViewRow],
}

impl<'a> ViewMap<'a> {
    /// The view whose first row is the list's at `top`, of `height` rows,
    /// over a list whose first row is numbered `first`; `rows` its rows, of
    /// folds, or none.
    pub(super) fn new(first: i64, top: usize, height: usize, rows: &'a [ViewRow]) -> Self {
        Self {
            first,
            top,
            height,
            rows,
        }
    }

    /// The index in the list of the row numbered `row`, if it is one.
    fn index(&self, row: i64) -> Option<usize> {
        usize::try_from(row.checked_sub(self.first)?).ok()
    }

    /// The number of the list's row at `index`.
    fn number(&self, index: usize) -> i64 {
        // Fewer rows than an `i64` holds.
        self.first.saturating_add(index as i64)
    }

    /// How many rows the view shows, the row below its last among them
    /// while it has one.
    pub(super) fn height(&self) -> usize {
        self.height
    }

    /// The number of the view's first row.
    pub(super) fn top(&self) -> i64 {
        self.number(self.top)
    }

    /// The number past the last row of the list that the view shows, of a
    /// fold's too.
    pub(super) fn end(&self) -> i64 {
        let past = if self.rows.is_empty() {
            self.top + self.height
        } else {
            self.rows
                .iter()
                .rev()
                .find(|row| **row != ViewRow::Past)
                .map_or(self.top, |row| row.last() + 1)
        };
        self.number(past)
    }

    /// The view's row that shows the row numbered `row`: its own, or its
    /// fold's placeholder; none of a row that the view does not show.
    pub(super) fn showing(&self, row: i64) -> Option<u16> {
        self.showing_index(self.index(row)?)
    }

    /// The view's row that shows the list's row at `index`, as
    /// [`ViewMap::showing`] finds it.
    fn showing_index(&self, index: usize) -> Option<u16> {
        let y = if self.rows.is_empty() {
            index.checked_sub(self.top).filter(|&y| y < self.height)?
        } else {
            let y = self
                .rows
                .partition_point(|row| row.first() <= index)
                .checked_sub(1)?;
            (index <= self.rows[y].last()).then_some(y)?
        };
        // The view has at most `u16::MAX` rows.
        Some(y as u16)
    }

    /// The view's row of the row numbered `row` itself, none of a row that
    /// a fold takes or that the view does not show.
    pub(super) fn own(&self, row: i64) -> Option<u16> {
        self.own_index(self.index(row)?)
    }

    /// The view's row of the list's row at `index` itself, as
    /// [`ViewMap::own`] finds it.
    pub(super) fn own_index(&self, index: usize) -> Option<u16> {
        let y = self.showing_index(index)?;
        matches!(self.rows.get(usize::from(y)), None | Some(ViewRow::Row(_))).then_some(y)
    }

    /// The view's row of the row numbered `row`, as the end of a span takes
    /// it: its own or its fold's placeholder's, negative above the view and
    /// past its last below it, by the list's rows between it and the view.
    pub(super) fn place(&self, row: i64) -> i64 {
        if let Some(y) = self.showing(row) {
            return i64::from(y);
        }
        let top = self.top();
        if row < top {
            return row - top;
        }
        let height = self.height as i64;
        row.saturating_sub(self.end()).saturating_add(height)
    }

    /// The rows of the list that the view shows, from the row numbered
    /// `first` to `last`, each of its view's row: those that a fold takes
    /// left out. Of no fold, the rows between the view's first and last, as
    /// a range of numbers.
    pub(super) fn shown(&self, first: i64, last: i64) -> ShownRows<'a> {
        if self.rows.is_empty() {
            let top = self.top();
            // The view has at most `u16::MAX` rows.
            let bottom = top.saturating_add(self.height as i64) - 1;
            return ShownRows::Plain {
                top,
                rows: first.max(top)..=last.min(bottom),
            };
        }
        let from = self.index(first.max(self.first)).unwrap_or(0);
        let to = self.index(last).map_or(0, |to| to + 1);
        let start = self.rows.partition_point(|row| row.last() < from);
        let end = self.rows.partition_point(|row| row.first() < to);
        ShownRows::Folded {
            map: *self,
            ys: start..end.max(start),
        }
    }

    /// The folds that the view shows, from its top, by their rows' numbers.
    pub(super) fn folds(&self) -> Vec<FoldMark> {
        self.rows
            .iter()
            .filter_map(|&row| match row {
                ViewRow::Folded {
                    command,
                    whole,
                    first,
                    rows,
                } if command != FILTERED => Some(FoldMark {
                    command,
                    whole,
                    first: self.number(first),
                    last: self.number(first + rows - 1),
                }),
                ViewRow::Folded { .. } | ViewRow::Row(_) | ViewRow::Past => None,
            })
            .collect()
    }

    /// The view's row `y`, by the numbers of the rows that it shows; none
    /// past the list's last or the view's.
    pub(super) fn at(&self, y: u16) -> Option<ShownRow> {
        let y = usize::from(y);
        if y >= self.height {
            return None;
        }
        match self.rows.get(y) {
            None => Some(ShownRow::Row(self.number(self.top + y))),
            Some(&ViewRow::Row(index)) => Some(ShownRow::Row(self.number(index))),
            Some(&ViewRow::Folded {
                command,
                whole,
                first,
                rows,
            }) => Some(ShownRow::Folded {
                command,
                whole,
                first: self.number(first),
                last: self.number(first + rows - 1),
            }),
            Some(ViewRow::Past) => None,
        }
    }
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

    /// Takes away the folds that take a row from the row numbered `first`
    /// to `last`, as a search's current match there shows its rows. Whether
    /// it took one.
    pub fn unfold_rows(&mut self, first: i64, last: i64) -> bool {
        if self.folds.is_empty() || self.alternate_active {
            return false;
        }
        let (Some(from), Some(to)) = (
            self.row_index(first.max(self.row_number(0))),
            self.row_index(last.min(self.row_number(self.primary.rows.total_rows() - 1))),
        ) else {
            return false;
        };
        let met: Vec<u64> = self
            .fold_spans()
            .into_iter()
            .filter(|span| span.first <= to && from <= span.last)
            .map(|span| span.command)
            .collect();
        for command in &met {
            self.unfold(*command);
        }
        !met.is_empty()
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
        // A search's filter in place of the folds, while it shows.
        if let Some(search) = self.filter {
            return self.filter_spans(search);
        }
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
    /// from its pinned first row, as far as they fill it short of the
    /// list's end, or else, as the view follows the screen or meets its
    /// end, the rows that end at the list's last; none of no fold, or on the alternate screen, whose view
    /// is the list's rows from its top.
    pub(super) fn folded_view(&self) -> Option<FoldedView> {
        if self.alternate_active || (self.folds.is_empty() && self.filter.is_none()) {
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
        // Its rows from its pinned row, where they fill it short of the
        // list's end: one that meets the end follows it, as the view of no
        // fold does at the screen.
        let below = pinned
            .map(|top| down_from(starting(&spans, top), &spans, total, height))
            .filter(|(rows, end)| rows.len() == height && *end < total);
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

impl Terminal {
    /// The index of the view's first row in the active screen's list and,
    /// of folds, the view's rows, which [`Terminal::view_map`] maps.
    pub(super) fn view_rows(&self) -> (usize, Vec<ViewRow>) {
        match self.folded_view() {
            Some(view) => (view.top, view.rows),
            None => (self.view_top(), Vec::new()),
        }
    }

    /// The folds that the view shows now, from its top, by their rows'
    /// numbers; none of no fold.
    pub fn shown_folds(&self) -> Vec<FoldMark> {
        if self.folds.is_empty() {
            return Vec::new();
        }
        let (top, rows) = self.view_rows();
        self.view_map(top, &rows).folds()
    }

    /// The map of the view whose first row is the list's at `top`, of the
    /// view's rows `rows`, as [`Terminal::view_rows`] gives them.
    pub(super) fn view_map<'a>(&self, top: usize, rows: &'a [ViewRow]) -> ViewMap<'a> {
        ViewMap::new(self.row_number(0), top, usize::from(self.rows), rows)
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
pub(super) fn starting(spans: &[Span], index: usize) -> usize {
    let at = spans.partition_point(|span| span.last < index);
    spans
        .get(at)
        .filter(|span| span.first <= index)
        .map_or(index, |span| span.first)
}

/// The index in the list of the row at `place` of the view's rows from the
/// list's first, each fold of `spans` one row: a placeholder's, a row of
/// its fold.
pub(super) fn shown_index(spans: &[Span], place: usize) -> usize {
    let mut index = place;
    for span in spans {
        if span.first > index {
            break;
        }
        index = index.saturating_add(span.rows() - 1);
    }
    index
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
                    first: span.first,
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
                    first: span.first,
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
