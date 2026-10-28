//! The selection: cells of the active screen and of its scrollback, from one
//! to another in reading order or as a rectangle, grown to words or to
//! logical lines, and held by two pins, so that it keeps its cells as output
//! scrolls them up. Output that writes over its cells on the screen clears
//! it, though it scrolls them into the scrollback before the next look, as
//! does a switch to the other screen.

use std::hash::{Hash, Hasher};
use std::ops::Range;

use super::Terminal;
use crate::page::{Content, ListRow, PageList, PinId, Position, RowFlag, StyleId, Width};

/// The characters that end a word, besides spaces, by default: quotes,
/// brackets and the punctuation that separates fields, so that a path or a
/// name with dots and dashes is one word.
pub const WORD_SEPARATORS: &str = "\t'\"│`|:;,()[]{}<>";

/// How a selection takes its cells.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelectionShape {
    /// The cells from the first to the last, in reading order, as text runs
    /// from row to row.
    Cells,
    /// The cells of the columns from the first's to the last's, on each row
    /// from the first's to the last's.
    Rectangle,
}

/// What a selection of [`SelectionShape::Cells`] grows to, from the cells
/// that it starts and ends at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelectionUnit {
    /// The cells themselves.
    Cell,
    /// The words that they are in: the cells around each that are not
    /// spaces nor separators, across soft wraps; a run of spaces, on a
    /// space; a separator alone.
    Word,
    /// The logical lines that they are in, their soft-wrapped rows whole.
    Line,
}

/// The selection, as a renderer shows it: its first and last cells, each
/// by its column and its row from the view's first, which can be above the
/// view or below it, and its shape. A rectangle's first cell is its top left
/// corner, its last its bottom right one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Selected {
    /// How it takes its cells.
    pub shape: SelectionShape,
    /// Its first cell: the column, and the row from the view's first.
    pub start: (u16, i64),
    /// Its last cell.
    pub end: (u16, i64),
}

impl Selected {
    /// Whether the cell at column `col` of the view's row `row` is selected.
    pub fn contains(&self, col: u16, row: u16) -> bool {
        let row = i64::from(row);
        match self.shape {
            SelectionShape::Cells => {
                (row, col) >= (self.start.1, self.start.0) && (row, col) <= (self.end.1, self.end.0)
            }
            SelectionShape::Rectangle => {
                (self.start.1..=self.end.1).contains(&row)
                    && (self.start.0..=self.end.0).contains(&col)
            }
        }
    }
}

/// A cell of a list, by the index of its row, oldest first, and its column.
type At = (usize, u16);

/// The selection that a terminal holds.
#[derive(Debug)]
pub(super) struct Selection {
    /// The pins of its first and last cells.
    start: PinId,
    end: PinId,
    shape: SelectionShape,
    /// Whether the pins are the alternate screen's.
    alternate: bool,
    /// How many rows it takes, from its first to its last, which a move of
    /// its rows that drops some, or brings others in, changes.
    rows: usize,
    /// The rows that were on the screen as the selection was taken, which
    /// output can write over: the first one's offset from the selection's
    /// first row, and the hash of each one's selected cells.
    first_hashed: usize,
    hashes: Vec<u64>,
}

impl Selection {
    /// Where the selection's first and last cells are now, if neither
    /// left the list, and the last does not come before the first, as a
    /// scroll within left and right margins, which moves the cells between
    /// them alone, can move their pins.
    fn bounds(&self, rows: &PageList) -> Option<(At, At)> {
        let at = |pin: PinId| {
            let position = rows.pin_position(pin)?;
            Some((rows.row_index(position.page, position.row)?, position.col))
        };
        let (start, end) = (at(self.start)?, at(self.end)?);
        let ordered = match self.shape {
            SelectionShape::Cells => start <= end,
            SelectionShape::Rectangle => start.0 <= end.0 && start.1 <= end.1,
        };
        ordered.then_some((start, end))
    }

    /// The selected columns of the row at `index`, between `start` and
    /// `end`, of a screen of `cols` columns.
    fn columns(&self, index: usize, (start, end): (At, At), cols: u16) -> Range<u16> {
        match self.shape {
            SelectionShape::Rectangle => start.1..end.1 + 1,
            SelectionShape::Cells => {
                let first = if index == start.0 { start.1 } else { 0 };
                let last = if index == end.0 { end.1 + 1 } else { cols };
                first..last
            }
        }
    }

    /// The hashes of the selected cells of the rows on the screen, from
    /// `bounds`, the selection's, in `rows`.
    fn screen_hashes(&self, rows: &PageList, bounds: (At, At)) -> (usize, Vec<u64>) {
        let (start, end) = bounds;
        let first = start.0.max(rows.scrollback_rows());
        let hashes = (first..=end.0)
            .zip(rows.rows_from(first))
            .map(|(index, row)| hash(row, self.columns(index, bounds, rows.cols())))
            .collect();
        (first.saturating_sub(start.0), hashes)
    }

