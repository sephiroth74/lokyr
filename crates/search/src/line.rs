//! The text of logical lines, the rows that soft-wrap into each other, with
//! where the text of each cell starts, read in one pass either way.

use std::iter::Peekable;
use std::ops::Range;
use std::sync::atomic::AtomicBool;

use bolt_vt_core::page::{Content, ListRow, PageId, PageList, Position, RowFlag, Width};

use crate::query::Text;
use crate::{Cancelled, Query};

/// The text of a logical line, as UTF-8, and its cells that show
/// something.
#[derive(Debug, Default)]
struct Line {
    text: Vec<u8>,
    /// Where the text of each cell starts: every cell starts before 4 GiB,
    /// as [`Line::push_row`] leaves out the cells past it, though the last
    /// cell's text can end past it.
    starts: Vec<u32>,
    /// The column of each cell.
    cols: Vec<u16>,
    /// The line's rows, in order: each one's page and row, and its first
    /// cell, an index into `starts`.
    rows: Vec<(PageId, u16, usize)>,
}

impl Line {
    fn clear(&mut self) {
        self.text.clear();
        self.starts.clear();
        self.cols.clear();
        self.rows.clear();
    }

    /// The text, as a query searches it.
    fn text(&self) -> Text<'_> {
        Text {
            bytes: &self.text,
            starts: &self.starts,
        }
    }

    /// Adds the text of `row`, the line's next. Empty cells read as spaces;
    /// the halves of wide characters that show nothing, as their second
    /// halves and the heads that wrapping leaves, have no text. The cells of
    /// a line that would start past 4 GiB of text, which only a scrollback
    /// that large can hold, are left out: the rest of the line is not
    /// searched.
    fn push_row(&mut self, row: ListRow<'_>) {
        self.rows.push((row.page(), row.row(), self.starts.len()));
        let mut utf8 = [0; 4];
        for (col, cell) in row.cells().enumerate() {
            // A row has `u16::MAX` cells at most. Counted with the cells
            // rather than with an inclusive range, which does not optimize
            // as well in this loop, the search's hottest.
            let col = col as u16;
            if matches!(cell.width(), Width::SpacerTail | Width::SpacerHead) {
                continue;
            }
            let Ok(start) = u32::try_from(self.text.len()) else {
                return;
            };
            self.starts.push(start);
            self.cols.push(col);
            match cell.content() {
                Content::Char('\0') | Content::BgPalette(_) | Content::BgRgb(_) => {
                    self.text.push(b' ');
                }
                Content::Char(c) => {
                    self.text
                        .extend_from_slice(c.encode_utf8(&mut utf8).as_bytes());
                }
                Content::Grapheme(first) => {
                    for &c in std::iter::once(&first).chain(row.grapheme(col)) {
                        self.text
                            .extend_from_slice(c.encode_utf8(&mut utf8).as_bytes());
                    }
                }
            }
        }
    }

    /// Ends the line: the spaces that end it go, and it is put in lower
    /// case if `lower`, as an insensitive literal is.
    fn finish(&mut self, lower: bool) {
        let len = self
            .text
            .iter()
            .rposition(|&byte| byte != b' ')
            .map_or(0, |at| at + 1);
        self.text.truncate(len);
        let cells = self.starts.partition_point(|&start| (start as usize) < len);
        self.starts.truncate(cells);
        self.cols.truncate(cells);
        if lower {
            self.text.make_ascii_lowercase();
        }
    }

    /// The cells of row `line_row` of the line, as indexes into `starts`.
    fn cells_in(&self, line_row: usize) -> std::ops::Range<usize> {
        let first = |line_row: usize| {
            self.rows
                .get(line_row)
                .map_or(self.starts.len(), |&(.., first)| {
                    first.min(self.starts.len())
                })
        };
        first(line_row)..first(line_row + 1)
    }

    /// Where the text of the first cell at column `col` of row `line_row`
    /// of the line, or after it, starts.
    fn offset_of(&self, line_row: usize, col: u16) -> usize {
        let cells = self.cells_in(line_row);
        let at = cells.start + self.cols[cells].partition_point(|&other| other < col);
        self.starts
            .get(at)
            .map_or(self.text.len(), |&start| start as usize)
    }

    /// The row of the line, from 0, that holds the cell whose text holds
    /// the byte at `offset`, and the cell.
    fn cell_at(&self, offset: usize) -> (usize, usize) {
        let cell = self
            .starts
            .partition_point(|&start| start as usize <= offset)
            - 1;
        let line_row = self.rows.partition_point(|&(.., first)| first <= cell) - 1;
        (line_row, cell)
    }

    /// The first and the last cell of the text in `range`, which is not
    /// empty.
    fn cells_of(&self, range: std::ops::Range<usize>) -> (Position, Position) {
        let position = |offset| {
            let (line_row, cell) = self.cell_at(offset);
            let (page, row, _) = self.rows[line_row];
            Position {
                page,
                row,
                col: self.cols[cell],
            }
        };
        (position(range.start), position(range.end - 1))
    }
}

