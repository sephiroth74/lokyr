//! The selection: cells of the active screen and of its scrollback, from one
//! to another in reading order or as a rectangle, grown to words or to
//! logical lines, and held by two pins, so that it keeps its cells as output
//! scrolls them up. Output that writes over its cells on the screen clears
//! it, though it scrolls them into the scrollback before the next look, as
//! does a switch to the other screen.

use std::hash::{Hash, Hasher};
use std::ops::Range;

use super::Terminal;
use crate::page::{Content, ListRow, PageList, PinId, Position, RowFlag, Style, StyleId, Width};

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

/// A row's text, as a read of rows gives it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RowText {
    /// The row's number.
    pub row: i64,
    /// Its text: a cell without a character a space, a cluster whole, a
    /// wide character once; without the spaces that end it, but where it
    /// wraps into the next.
    pub text: String,
    /// Whether it soft-wraps into the next row, its logical line going on
    /// there.
    pub wrapped: bool,
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

/// A cell that keeps its text: output scrolls it into the scrollback, a
/// resize reflows it, and the pin goes with it, until the scrollback's
/// limit, or a clearing, takes its row away. Its screen's own: the other
/// screen's rows do not have it. [`Terminal::unpin_cell`] lets it go, and a
/// pin let go, or whose row went, tells nothing, though another pin takes
/// its place in the list.
#[derive(Debug)]
pub struct CellPin {
    pin: PinId,
    alternate: bool,
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
pub(super) fn row_at(rows: &PageList, index: usize) -> Option<ListRow<'_>> {
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

/// The first and the last row of the logical line that the row at `index`
/// is in, as far as `reach` rows before it and after it: each way in one
/// pass, so that a line of many rows costs no more than `reach` rows.
fn line_near(rows: &PageList, index: usize, reach: usize) -> (usize, usize) {
    let goes_on = |row: ListRow<'_>, next: ListRow<'_>| {
        row.metadata().has(RowFlag::SoftWrapped) && next.metadata().has(RowFlag::Continuation)
    };
    let Some(at) = rows.rows_from(index).next() else {
        return (index, index);
    };
    let mut first = index;
    let mut after = at;
    for before in rows.rows_before(index).take(reach) {
        if !goes_on(before, after) {
            break;
        }
        first -= 1;
        after = before;
    }
    let mut last = index;
    let mut row = at;
    for next in rows.rows_from(index).skip(1).take(reach) {
        if !goes_on(row, next) {
            break;
        }
        last += 1;
        row = next;
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
    /// Selects from the cell `from`, its column and its row's number, of
    /// the screen or of its scrollback, to the cell `to`, either before the
    /// other, in `shape`, grown to `unit`, with `separators` ending the
    /// words besides spaces, as [`WORD_SEPARATORS`] does by default. A
    /// rectangle takes no unit. A row past the list's ends takes the
    /// nearest end's. The selection that was there goes.
    pub fn select(
        &mut self,
        from: (u16, i64),
        to: (u16, i64),
        shape: SelectionShape,
        unit: SelectionUnit,
        separators: &str,
    ) {
        self.clear_selection();
        let cols = self.screen().rows.cols();
        let at = |(col, row): (u16, i64)| (self.nearest_index(row), col.min(cols - 1));
        let (a, b) = (at(from), at(to));
        let rows = &self.screen().rows;
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

    /// The text of the logical line that the row numbered `row` is in, of
    /// the screen or of its scrollback, as far as `reach` rows before it
    /// and after it: its rows that soft-wrap joined, with the cell of each
    /// of its characters, by its column and its row's number: a wide
    /// character's first cell, each of a cluster's characters its cell's,
    /// and a cell without a character a space. `None` for a row that the
    /// list does not hold.
    pub fn line_text(&self, row: i64, reach: usize) -> Option<(String, Vec<(u16, i64)>)> {
        let index = self.row_index(row)?;
        let rows = &self.screen().rows;
        let (first, last) = line_near(rows, index, reach);
        let (mut text, mut cells) = (String::new(), Vec::new());
        for (at, list_row) in (first..=last).zip(rows.rows_from(first)) {
            let number = self.row_number(at);
            let end = list_row.cols() - 1;
            for x in 0..list_row.cols() {
                let cell = list_row.cell(x);
                match cell.width() {
                    Width::SpacerTail => continue,
                    Width::SpacerHead if x == end => continue,
                    _ => {}
                }
                match cell.content() {
                    Content::Char('\0') | Content::BgPalette(_) | Content::BgRgb(_) => {
                        text.push(' ');
                        cells.push((x, number));
                    }
                    Content::Char(character) => {
                        text.push(character);
                        cells.push((x, number));
                    }
                    Content::Grapheme(character) => {
                        for &character in std::iter::once(&character).chain(list_row.grapheme(x)) {
                            text.push(character);
                            cells.push((x, number));
                        }
                    }
                }
            }
        }
        Some((text, cells))
    }

    /// The target of the OSC 8 link that the cell at `column` of the row
    /// numbered `row` is part of, if it is one's.
    pub fn link_at(&self, column: u16, row: i64) -> Option<String> {
        let list_row = row_at(&self.screen().rows, self.row_index(row)?)?;
        list_row
            .link(column)
            .map(|uri| String::from_utf8_lossy(uri).into_owned())
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

    /// Anchors the selection that the mouse makes at the cell `at`, its
    /// column and its row's number: a pin, which output moves with its
    /// text, as it scrolls the text into the scrollback, and a resize as it
    /// reflows it. A row past the list's ends takes the nearest end's. The
    /// anchor that was there goes.
    pub fn anchor_selection(&mut self, at: (u16, i64)) {
        self.drop_selection_anchor();
        self.selection_anchor = self.pin_nearest(at);
    }

    /// The cell that the selection by the mouse grows from, by its column
    /// and its row's number, while its text is in the list and its screen
    /// is the active one.
    pub fn selection_anchor(&self) -> Option<(u16, i64)> {
        self.pinned_cell(self.selection_anchor.as_ref()?)
    }

    /// Lets the selection's anchor go, if there is one.
    fn drop_selection_anchor(&mut self) {
        if let Some(anchor) = self.selection_anchor.take() {
            self.unpin_cell(&anchor);
        }
    }

    /// Marks the cell `at`, its column and its row's number, as the one
    /// that a menu opened on, whose commands run later: a pin, as the
    /// selection's anchor is, which output moves with its text. A row past
    /// the list's ends takes the nearest end's. The mark that was there
    /// goes.
    pub fn mark_cell(&mut self, at: (u16, i64)) {
        self.unmark_cell();
        self.cell_mark = self.pin_nearest(at);
    }

    /// The cell that [`Terminal::mark_cell`] marked, by its column and its
    /// row's number, while its text is in the list and its screen is the
    /// active one.
    pub fn marked_cell(&self) -> Option<(u16, i64)> {
        self.pinned_cell(self.cell_mark.as_ref()?)
    }

    /// Lets the cell's mark go, if there is one.
    pub fn unmark_cell(&mut self) {
        if let Some(mark) = self.cell_mark.take() {
            self.unpin_cell(&mark);
        }
    }

    /// A pin of the active screen's cell `at`, its column and its row's
    /// number, which keeps its text as [`CellPin`] tells; `None` for a row
    /// that the list does not hold, or a column past the rows' last.
    pub fn pin_cell(&mut self, at: (u16, i64)) -> Option<CellPin> {
        let index = self.row_index(at.1)?;
        if at.0 >= self.screen().rows.cols() {
            return None;
        }
        self.pin_index(index, at.0)
    }

    /// A pin of the active screen's cell `at`, of the row past the list's
    /// ends at the nearest end's, and of the last column past it.
    fn pin_nearest(&mut self, at: (u16, i64)) -> Option<CellPin> {
        let index = self.nearest_index(at.1);
        let col = at.0.min(self.screen().rows.cols() - 1);
        self.pin_index(index, col)
    }

    /// A pin of the cell at `col` of the active screen's row at `index` of
    /// its list; none where the list has no pin left.
    fn pin_index(&mut self, index: usize, col: u16) -> Option<CellPin> {
        let alternate = self.alternate_active;
        let rows = &mut self.screen_mut().rows;
        let (page, row) = rows.row_at(index)?;
        let pin = rows.pin(Position { page, row, col })?;
        Some(CellPin { pin, alternate })
    }

    /// `pin`, a pin of the active screen's list, as a search of
    /// [`Terminal::screen_rows_mut`] makes one, as a cell pin of that
    /// screen, which [`Terminal::pinned_cell`] tells and
    /// [`Terminal::unpin_cell`] lets go.
    pub fn cell_pin(&self, pin: PinId) -> CellPin {
        CellPin {
            pin,
            alternate: self.alternate_active,
        }
    }

    /// Where `pin`'s cell is, by its column and its row's number, while its
    /// text is in the list and its screen is the active one.
    pub fn pinned_cell(&self, pin: &CellPin) -> Option<(u16, i64)> {
        if pin.alternate != self.alternate_active {
            return None;
        }
        let rows = &self.screen().rows;
        let position = rows.pin_position(pin.pin)?;
        let index = rows.row_index(position.page, position.row)?;
        Some((position.col, self.row_number(index)))
    }

    /// Where `pin`'s cell is in the active screen's list, if it is of the
    /// active screen and still there: its page, its row in the page and its
    /// column.
    pub(super) fn pin_place(&self, pin: &CellPin) -> Option<Position> {
        if pin.alternate != self.alternate_active {
            return None;
        }
        self.screen().rows.pin_position(pin.pin)
    }

    /// Whether `pin`'s cell left its screen's list, or the pin was let go.
    pub(super) fn is_lost(&self, pin: &CellPin) -> bool {
        let rows = if pin.alternate {
            &self.alternate.rows
        } else {
            &self.primary.rows
        };
        rows.pin_position(pin.pin).is_none()
    }

    /// The texts of the rows of the active screen and of its scrollback
    /// numbered from `first`, `count` at most, those that the list holds,
    /// from its first if `first` left it, read in one pass, oldest first.
    pub fn read_rows(&self, first: i64, count: usize) -> Vec<RowText> {
        let Some(index) = self.row_index(first.max(self.row_number(0))) else {
            return Vec::new();
        };
        let rows = &self.screen().rows;
        let count = count.min(rows.total_rows() - index);
        let mut read = Vec::with_capacity(count);
        let mut list = rows.rows_from(index).peekable();
        for at in index..index + count {
            let Some(row) = list.next() else {
                break;
            };
            let wrapped = row.metadata().has(RowFlag::SoftWrapped)
                && list
                    .peek()
                    .is_some_and(|next| next.metadata().has(RowFlag::Continuation));
            let mut text = String::new();
            push_cells(&mut text, row, 0..row.cols());
            if !wrapped {
                text.truncate(text.trim_end_matches(' ').len());
            }
            read.push(RowText {
                row: self.row_number(at),
                text,
                wrapped,
            });
        }
        read
    }

    /// The styles of the row numbered `row`, as runs: each the columns
    /// from the first to the last of one style, with the style; `None` for
    /// a row that the list does not hold.
    pub fn row_styles(&self, row: i64) -> Option<Vec<(u16, u16, Style)>> {
        let list_row = row_at(&self.screen().rows, self.row_index(row)?)?;
        let (page, _) = list_row.parts();
        let mut runs: Vec<(u16, u16, StyleId)> = Vec::new();
        for x in 0..list_row.cols() {
            let style = list_row.cell(x).style();
            match runs.last_mut() {
                Some((_, last, id)) if *id == style => *last = x,
                _ => runs.push((x, x, style)),
            }
        }
        Some(
            runs.into_iter()
                .map(|(first, last, id)| (first, last, *page.styles().get(id)))
                .collect(),
        )
    }

    /// Lets `pin` go, from its own screen's list, which then tells nothing
    /// of it; a pin whose row went needs nothing more.
    pub fn unpin_cell(&mut self, pin: &CellPin) {
        let rows = if pin.alternate {
            &mut self.alternate.rows
        } else {
            &mut self.primary.rows
        };
        rows.unpin(pin.pin);
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
    use crate::page::Scrollback;
    use crate::parser::Parser;
    use crate::terminal::{Options, Scroll, Snapshot};

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
        from: (u16, i64),
        to: (u16, i64),
        unit: SelectionUnit,
    ) -> String {
        terminal.select(from, to, SelectionShape::Cells, unit, WORD_SEPARATORS);
        terminal.selection_text().expect("a selection")
    }

    fn word_at(terminal: &mut Terminal, at: (u16, i64)) -> String {
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
    fn a_selection_s_rows_can_be_above_the_view_and_past_the_list() {
        // Rows 0 to 7, of which the screen shows the last three.
        let mut terminal = fed(10, 3, "0\r\n1\r\n2\r\n3\r\n4\r\n5\r\n6\r\n7");
        // From two rows above the view, in the scrollback, to its last.
        assert_eq!(terminal.origin().top, 5);
        assert_eq!(
            selected(&mut terminal, (0, 3), (0, 7), SelectionUnit::Line),
            "3\n4\n5\n6\n7"
        );
        // Past the list's first row and its last, its ends.
        assert_eq!(
            selected(&mut terminal, (0, -100), (0, 50), SelectionUnit::Line)
                .lines()
                .count(),
            8
        );
    }

    #[test]
    fn a_line_s_text_joins_its_wraps_with_each_character_s_cell() {
        // `ab日cd` and an `e` with its accent, wrapped in rows of 4.
        let terminal = fed(4, 4, "ab日cde\u{301}\r\nnext");
        let (text, cells) = terminal.line_text(1, 9).unwrap();
        // The second row's last cell is empty.
        assert_eq!(text, "ab日cde\u{301} ");
        assert_eq!(
            cells,
            [
                (0, 0),
                (1, 0),
                (2, 0),
                (0, 1),
                (1, 1),
                (2, 1),
                (2, 1),
                (3, 1)
            ]
        );
        // As far as no row from it: its own row's.
        let (text, cells) = terminal.line_text(1, 0).unwrap();
        assert_eq!(text, "cde\u{301} ");
        assert_eq!(cells, [(0, 1), (1, 1), (2, 1), (2, 1), (3, 1)]);
        assert_eq!(terminal.line_text(0, 0).unwrap().0, "ab日");
        // The row on its own, and past the list.
        assert_eq!(terminal.line_text(2, 9).unwrap().0, "next");
        assert_eq!(terminal.line_text(4, 9), None);
        // A wide character that did not fit at a row's end leaves nothing
        // there.
        let (text, cells) = fed(4, 4, "abc日").line_text(0, 9).unwrap();
        assert_eq!(text, "abc日  ");
        assert_eq!(cells[2..4], [(2, 0), (0, 1)]);
    }

    #[test]
    fn a_selection_s_anchor_moves_with_its_text() {
        let mut terminal = fed(10, 4, "a\r\nb\r\nc\r\nd");
        assert_eq!(terminal.selection_anchor(), None);
        let pins = terminal.screen().rows.pins();
        terminal.anchor_selection((0, 1));
        assert_eq!(terminal.selection_anchor(), Some((0, 1)));
        // Anchored again, the first one's pin goes.
        terminal.anchor_selection((0, 1));
        assert_eq!(terminal.screen().rows.pins(), pins + 1);
        // Output scrolls `b` above the screen, and a view scrolled up shows
        // it lower: its number stays.
        Parser::new().advance(&mut terminal, b"\r\ne\r\nf");
        assert_eq!(terminal.origin().top, 2);
        assert_eq!(terminal.selection_anchor(), Some((0, 1)));
        terminal.scroll_viewport(Scroll::Up(1));
        assert_eq!(terminal.origin().top, 1);
        assert_eq!(terminal.selection_anchor(), Some((0, 1)));
        terminal.scroll_viewport(Scroll::Bottom);
        // On the other screen, none, whatever that screen pins; back on its
        // own, again.
        Parser::new().advance(&mut terminal, b"\x1b[?1049hx");
        terminal.select_all();
        assert_eq!(terminal.selection_anchor(), None);
        Parser::new().advance(&mut terminal, b"\x1b[?1049l");
        assert_eq!(terminal.selection_anchor(), Some((0, 1)));
        // A new anchor takes its place, a cell past the row's or the
        // list's end at the nearest one.
        terminal.anchor_selection((30, 99));
        assert_eq!(terminal.selection_anchor(), Some((9, 5)));
        terminal.anchor_selection((0, -99));
        assert_eq!(terminal.selection_anchor(), Some((0, 0)));
        // A clear of the scrollback takes the anchor's text away.
        terminal.anchor_selection((0, 1));
        Parser::new().advance(&mut terminal, b"\x1b[3J");
        assert_eq!(terminal.selection_anchor(), None);
    }

    #[test]
    fn a_line_of_the_scrollback_or_below_the_view_reads_by_its_number() {
        // Rows of 4 cells, 2 of them on the screen: `a` and `b` scroll into
        // the scrollback.
        let mut terminal = fed(
            4,
            2,
            "\x1b]8;;https://lokyr.dev\x07a\x1b]8;;\x07\r\nb\r\nc\r\nd",
        );
        assert_eq!(terminal.origin().top, 2);
        assert_eq!(terminal.line_text(0, 9).unwrap().0, "a   ");
        assert_eq!(terminal.line_text(1, 9).unwrap().1[0], (0, 1));
        assert_eq!(terminal.link_at(0, 0).as_deref(), Some("https://lokyr.dev"));
        assert_eq!(terminal.line_text(-1, 9), None);
        assert_eq!(terminal.link_at(0, -1), None);
        // A view scrolled up reads the rows below it too, and their links,
        // by the same numbers; none past the screen.
        feed(
            &mut terminal,
            "\x1b]8;;https://lokyr.dev/d\x07d\x1b]8;;\x07",
        );
        terminal.scroll_viewport(Scroll::Up(1));
        assert_eq!(terminal.origin().top, 1);
        assert_eq!(terminal.line_text(2, 9).unwrap().0, "c   ");
        assert_eq!(terminal.line_text(3, 9).unwrap().0, "dd  ");
        assert_eq!(
            terminal.link_at(1, 3).as_deref(),
            Some("https://lokyr.dev/d")
        );
        assert_eq!(terminal.line_text(4, 9), None);
    }

    /// The text of the logical line of the row numbered `row`, without the
    /// spaces that end it.
    fn line_at(terminal: &Terminal, row: i64) -> Option<String> {
        let (text, _) = terminal.line_text(row, 99)?;
        Some(text.trim_end().to_owned())
    }

    #[test]
    fn a_line_keeps_its_place_through_output_a_reflow_and_the_scrollback_s_limit() {
        // Rows of 10 cells, 3 on the screen, and a scrollback that keeps
        // only the pages of the screen's rows.
        let options = Options {
            scrollback: Scrollback {
                limit: 0,
                ..Scrollback::default()
            },
            ..Options::default()
        };
        let mut terminal = Terminal::new(10, 3, options);
        // `two three ` and `four`, the line of the row numbered 1.
        feed(&mut terminal, "one\r\ntwo three four\r\nfive");
        let pin = terminal.pin_cell((0, 1)).unwrap();
        assert_eq!(line_at(&terminal, 1).as_deref(), Some("two three four"));
        // Output scrolls it into the scrollback, and the view goes up and
        // down: its number stays, and its pin.
        feed(&mut terminal, "\r\nsix\r\nseven\r\neight");
        assert_eq!(terminal.origin().top, 4);
        assert_eq!(line_at(&terminal, 1).as_deref(), Some("two three four"));
        terminal.scroll_viewport(Scroll::Up(2));
        assert_eq!(terminal.origin().top, 2);
        assert_eq!(terminal.pinned_cell(&pin), Some((0, 1)));
        assert_eq!(line_at(&terminal, 1).as_deref(), Some("two three four"));
        terminal.scroll_viewport(Scroll::Bottom);
        // A reflow lays it out in one row, of another epoch, whose number
        // the pin tells; then in three.
        let epoch = terminal.origin().epoch;
        terminal.resize(20, 3);
        assert_ne!(terminal.origin().epoch, epoch);
        let (_, row) = terminal.pinned_cell(&pin).unwrap();
        assert_eq!(line_at(&terminal, row).as_deref(), Some("two three four"));
        assert_eq!(
            terminal.line_text(row, 0).unwrap().0.trim_end(),
            "two three four"
        );
        terminal.resize(5, 3);
        let (col, row) = terminal.pinned_cell(&pin).unwrap();
        assert_eq!(col, 0);
        assert_eq!(line_at(&terminal, row).as_deref(), Some("two three four"));
        assert_eq!(terminal.line_text(row, 0).unwrap().0, "two t");
        // The scrollback's limit takes its page away, and the pin with it;
        // the rows that stay keep their numbers.
        let mut lines = 0;
        while terminal.pinned_cell(&pin).is_some() {
            let (top, shown) = (
                terminal.origin().top,
                line_at(&terminal, terminal.origin().top),
            );
            feed(&mut terminal, &format!("\r\n{lines}"));
            lines += 1;
            assert!(lines < 100_000, "the scrollback keeps every row");
            assert_eq!(line_at(&terminal, top), shown, "after {lines} lines");
        }
        assert_eq!(line_at(&terminal, row), None);
        assert_eq!(terminal.pinned_cell(&pin), None);
        terminal.unpin_cell(&pin);
        // A pin of a row that the list does not hold, or past the rows'
        // last column, is none.
        assert!(terminal.pin_cell((0, row)).is_none());
        assert!(terminal.pin_cell((5, terminal.origin().top)).is_none());
        // A pin let go tells nothing, though another takes its place.
        let top = terminal.origin().top;
        let other = terminal.pin_cell((0, top)).unwrap();
        terminal.unpin_cell(&other);
        assert_eq!(terminal.pinned_cell(&other), None);
        let taken = terminal.pin_cell((1, top)).unwrap();
        assert_eq!(terminal.pinned_cell(&other), None);
        assert_eq!(terminal.pinned_cell(&taken), Some((1, top)));
    }

    #[test]
    fn a_snapshot_keeps_the_origin_of_its_update() {
        let mut terminal = fed(10, 3, "a\r\nb\r\nc");
        let mut snapshot = Snapshot::default();
        assert_eq!(snapshot.origin(), None);
        terminal.snapshot(&mut snapshot);
        assert_eq!(snapshot.origin(), Some(terminal.origin()));
        // Output after it moves the terminal's view, not the frame's: the
        // frame's first row is still the row numbered 0, `a`.
        feed(&mut terminal, "\r\nd");
        let drawn = snapshot.origin().unwrap();
        assert_eq!((drawn.top, terminal.origin().top), (0, 1));
        assert_eq!(line_at(&terminal, drawn.row(0)).as_deref(), Some("a"));
        terminal.snapshot(&mut snapshot);
        assert_eq!(snapshot.origin(), Some(terminal.origin()));
    }

    #[test]
    fn a_line_s_read_takes_the_time_of_its_reach() {
        // A line of 100 000 rows of 10 cells, between two short ones.
        let mut terminal = fed(10, 4, "short\r\n");
        feed(&mut terminal, &"0123456789".repeat(100_000));
        feed(&mut terminal, "\r\nend");
        let middle = 50_000;
        let start = std::time::Instant::now();
        let (text, _) = terminal.line_text(middle, 2).unwrap();
        let long = start.elapsed();
        assert_eq!(text, "0123456789".repeat(5));
        let start = std::time::Instant::now();
        assert_eq!(terminal.line_text(0, 2).unwrap().0.trim_end(), "short");
        let short = start.elapsed();
        // The walk of the whole line took 200 times a short one's.
        assert!(
            long < short * 20 + std::time::Duration::from_millis(2),
            "{long:?} against {short:?}"
        );
        // Its first rows, and its last, as far as the reach.
        let first = terminal.line_text(1, 3).unwrap().1;
        assert_eq!((first[0].1, first.last().unwrap().1), (1, 4));
        let last = terminal.line_text(100_000, 3).unwrap().1;
        assert_eq!((last[0].1, last.last().unwrap().1), (99_997, 100_000));
    }

    #[test]
    fn rows_read_by_their_numbers_with_their_wraps() {
        // `hello wond`, `erful`, then `x`, of rows of 10, 3 on the screen.
        let terminal = fed(10, 3, "one\r\nhello wonderful\r\nx\r\ny");
        assert_eq!(terminal.origin().top, 2);
        let rows = terminal.read_rows(0, 10);
        let read: Vec<(i64, &str, bool)> = rows
            .iter()
            .map(|row| (row.row, row.text.as_str(), row.wrapped))
            .collect();
        assert_eq!(
            read,
            [
                (0, "one", false),
                (1, "hello wond", true),
                (2, "erful", false),
                (3, "x", false),
                (4, "y", false)
            ]
        );
        assert_eq!(terminal.read_rows(3, 1)[0].text, "x");
        assert!(terminal.read_rows(9, 5).is_empty());
        // From before the first row, from the first.
        assert_eq!(terminal.read_rows(-5, 1)[0].row, 0);
        // A wide character once, and its spacer head left out.
        let terminal = fed(5, 3, "abcd日本");
        let texts: Vec<String> = terminal
            .read_rows(0, 2)
            .into_iter()
            .map(|row| row.text)
            .collect();
        assert_eq!(texts, ["abcd", "日本"]);
    }

    #[test]
    fn a_row_s_styles_come_as_runs() {
        let terminal = fed(10, 2, "ab\x1b[1;31mcd\x1b[0me");
        let runs = terminal.row_styles(0).unwrap();
        let shape: Vec<(u16, u16, bool)> = runs
            .iter()
            .map(|&(first, last, style)| (first, last, style.bold))
            .collect();
        assert_eq!(shape, [(0, 1, false), (2, 3, true), (4, 9, false)]);
        assert_eq!(runs[1].2.fg, crate::page::Color::Palette(1));
        assert!(terminal.row_styles(5).is_none());
    }

    #[test]
    fn a_cell_s_mark_moves_with_its_text() {
        let mut terminal = fed(10, 4, "a\r\nb\r\nc\r\nd");
        let pins = terminal.screen().rows.pins();
        assert_eq!(terminal.marked_cell(), None);
        terminal.mark_cell((1, 1));
        terminal.anchor_selection((0, 2));
        // Marked again, the first one's pin goes; the anchor stays.
        terminal.mark_cell((1, 1));
        assert_eq!(terminal.screen().rows.pins(), pins + 2);
        feed(&mut terminal, "\r\ne\r\nf");
        assert_eq!(terminal.marked_cell(), Some((1, 1)));
        assert_eq!(terminal.selection_anchor(), Some((0, 2)));
        // Not on the other screen.
        feed(&mut terminal, "\x1b[?1049h");
        assert_eq!(terminal.marked_cell(), None);
        feed(&mut terminal, "\x1b[?1049l");
        assert_eq!(terminal.marked_cell(), Some((1, 1)));
        // Unmarked, its pin goes, and the anchor's stays.
        terminal.unmark_cell();
        assert_eq!(terminal.marked_cell(), None);
        assert_eq!(terminal.screen().rows.pins(), pins + 1);
        terminal.unmark_cell();
        // A clear of the scrollback takes its text away, and a reset the
        // mark.
        terminal.mark_cell((0, 1));
        feed(&mut terminal, "\x1b[3J");
        assert_eq!(terminal.marked_cell(), None);
        terminal.mark_cell((0, terminal.origin().top));
        feed(&mut terminal, "\x1bc");
        assert_eq!(terminal.marked_cell(), None);
    }

    #[test]
    fn an_anchor_of_the_other_screen_leaves_its_own_list() {
        let mut terminal = fed(10, 4, "a\r\nb");
        let primary = terminal.primary.rows.pins();
        feed(&mut terminal, "\x1b[?1049hx");
        let alternate = terminal.alternate.rows.pins();
        terminal.anchor_selection((0, 0));
        feed(&mut terminal, "\x1b[?1049l");
        // Anchored again on the primary screen, with a selection there:
        // the alternate screen's pin goes, and the selection stays.
        let (from, to) = ((0, 0), (0, 1));
        terminal.select(
            from,
            to,
            SelectionShape::Cells,
            SelectionUnit::Cell,
            WORD_SEPARATORS,
        );
        let selected = terminal.selection_text();
        assert!(selected.is_some());
        terminal.anchor_selection((0, 1));
        assert_eq!(terminal.selection_text(), selected);
        assert_eq!(terminal.alternate.rows.pins(), alternate);
        assert_eq!(terminal.primary.rows.pins(), primary + 3);
    }

    #[test]
    fn a_selection_s_anchor_is_its_own_screen_s() {
        // The anchor, the primary screen's first pin, and the alternate
        // screen's own first pins, of the same numbers.
        let mut terminal = fed(10, 4, "a\r\nb");
        terminal.anchor_selection((0, 1));
        Parser::new().advance(&mut terminal, b"\x1b[?1049hx\r\ny");
        terminal.select_all();
        assert_eq!(terminal.selection_anchor(), None);
        Parser::new().advance(&mut terminal, b"\x1b[?1049l");
        assert_eq!(terminal.selection_anchor(), Some((0, 1)));
    }

    #[test]
    fn a_cell_tells_its_link_s_target() {
        let terminal = fed(
            20,
            3,
            "go \x1b]8;;https://lokyr.dev\x07here\x1b]8;;\x07 日\x1b]8;;file:///tmp\x07本\x1b]8;;\x07",
        );
        assert_eq!(terminal.link_at(3, 0).as_deref(), Some("https://lokyr.dev"));
        assert_eq!(terminal.link_at(6, 0).as_deref(), Some("https://lokyr.dev"));
        assert_eq!(terminal.link_at(2, 0), None);
        assert_eq!(terminal.link_at(7, 0), None);
        // Both halves of a wide character.
        assert_eq!(terminal.link_at(10, 0).as_deref(), Some("file:///tmp"));
        assert_eq!(terminal.link_at(11, 0).as_deref(), Some("file:///tmp"));
        // Past the row and the list.
        assert_eq!(terminal.link_at(40, 0), None);
        assert_eq!(terminal.link_at(0, 9), None);
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
        let row = terminal.origin().row(5);
        assert_eq!(word_at(&mut terminal, (0, row)), "red");
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
        let row = terminal.origin().row(1);
        word_at(&mut terminal, (0, row));
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