    /// Takes the hashes of the selected cells on the screen, at `bounds`,
    /// as they are: the cells that output can write over.
    fn baseline(&mut self, rows: &PageList, bounds: (At, At)) {
        self.rows = bounds.1.0 - bounds.0.0 + 1;
        (self.first_hashed, self.hashes) = self.screen_hashes(rows, bounds);
    }

    /// Whether the selection's cells on the screen are as they were taken:
    /// its rows as many, and the cells of those that were on the screen
    /// then the same. With `copied`, the list's index of the view's first
    /// row and which rows of the view a snapshot copied, only those rows
    /// are looked at, as the others did not change since the last look:
    /// a row that output writes over is copied as it comes into the view.
    ///
    /// The rows that output scrolled into the scrollback since the last
    /// look are looked at whatever the snapshot copied, as output could
    /// write over them before, and once: nothing writes over them there,
    /// and their hashes go.
    fn unwritten(
        &mut self,
        rows: &PageList,
        bounds: (At, At),
        copied: Option<(usize, &[bool])>,
    ) -> bool {
        let (start, end) = bounds;
        if end.0 - start.0 + 1 != self.rows {
            return false;
        }
        let hashed = start.0 + self.first_hashed;
        let gone = rows
            .scrollback_rows()
            .saturating_sub(hashed)
            .min(self.hashes.len());
        if gone > 0 {
            let same = (hashed..hashed + gone)
                .zip(rows.rows_from(hashed))
                .zip(&self.hashes)
                .all(|((index, row), &then)| {
                    then == hash(row, self.columns(index, bounds, rows.cols()))
                });
            if !same {
                return false;
            }
            self.hashes.drain(..gone);
            self.first_hashed += gone;
        }
        let first = start.0.max(rows.scrollback_rows());
        (first..=end.0)
            .zip(rows.rows_from(first))
            .all(|(index, row)| {
                let looked_at = copied.is_none_or(|(top, copied)| {
                    index
                        .checked_sub(top)
                        .and_then(|y| copied.get(y))
                        .is_some_and(|&copied| copied)
                });
                // The rows that came from the scrollback since the hashes,
                // as only a resize brings them, which takes them again,
                // have none.
                let then = (index - start.0)
                    .checked_sub(self.first_hashed)
                    .and_then(|at| self.hashes.get(at));
                match then {
                    Some(&then) if looked_at => {
                        then == hash(row, self.columns(index, bounds, rows.cols()))
                    }
                    _ => true,
                }
            })
    }
}

/// A hasher of cells, a word a step, as the selection's checks run at each
/// frame: rotate, mix in the word, multiply.
#[derive(Default)]
struct Mix(u64);

impl Hasher for Mix {
    fn write(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.write_u64(u64::from(byte));
        }
    }

    fn write_u32(&mut self, value: u32) {
        self.write_u64(u64::from(value));
    }

    fn write_u64(&mut self, value: u64) {
        self.0 = (self.0.rotate_left(5) ^ value).wrapping_mul(0x51_7c_c1_b7_27_22_0a_95);
    }

    fn finish(&self) -> u64 {
        self.0
    }
}

/// The hash of the cells of `row` in `columns`, with their clusters'
/// codepoints, and their styles by their values: a page gives a style
/// another id as a row is written again, or moves to another page. A
/// style's fields go in where the style changes, as a row's cells come in
/// runs of one: a row's page gives equal styles one id.
fn hash(row: ListRow<'_>, columns: Range<u16>) -> u64 {
    let (page, _) = row.parts();
    let mut hasher = Mix::default();
    // The last cell's style; the default's fields pack to 0.
    let mut id = StyleId::DEFAULT;
    for x in columns {
        let cell = row.cell(x);
        cell.with_style(StyleId::DEFAULT).hash(&mut hasher);
        if cell.style() != id {
            id = cell.style();
            let bits = page.styles().get(id).bits();
            // The 89 bits, in two words.
            hasher.write_u64(bits as u64);
            hasher.write_u64((bits >> 64) as u64);
        }
        if let Content::Grapheme(_) = cell.content() {
            row.grapheme(x).hash(&mut hasher);
        }
    }
    hasher.finish()
}

/// What a cell is to a word.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Class {
    /// A character of a word.
    Word,
    /// A space, or a cell without a character.
    Space,
    /// A separator, a word of its own.
    Separator,
}

/// What the cell at column `col` of `row` is to a word, with `separators`;
/// a wide character's second half is what its first is.
fn class(row: ListRow<'_>, col: u16, separators: &str) -> Class {
    let mut cell = row.cell(col);
    if cell.width() == Width::SpacerTail && col > 0 {
        cell = row.cell(col - 1);
    }
    match cell.content() {
        Content::Char('\0' | ' ') | Content::BgPalette(_) | Content::BgRgb(_) => Class::Space,
        Content::Char(character) | Content::Grapheme(character)
            if separators.contains(character) =>
        {
            Class::Separator
        }
        Content::Char(_) | Content::Grapheme(_) => Class::Word,
    }
}

