//! The text of logical lines, the rows that soft-wrap into each other, with
//! where the text of each cell starts, read in one pass either way.

use std::sync::atomic::AtomicBool;

use bolt_vt_core::page::{Content, ListRow, PageList, Position, RowFlag, Width};

use crate::{Cancelled, Query};

/// A cell that shows something, and where its text starts in its line's.
#[derive(Clone, Copy, Debug)]
struct Spot {
    offset: usize,
    /// The row of the line that the cell is in, from 0.
    line_row: usize,
    position: Position,
}

/// The text of a logical line, as UTF-8.
#[derive(Debug, Default)]
struct Line {
    text: Vec<u8>,
    spots: Vec<Spot>,
}

impl Line {
    fn clear(&mut self) {
        self.text.clear();
        self.spots.clear();
    }

    /// Adds the text of `row`, row `line_row` of the line. Empty cells read
    /// as spaces; the halves of wide characters that show nothing, as their
    /// second halves and the heads that wrapping leaves, have no text.
    fn push_row(&mut self, row: ListRow<'_>, line_row: usize) {
        let mut utf8 = [0; 4];
        for (col, cell) in (0..).zip(row.cells()) {
            if matches!(cell.width(), Width::SpacerTail | Width::SpacerHead) {
                continue;
            }
            self.spots.push(Spot {
                offset: self.text.len(),
                line_row,
                position: Position {
                    page: row.page(),
                    row: row.row(),
                    col,
                },
            });
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
        let spots = self.spots.partition_point(|spot| spot.offset < len);
        self.spots.truncate(spots);
        if lower {
            self.text.make_ascii_lowercase();
        }
    }

    /// Where the text of the first cell at column `col` of row `line_row`
    /// of the line, or after it, starts.
    fn offset_of(&self, line_row: usize, col: u16) -> usize {
        let at = self
            .spots
            .partition_point(|spot| (spot.line_row, spot.position.col) < (line_row, col));
        self.spots
            .get(at)
            .map_or(self.text.len(), |spot| spot.offset)
    }

    /// The first and the last cell of the text in `range`, which is not
    /// empty.
    fn cells_of(&self, range: std::ops::Range<usize>) -> (Position, Position) {
        let cell = |offset| {
            let at = self.spots.partition_point(|spot| spot.offset <= offset);
            self.spots[at - 1].position
        };
        (cell(range.start), cell(range.end - 1))
    }
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
    while let Some(mut row) = rows.next() {
        Cancelled::check(cancel)?;
        line.clear();
        line.push_row(row, 0);
        let mut line_row = 0;
        while let Some(&after) = rows.peek() {
            if !wraps(row, after) {
                break;
            }
            rows.next();
            line_row += 1;
            line.push_row(after, line_row);
            row = after;
        }
        line.finish(query.lowers_ascii());
        let min = first
            .take()
            .map_or(0, |line_row| line.offset_of(line_row, from.col));
        if let Some(found) = query.first_from(&line.text, min, cancel)? {
            return Ok(Some(line.cells_of(found)));
        }
    }
    Ok(None)
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
        for (line_row, &row) in group.iter().rev().enumerate() {
            line.push_row(row, line_row);
        }
        line.finish(query.lowers_ascii());
        // Every match of the line, which is not empty, starts before its
        // end.
        let before = first.take().map_or(line.text.len(), |line_row| {
            line.offset_of(line_row, from.col)
        });
        if let Some(found) = query.last_before(&line.text, before, cancel)? {
            return Ok(Some(line.cells_of(found)));
        }
    }
    Ok(None)
}
