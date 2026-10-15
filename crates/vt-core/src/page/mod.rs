//! The paged grid: cells, rows, pages, the scrollback, and pins into it.

mod arena;
mod cell;
mod cell_map;
mod grapheme;
mod hyperlink;
mod index;
mod list;
mod pin;
mod row;
mod style;

use std::ops::Range;

use thiserror::Error;

use grapheme::GraphemeTable;

pub use cell::{Cell, Content, Semantic, Width};
pub use hyperlink::{Hyperlink, HyperlinkId, HyperlinkTable};
pub use list::{Budget, PageId, PageList, SCROLLBACK_BUDGET_BYTES, SCROLLBACK_BYTES, Scrollback};
pub use pin::{PinId, Position};
pub use row::{Row, RowFlag};
pub use style::{Color, Rgb, Style, StyleId, StyleTable, Underline};

/// A table of a page has no room left, so the page must grow.
#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum TableFull {
    /// The style table.
    #[error("the style table is full")]
    Styles,
    /// The grapheme table.
    #[error("the grapheme table is full")]
    Graphemes,
    /// The hyperlink table, for the targets of links.
    #[error("the hyperlink table is full")]
    Hyperlinks,
    /// The hyperlink table, for the cells that are part of links.
    #[error("the hyperlink table has no room for another linked cell")]
    LinkedCells,
}

/// The memory of a standard page, from which its capacity follows.
pub const PAGE_BYTES: usize = 512 * 1024;

/// How many styles a standard page has room for, the default one aside.
pub const PAGE_STYLES: u16 = 256;

/// How many cells of a standard page can have a grapheme of several
/// codepoints.
pub const PAGE_GRAPHEMES: u16 = 128;

/// How many link targets a standard page has room for.
pub const PAGE_HYPERLINKS: u16 = 32;

/// How many cells of a standard page can be part of a link.
pub const PAGE_LINKED_CELLS: u16 = 512;

/// How much a page holds: its width, its rows and the capacity of its
/// tables. The page's memory follows from it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Capacity {
    /// The cells of each row.
    pub cols: u16,
    /// The rows.
    pub rows: u16,
    /// The styles, the default one aside.
    pub styles: u16,
    /// The cells with a grapheme of several codepoints.
    pub graphemes: u16,
    /// The targets of links.
    pub hyperlinks: u16,
    /// The cells that are part of links.
    pub linked_cells: u16,
}

impl Capacity {
    /// The capacity of a standard page with `cols` columns: the default
    /// tables, and as many rows as fit in [`PAGE_BYTES`] with them. A row
    /// too wide for the block still gets a page of one row.
    pub fn standard(cols: u16) -> Self {
        let tables = Self {
            cols,
            rows: 0,
            styles: PAGE_STYLES,
            graphemes: PAGE_GRAPHEMES,
            hyperlinks: PAGE_HYPERLINKS,
            linked_cells: PAGE_LINKED_CELLS,
        };
        let row = Self { rows: 1, ..tables }.bytes() - tables.bytes();
        let rows = PAGE_BYTES.saturating_sub(tables.bytes()) / row;
        Self {
            rows: u16::try_from(rows).unwrap_or(u16::MAX).max(1),
            ..tables
        }
    }

    /// The bytes that a page with this capacity takes.
    pub fn bytes(&self) -> usize {
        let rows = usize::from(self.rows);
        size_of::<Page>()
            + rows * size_of::<Row>()
            + rows * usize::from(self.cols) * size_of::<Cell>()
            + StyleTable::memory_for(self.styles)
            + GraphemeTable::memory_for(self.graphemes)
            + HyperlinkTable::memory_for(self.hyperlinks, self.linked_cells)
    }

    /// This capacity, with twice the room in the table that is `full`, or
    /// `None` if that table is as large as it can be.
    #[must_use]
    pub fn grown_for(self, full: TableFull) -> Option<Self> {
        let grown = match full {
            TableFull::Styles => Self {
                styles: self.styles.saturating_mul(2),
                ..self
            },
            TableFull::Graphemes => Self {
                graphemes: self.graphemes.saturating_mul(2),
                ..self
            },
            TableFull::Hyperlinks => Self {
                hyperlinks: self.hyperlinks.saturating_mul(2),
                ..self
            },
            TableFull::LinkedCells => Self {
                linked_cells: self.linked_cells.saturating_mul(2),
                ..self
            },
        };
        (grown != self).then_some(grown)
    }
}