/// The row at `index` of `rows`.
fn row_at(rows: &PageList, index: usize) -> Option<ListRow<'_>> {
    rows.rows_from(index).next()
}

/// Whether the row at `index` wraps into the next: it soft-wraps, and the
/// next goes on from it, as rows that scroll regions move do not.
fn wraps(rows: &PageList, index: usize) -> bool {
    let mut both = rows.rows_from(index);
    let (Some(row), Some(next)) = (both.next(), both.next()) else {
        return false;
    };
    row.metadata().has(RowFlag::SoftWrapped) && next.metadata().has(RowFlag::Continuation)
}

/// The last column of the row at `index` that a line goes on from, as it
/// wraps into the next: the last, or the one before a spacer head that a
/// wide character left as it went on in the next row.
fn wrap_column(rows: &PageList, index: usize) -> Option<u16> {
    let row = row_at(rows, index)?;
    let last = row.cols() - 1;
    match row.cell(last).width() {
        Width::SpacerHead => last.checked_sub(1),
        _ => Some(last),
    }
}

/// The first and the last cell of the word at `at`, with `separators`.
fn word(rows: &PageList, at: At, separators: &str) -> (At, At) {
    let Some(row) = row_at(rows, at.0) else {
        return (at, at);
    };
    let kind = class(row, at.1, separators);
    if kind == Class::Separator {
        return (at, at);
    }
    let (mut start, mut end) = (at, at);
    // Back, across the rows that wrap into the next.
    let mut row = row;
    loop {
        while start.1 > 0 && class(row, start.1 - 1, separators) == kind {
            start.1 -= 1;
        }
        let Some(before) = start.0.checked_sub(1) else {
            break;
        };
        if start.1 > 0 || !wraps(rows, before) {
            break;
        }
        let (Some(previous), Some(last)) = (row_at(rows, before), wrap_column(rows, before)) else {
            break;
        };
        if class(previous, last, separators) != kind {
            break;
        }
        (row, start) = (previous, (before, last));
    }
    // On, likewise.
    let Some(mut row) = row_at(rows, at.0) else {
        return (start, end);
    };
    loop {
        let last = wrap_column(rows, end.0).unwrap_or(0);
        while end.1 < row.cols() - 1 && class(row, end.1 + 1, separators) == kind {
            end.1 += 1;
        }
        if end.1 < last || !wraps(rows, end.0) {
            break;
        }
        let Some(next) = row_at(rows, end.0 + 1) else {
            break;
        };
        if class(next, 0, separators) != kind {
            break;
        }
        (row, end) = (next, (end.0 + 1, 0));
    }
    (start, end)
}

/// The first and the last row of the logical line of the row at `index`.
fn line(rows: &PageList, index: usize) -> (usize, usize) {
    let mut first = index;
    while first > 0 && wraps(rows, first - 1) {
        first -= 1;
    }
    let mut last = index;
    while wraps(rows, last) {
        last += 1;
    }
    (first, last)
}

/// Whether the cells of `row` from column `from` on show nothing.
fn blank_from(row: ListRow<'_>, from: u16) -> bool {
    (from..row.cols()).all(|x| {
        matches!(
            row.cell(x).content(),
            Content::Char('\0' | ' ') | Content::BgPalette(_) | Content::BgRgb(_)
        )
    })
}

/// Appends the text of the cells of `row` in `columns` to `text`: a cell
/// without a character as a space, a cluster whole, a wide character once,
/// and nothing of the spacer head that a wide character that went on in the
/// next row left at the row's end.
fn push_cells(text: &mut String, row: ListRow<'_>, columns: Range<u16>) {
    let last = row.cols() - 1;
    for x in columns {
        let cell = row.cell(x);
        match cell.width() {
            Width::SpacerTail => continue,
            Width::SpacerHead if x == last => continue,
            _ => {}
        }
        match cell.content() {
            Content::Char('\0') | Content::BgPalette(_) | Content::BgRgb(_) => text.push(' '),
            Content::Char(character) => text.push(character),
            Content::Grapheme(first) => {
                text.push(first);
                text.extend(row.grapheme(x));
            }
        }
    }
}

/// Drops the spaces that end `text`'s last line.
fn trim_line(text: &mut String) {
    text.truncate(text.trim_end_matches(' ').len());
}

