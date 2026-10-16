//! The text of logical lines, the rows that soft-wrap into each other, with
//! where the text of each cell starts, read in one pass either way.

use bolt_vt_core::page::{Content, Page, PageId, PageList, Position, RowFlag, Width};

use crate::literal::{Case, Literal};

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

    /// Adds the text of row `row` of `page`, row `line_row` of the line.
    /// Empty cells read as spaces; the halves of wide characters that show
    /// nothing, as their second halves and the heads that wrapping leaves,
    /// have no text.
    fn push_row(&mut self, id: PageId, page: &Page, row: u16, line_row: usize) {
        let mut utf8 = [0; 4];
        for (col, cell) in (0..).zip(page.cells(row)) {
            if matches!(cell.width(), Width::SpacerTail | Width::SpacerHead) {
                continue;
            }
            self.spots.push(Spot {
                offset: self.text.len(),
                line_row,
                position: Position { page: id, row, col },
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
                    for &c in std::iter::once(&first).chain(page.grapheme(row, col)) {
                        self.text
                            .extend_from_slice(c.encode_utf8(&mut utf8).as_bytes());
                    }
                }
            }
        }
    }

    /// Ends the line: the spaces that end it go, and with `case`, it is put
    /// in lower case, as the literal is.
    fn finish(&mut self, case: Case) {
        let len = self
            .text
            .iter()
            .rposition(|&byte| byte != b' ')
            .map_or(0, |at| at + 1);
        self.text.truncate(len);
        let spots = self.spots.partition_point(|spot| spot.offset < len);
        self.spots.truncate(spots);
        if case == Case::Insensitive {
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

    /// The first and the last cell of `len` bytes of text at `start`.
    fn cells_of(&self, start: usize, len: usize) -> (Position, Position) {
        let cell = |offset| {
            let at = self.spots.partition_point(|spot| spot.offset <= offset);
            self.spots[at - 1].position
        };
        (cell(start), cell(start + len - 1))
    }
}

/// Whether row `row` of `page` goes on into the row after it, which
/// continues it, as `next` says.
fn wraps(page: &Page, row: u16, next: (&Page, u16)) -> bool {
    page.row(row).has(RowFlag::SoftWrapped) && next.0.row(next.1).has(RowFlag::Continuation)
}

/// The index of the first row of the logical line that holds the row at
/// `index`.
fn line_start(list: &PageList, index: usize) -> usize {
    let mut rows = list.rows_before(index + 1);
    let Some((_, mut page, mut row)) = rows.next() else {
        return index;
    };
    let mut start = index;
    for (_, before, before_row) in rows {
        if !wraps(before, before_row, (page, row)) {
            break;
        }
        start -= 1;
        (page, row) = (before, before_row);
    }
    start
}

/// The index past the last row of the logical line that holds the row at
/// `index`.
fn line_end(list: &PageList, index: usize) -> usize {
    let mut rows = list.rows_from(index);
    let Some((_, mut page, mut row)) = rows.next() else {
        return index + 1;
    };
    let mut end = index + 1;
    for (_, after, after_row) in rows {
        if !wraps(page, row, (after, after_row)) {
            break;
        }
        end += 1;
        (page, row) = (after, after_row);
    }
    end
}

/// The first match of `literal` that starts at `from`, in the row at
/// `index` of `list`, or after it.
pub(crate) fn find_forward(
    list: &PageList,
    index: usize,
    literal: &Literal,
    from: Position,
) -> Option<(Position, Position)> {
    let start = line_start(list, index);
    let mut rows = list.rows_from(start).peekable();
    let mut line = Line::default();
    // The row of `from` in the first line.
    let mut first = Some(index - start);
    while let Some((id, page, row)) = rows.next() {
        line.clear();
        line.push_row(id, page, row, 0);
        let (mut page, mut row) = (page, row);
        let mut line_row = 0;
        while let Some(&(id, after, after_row)) = rows.peek() {
            if !wraps(page, row, (after, after_row)) {
                break;
            }
            rows.next();
            line_row += 1;
            line.push_row(id, after, after_row, line_row);
            (page, row) = (after, after_row);
        }
        line.finish(literal.case());
        let min = first
            .take()
            .map_or(0, |line_row| line.offset_of(line_row, from.col));
        if let Some(at) = literal.find_in(&line.text[min..]) {
            return Some(line.cells_of(min + at, literal.len()));
        }
    }
    None
}

/// The last match of `literal` that starts before `from`, in the row at
/// `index` of `list`, or before it.
pub(crate) fn find_backward(
    list: &PageList,
    index: usize,
    literal: &Literal,
    from: Position,
) -> Option<(Position, Position)> {
    let start = line_start(list, index);
    let mut rows = list.rows_before(line_end(list, index)).peekable();
    let mut group: Vec<(PageId, &Page, u16)> = Vec::new();
    let mut line = Line::default();
    let mut first = Some(index - start);
    while let Some(newest) = rows.next() {
        // The line's rows, newest first.
        group.clear();
        group.push(newest);
        while let Some(&(id, before, before_row)) = rows.peek() {
            let &(_, page, row) = group.last().expect("the newest row");
            if !wraps(before, before_row, (page, row)) {
                break;
            }
            rows.next();
            group.push((id, before, before_row));
        }
        line.clear();
        for (line_row, &(id, page, row)) in group.iter().rev().enumerate() {
            line.push_row(id, page, row, line_row);
        }
        line.finish(literal.case());
        // A match that starts before `from` ends before `from` and the
        // literal's length, but its last byte.
        let end = first.take().map_or(line.text.len(), |line_row| {
            let before = line.offset_of(line_row, from.col);
            (before + literal.len() - 1).min(line.text.len())
        });
        if let Some(at) = literal.rfind_in(&line.text[..end]) {
            return Some(line.cells_of(at, literal.len()));
        }
    }
    None
}