/// What a copy into a page does when one of the page's tables is full.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WhenFull {
    /// It leaves the cells empty and names the table, so that the page can
    /// grow before the copy starts again.
    Fail,
    /// It copies each cell without what the table has no room for: its
    /// style, the codepoints of its cluster after the first that do not
    /// fit, or its link. For a page that cannot grow any more.
    Drop,
}

/// A block of the grid: a fixed number of rows of a fixed width, with the
/// tables that their cells refer to.
///
/// Its parts are allocated once, when the page is created, for its
/// [`Capacity`], and they never grow. A page whose table is full is rebuilt
/// with more capacity, with [`Page::grown`].
#[derive(Clone, Debug)]
pub struct Page {
    capacity: Capacity,
    /// How many rows are in use, from the first.
    len: u16,
    rows: Box<[Row]>,
    /// The cells, one row after the other.
    cells: Box<[Cell]>,
    styles: StyleTable,
    graphemes: GraphemeTable,
    hyperlinks: HyperlinkTable,
}

impl Page {
    /// An empty page with room for `capacity`.
    pub fn new(capacity: Capacity) -> Self {
        let rows = usize::from(capacity.rows);
        Self {
            capacity,
            len: 0,
            rows: vec![Row::default(); rows].into_boxed_slice(),
            cells: vec![Cell::EMPTY; rows * usize::from(capacity.cols)].into_boxed_slice(),
            styles: StyleTable::new(capacity.styles),
            graphemes: GraphemeTable::new(capacity.graphemes),
            hyperlinks: HyperlinkTable::new(capacity.hyperlinks, capacity.linked_cells),
        }
    }

    /// What the page has room for.
    pub fn capacity(&self) -> Capacity {
        self.capacity
    }

    /// How many cells each row has.
    pub fn cols(&self) -> u16 {
        self.capacity.cols
    }

    /// How many rows are in use.
    pub fn len(&self) -> u16 {
        self.len
    }

    /// Whether no row is in use.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Whether every row is in use.
    pub fn is_full(&self) -> bool {
        self.len == self.capacity.rows
    }

    /// Puts the next row to use, an empty one, and returns its index, or
    /// `None` if the page is full.
    pub fn push_row(&mut self) -> Option<u16> {
        if self.is_full() {
            return None;
        }
        self.len += 1;
        Some(self.len - 1)
    }

    /// The metadata of row `y`.
    pub fn row(&self, y: u16) -> Row {
        self.rows[self.row_index(y)]
    }

    /// The metadata of row `y`, to change.
    pub fn row_mut(&mut self, y: u16) -> &mut Row {
        let y = self.row_index(y);
        &mut self.rows[y]
    }

    /// The cells of row `y`.
    pub fn cells(&self, y: u16) -> &[Cell] {
        let range = self.cell_range(y);
        &self.cells[range]
    }

    /// The style table that the page's cells refer to.
    pub fn styles(&self) -> &StyleTable {
        &self.styles
    }

    /// The style table, to acquire and release styles in.
    pub fn styles_mut(&mut self) -> &mut StyleTable {
        &mut self.styles
    }

    /// How many cells have an entry in the grapheme table.
    #[cfg(test)]
    pub(crate) fn grapheme_entries(&self) -> usize {
        self.graphemes.len()
    }

    /// The hyperlink table that the page's cells refer to.
    pub fn hyperlinks(&self) -> &HyperlinkTable {
        &self.hyperlinks
    }

    /// The hyperlink table, to acquire and release links in.
    pub fn hyperlinks_mut(&mut self) -> &mut HyperlinkTable {
        &mut self.hyperlinks
    }

    /// The link that the cell at column `x` of row `y` is part of.
    pub fn hyperlink(&self, y: u16, x: u16) -> Option<HyperlinkId> {
        let index = self.cell_index(y, x);
        if self.cells[index].has_hyperlink() {
            self.hyperlinks.cell(Self::key(index))
        } else {
            None
        }
    }