impl Terminal {
    /// Selects from the view's cell `from`, its column and its row, to its
    /// cell `to`, either before the other, in `shape`, grown to `unit`, with
    /// `separators` ending the words besides spaces, as [`WORD_SEPARATORS`]
    /// does by default. A rectangle takes no unit. The selection that was
    /// there goes.
    pub fn select(
        &mut self,
        from: (u16, u16),
        to: (u16, u16),
        shape: SelectionShape,
        unit: SelectionUnit,
        separators: &str,
    ) {
        self.clear_selection();
        let top = self.view_top();
        let rows = &self.screen().rows;
        let (cols, last_row) = (rows.cols(), rows.rows() - 1);
        let at = |(col, row): (u16, u16)| (top + usize::from(row.min(last_row)), col.min(cols - 1));
        let (a, b) = (at(from), at(to));
        let (mut start, mut end) = match shape {
            SelectionShape::Rectangle => {
                ((a.0.min(b.0), a.1.min(b.1)), (a.0.max(b.0), a.1.max(b.1)))
            }
            SelectionShape::Cells => {
                let (first, last) = if (a.0, a.1) <= (b.0, b.1) {
                    (a, b)
                } else {
                    (b, a)
                };
                match unit {
                    SelectionUnit::Cell => (first, last),
                    SelectionUnit::Word => (
                        word(rows, first, separators).0,
                        word(rows, last, separators).1,
                    ),
                    SelectionUnit::Line => {
                        ((line(rows, first.0).0, 0), (line(rows, last.0).1, cols - 1))
                    }
                }
            }
        };
        // A wide character whole, from either of its halves.
        if let Some(row) = row_at(rows, start.0)
            && start.1 > 0
            && row.cell(start.1).width() == Width::SpacerTail
        {
            start.1 -= 1;
        }
        if let Some(row) = row_at(rows, end.0)
            && end.1 + 1 < cols
            && row.cell(end.1).width() == Width::Wide
        {
            end.1 += 1;
        }
        self.take_selection((start, end), shape);
    }

    /// Selects every cell of the active screen and of its scrollback, to
    /// the last row that shows something, or the cursor's, which is later:
    /// the empty rows below them are left out.
    pub fn select_all(&mut self) {
        self.clear_selection();
        let cursor = self.screen().rows.scrollback_rows() + usize::from(self.cursor().y);
        let rows = &self.screen().rows;
        let total = rows.total_rows();
        let shown = (cursor + 1..total)
            .rev()
            .zip(rows.rows_before(total))
            .find(|&(_, row)| !blank_from(row, 0))
            .map_or(cursor, |(index, _)| index);
        let last = (shown, rows.cols() - 1);
        self.take_selection(((0, 0), last), SelectionShape::Cells);
    }

    /// Lets the selection go, if there is one.
    pub fn clear_selection(&mut self) {
        let Some(selection) = self.selection.take() else {
            return;
        };
        let rows = if selection.alternate {
            &mut self.alternate.rows
        } else {
            &mut self.primary.rows
        };
        rows.unpin(selection.start);
        rows.unpin(selection.end);
    }

    /// Pins the selection from `bounds`' first cell to its last, in
    /// `shape`, with the hashes of its cells on the screen.
    fn take_selection(&mut self, bounds: (At, At), shape: SelectionShape) {
        let alternate = self.alternate_active;
        let rows = &mut self.screen_mut().rows;
        let mut pin = |(index, col): At| {
            let (page, row) = rows.row_at(index)?;
            rows.pin(Position { page, row, col })
        };
        let (start, end) = (pin(bounds.0), pin(bounds.1));
        let (Some(start), Some(end)) = (start, end) else {
            for pin in [start, end].into_iter().flatten() {
                rows.unpin(pin);
            }
            return;
        };
        let mut selection = Selection {
            start,
            end,
            shape,
            alternate,
            rows: 0,
            first_hashed: 0,
            hashes: Vec::new(),
        };
        selection.baseline(rows, bounds);
        self.selection = Some(selection);
    }

    /// The list's index of the view's first row.
    fn view_top(&self) -> usize {
        self.screen().rows.scrollback_rows() - self.viewport_offset()
    }

    /// The selection's first and last cells, once the selection is still
    /// there: it goes when output wrote over its cells, a move of its rows
    /// dropped or brought in some, its first or last cell left the list, or
    /// the other screen became the active one.
    ///
    /// With `copied`, the rows that a snapshot copied of the view whose
    /// first row is at that index of the list, only those are looked at.
    fn settle_selection(&mut self, copied: Option<(usize, &[bool])>) -> Option<(At, At)> {
        let alternate = self.alternate_active;
        let rows = if alternate {
            &self.alternate.rows
        } else {
            &self.primary.rows
        };
        let selection = self.selection.as_mut()?;
        let bounds = (selection.alternate == alternate)
            .then(|| selection.bounds(rows))
            .flatten()
            .filter(|&bounds| selection.unwritten(rows, bounds, copied));
        if bounds.is_none() {
            self.clear_selection();
        }
        bounds
    }

    /// Takes the hashes of the selection's cells on the screen again, as a
    /// resize moves its rows; a rectangle, whose columns a reflow does not
    /// keep, goes.
    pub(super) fn resized_selection(&mut self) {
        let Some(selection) = &self.selection else {
            return;
        };
        if selection.shape == SelectionShape::Rectangle
            || selection.alternate != self.alternate_active
        {
            self.clear_selection();
            return;
        }
        let rows = if selection.alternate {
            &self.alternate.rows
        } else {
            &self.primary.rows
        };
        match selection.bounds(rows) {
            Some(bounds) => {
                if let Some(selection) = &mut self.selection {
                    selection.baseline(rows, bounds);
                }
            }
            None => self.clear_selection(),
        }
    }

