//! The paged grid: cells, rows, pages, the scrollback, and pins into it.

mod arena;
mod cell;
mod cell_map;
mod compact;
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
pub use list::{
    Budget, Grow, ListRow, PageId, PageList, SCROLLBACK_BUDGET_BYTES, SCROLLBACK_BYTES, Scrollback,
};
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
            // Each row's metadata and slot, and its block's extent.
            + rows * (size_of::<Row>() + 2 * size_of::<u16>())
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
    /// The block of cells that each row uses. Rows move within the page by
    /// trading blocks, so that their cells stay where they are, with their
    /// entries in the tables keyed by cell.
    slots: Box<[u16]>,
    /// The cells, a block of a row's width after the other.
    cells: Box<[Cell]>,
    /// For each block of cells, a column from which its cells are all
    /// empty, so that emptying the block fills the cells before it only.
    extents: Box<[u16]>,
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
            slots: (0..capacity.rows).collect(),
            cells: vec![Cell::EMPTY; rows * usize::from(capacity.cols)].into_boxed_slice(),
            extents: vec![0; rows].into_boxed_slice(),
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
        let y = self.len - 1;
        // A reset leaves the cells as they were, and the row empties its
        // own as it comes into use, while they are about to be written.
        self.empty_block(y);
        Some(y)
    }

    /// Stops using the last row in use, which drops what it held.
    pub fn pop_row(&mut self) {
        assert!(self.len > 0, "only a row in use can be dropped");
        self.clear_row(self.len - 1);
        self.len -= 1;
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
                self.extend(y, x + 1);
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

    /// The codepoints after the first of the grapheme of the cell at column
    /// `x` of row `y`, which holds one, read from the table alone, as on a
    /// cold page, which has no cells.
    pub(crate) fn grapheme_of(&self, y: u16, x: u16) -> &[char] {
        self.graphemes.get(Self::key(self.cell_index(y, x)))
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
        if cell != Cell::EMPTY {
            self.extend(y, x + 1);
        }
        let row = self.row_mut(y);
        row.set(RowFlag::Dirty, true);
        if cell.style() != StyleId::DEFAULT {
            row.set(RowFlag::Styles, true);
        }
    }

    /// Puts `cells` in the cells from column `x` of row `y`, as
    /// [`Page::set_cell`] would put them one by one, but finding the row's
    /// block once: a character and the spacer of its second half.
    pub fn set_cells(&mut self, y: u16, x: u16, cells: &[Cell]) {
        let block = self.block(y);
        assert!(
            usize::from(x) + cells.len() <= usize::from(self.cols()),
            "the cells pass the end of row {y}"
        );
        let start = block * usize::from(self.cols()) + usize::from(x);
        let mut styled = false;
        let mut filled = None;
        for (offset, &cell) in cells.iter().enumerate() {
            debug_assert!(
                !matches!(cell.content(), Content::Grapheme(_)),
                "a grapheme is built with push_codepoint"
            );
            debug_assert!(!cell.has_hyperlink(), "a link is set with set_hyperlink");
            let index = start + offset;
            let old = self.cells[index];
            // Retain first, in case the old cell holds the last reference.
            self.styles.retain(cell.style());
            self.release(index, old);
            self.cells[index] = cell;
            styled |= cell.style() != StyleId::DEFAULT;
            if cell != Cell::EMPTY {
                filled = Some(offset);
            }
        }
        if let Some(offset) = filled {
            // The cells fit in the row, whose width is a u16.
            let end = x + offset as u16 + 1;
            self.extents[block] = self.extents[block].max(end);
        }
        let row = self.row_mut(y);
        row.set(RowFlag::Dirty, true);
        if styled {
            row.set(RowFlag::Styles, true);
        }
    }

    /// Puts the characters of `chars`, each with its width, 1 or 2, in the
    /// cells from column `x` of row `y`, one after the other: each as
    /// `template` with the character in it, and a wide one followed by
    /// `template` as its spacer, as [`Page::set_cells`] would put them. The
    /// template's style must be in the page's table, and the cells must fit
    /// in the row.
    pub fn write_chars(&mut self, y: u16, x: u16, chars: &[(char, u8)], template: Cell) {
        debug_assert!(
            !template.has_hyperlink(),
            "a link is set with set_hyperlink"
        );
        let block = self.block(y);
        let cells: usize = chars.iter().map(|&(_, width)| usize::from(width)).sum();
        assert!(
            usize::from(x) + cells <= usize::from(self.cols()),
            "the characters pass the end of row {y}"
        );
        let start = block * usize::from(self.cols()) + usize::from(x);
        let range = start..start + cells;
        let style = template.style();
        // Retain first, in case the old cells hold the last reference. The
        // row has fewer cells than a u32 counts.
        self.styles.retain_many(style, cells as u32);
        let metadata = self.row(y);
        // Old cells that hold nothing in the tables have nothing to drop.
        if metadata.has(RowFlag::Styles)
            || metadata.has(RowFlag::Graphemes)
            || metadata.has(RowFlag::Hyperlinks)
        {
            for index in range.clone() {
                let old = self.cells[index];
                self.release(index, old);
            }
        }
        let mut index = start;
        for &(c, width) in chars {
            debug_assert!(matches!(width, 1 | 2), "{c:?} is {width} cells wide");
            let cell = template.with_content(Content::Char(c));
            if width == 2 {
                self.cells[index] = cell.with_width(Width::Wide);
                self.cells[index + 1] = template.with_width(Width::SpacerTail);
            } else {
                self.cells[index] = cell;
            }
            index += usize::from(width);
        }
        // The cells fit in the row, whose width is a u16.
        let end = x + cells as u16;
        self.extents[block] = self.extents[block].max(end);
        let row = self.row_mut(y);
        row.set(RowFlag::Dirty, true);
        if style != StyleId::DEFAULT {
            row.set(RowFlag::Styles, true);
        }
    }

    /// Puts the characters of `text`, printable ASCII, in the cells from
    /// column `x` of row `y`, each as `template` with the character in it,
    /// as [`Page::set_cell`] would put them one by one. The template's
    /// style must be in the page's table, and the text must fit in the row.
    pub fn write_ascii(&mut self, y: u16, x: u16, text: &[u8], template: Cell) {
        let block = self.block(y);
        assert!(
            usize::from(x) + text.len() <= usize::from(self.cols()),
            "the text passes the end of row {y}"
        );
        self.write_ascii_in(y, block, x, text, template);
    }

    /// Puts `text` in the cells from column `x` of row `y` as
    /// [`Page::write_ascii`] does, after emptying the halves of wide
    /// characters that it would leave, as [`Page::clear_halves`] does, with
    /// the row's block found once. With `wrap_start`, the column where the
    /// line's wraps land, it also tells whether the text went over a wide
    /// character there, or over its second half, for which the row above
    /// may have left a spacer head: never when `x` is past them.
    #[inline(always)]
    pub fn overwrite_ascii(
        &mut self,
        y: u16,
        x: u16,
        text: &[u8],
        template: Cell,
        wrap_start: Option<u16>,
    ) -> bool {
        let block = self.block(y);
        let cols = self.cols();
        assert!(
            usize::from(x) + text.len() <= usize::from(cols),
            "the text passes the end of row {y}"
        );
        // The text fits in the row, whose width is a u16.
        let last = x + text.len().max(1) as u16 - 1;
        let base = block * usize::from(cols);
        let row = &self.cells[base..base + usize::from(cols)];
        let over_wide = wrap_start
            .filter(|&start| x <= start + 1 && last >= start)
            .is_some_and(|start| {
                row[usize::from(x.max(start))..=usize::from(last.min(start + 1))]
                    .iter()
                    .any(|cell| matches!(cell.width(), Width::Wide | Width::SpacerTail))
            });
        let first_half = x > 0 && row[usize::from(x)].width() == Width::SpacerTail;
        let second_half = last + 1 < cols && row[usize::from(last)].width() == Width::Wide;
        if first_half {
            self.set_cell(y, x - 1, Cell::EMPTY);
        }
        if second_half {
            self.set_cell(y, last + 1, Cell::EMPTY);
        }
        self.write_ascii_in(y, block, x, text, template);
        over_wide
    }

    /// The cells of [`Page::write_ascii`], for row `y`, whose block is
    /// `block`.
    #[inline(always)]
    fn write_ascii_in(&mut self, y: u16, block: usize, x: u16, text: &[u8], template: Cell) {
        debug_assert!(
            text.iter().all(|byte| (0x20..=0x7E).contains(byte)),
            "the text is printable ASCII"
        );
        debug_assert_eq!(block, self.block(y), "the block of row {y}");
        debug_assert!(
            !template.has_hyperlink(),
            "a link is set with set_hyperlink"
        );
        let cols = usize::from(self.cols());
        let start = block * cols + usize::from(x);
        let range = start..start + text.len();
        debug_assert!(
            usize::from(x) + text.len() <= cols,
            "the text passes the end of row {y}"
        );
        // The row whose block `block` is, is in use.
        let metadata = self.rows[usize::from(y)];
        let style = template.style();
        // Retain first, in case the old cells hold the last reference. The
        // row has fewer cells than a u32 counts.
        self.styles.retain_many(style, text.len() as u32);
        // Old cells that hold nothing in the tables have nothing to drop.
        if metadata.has(RowFlag::Styles)
            || metadata.has(RowFlag::Graphemes)
            || metadata.has(RowFlag::Hyperlinks)
        {
            self.release_cells(range.clone());
        }
        let cells = &mut self.cells[range];
        for (cell, &byte) in cells.iter_mut().zip(text) {
            *cell = template.with_content(Content::Char(char::from(byte)));
        }
        // The text fits in the row, whose width is a u16.
        let end = x + text.len() as u16;
        self.extents[block] = self.extents[block].max(end);
        let row = &mut self.rows[usize::from(y)];
        row.set(RowFlag::Dirty, true);
        if style != StyleId::DEFAULT {
            row.set(RowFlag::Styles, true);
        }
    }

    /// Drops what the cells at `range` hold in the tables, which they keep.
    #[inline(never)]
    fn release_cells(&mut self, range: std::ops::Range<usize>) {
        for index in range {
            let old = self.cells[index];
            self.release(index, old);
        }
    }

    /// Changes the width of the cell at column `x` of row `y`, and nothing
    /// else, as when a variation selector changes a cluster's width.
    pub fn set_width(&mut self, y: u16, x: u16, width: Width) {
        let index = self.cell_index(y, x);
        self.cells[index] = self.cells[index].with_width(width);
        if self.cells[index] != Cell::EMPTY {
            self.extend(y, x + 1);
        }
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
        let block = self.block(y);
        let base = block * usize::from(self.cols());
        let extent = self.extents[block];
        // The cells past the extent are empty, and an empty blank leaves
        // them so; each was written all the same.
        let stop = if blank == Cell::EMPTY {
            end.min(extent).max(start)
        } else {
            end
        };
        let mut written = stop < end;
        for index in base + usize::from(start)..base + usize::from(stop) {
            let cell = self.cells[index];
            if !keeps(cell) {
                self.release(index, cell);
                self.cells[index] = blank;
                written = true;
            }
        }
        if blank != Cell::EMPTY {
            self.extend(y, end);
        } else if !selective && end >= extent {
            self.extents[block] = extent.min(start);
        }
        if written {
            self.row_mut(y).set(RowFlag::Dirty, true);
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
        let last = x + width - 1;
        let cells = self.cells(y);
        let (first, after) = (
            x > 0 && cells[usize::from(x)].width() == Width::SpacerTail,
            last + 1 < self.cols() && cells[usize::from(last)].width() == Width::Wide,
        );
        if first {
            self.set_cell(y, x - 1, Cell::EMPTY);
        }
        if after {
            self.set_cell(y, last + 1, Cell::EMPTY);
        }
    }

    /// Inserts `n` empty cells at column `x` of row `y`: the cells from
    /// there to column `end`, excluded, move right, with their graphemes
    /// and links, and the ones that pass `end` drop what they held. A wide
    /// character that the insertion would split, at `x`, at `end` or past
    /// it, is emptied whole.
    ///
    /// `x` is a column before `end`, which is at most the page's width.
    pub fn insert_blanks(&mut self, y: u16, x: u16, n: u16, end: u16) {
        debug_assert!(x < end && end <= self.cols(), "columns {x} to {end}");
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
    /// whole. Its columns are as [`Page::insert_blanks`] takes them.
    pub fn delete_cells(&mut self, y: u16, x: u16, n: u16, end: u16) {
        debug_assert!(x < end && end <= self.cols(), "columns {x} to {end}");
        let n = n.min(end - x);
        self.clear_crossing(y, x..end);
        if x + n < end && self.cells(y)[usize::from(x + n)].width() == Width::SpacerTail {
            self.set_cell(y, x + n, Cell::EMPTY);
        }
        // A spacer head, which a wide character that did not fit at the end
        // of the line leaves, stands for nothing once it moves from there.
        if self.cells(y)[usize::from(end - 1)].width() == Width::SpacerHead {
            self.set_cell(y, end - 1, Cell::EMPTY);
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
        // The cells that are not empty move right by n, within the end.
        let extent = self.extents[self.block(y)];
        if extent > x {
            self.extend(y, extent.saturating_add(n).min(end));
        }
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
        let metadata = self.row(y);
        if !metadata.has(RowFlag::Styles)
            && !metadata.has(RowFlag::Graphemes)
            && !metadata.has(RowFlag::Hyperlinks)
        {
            // The flags say that no cell holds anything in the tables.
            debug_assert!(
                self.cells(y).iter().all(|cell| {
                    cell.style() == StyleId::DEFAULT
                        && !cell.has_hyperlink()
                        && !matches!(cell.content(), Content::Grapheme(_))
                }),
                "row {y} holds what its flags leave out"
            );
            self.empty_block(y);
        } else {
            let block = self.block(y);
            let start = block * usize::from(self.cols());
            self.debug_assert_extent(y);
            for index in start..start + usize::from(self.extents[block]) {
                let cell = std::mem::replace(&mut self.cells[index], Cell::EMPTY);
                self.release(index, cell);
            }
            self.extents[block] = 0;
        }
        let row = self.row_mut(y);
        *row = Row::default();
        row.set(RowFlag::Dirty, true);
    }

    /// Moves row `from` into row `to`, within the page: the cells move with
    /// their graphemes and links, and the row's metadata goes along. What
    /// `to` held is dropped, and `from` is left empty, as
    /// [`Page::clear_row`] leaves it.
    ///
    /// The rows trade their blocks of cells, emptied first for `to`, so a
    /// move costs no copy of cells.
    pub fn move_row(&mut self, from: u16, to: u16) {
        if from == to {
            return;
        }
        self.clear_row(to);
        self.move_row_into_empty(from, to);
    }

    /// Moves the rows of `rows` up `n` rows within the page, as a scroll
    /// region does: the first `n` are emptied, as [`Page::clear_row`]
    /// empties a row, and come in at the end, and the others move up, each
    /// with its block of cells, its metadata and its entries in the tables,
    /// so that no cell is copied. Every row of `rows` is marked dirty.
    ///
    /// `n` is at most the length of `rows`, whose rows are in use.
    pub fn rotate_rows_up(&mut self, rows: Range<u16>, n: u16) {
        for y in rows.start..rows.start + n {
            self.clear_row(y);
        }
        self.rotate_rows(rows, n, false);
    }

    /// Moves the rows of `rows` down `n` rows within the page, as
    /// [`Page::rotate_rows_up`] moves them up: the last `n` are emptied and
    /// come in at the start.
    pub fn rotate_rows_down(&mut self, rows: Range<u16>, n: u16) {
        for y in rows.end - n..rows.end {
            self.clear_row(y);
        }
        self.rotate_rows(rows, n, true);
    }

    /// Moves the rows of `rows` down `n` rows, as
    /// [`Page::rotate_rows_down`] does, when the last `n` are empty
    /// already, as the rows that a scroll brings in are.
    #[inline]
    pub fn rotate_empty_rows_down(&mut self, rows: Range<u16>, n: u16) {
        debug_assert!(
            (rows.end - n..rows.end).all(|y| self.cells(y).iter().all(|&cell| cell == Cell::EMPTY)),
            "the last {n} of rows {rows:?} are empty"
        );
        self.rotate_rows(rows, n, true);
    }

    /// Rotates the rows of `rows` by `n`, down if `down` and up if not,
    /// each with its block of cells and its metadata, and marks them dirty.
    #[inline(always)]
    fn rotate_rows(&mut self, rows: Range<u16>, n: u16, down: bool) {
        if rows.len() == 2 && n == 1 {
            // Two rows trade places, either way: the last row below a
            // region and the row that a scroll brings in.
            let (a, b) = (usize::from(rows.start), usize::from(rows.start) + 1);
            self.slots.swap(a, b);
            self.rows.swap(a, b);
            for row in [a, b] {
                self.rows[row].set(RowFlag::Dirty, true);
            }
        } else {
            self.rotate_more_rows(rows, n, down);
        }
    }

    /// [`Page::rotate_rows`], for more than two rows, or by more than one.
    fn rotate_more_rows(&mut self, rows: Range<u16>, n: u16, down: bool) {
        let range = usize::from(rows.start)..usize::from(rows.end);
        rotate(&mut self.slots[range.clone()], n, down);
        rotate(&mut self.rows[range.clone()], n, down);
        for row in &mut self.rows[range] {
            row.set(RowFlag::Dirty, true);
        }
    }

    /// Moves row `from` into row `to`, an empty row, as [`Page::move_row`]
    /// does, without emptying `to` first: what scrolling does, as it moves
    /// each row into the one that the move before it left empty.
    pub fn move_row_into_empty(&mut self, from: u16, to: u16) {
        debug_assert!(
            self.cells(to).iter().all(|&cell| cell == Cell::EMPTY),
            "row {to} is empty"
        );
        let (from, to) = (self.row_index(from), self.row_index(to));
        self.slots.swap(from, to);
        let mut metadata = self.rows[from];
        metadata.set(RowFlag::Dirty, true);
        self.rows[to] = metadata;
        self.rows[from] = Row::default();
        self.rows[from].set(RowFlag::Dirty, true);
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
        // The cells that are not empty, before the source's extent, arrive
        // in the same columns of `to`.
        let extent = self.extents[self.block(from)].min(columns.end);
        if extent > columns.start {
            self.extend(to, extent);
        }
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
        let mut metadata = source.row(from);
        metadata.set(RowFlag::Dirty, true);
        if !metadata.has(RowFlag::Styles)
            && !metadata.has(RowFlag::Graphemes)
            && !metadata.has(RowFlag::Hyperlinks)
        {
            // Cells that hold nothing in the tables are copied as they are.
            self.clear_row(to);
            let extent = source.extent(from);
            let block = self.block(to);
            let start = block * usize::from(self.cols());
            self.cells[start..start + usize::from(extent)]
                .copy_from_slice(&source.cells(from)[..usize::from(extent)]);
            self.extents[block] = extent;
            *self.row_mut(to) = metadata;
            return Ok(());
        }
        if let Err(full) = self.copy_cells(to, source, from, 0..self.cols(), when_full) {
            self.clear_row(to);
            return Err(full);
        }
        *self.row_mut(to) = metadata;
        Ok(())
    }

    /// Copies row `from` of `source`, a page that may have another width,
    /// into row `to`, cut to this page's width or extended with empty
    /// cells, as a resize without reflow does. A wide character whose
    /// second half the cut drops is emptied, and so are spacer heads, since
    /// the row no longer wraps into the next one: its metadata goes along
    /// without the wrap flags. `when_full` says what a full table does, as
    /// for [`Page::copy_row`].
    pub fn copy_row_resized(
        &mut self,
        to: u16,
        source: &Page,
        from: u16,
        when_full: WhenFull,
    ) -> Result<(), TableFull> {
        let cols = self.cols().min(source.cols());
        self.clear_row(to);
        if let Err(full) = self.copy_into_empty(to, source, from, 0..cols, when_full) {
            self.clear_row(to);
            return Err(full);
        }
        for x in 0..cols {
            let cell = self.cells(to)[usize::from(x)];
            let cut = cell.width() == Width::Wide && x + 1 == self.cols();
            if cut || cell.width() == Width::SpacerHead {
                self.set_cell(to, x, Cell::EMPTY);
            }
        }
        let metadata = source.row(from);
        let row = self.row_mut(to);
        if metadata.has(RowFlag::PromptStart) {
            row.set(RowFlag::PromptStart, true);
        }
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
        for x in columns {
            self.copy_cell(to, x, source, from, x, when_full)?;
        }
        Ok(())
    }

    /// Copies the `n` cells from column `from_x` of row `from` of `source`
    /// into the cells from column `x` of row `to`, as [`Page::copy_cell`]
    /// copies each, for a stretch of cells that a wide character does not
    /// cross: a row whose cells hold nothing in the tables is copied as a
    /// block, and the cells that share a style find it once. `when_full`
    /// says what a full table does, as for [`Page::copy_cell`]; with
    /// [`WhenFull::Fail`], the cells may be left partly copied, for the
    /// caller to copy again once the page has grown.
    #[allow(clippy::too_many_arguments)]
    pub fn copy_run(
        &mut self,
        to: u16,
        x: u16,
        source: &Page,
        from: u16,
        from_x: u16,
        n: u16,
        when_full: WhenFull,
    ) -> Result<(), TableFull> {
        let metadata = source.row(from);
        let cells = &source.cells(from)[usize::from(from_x)..usize::from(from_x + n)];
        assert!(x + n <= self.cols(), "the cells pass the end of row {to}");
        let plain = |row: Row| {
            !row.has(RowFlag::Styles)
                && !row.has(RowFlag::Graphemes)
                && !row.has(RowFlag::Hyperlinks)
        };
        if plain(metadata) && plain(self.row(to)) {
            // Cells that hold nothing in the tables are copied as they are,
            // over cells that hold nothing either.
            let block = self.block(to);
            let start = block * usize::from(self.cols()) + usize::from(x);
            self.cells[start..start + usize::from(n)].copy_from_slice(cells);
            if let Some(last) = cells.iter().rposition(|&cell| cell != Cell::EMPTY) {
                // The run fits in the row, whose width is a u16.
                self.extend(to, x + last as u16 + 1);
            }
            self.row_mut(to).set(RowFlag::Dirty, true);
            return Ok(());
        }
        let mut offset = 0;
        while offset < n {
            let cell = cells[usize::from(offset)];
            let entries = |cell: &Cell| {
                matches!(cell.content(), Content::Grapheme(_)) || cell.has_hyperlink()
            };
            if entries(&cell) {
                self.copy_cell(to, x + offset, source, from, from_x + offset, when_full)?;
                offset += 1;
                continue;
            }
            // The cells that share the style, with nothing else in the
            // tables, take it at once. The run holds fewer cells than a
            // u16 counts.
            let same = cells[usize::from(offset)..]
                .iter()
                .take_while(|other| other.style() == cell.style() && !entries(other))
                .count() as u16;
            let ours = match self.styles.acquire(source.styles().get(cell.style())) {
                Ok(ours) => ours,
                Err(full) if when_full == WhenFull::Fail => return Err(full),
                Err(_) => StyleId::DEFAULT,
            };
            self.write_cells_as(
                to,
                x + offset,
                &cells[usize::from(offset)..][..usize::from(same)],
                ours,
            );
            self.styles.release(ours);
            offset += same;
        }
        Ok(())
    }

    /// Puts `cells`, which hold nothing in the tables, in the cells from
    /// column `x` of row `y`, each with the style `style`, which must be in
    /// the page's table, in place of what those held.
    fn write_cells_as(&mut self, y: u16, x: u16, cells: &[Cell], style: StyleId) {
        let block = self.block(y);
        let start = block * usize::from(self.cols()) + usize::from(x);
        let range = start..start + cells.len();
        // Retain first, in case the old cells hold the last reference. The
        // row has fewer cells than a u32 counts.
        self.styles.retain_many(style, cells.len() as u32);
        let metadata = self.row(y);
        if metadata.has(RowFlag::Styles)
            || metadata.has(RowFlag::Graphemes)
            || metadata.has(RowFlag::Hyperlinks)
        {
            for index in range.clone() {
                let old = self.cells[index];
                self.release(index, old);
            }
        }
        for (target, &cell) in self.cells[range].iter_mut().zip(cells) {
            *target = cell.with_style(style);
        }
        if let Some(last) = cells
            .iter()
            .rposition(|&cell| cell.with_style(style) != Cell::EMPTY)
        {
            // The cells fit in the row, whose width is a u16.
            let end = x + last as u16 + 1;
            self.extents[block] = self.extents[block].max(end);
        }
        let row = self.row_mut(y);
        row.set(RowFlag::Dirty, true);
        if style != StyleId::DEFAULT {
            row.set(RowFlag::Styles, true);
        }
    }

    /// Copies the cell at column `from_x` of row `from` of `source` into
    /// column `x` of row `to`, with its style, grapheme and link in this
    /// page's tables, in place of what the cell held. `when_full` says what
    /// a full table does, as for [`Page::copy_cells`]; with
    /// [`WhenFull::Fail`], the cell may be left partly copied, for the
    /// caller to copy again once the page has grown.
    pub fn copy_cell(
        &mut self,
        to: u16,
        x: u16,
        source: &Page,
        from: u16,
        from_x: u16,
        when_full: WhenFull,
    ) -> Result<(), TableFull> {
        // What a full table does: stop the copy, or let the cell go on
        // without what it has no room for.
        let full = |full| match when_full {
            WhenFull::Fail => Err(full),
            WhenFull::Drop => Ok(()),
        };
        let cell = source.cells(from)[usize::from(from_x)];
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
        for &c in source.grapheme(from, from_x) {
            if let Err(error) = self.push_codepoint(to, x, c) {
                full(error)?;
                break;
            }
        }
        if let Some(link) = source.hyperlink(from, from_x) {
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
        Ok(())
    }

    /// Empties the page as it was new, with no row in use, without
    /// allocating: what the pool does before handing a page out again.
    /// The cells stay as they were until [`Page::push_row`] puts their row
    /// in use again, which empties them.
    pub fn reset(&mut self) {
        self.rows[..usize::from(self.len)].fill(Row::default());
        self.styles.clear();
        self.graphemes.clear();
        self.hyperlinks.clear();
        self.len = 0;
    }

    /// A copy of the page with `capacity`, which must have its width and at
    /// least its rows. Styles keep their ids, and cells and rows their
    /// blocks, so cells are copied as they are.
    pub fn grown(&self, capacity: Capacity) -> Self {
        assert_eq!(capacity.cols, self.capacity.cols, "a page keeps its width");
        assert!(capacity.rows >= self.capacity.rows, "the rows must fit");
        let mut page = Self::new(capacity);
        let rows = usize::from(self.len);
        page.len = self.len;
        page.rows[..rows].copy_from_slice(&self.rows[..rows]);
        page.slots[..self.slots.len()].copy_from_slice(&self.slots);
        page.cells[..self.cells.len()].copy_from_slice(&self.cells);
        page.extents[..self.extents.len()].copy_from_slice(&self.extents);
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

    /// Whether the rows that the page holds take more styles, clusters,
    /// links or linked cells than a page of `standard` capacity has room
    /// for, or more of the room that clusters' codepoints and links' URIs
    /// take: whether they need its tables as they grew.
    pub(crate) fn outgrew(&self, standard: Capacity) -> bool {
        self.styles.len() > usize::from(standard.styles)
            || !self.graphemes.fits(standard.graphemes)
            || !self
                .hyperlinks
                .fits(standard.hyperlinks, standard.linked_cells)
    }

    /// The bytes that the page takes, header included.
    pub fn memory(&self) -> usize {
        size_of::<Self>()
            + size_of_val(&*self.rows)
            + size_of_val(&*self.slots)
            + size_of_val(&*self.cells)
            + size_of_val(&*self.extents)
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

    /// The block of cells of row `y`, which must be in use.
    fn block(&self, y: u16) -> usize {
        // One check covers the row in use and the slot: the slots past the
        // rows in use are not read.
        let Some(&slot) = self.slots[..usize::from(self.len)].get(usize::from(y)) else {
            panic!("row {y} is not in use");
        };
        usize::from(slot)
    }

    fn cell_range(&self, y: u16) -> std::ops::Range<usize> {
        let start = self.block(y) * usize::from(self.cols());
        start..start + usize::from(self.cols())
    }

    /// The column of row `y` from which its cells are all empty, or one
    /// before which some may be empty too.
    pub(crate) fn extent(&self, y: u16) -> u16 {
        self.extents[self.block(y)]
    }

    /// Takes the extent of row `y` to column `end`, if it is short of it,
    /// for a cell before `end` that is no longer empty.
    fn extend(&mut self, y: u16, end: u16) {
        let extent = &mut self.extents[self.block(y)];
        *extent = (*extent).max(end);
    }

    /// Empties the cells of row `y` as they are, without dropping what
    /// they hold in the tables: for cells that hold nothing there, or that
    /// a reset left behind.
    fn empty_block(&mut self, y: u16) {
        /// The cells that are emptied with stores of a fixed size, which
        /// take less than a call to fill memory.
        const FEW: usize = 16;
        self.debug_assert_extent(y);
        let block = self.block(y);
        let cols = usize::from(self.cols());
        let start = block * cols;
        let extent = usize::from(self.extents[block]);
        // The cells past the extent are empty already.
        if extent <= FEW && FEW <= cols {
            self.cells[start..start + FEW].copy_from_slice(&[Cell::EMPTY; FEW]);
        } else {
            self.cells[start..start + extent].fill(Cell::EMPTY);
        }
        self.extents[block] = 0;
    }

    /// Checks that the cells of row `y` past its extent are empty.
    fn debug_assert_extent(&self, y: u16) {
        let extent = usize::from(self.extent(y));
        debug_assert!(
            self.cells(y)[extent..]
                .iter()
                .all(|&cell| cell == Cell::EMPTY),
            "row {y} has cells past its extent, {extent}"
        );
    }

    fn cell_index(&self, y: u16, x: u16) -> usize {
        assert!(x < self.cols(), "column {x} is past the row");
        self.cell_range(y).start + usize::from(x)
    }
}

/// Rotates `items` left by `n`, or right if `right`, as `rotate_left` and
/// `rotate_right` do. By one, as a line feed scrolls, the items move in
/// chunks of a fixed size, which stay in registers: the standard rotation
/// copies overlapping memory, which takes longer than the few rows of a
/// scroll region.
fn rotate<T: Copy>(items: &mut [T], n: u16, right: bool) {
    const CHUNK: usize = 8;
    let len = items.len();
    if n != 1 || len < 2 {
        if right {
            items.rotate_right(usize::from(n));
        } else {
            items.rotate_left(usize::from(n));
        }
        return;
    }
    if right {
        let last = items[len - 1];
        let mut end = len - 1;
        while end >= CHUNK {
            let chunk: [T; CHUNK] = items[end - CHUNK..end].try_into().expect("a chunk");
            items[end - CHUNK + 1..=end].copy_from_slice(&chunk);
            end -= CHUNK;
        }
        for i in (1..=end).rev() {
            items[i] = items[i - 1];
        }
        items[0] = last;
    } else {
        let first = items[0];
        let mut start = 0;
        while start + CHUNK < len {
            let chunk: [T; CHUNK] = items[start + 1..=start + CHUNK]
                .try_into()
                .expect("a chunk");
            items[start..start + CHUNK].copy_from_slice(&chunk);
            start += CHUNK;
        }
        for i in start..len - 1 {
            items[i] = items[i + 1];
        }
        items[len - 1] = first;
    }
}

#[cfg(test)]
mod tests;