    /// Makes the cell at column `x` of row `y` part of the link with `link`,
    /// which must be in the page's table, or of no link.
    pub fn set_hyperlink(
        &mut self,
        y: u16,
        x: u16,
        link: Option<HyperlinkId>,
    ) -> Result<(), TableFull> {
        let index = self.cell_index(y, x);
        let cell = self.cells[index];
        match link {
            Some(link) => {
                self.hyperlinks.set_cell(Self::key(index), link)?;
                self.cells[index] = cell.with_hyperlink(true);
                self.row_mut(y).set(RowFlag::Hyperlinks, true);
            }
            None => {
                self.hyperlinks.remove_cell(Self::key(index));
                self.cells[index] = cell.with_hyperlink(false);
            }
        }
        self.row_mut(y).set(RowFlag::Dirty, true);
        Ok(())
    }

    /// The codepoints of the grapheme at column `x` of row `y` after the
    /// first, which the cell holds: none for most cells.
    pub fn grapheme(&self, y: u16, x: u16) -> &[char] {
        let index = self.cell_index(y, x);
        match self.cells[index].content() {
            Content::Grapheme(_) => self.graphemes.get(Self::key(index)),
            _ => &[],
        }
    }

    /// Adds `c` to the grapheme at column `x` of row `y`, as when a
    /// combining mark follows a character. The cell must hold a character.
    pub fn push_codepoint(&mut self, y: u16, x: u16, c: char) -> Result<(), TableFull> {
        let index = self.cell_index(y, x);
        let cell = self.cells[index];
        let first = match cell.content() {
            Content::Char(first) | Content::Grapheme(first) => first,
            content => {
                debug_assert!(false, "{content:?} has no character to add to");
                return Ok(());
            }
        };
        self.graphemes.push(Self::key(index), c)?;
        self.cells[index] = cell.with_content(Content::Grapheme(first));
        let row = self.row_mut(y);
        row.set(RowFlag::Graphemes, true);
        row.set(RowFlag::Dirty, true);
        Ok(())
    }

    /// Replaces the codepoints of the cluster at column `x` of row `y`, a
    /// cell that holds a character, with `first` and `rest`, as
    /// normalization does. The cell keeps its style, width and flags.
    pub fn set_cluster(
        &mut self,
        y: u16,
        x: u16,
        first: char,
        rest: &[char],
    ) -> Result<(), TableFull> {
        let index = self.cell_index(y, x);
        let cell = self.cells[index];
        if let Content::Grapheme(_) = cell.content() {
            self.graphemes.remove(Self::key(index));
        }
        self.cells[index] = cell.with_content(Content::Char(first));
        for &c in rest {
            self.push_codepoint(y, x, c)?;
        }
        self.row_mut(y).set(RowFlag::Dirty, true);
        Ok(())
    }

    /// Puts `cell` at column `x` of row `y`. The cell takes a reference to
    /// its style, which must be in the page's table, and the cell it
    /// replaces drops one, its grapheme and its link. A cell becomes a
    /// grapheme only through [`Page::push_codepoint`], and part of a link
    /// only through [`Page::set_hyperlink`].
    pub fn set_cell(&mut self, y: u16, x: u16, cell: Cell) {
        debug_assert!(
            !matches!(cell.content(), Content::Grapheme(_)),
            "a grapheme is built with push_codepoint"
        );
        debug_assert!(!cell.has_hyperlink(), "a link is set with set_hyperlink");
        let index = self.cell_index(y, x);
        let old = self.cells[index];
        // Retain first, in case the old cell holds the last reference.
        self.styles.retain(cell.style());
        self.release(index, old);
        self.cells[index] = cell;
        let row = self.row_mut(y);
        row.set(RowFlag::Dirty, true);
        if cell.style() != StyleId::DEFAULT {
            row.set(RowFlag::Styles, true);
        }
    }