    /// The selection, as the view shows it, if there is one.
    pub fn selection(&mut self) -> Option<Selected> {
        self.selected(None)
    }

    /// The selection, as the view shows it, once the rows of the view that
    /// a snapshot copied, `copied`, are looked at.
    pub(super) fn selection_in_snapshot(&mut self, copied: &[bool]) -> Option<Selected> {
        let top = self.view_top();
        self.selected(Some((top, copied)))
    }

    fn selected(&mut self, copied: Option<(usize, &[bool])>) -> Option<Selected> {
        let (start, end) = self.settle_selection(copied)?;
        let shape = self.selection.as_ref()?.shape;
        let top = self.view_top() as i64;
        let row = |index: usize| index as i64 - top;
        Some(Selected {
            shape,
            start: (start.1, row(start.0)),
            end: (end.1, row(end.0)),
        })
    }

    /// The selection's text, if there is one: rows that soft-wrap joined to
    /// the next, each line's ending spaces dropped, a line's own and the
    /// selection's where the rest of its row shows nothing, clusters whole,
    /// and a rectangle's rows each a line.
    pub fn selection_text(&mut self) -> Option<String> {
        let bounds = self.settle_selection(None)?;
        let selection = self.selection.as_ref()?;
        let rows = &self.screen().rows;
        let cols = rows.cols();
        let (start, end) = bounds;
        let mut text = String::new();
        for (index, row) in (start.0..=end.0).zip(rows.rows_from(start.0)) {
            let columns = selection.columns(index, bounds, cols);
            let to = columns.end;
            push_cells(&mut text, row, columns);
            let last = index == end.0;
            if selection.shape == SelectionShape::Cells && !last && to == cols && wraps(rows, index)
            {
                continue;
            }
            let line_ends = match selection.shape {
                SelectionShape::Rectangle => true,
                SelectionShape::Cells => !last || (!wraps(rows, index) && blank_from(row, to)),
            };
            if line_ends {
                trim_line(&mut text);
            }
            if !last {
                text.push('\n');
            }
        }
        Some(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::Parser;
    use crate::terminal::{Options, Scroll};

    fn fed(cols: u16, rows: u16, text: &str) -> Terminal {
        let mut terminal = Terminal::new(cols, rows, Options::default());
        Parser::new().advance(&mut terminal, text.as_bytes());
        terminal
    }

    fn feed(terminal: &mut Terminal, text: &str) {
        Parser::new().advance(terminal, text.as_bytes());
    }

    /// The text that `terminal` selects from `from` to `to`, by `unit`.
    fn selected(
        terminal: &mut Terminal,
        from: (u16, u16),
        to: (u16, u16),
        unit: SelectionUnit,
    ) -> String {
        terminal.select(from, to, SelectionShape::Cells, unit, WORD_SEPARATORS);
        terminal.selection_text().expect("a selection")
    }

    fn word_at(terminal: &mut Terminal, at: (u16, u16)) -> String {
        selected(terminal, at, at, SelectionUnit::Word)
    }

    #[test]
    fn a_word_ends_at_spaces_and_separators() {
        let mut terminal = fed(40, 3, "ls /usr/lib-x.so (a,b)   end");
        assert_eq!(word_at(&mut terminal, (5, 0)), "/usr/lib-x.so");
        assert_eq!(word_at(&mut terminal, (0, 0)), "ls");
        // A separator alone, and the word inside it.
        assert_eq!(word_at(&mut terminal, (17, 0)), "(");
        assert_eq!(word_at(&mut terminal, (18, 0)), "a");
        // A run of spaces.
        assert_eq!(word_at(&mut terminal, (23, 0)), "   ");
        // Other separators, as the caller gives them.
        terminal.select(
            (5, 0),
            (5, 0),
            SelectionShape::Cells,
            SelectionUnit::Word,
            " /",
        );
        assert_eq!(terminal.selection_text().unwrap(), "usr");
        // From one word to another.
        assert_eq!(
            selected(&mut terminal, (1, 0), (26, 0), SelectionUnit::Word),
            "ls /usr/lib-x.so (a,b)   end"
        );
    }

    #[test]
    fn a_word_and_a_line_go_on_across_soft_wraps() {
        // `hello wond`, `erful worl` and `d`, which soft-wrap.
        let mut terminal = fed(10, 4, "hello wonderful world\r\nnext");
        assert_eq!(word_at(&mut terminal, (7, 0)), "wonderful");
        assert_eq!(word_at(&mut terminal, (2, 1)), "wonderful");
        assert_eq!(word_at(&mut terminal, (0, 2)), "world");
        assert_eq!(
            selected(&mut terminal, (3, 1), (3, 1), SelectionUnit::Line),
            "hello wonderful world"
        );
        assert_eq!(
            selected(&mut terminal, (0, 3), (0, 3), SelectionUnit::Line),
            "next"
        );
        // Lines from one to another, a line each.
        assert_eq!(
            selected(&mut terminal, (5, 0), (1, 3), SelectionUnit::Line),
            "hello wonderful world\nnext"
        );
    }

    #[test]
    fn a_wide_character_and_a_cluster_are_taken_whole() {
        let mut terminal = fed(20, 3, "日本語 e\u{301}x");
        // From the second half of 本.
        assert_eq!(word_at(&mut terminal, (3, 0)), "日本語");
        assert_eq!(
            selected(&mut terminal, (3, 0), (3, 0), SelectionUnit::Cell),
            "本"
        );
        assert_eq!(
            selected(&mut terminal, (7, 0), (7, 0), SelectionUnit::Cell),
            "e\u{301}"
        );
        // A wide character that went on in the next row leaves nothing of
        // the spacer head at the row's end.
        let mut terminal = fed(5, 3, "abcd日本");
        assert_eq!(
            selected(&mut terminal, (0, 0), (0, 0), SelectionUnit::Line),
            "abcd日本"
        );
        assert_eq!(word_at(&mut terminal, (1, 1)), "abcd日本");
    }

    #[test]
    fn the_text_joins_soft_wraps_and_drops_the_spaces_that_end_lines() {
        let mut terminal = fed(10, 4, "ab   \r\nhello wonderful\r\nx y z");
        assert_eq!(
            selected(&mut terminal, (0, 0), (9, 3), SelectionUnit::Cell),
            "ab\nhello wonderful\nx y z"
        );
        // A selection that ends inside a line keeps its spaces.
        assert_eq!(
            selected(&mut terminal, (0, 3), (1, 3), SelectionUnit::Cell),
            "x "
        );
        // Either way round.
        assert_eq!(
            selected(&mut terminal, (4, 3), (2, 3), SelectionUnit::Cell),
            "y z"
        );
    }

    #[test]
    fn a_rectangle_takes_its_columns_of_each_row() {
        let mut terminal = fed(10, 4, "abcdef\r\ngh\r\nmnopqr");
        terminal.select(
            (3, 2),
            (1, 0),
            SelectionShape::Rectangle,
            SelectionUnit::Word,
            WORD_SEPARATORS,
        );
        assert_eq!(terminal.selection_text().unwrap(), "bcd\nh\nnop");
        let selected = terminal.selection().unwrap();
        assert_eq!((selected.start, selected.end), ((1, 0), (3, 2)));
        assert!(selected.contains(2, 1) && !selected.contains(4, 1) && !selected.contains(0, 0));
    }

    #[test]
    fn a_selection_across_pages_and_the_scrollback_reads_whole() {
        // Pages of about 300 rows of 200 columns.
        let lines: Vec<String> = (0..1000).map(|n| format!("line {n}")).collect();
        let mut terminal = fed(200, 5, &lines.join("\r\n"));
        assert!(terminal.screen_rows().pages() > 2);
        terminal.select_all();
        assert_eq!(terminal.selection_text().unwrap(), lines.join("\n"));
        // A line of the scrollback, through the view at its top.
        terminal.scroll_viewport(Scroll::Top);
        assert_eq!(
            selected(&mut terminal, (2, 3), (2, 3), SelectionUnit::Line),
            "line 3"
        );
        // It keeps its text as the view comes back down.
        terminal.scroll_viewport(Scroll::Bottom);
        assert_eq!(terminal.selection_text().unwrap(), "line 3");
        let selected = terminal.selection().unwrap();
        assert_eq!(selected.start, (0, 3 - 995));
        assert!(!selected.contains(0, 0));
    }

    #[test]
    fn select_all_stops_at_the_last_row_shown_or_the_cursor_s() {
        let mut terminal = fed(20, 6, "one\r\ntwo\r\n");
        terminal.select_all();
        assert_eq!(terminal.selection_text().as_deref(), Some("one\ntwo\n"));
        // A row shown below the cursor counts.
        feed(&mut terminal, "\x1b[5;1Hfive\x1b[3;1H");
        terminal.select_all();
        assert_eq!(
            terminal.selection_text().as_deref(),
            Some("one\ntwo\n\n\nfive")
        );
        // With the scrollback, from its first row.
        let mut terminal = fed(10, 2, "a\r\nb\r\nc");
        terminal.select_all();
        assert_eq!(terminal.selection_text().as_deref(), Some("a\nb\nc"));
    }

    #[test]
    fn output_over_the_selection_clears_it() {
        let mut terminal = fed(20, 4, "foo bar\r\nbaz");
        assert_eq!(word_at(&mut terminal, (5, 0)), "bar");
        // The same text written again, and text elsewhere, leave it.
        feed(&mut terminal, "\x1b[1;5Hbar\x1b[2;1Hqux");
        assert_eq!(terminal.selection_text().as_deref(), Some("bar"));
        // A style, or other text, over it clears it.
        feed(&mut terminal, "\x1b[1;5H\x1b[1mbar");
        assert_eq!(terminal.selection_text(), None);
        assert_eq!(terminal.selection(), None);
        assert_eq!(terminal.screen_rows().pins(), 0, "the pins went");
        // An erase too.
        assert_eq!(word_at(&mut terminal, (1, 1)), "qux");
        feed(&mut terminal, "\x1b[2;1H\x1b[K");
        assert_eq!(terminal.selection_text(), None);
    }

    #[test]
    fn the_same_styled_text_written_again_keeps_it() {
        // A repaint, as vim's Ctrl-L or `less -R` give one, which takes
        // the styles anew.
        let line = "\x1b[31mfoo \x1b[32mbar\x1b[0m baz";
        let mut terminal = fed(20, 3, line);
        assert_eq!(word_at(&mut terminal, (5, 0)), "bar");
        feed(&mut terminal, &format!("\r\x1b[K{line}"));
        assert_eq!(terminal.selection_text().as_deref(), Some("bar"));
        // Another color is other text.
        feed(&mut terminal, "\r\x1b[K\x1b[31mfoo \x1b[33mbar");
        assert_eq!(terminal.selection_text(), None);
    }

    #[test]
    fn a_colored_row_that_a_region_moves_to_another_page_keeps_it() {
        let mut terminal = fed(80, 10, "\x1b[33m1\x1b[34m2\x1b[35m3\x1b[0m");
        // Lines until the page's end falls between the screen's rows 4
        // and 5, the first page's styles in use above.
        let mut lines = 0;
        while terminal.screen_rows().screen_row_id(4).0 == terminal.screen_rows().screen_row_id(5).0
        {
            feed(&mut terminal, "\r\nline");
            lines += 1;
            assert!(lines < 5000, "no page's end on the screen");
        }
        feed(&mut terminal, "\x1b[6;1H\x1b[2K\x1b[31mred\x1b[0m");
        assert_eq!(word_at(&mut terminal, (0, 5)), "red");
        // The region of rows 2 to 9 scrolls a row: `red` goes up a row,
        // into the other page.
        feed(&mut terminal, "\x1b[2;9r\x1b[9;1H\n");
        assert_eq!(terminal.selection().unwrap().start, (0, 4));
        assert_eq!(terminal.selection_text().as_deref(), Some("red"));
    }

    #[test]
    fn text_written_over_then_scrolled_away_clears_it() {
        let mut terminal = fed(20, 4, "Downloading 10%");
        selected(&mut terminal, (0, 0), (0, 0), SelectionUnit::Line);
        // The last update of a progress line, and output after it, in one
        // feed.
        feed(&mut terminal, "\rDownloading 99%\r\n\r\n\r\n\r\n");
        assert_eq!(terminal.selection_text(), None);
        // Through the snapshot, which copies no row of the scrollback.
        let mut terminal = fed(20, 4, "Downloading 10%");
        let mut snapshot = crate::terminal::Snapshot::default();
        terminal.snapshot(&mut snapshot);
        selected(&mut terminal, (0, 0), (0, 0), SelectionUnit::Line);
        terminal.snapshot(&mut snapshot);
        feed(&mut terminal, "\rDownloading 99%\r\n\r\n\r\n\r\n");
        terminal.snapshot(&mut snapshot);
        assert_eq!(snapshot.selection(), None);
        // The same text, scrolled away, keeps it, as more output does.
        let mut terminal = fed(20, 4, "Downloading 10%\r\nnext");
        selected(&mut terminal, (0, 0), (0, 1), SelectionUnit::Line);
        feed(
            &mut terminal,
            "\x1b[1;1HDownloading 10%\x1b[2;1Hnext\r\n\r\n\r\n\r\n",
        );
        assert_eq!(
            terminal.selection_text().as_deref(),
            Some("Downloading 10%\nnext")
        );
        // The rows in the scrollback were looked at once, and keep no
        // hashes.
        let selection = terminal.selection.as_ref().unwrap();
        assert_eq!((selection.first_hashed, selection.hashes.len()), (2, 0));
        feed(&mut terminal, "\r\nmore\r\nmore");
        assert_eq!(
            terminal.selection_text().as_deref(),
            Some("Downloading 10%\nnext")
        );
    }

    #[test]
    fn a_selection_whose_pins_a_scroll_within_margins_reverses_is_lost() {
        let mut terminal = fed(40, 10, "\x1b[6;1Hsixth row\x1b[7;1Hseventh row");
        terminal.select(
            (30, 5),
            (5, 6),
            SelectionShape::Cells,
            SelectionUnit::Cell,
            WORD_SEPARATORS,
        );
        // Margins of columns 1 to 20, and a scroll of two rows within
        // them: the last cell's pin goes up, the first's stays.
        feed(&mut terminal, "\x1b[?69h\x1b[1;20s\x1b[2S");
        assert_eq!(terminal.selection_text(), None);
        assert_eq!(terminal.selection(), None);
        assert_eq!(terminal.screen_rows().pins(), 0);
    }

    #[test]
    fn scrolling_keeps_the_selection_with_its_text() {
        let mut terminal = fed(20, 3, "one\r\ntwo\r\nthree");
        assert_eq!(word_at(&mut terminal, (1, 1)), "two");
        // Up the screen, then into the scrollback, as output goes on.
        feed(&mut terminal, "\r\nfour\r\nfive\r\nsix");
        assert_eq!(terminal.selection_text().as_deref(), Some("two"));
        assert_eq!(terminal.selection().unwrap().start, (0, -2));
        // A scroll region's rows that move keep it with them; one that
        // drops a row of it clears it.
        let mut terminal = fed(20, 4, "a\r\nb\r\nc\r\nd");
        assert_eq!(word_at(&mut terminal, (0, 2)), "c");
        feed(&mut terminal, "\x1b[2;4r\x1b[4;1H\n");
        assert_eq!(terminal.selection_text().as_deref(), Some("c"));
        assert_eq!(terminal.selection().unwrap().start, (0, 1));
        assert_eq!(
            selected(&mut terminal, (0, 1), (0, 2), SelectionUnit::Cell),
            "c\nd"
        );
        feed(&mut terminal, "\n");
        assert_eq!(terminal.selection_text(), None);
    }

    #[test]
    fn the_other_screen_and_a_cleared_scrollback_clear_it() {
        let mut terminal = fed(20, 3, "one\r\ntwo");
        word_at(&mut terminal, (0, 0));
        feed(&mut terminal, "\x1b[?1049h\x1b[?1049l");
        assert_eq!(terminal.selection(), None);
        word_at(&mut terminal, (0, 0));
        feed(&mut terminal, "\x1b[?1049h");
        assert_eq!(terminal.selection(), None);
        // A selection in the scrollback, whose rows ED 3 drops.
        let mut terminal = fed(20, 2, "one\r\ntwo\r\nthree");
        terminal.scroll_viewport(Scroll::Up(1));
        assert_eq!(word_at(&mut terminal, (0, 0)), "one");
        feed(&mut terminal, "\x1b[3J");
        assert_eq!(terminal.selection_text(), None);
    }

    #[test]
    fn a_resize_keeps_the_text_of_a_selection_and_drops_a_rectangle() {
        let mut terminal = fed(10, 4, "hello wonderful world");
        selected(&mut terminal, (0, 0), (0, 0), SelectionUnit::Line);
        terminal.resize(30, 4);
        assert_eq!(
            terminal.selection_text().as_deref(),
            Some("hello wonderful world")
        );
        terminal.resize(7, 6);
        assert_eq!(
            terminal.selection_text().as_deref(),
            Some("hello wonderful world")
        );
        terminal.select(
            (0, 0),
            (2, 1),
            SelectionShape::Rectangle,
            SelectionUnit::Cell,
            WORD_SEPARATORS,
        );
        terminal.resize(12, 6);
        assert_eq!(terminal.selection(), None);
    }

    #[test]
    fn a_snapshot_sees_output_over_the_selection_in_the_view() {
        let mut terminal = fed(20, 3, "foo\r\nbar");
        let mut snapshot = crate::terminal::Snapshot::default();
        terminal.snapshot(&mut snapshot);
        word_at(&mut terminal, (0, 0));
        terminal.snapshot(&mut snapshot);
        assert!(snapshot.selection().is_some());
        // Unchanged rows are not looked at again; a changed one is.
        terminal.snapshot(&mut snapshot);
        assert!(snapshot.selection().is_some());
        feed(&mut terminal, "\x1b[1;1Hzzz");
        terminal.snapshot(&mut snapshot);
        assert_eq!(snapshot.selection(), None);
        // A row of the screen below a view scrolled up is looked at as the
        // text is taken.
        let mut terminal = fed(20, 2, "a\r\nb\r\nc");
        terminal.snapshot(&mut snapshot);
        word_at(&mut terminal, (0, 1));
        terminal.scroll_viewport(Scroll::Up(1));
        feed(&mut terminal, "\x1b[2;1Hz");
        terminal.snapshot(&mut snapshot);
        assert!(snapshot.selection().is_some());
        assert_eq!(terminal.selection_text(), None);
    }

    #[test]
    fn the_snapshot_shows_the_selection_in_the_view() {
        let mut terminal = fed(20, 3, "one\r\ntwo");
        let mut snapshot = crate::terminal::Snapshot::default();
        terminal.snapshot(&mut snapshot);
        assert_eq!(snapshot.selection(), None);
        selected(&mut terminal, (1, 0), (1, 1), SelectionUnit::Cell);
        terminal.snapshot(&mut snapshot);
        let shown = snapshot.selection().unwrap();
        assert!(shown.contains(1, 0) && shown.contains(5, 0) && shown.contains(0, 1));
        assert!(!shown.contains(0, 0) && !shown.contains(2, 1));
        terminal.clear_selection();
        terminal.snapshot(&mut snapshot);
        assert_eq!(snapshot.selection(), None);
        assert_eq!(terminal.screen_rows().pins(), 0);
    }
}
