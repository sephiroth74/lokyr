//! Cold pages: pages far from the screen, kept without the empty cells that
//! end their rows, until they are needed again.

use super::{Cell, Page};

/// A page kept small: the cells of each row up to its last one that is not
/// empty, and the page itself without its cells. The page's tables stay as
/// they are, and so do its rows' blocks, so the entries keyed by cell are
/// still right when the page is expanded.
#[derive(Debug)]
pub(crate) struct CompactPage {
    page: Page,
    /// The kept cells of each row, one row after the other.
    cells: Box<[Cell]>,
    /// Where each row's cells start in `cells`; the last entry is where the
    /// last row's end.
    starts: Box<[u32]>,
}

impl CompactPage {
    /// Whether keeping `page` small makes it smaller, which it does not
    /// when its rows are full, or nearly: where each row starts takes bytes
    /// too.
    pub(crate) fn shrinks(page: &Page) -> bool {
        let total: usize = (0..page.len()).map(|y| kept(page, y)).sum();
        let starts = usize::from(page.len()) + 1;
        total * size_of::<Cell>() + starts * size_of::<u32>() < size_of_val(&*page.cells)
    }

    /// `page`, kept small. Only cells that are entirely empty are left
    /// out, so nothing that a cell holds is lost.
    pub(crate) fn new(mut page: Page) -> Self {
        let total = (0..page.len()).map(|y| kept(&page, y)).sum();
        let mut cells = Vec::with_capacity(total);
        let mut starts = Vec::with_capacity(usize::from(page.len()) + 1);
        for y in 0..page.len() {
            starts.push(Self::index(cells.len()));
            cells.extend_from_slice(&page.cells(y)[..kept(&page, y)]);
        }
        starts.push(Self::index(cells.len()));
        page.cells = Box::default();
        Self {
            page,
            cells: cells.into_boxed_slice(),
            starts: starts.into_boxed_slice(),
        }
    }

    /// How many rows the page has in use.
    pub(crate) fn len(&self) -> u16 {
        self.page.len()
    }

    /// The page without its cells: its rows' metadata and its tables.
    pub(crate) fn page(&self) -> &Page {
        &self.page
    }

    /// The kept cells of row `y`: those up to its last that is not empty.
    pub(crate) fn cells(&self, y: u16) -> &[Cell] {
        let y = usize::from(y);
        &self.cells[self.starts[y] as usize..self.starts[y + 1] as usize]
    }

    /// The bytes that the page takes, as kept.
    pub(crate) fn memory(&self) -> usize {
        self.page.memory() + size_of_val(&*self.cells) + size_of_val(&*self.starts)
    }

    /// The page, with all its cells again.
    pub(crate) fn expand(self) -> Page {
        let mut page = self.page;
        let cols = usize::from(page.cols());
        page.cells = vec![Cell::EMPTY; usize::from(page.capacity.rows) * cols].into_boxed_slice();
        for y in 0..page.len() {
            let row = &self.cells
                [self.starts[usize::from(y)] as usize..self.starts[usize::from(y) + 1] as usize];
            let start = page.cell_range(y).start;
            page.cells[start..start + row.len()].copy_from_slice(row);
        }
        page
    }

    fn index(cell: usize) -> u32 {
        // A page holds fewer than 2^32 cells: its rows and columns are u16.
        cell as u32
    }
}

/// How many cells of row `y` of `page` a cold page keeps: those up to its
/// last that is not empty.
fn kept(page: &Page, y: u16) -> usize {
    page.cells(y)[..usize::from(page.extent(y))]
        .iter()
        .rposition(|&cell| cell != Cell::EMPTY)
        .map_or(0, |x| x + 1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::page::{Capacity, Color, Content, Style};

    #[test]
    fn a_page_comes_back_as_it_was_kept() {
        let mut page = Page::new(Capacity::standard(80));
        for _ in 0..4 {
            page.push_row();
        }
        let red = Style {
            fg: Color::Palette(1),
            ..Style::DEFAULT
        };
        let style = page.styles_mut().acquire(&red).unwrap();
        for y in 0..4 {
            let c = char::from(b'a' + y as u8);
            page.set_cell(y, 0, Cell::EMPTY.with_content(Content::Char(c)));
            page.push_codepoint(y, 0, '\u{301}').unwrap();
        }
        // A cell with a style and nothing in it is not empty.
        page.set_cell(1, 40, Cell::EMPTY.with_style(style));
        page.styles_mut().release(style);
        // Rows that traded their blocks.
        page.move_row(0, 3);
        let before: Vec<Vec<Cell>> = (0..4).map(|y| page.cells(y).to_vec()).collect();
        let graphemes: Vec<Vec<char>> = (0..4).map(|y| page.grapheme(y, 0).to_vec()).collect();
        let cold = CompactPage::new(page);
        assert!(cold.memory() < Capacity::standard(80).bytes() / 2);
        let page = cold.expand();
        for y in 0..4 {
            assert_eq!(page.cells(y), &before[usize::from(y)][..], "row {y}");
            assert_eq!(
                page.grapheme(y, 0),
                &graphemes[usize::from(y)][..],
                "row {y}"
            );
        }
        assert_eq!(page.styles().get(page.cells(1)[40].style()), &red);
        assert_eq!(page.memory(), Capacity::standard(80).bytes());
    }

    /// A page of `capacity`, all its rows used, with `x` in every cell but
    /// the last cell of the first `short` rows.
    fn full_page(capacity: Capacity, short: u16) -> Page {
        let mut page = Page::new(capacity);
        let cols = page.cols();
        for y in 0..capacity.rows {
            page.push_row();
            let width = if y < short { cols - 1 } else { cols };
            for x in 0..width {
                page.set_cell(y, x, Cell::EMPTY.with_content(Content::Char('x')));
            }
        }
        page
    }

    #[test]
    fn a_page_that_would_not_be_smaller_is_not_kept_small() {
        let capacity = Capacity::standard(120);
        assert!(!CompactPage::shrinks(&full_page(capacity, 0)), "full rows");
        // Where each row starts takes four bytes a row, and one more entry;
        // a cell left out gives back eight. The most short rows that do not
        // make up for that save nothing, so the page stays; one more saves
        // bytes.
        let most = capacity.rows.div_ceil(2);
        assert!(!CompactPage::shrinks(&full_page(capacity, most)));
        let page = full_page(capacity, most + 1);
        assert!(CompactPage::shrinks(&page));
        let bytes = page.memory();
        let cold = CompactPage::new(page);
        // The cells left out, less where each row starts.
        let saved = usize::from(most + 1) * size_of::<Cell>()
            - (usize::from(capacity.rows) + 1) * size_of::<u32>();
        assert_eq!(cold.memory(), bytes - saved);
    }
}