    /// Puts the characters of `text`, printable ASCII, in the cells from
    /// column `x` of row `y`, each as `template` with the character in it,
    /// as [`Page::set_cell`] would put them one by one. The template's
    /// style must be in the page's table, and the text must fit in the row.
    pub fn write_ascii(&mut self, y: u16, x: u16, text: &[u8], template: Cell) {
        debug_assert!(
            text.iter().all(|byte| (0x20..=0x7E).contains(byte)),
            "the text is printable ASCII"
        );
        let start = self.cell_index(y, x);
        assert!(
            usize::from(x) + text.len() <= usize::from(self.cols()),
            "the text passes the end of row {y}"
        );
        debug_assert!(
            !template.has_hyperlink(),
            "a link is set with set_hyperlink"
        );
        let style = template.style();
        for (index, &byte) in (start..).zip(text) {
            // Retain first, in case the old cell holds the last reference.
            self.styles.retain(style);
            let old = self.cells[index];
            self.release(index, old);
            self.cells[index] = template.with_content(Content::Char(char::from(byte)));
        }
        let row = self.row_mut(y);
        row.set(RowFlag::Dirty, true);
        if style != StyleId::DEFAULT {
            row.set(RowFlag::Styles, true);
        }
    }

    /// Changes the width of the cell at column `x` of row `y`, and nothing
    /// else, as when a variation selector changes a cluster's width.
    pub fn set_width(&mut self, y: u16, x: u16, width: Width) {
        let index = self.cell_index(y, x);
        self.cells[index] = self.cells[index].with_width(width);
        self.row_mut(y).set(RowFlag::Dirty, true);
    }

    /// Puts `blank`, a cell with no character, style or link, in the cells
    /// of `columns` in row `y`, but leaves the protected ones if
    /// `selective`, as the selective erases do. A wide character that the
    /// range would split is erased whole, unless it is kept.
    pub fn erase(&mut self, y: u16, columns: Range<u16>, blank: Cell, selective: bool) {
        debug_assert!(
            blank.style() == StyleId::DEFAULT
                && !blank.has_hyperlink()
                && !matches!(blank.content(), Content::Grapheme(_)),
            "a blank holds nothing in the tables"
        );
        let Range { start, end } = columns;
        if start >= end {
            return;
        }
        let keeps = |cell: Cell| selective && cell.is_protected();
        let cells = self.cells(y);
        let (first, last) = (cells[usize::from(start)], cells[usize::from(end - 1)]);
        if start > 0 && first.width() == Width::SpacerTail && !keeps(first) {
            self.set_cell(y, start - 1, blank);
        }
        if end < self.cols() && last.width() == Width::Wide && !keeps(last) {
            self.set_cell(y, end, blank);
        }
        for x in start..end {
            if !keeps(self.cells(y)[usize::from(x)]) {
                self.set_cell(y, x, blank);
            }
        }
    }

    /// Drops the first `n` rows in use, which drop what they held, and moves
    /// the others up in their place.
    pub fn drop_first_rows(&mut self, n: u16) {
        assert!(n <= self.len, "only rows in use can be dropped");
        for y in 0..n {
            self.clear_row(y);
        }
        for y in n..self.len {
            self.move_row(y, y - n);
        }
        self.len -= n;
    }

    /// Empties the halves of wide characters that writing `width` cells
    /// from column `x` of row `y` would leave without their other half.
    pub fn clear_halves(&mut self, y: u16, x: u16, width: u16) {
        if x > 0 && self.cells(y)[usize::from(x)].width() == Width::SpacerTail {
            self.set_cell(y, x - 1, Cell::EMPTY);
        }
        let last = x + width - 1;
        if last + 1 < self.cols() && self.cells(y)[usize::from(last)].width() == Width::Wide {
            self.set_cell(y, last + 1, Cell::EMPTY);
        }
    }

    /// Inserts `n` empty cells at column `x` of row `y`: the cells from
    /// there to column `end`, excluded, move right, with their graphemes
    /// and links, and the ones that pass `end` drop what they held. A wide
    /// character that the insertion would split, at `x`, at `end` or past
    /// it, is emptied whole.
    pub fn insert_blanks(&mut self, y: u16, x: u16, n: u16, end: u16) {
        self.clear_crossing(y, x..end);
        self.shift_right(y, x, n, end);
        // A wide character whose second half fell past the end.
        if self.cells(y)[usize::from(end - 1)].width() == Width::Wide {
            self.set_cell(y, end - 1, Cell::EMPTY);
        }
    }

