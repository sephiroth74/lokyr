//! The paged grid: cells, rows, pages, the scrollback, and pins into it.

mod cell;
mod row;
mod style;

pub use cell::{Cell, Content, Semantic, Width};
pub use row::{Row, RowFlag};
pub use style::{Color, Rgb, Style, StyleId, StyleTable, TableFull, Underline};

/// The memory of a standard page, from which its capacity follows.
pub const PAGE_BYTES: usize = 512 * 1024;

/// How many styles a standard page has room for, the default one aside.
pub const PAGE_STYLES: u16 = 256;

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
    }
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

    /// Puts `cell` at column `x` of row `y`. The cell takes a reference to
    /// its style, which must be in the page's table, and the cell it
    /// replaces drops one.
    pub fn set_cell(&mut self, y: u16, x: u16, cell: Cell) {
        let index = self.cell_index(y, x);
        let old = self.cells[index];
        // Retain first, in case the old cell holds the last reference.
        self.styles.retain(cell.style());
        self.styles.release(old.style());
        self.cells[index] = cell;
        let row = self.row_mut(y);
        row.set(RowFlag::Dirty, true);
        if cell.style() != StyleId::DEFAULT {
            row.set(RowFlag::Styles, true);
        }
    }

    /// Empties every cell of row `y`, which drop their styles, and resets
    /// its metadata. The row is dirty.
    pub fn clear_row(&mut self, y: u16) {
        let range = self.cell_range(y);
        for cell in &mut self.cells[range] {
            self.styles.release(cell.style());
            *cell = Cell::EMPTY;
        }
        let row = self.row_mut(y);
        *row = Row::default();
        row.set(RowFlag::Dirty, true);
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
        page.capacity.styles = page.styles.capacity() as u16;
        page
    }

    /// The bytes that the page takes, header included.
    pub fn memory(&self) -> usize {
        size_of::<Self>()
            + size_of_val(&*self.rows)
            + size_of_val(&*self.cells)
            + self.styles.memory()
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