/// Reads the next logical line of `rows` into `line`, oldest row first,
/// and ends it as `query` searches it; false once the rows have run out.
fn read_line<'a>(
    rows: &mut Peekable<impl Iterator<Item = ListRow<'a>>>,
    line: &mut Line,
    query: &Query,
) -> bool {
    let Some(mut row) = rows.next() else {
        return false;
    };
    line.clear();
    line.push_row(row);
    while let Some(&after) = rows.peek() {
        if !wraps(row, after) {
            break;
        }
        rows.next();
        line.push_row(after);
        row = after;
    }
    line.finish(query.lowers_ascii());
    true
}

/// Whether `row` goes on into `next`, the row after it, which continues
/// it.
fn wraps(row: ListRow<'_>, next: ListRow<'_>) -> bool {
    row.metadata().has(RowFlag::SoftWrapped) && next.metadata().has(RowFlag::Continuation)
}

/// The index of the first row of the logical line that holds the row at
/// `index`.
fn line_start(list: &PageList, index: usize) -> usize {
    let mut rows = list.rows_before(index + 1);
    let Some(mut row) = rows.next() else {
        return index;
    };
    let mut start = index;
    for before in rows {
        if !wraps(before, row) {
            break;
        }
        start -= 1;
        row = before;
    }
    start
}

/// The index past the last row of the logical line that holds the row at
/// `index`.
fn line_end(list: &PageList, index: usize) -> usize {
    let mut rows = list.rows_from(index);
    let Some(mut row) = rows.next() else {
        return index + 1;
    };
    let mut end = index + 1;
    for after in rows {
        if !wraps(row, after) {
            break;
        }
        end += 1;
        row = after;
    }
    end
}

/// A match's first and last cells.
pub(crate) type Found = Result<Option<(Position, Position)>, Cancelled>;

/// The first match of `query` that starts at `from`, in the row at `index`
/// of `list`, or after it, unless `cancel` is set first.
pub(crate) fn find_forward(
    list: &PageList,
    index: usize,
    query: &Query,
    from: Position,
    cancel: &AtomicBool,
) -> Found {
    let start = line_start(list, index);
    let mut rows = list.rows_from(start).peekable();
    let mut line = Line::default();
    // The row of `from` in the first line.
    let mut first = Some(index - start);
    loop {
        Cancelled::check(cancel)?;
        if !read_line(&mut rows, &mut line, query) {
            return Ok(None);
        }
        let min = first
            .take()
            .map_or(0, |line_row| line.offset_of(line_row, from.col));
        if let Some(found) = query.first_from(&line.text(), min, cancel)? {
            return Ok(Some(line.cells_of(found)));
        }
    }
}

/// The last match of `query` that starts before `from`, in the row at
/// `index` of `list`, or before it, unless `cancel` is set first.
pub(crate) fn find_backward(
    list: &PageList,
    index: usize,
    query: &Query,
    from: Position,
    cancel: &AtomicBool,
) -> Found {
    let start = line_start(list, index);
    let mut rows = list.rows_before(line_end(list, index)).peekable();
    let mut group: Vec<ListRow<'_>> = Vec::new();
    let mut line = Line::default();
    let mut first = Some(index - start);
    while let Some(newest) = rows.next() {
        Cancelled::check(cancel)?;
        // The line's rows, newest first.
        group.clear();
        group.push(newest);
        while let Some(&before) = rows.peek() {
            let &row = group.last().expect("the newest row");
            if !wraps(before, row) {
                break;
            }
            rows.next();
            group.push(before);
        }
        line.clear();
        for &row in group.iter().rev() {
            line.push_row(row);
        }
        line.finish(query.lowers_ascii());
        // Every match of the line, which is not empty, starts before its
        // end.
        let before = first.take().map_or(line.text.len(), |line_row| {
            line.offset_of(line_row, from.col)
        });
        if let Some(found) = query.last_before(&line.text(), before, cancel)? {
            return Ok(Some(line.cells_of(found)));
        }
    }
    Ok(None)
}

/// The first `limit` matches of `query` that start in a row of `list` whose
/// index is in `rows`, in order, and how many there are, unless `cancel` is
/// set first: each logical line is read once, and its matches gone through
/// once.
pub(crate) fn find_all(
    list: &PageList,
    rows: Range<usize>,
    query: &Query,
    limit: usize,
    cancel: &AtomicBool,
) -> Result<(Vec<(Position, Position)>, usize), Cancelled> {
    let mut found = Vec::new();
    let mut count = 0;
    if rows.is_empty() {
        return Ok((found, count));
    }
    // The index of the first row of the line that is read next.
    let mut index = line_start(list, rows.start);
    let mut lines = list.rows_from(index).peekable();
    let mut line = Line::default();
    while index < rows.end {
        Cancelled::check(cancel)?;
        if !read_line(&mut lines, &mut line, query) {
            break;
        }
        let first = index;
        index += line.rows.len();
        query.each(&line.text(), cancel, |range| {
            let (line_row, _) = line.cell_at(range.start);
            if rows.contains(&(first + line_row)) {
                if found.len() < limit {
                    found.push(line.cells_of(range));
                }
                count += 1;
            }
        })?;
    }
    Ok((found, count))
}