    /// Deletes `n` cells at column `x` of row `y`, which drop what they
    /// held: the cells after them, to column `end`, excluded, move left,
    /// with their graphemes and links, and empty cells come in before
    /// `end`. A wide character that the deletion would split is emptied
    /// whole.
    pub fn delete_cells(&mut self, y: u16, x: u16, n: u16, end: u16) {
        let n = n.min(end - x);
        self.clear_crossing(y, x..end);
        if x + n < end && self.cells(y)[usize::from(x + n)].width() == Width::SpacerTail {
            self.set_cell(y, x + n, Cell::EMPTY);
        }
        self.clear_cells(y, x..x + n);
        for column in x + n..end {
            let (from, to) = (self.cell_index(y, column), self.cell_index(y, column - n));
            self.move_cell(from, to);
        }
        self.row_mut(y).set(RowFlag::Dirty, true);
    }

    /// Moves the cells from column `x` of row `y` right by `n`, within
    /// column `end`, as [`Page::insert_blanks`] does, but whatever their
    /// widths.
    fn shift_right(&mut self, y: u16, x: u16, n: u16, end: u16) {
        let n = n.min(end - x);
        self.clear_cells(y, end - n..end);
        for column in (x..end - n).rev() {
            let (from, to) = (self.cell_index(y, column), self.cell_index(y, column + n));
            self.move_cell(from, to);
        }
        self.row_mut(y).set(RowFlag::Dirty, true);
    }

    /// Empties, whole, the wide characters that cross the edges of
    /// `columns` in row `y`, which moving the cells within them would
    /// split.
    pub fn clear_crossing(&mut self, y: u16, columns: Range<u16>) {
        let Range { start, end } = columns;
        if start > 0 && self.cells(y)[usize::from(start)].width() == Width::SpacerTail {
            self.set_cell(y, start - 1, Cell::EMPTY);
            self.set_cell(y, start, Cell::EMPTY);
        }
        if end < self.cols() && self.cells(y)[usize::from(end - 1)].width() == Width::Wide {
            self.set_cell(y, end - 1, Cell::EMPTY);
            self.set_cell(y, end, Cell::EMPTY);
        }
    }

    /// Empties the cells of `columns` in row `y`, which drop their styles,
    /// graphemes and links, whatever their widths. The row is dirty.
    pub fn clear_cells(&mut self, y: u16, columns: Range<u16>) {
        for x in columns {
            let index = self.cell_index(y, x);
            let cell = std::mem::replace(&mut self.cells[index], Cell::EMPTY);
            self.release(index, cell);
        }
        self.row_mut(y).set(RowFlag::Dirty, true);
    }

    /// Empties every cell of row `y`, which drop their styles, their
    /// graphemes and their links, and resets its metadata. The row is dirty.
    pub fn clear_row(&mut self, y: u16) {
        for index in self.cell_range(y) {
            let cell = std::mem::replace(&mut self.cells[index], Cell::EMPTY);
            self.release(index, cell);
        }
        let row = self.row_mut(y);
        *row = Row::default();
        row.set(RowFlag::Dirty, true);
    }

    /// Moves row `from` into row `to`, within the page: the cells move with
    /// their graphemes and links, and the row's metadata goes along. What
    /// `to` held is dropped, and `from` is left empty, as
    /// [`Page::clear_row`] leaves it.
    pub fn move_row(&mut self, from: u16, to: u16) {
        if from == to {
            return;
        }
        self.move_cells(from, to, 0..self.cols());
        let mut metadata = self.row(from);
        metadata.set(RowFlag::Dirty, true);
        *self.row_mut(to) = metadata;
        self.clear_row(from);
    }

    /// Moves the cells of `columns` in row `from` into the same columns of
    /// row `to`, within the page, as they are, with their graphemes and
    /// links. What those cells of `to` held is dropped, and those of `from`
    /// are left empty. The rows' metadata stays, but for the flags that say
    /// what the cells may hold, which `to` takes from `from` too.
    pub fn move_cells(&mut self, from: u16, to: u16, columns: Range<u16>) {
        if from == to {
            return;
        }
        self.clear_cells(to, columns.clone());
        let (from_start, to_start) = (self.cell_range(from).start, self.cell_range(to).start);
        for x in columns {
            let x = usize::from(x);
            self.move_cell(from_start + x, to_start + x);
        }
        let source = self.row(from);
        let target = self.row_mut(to);
        for flag in [
            RowFlag::Graphemes,
            RowFlag::Hyperlinks,
            RowFlag::Images,
            RowFlag::Styles,
        ] {
            if source.has(flag) {
                target.set(flag, true);
            }
        }
        self.row_mut(from).set(RowFlag::Dirty, true);
    }

    /// Copies row `from` of `source`, a page of the same width, into row
    /// `to`: each cell takes its style, grapheme and link in this page's
    /// tables, and the row's metadata goes along. What `to` held is
    /// dropped. If one of the tables is full, `when_full` says what
    /// happens: with [`WhenFull::Fail`], `to` is left empty and the error
    /// tells which table it is.
    pub fn copy_row(
        &mut self,
        to: u16,
        source: &Page,
        from: u16,
        when_full: WhenFull,
    ) -> Result<(), TableFull> {
        assert_eq!(source.cols(), self.cols(), "the pages have one width");
        if let Err(full) = self.copy_cells(to, source, from, 0..self.cols(), when_full) {
            self.clear_row(to);
            return Err(full);
        }
        let mut metadata = source.row(from);
        metadata.set(RowFlag::Dirty, true);
        *self.row_mut(to) = metadata;
        Ok(())
    }

    /// Copies the cells of `columns` in row `from` of `source`, a page of
    /// the same width, into the same columns of row `to`, as they are, each
    /// with its style, grapheme and link in this page's tables. What those
    /// cells of `to` held is dropped. If one of the tables is full,
    /// `when_full` says what happens: with [`WhenFull::Fail`], the cells
    /// are left empty and the error tells which table it is.
    pub fn copy_cells(
        &mut self,
        to: u16,
        source: &Page,
        from: u16,
        columns: Range<u16>,
        when_full: WhenFull,
    ) -> Result<(), TableFull> {
        assert_eq!(source.cols(), self.cols(), "the pages have one width");
        self.clear_cells(to, columns.clone());
        let copied = self.copy_into_empty(to, source, from, columns.clone(), when_full);
        if copied.is_err() {
            self.clear_cells(to, columns);
        }
        copied
    }

    /// The cells of [`Page::copy_cells`], into empty cells.
    fn copy_into_empty(
        &mut self,
        to: u16,
        source: &Page,
        from: u16,
        columns: Range<u16>,
        when_full: WhenFull,
    ) -> Result<(), TableFull> {
        // What a full table does: stop the copy, or let the cell go on
        // without what it has no room for.
        let full = |full| match when_full {
            WhenFull::Fail => Err(full),
            WhenFull::Drop => Ok(()),
        };
        for x in columns {
            let cell = source.cells(from)[usize::from(x)];
            let style = match self.styles.acquire(source.styles().get(cell.style())) {
                Ok(style) => style,
                Err(error) => {
                    full(error)?;
                    StyleId::DEFAULT
                }
            };
            let mut copy = cell.with_style(style).with_hyperlink(false);
            if let Content::Grapheme(first) = cell.content() {
                copy = copy.with_content(Content::Char(first));
            }
            self.set_cell(to, x, copy);
            self.styles.release(style);
            for &c in source.grapheme(from, x) {
                if let Err(error) = self.push_codepoint(to, x, c) {
                    full(error)?;
                    break;
                }
            }
            if let Some(link) = source.hyperlink(from, x) {
                match self.hyperlinks.acquire(source.hyperlinks().get(link)) {
                    Ok(link) => {
                        let linked = self.set_hyperlink(to, x, Some(link));
                        self.hyperlinks.release(link);
                        if let Err(error) = linked {
                            full(error)?;
                        }
                    }
                    Err(error) => full(error)?,
                }
            }
        }
        Ok(())
    }

    /// Empties the page as it was new, with no row in use, without
    /// allocating: what the pool does before handing a page out again.
    pub fn reset(&mut self) {
        let used = usize::from(self.len) * usize::from(self.cols());
        self.cells[..used].fill(Cell::EMPTY);
        self.rows[..usize::from(self.len)].fill(Row::default());
        self.styles.clear();
        self.graphemes.clear();
        self.hyperlinks.clear();
        self.len = 0;
    }

    /// A copy of the page with `capacity`, which must have its width and at
    /// least its rows in use. Styles keep their ids, so cells are copied as
    /// they are.
    pub fn grown(&self, capacity: Capacity) -> Self {
        assert_eq!(capacity.cols, self.capacity.cols, "a page keeps its width");
        assert!(capacity.rows >= self.len, "the rows in use must fit");
        let mut page = Self::new(capacity);
        let rows = usize::from(self.len);
        page.len = self.len;
        page.rows[..rows].copy_from_slice(&self.rows[..rows]);
        let cells = rows * usize::from(self.cols());
        page.cells[..cells].copy_from_slice(&self.cells[..cells]);
        page.styles = self.styles.grown(capacity.styles);
        page.graphemes = self.graphemes.grown(capacity.graphemes);
        page.hyperlinks = self
            .hyperlinks
            .grown(capacity.hyperlinks, capacity.linked_cells);
        page.capacity.styles = page.styles.capacity() as u16;
        page.capacity.graphemes = page.graphemes.capacity();
        page.capacity.hyperlinks = page.hyperlinks.capacity();
        page.capacity.linked_cells = page.hyperlinks.cell_capacity();
        page
    }

    /// The bytes that the page takes, header included.
    pub fn memory(&self) -> usize {
        size_of::<Self>()
            + size_of_val(&*self.rows)
            + size_of_val(&*self.cells)
            + self.styles.memory()
            + self.graphemes.memory()
            + self.hyperlinks.memory()
    }

    /// Moves the cell at `from` to `to`, an empty cell, with its entries in
    /// the tables keyed by cell; `from` is left empty.
    fn move_cell(&mut self, from: usize, to: usize) {
        let cell = std::mem::replace(&mut self.cells[from], Cell::EMPTY);
        debug_assert_eq!(
            self.cells[to],
            Cell::EMPTY,
            "a cell moves into an empty one"
        );
        self.cells[to] = cell;
        if let Content::Grapheme(_) = cell.content() {
            self.graphemes.move_cell(Self::key(from), Self::key(to));
        }
        if cell.has_hyperlink() {
            self.hyperlinks.move_cell(Self::key(from), Self::key(to));
        }
    }

    /// Drops what `cell`, which was at `index`, held in the tables.
    ///
    /// Printing calls it for every cell it writes, and the compiler would
    /// otherwise leave it out of line: the call costs more than the rest.
    #[inline(always)]
    fn release(&mut self, index: usize, cell: Cell) {
        self.styles.release(cell.style());
        let grapheme = matches!(cell.content(), Content::Grapheme(_));
        // Most cells hold neither, so the work on the tables stays out of
        // line.
        if grapheme || cell.has_hyperlink() {
            self.release_entries(index, grapheme, cell.has_hyperlink());
        }
    }

    /// Drops the grapheme and the link of the cell at `index`, as asked.
    fn release_entries(&mut self, index: usize, grapheme: bool, hyperlink: bool) {
        if grapheme {
            self.graphemes.remove(Self::key(index));
        }
        if hyperlink {
            self.hyperlinks.remove_cell(Self::key(index));
        }
    }

    /// The key of the cell at `index` in the tables keyed by cell.
    fn key(index: usize) -> u32 {
        // A page holds fewer than 2^32 cells: its rows and columns are u16.
        index as u32
    }

    fn row_index(&self, y: u16) -> usize {
        assert!(y < self.len, "row {y} is not in use");
        usize::from(y)
    }

    fn cell_range(&self, y: u16) -> std::ops::Range<usize> {
        let start = self.row_index(y) * usize::from(self.cols());
        start..start + usize::from(self.cols())
    }

    fn cell_index(&self, y: u16, x: u16) -> usize {
        assert!(x < self.cols(), "column {x} is past the row");
        self.cell_range(y).start + usize::from(x)
    }
}

#[cfg(test)]
mod tests;
