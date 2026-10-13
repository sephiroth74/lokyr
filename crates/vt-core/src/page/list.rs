//! The page list: the pages of a screen and of its scrollback, oldest
//! first, with a pool of spare pages.

use super::{Capacity, Page, TableFull};

/// No node, at either end of the list.
const NONE: u32 = u32::MAX;

/// A page of a [`PageList`], stable while the page stays in the list.
///
/// When the page leaves the list, its id goes stale rather than naming
/// another page: [`PageList::page`] returns `None` for it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PageId {
    index: u32,
    generation: u32,
}

/// A slot of the slab.
#[derive(Debug)]
struct Node {
    /// `None` while the slot is free.
    page: Option<Page>,
    prev: u32,
    next: u32,
    /// How many pages the slot has held, so that ids go stale.
    generation: u32,
}

/// The rows of a screen and of its scrollback, in pages of one width.
///
/// The pages form a doubly linked list of indexes into a slab, oldest
/// first. The last `rows` rows are the active screen; the rows above them
/// are the scrollback. Scrolling adds an empty row at the bottom. When the
/// list holds its most pages, the oldest page leaves it to make room: it is
/// emptied and kept in a pool, from which new pages come. A page that grew
/// past the standard size leaves for good instead.
///
/// Once the pool and the slab have reached their size, scrolling allocates
/// nothing.
#[derive(Debug)]
pub struct PageList {
    nodes: Vec<Node>,
    /// The free slots of the slab.
    free: Vec<u32>,
    head: u32,
    tail: u32,
    /// How many pages are in the list.
    pages: usize,
    /// Empty pages of the standard capacity, to use again.
    pool: Vec<Page>,
    /// The capacity of a standard page of the list's width.
    capacity: Capacity,
    /// The rows of the active screen.
    rows: u16,
    /// The rows in the list: the scrollback and the screen.
    total_rows: usize,
    max_pages: usize,
}

impl PageList {
    /// A list of `cols` columns, whose screen has `rows` empty rows. It
    /// holds at most `max_pages` pages, or the fewest that the screen needs,
    /// if that is more.
    pub fn new(cols: u16, rows: u16, max_pages: usize) -> Self {
        let capacity = Capacity::standard(cols);
        // The screen can start in the middle of a page, so it can take one
        // page more than its rows fill.
        let screen_pages = usize::from(rows).div_ceil(usize::from(capacity.rows)) + 1;
        let mut list = Self {
            nodes: Vec::new(),
            free: Vec::new(),
            head: NONE,
            tail: NONE,
            pages: 0,
            pool: Vec::new(),
            capacity,
            rows,
            total_rows: 0,
            max_pages: max_pages.max(screen_pages),
        };
        for _ in 0..rows {
            list.push_row();
        }
        list
    }

    /// How many cells each row has.
    pub fn cols(&self) -> u16 {
        self.capacity.cols
    }

    /// How many rows the active screen has.
    pub fn rows(&self) -> u16 {
        self.rows
    }

    /// How many rows the list holds: the scrollback and the screen.
    pub fn total_rows(&self) -> usize {
        self.total_rows
    }

    /// How many rows are in the scrollback, above the screen.
    pub fn scrollback_rows(&self) -> usize {
        self.total_rows - usize::from(self.rows)
    }

    /// How many pages are in the list.
    pub fn pages(&self) -> usize {
        self.pages
    }

    /// How many spare pages are in the pool.
    pub fn pool_len(&self) -> usize {
        self.pool.len()
    }

    /// The most pages that the list holds.
    pub fn max_pages(&self) -> usize {
        self.max_pages
    }

    /// The bytes that the pages take, in the list and in the pool.
    pub fn memory(&self) -> usize {
        let listed: usize = self.iter_pages().map(Page::memory).sum();
        listed + self.pool.iter().map(Page::memory).sum::<usize>()
    }

    /// Scrolls the screen up one row: its top row goes into the scrollback,
    /// and an empty row appears at the bottom.
    pub fn scroll_up(&mut self) {
        self.push_row();
    }

    /// The page that holds row `y` of the screen, and the row in it.
    pub fn screen_row(&self, y: u16) -> (&Page, u16) {
        let (index, row) = self.locate(y);
        (self.page_at(index), row)
    }

    /// The page that holds row `y` of the screen, to change, and the row in
    /// it.
    pub fn screen_row_mut(&mut self, y: u16) -> (&mut Page, u16) {
        let (index, row) = self.locate(y);
        (self.page_at_mut(index), row)
    }

    /// The id of the page that holds row `y` of the screen, and the row in
    /// it.
    pub fn screen_row_id(&self, y: u16) -> (PageId, u16) {
        let (index, row) = self.locate(y);
        let id = PageId {
            index,
            generation: self.nodes[index as usize].generation,
        };
        (id, row)
    }

    /// The page with `id`, or `None` if it has left the list.
    pub fn page(&self, id: PageId) -> Option<&Page> {
        let node = self.nodes.get(id.index as usize)?;
        if node.generation == id.generation {
            node.page.as_ref()
        } else {
            None
        }
    }

    /// Rebuilds the page that holds row `y` of the screen with room in the
    /// table that is `full`. Its rows keep their contents, and its id stays
    /// the same.
    pub fn grow_page(&mut self, y: u16, full: TableFull) {
        let (index, _) = self.locate(y);
        let page = self.page_at(index);
        let grown = page.grown(page.capacity().grown_for(full));
        self.nodes[index as usize].page = Some(grown);
    }

    /// Every row of the list, oldest first, as its page and its row in it.
    pub fn iter_rows(&self) -> impl Iterator<Item = (&Page, u16)> + '_ {
        self.iter_pages()
            .flat_map(|page| (0..page.len()).map(move |row| (page, row)))
    }

    fn iter_pages(&self) -> impl Iterator<Item = &Page> + '_ {
        let mut index = self.head;
        std::iter::from_fn(move || {
            if index == NONE {
                return None;
            }
            let node = &self.nodes[index as usize];
            index = node.next;
            node.page.as_ref()
        })
    }

    /// Adds an empty row at the bottom, in a new page if the last one is
    /// full, which first makes the oldest page leave if the list is at its
    /// most pages.
    fn push_row(&mut self) {
        if self.tail == NONE || self.page_at(self.tail).is_full() {
            if self.pages >= self.max_pages {
                self.evict_head();
            }
            self.push_page();
        }
        let tail = self.tail;
        self.page_at_mut(tail).push_row();
        self.total_rows += 1;
    }

    /// Links a page from the pool, or a new one, at the end of the list.
    fn push_page(&mut self) {
        let page = self.pool.pop().unwrap_or_else(|| Page::new(self.capacity));
        let node = Node {
            page: Some(page),
            prev: self.tail,
            next: NONE,
            generation: 0,
        };
        let index = match self.free.pop() {
            Some(index) => {
                let slot = &mut self.nodes[index as usize];
                *slot = Node {
                    generation: slot.generation,
                    ..node
                };
                index
            }
            None => {
                self.nodes.push(node);
                u32::try_from(self.nodes.len() - 1).expect("fewer than 2^32 pages")
            }
        };
        if self.tail == NONE {
            self.head = index;
        } else {
            self.nodes[self.tail as usize].next = index;
        }
        self.tail = index;
        self.pages += 1;
    }

    /// Unlinks the oldest page, and keeps it in the pool if it has the
    /// standard capacity.
    fn evict_head(&mut self) {
        let index = self.head;
        let node = &mut self.nodes[index as usize];
        let mut page = node.page.take().expect("a linked node has a page");
        node.generation = node.generation.wrapping_add(1);
        let next = node.next;
        self.head = next;
        if next == NONE {
            self.tail = NONE;
        } else {
            self.nodes[next as usize].prev = NONE;
        }
        self.free.push(index);
        self.pages -= 1;
        self.total_rows -= usize::from(page.len());
        if page.capacity() == self.capacity {
            page.reset();
            self.pool.push(page);
        }
    }

    /// The node and the row in its page of row `y` of the screen.
    fn locate(&self, y: u16) -> (u32, u16) {
        assert!(y < self.rows, "row {y} is past the screen");
        let row = self.total_rows - usize::from(self.rows) + usize::from(y);
        // The screen is at the end of the list, so search from the tail.
        let mut index = self.tail;
        let mut end = self.total_rows;
        loop {
            let page = self.page_at(index);
            let start = end - usize::from(page.len());
            if row >= start {
                return (index, (row - start) as u16);
            }
            end = start;
            index = self.nodes[index as usize].prev;
        }
    }

    fn page_at(&self, index: u32) -> &Page {
        self.nodes[index as usize]
            .page
            .as_ref()
            .expect("a linked node has a page")
    }

    fn page_at_mut(&mut self, index: u32) -> &mut Page {
        self.nodes[index as usize]
            .page
            .as_mut()
            .expect("a linked node has a page")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::page::{Cell, Content, Rgb};

    /// A cell that holds the number `n`, for checking where rows went.
    fn numbered(n: u32) -> Cell {
        let [_, r, g, b] = n.to_be_bytes();
        Cell::EMPTY.with_content(Content::BgRgb(Rgb::new(r, g, b)))
    }

    /// Scrolls `lines` times, numbering each new bottom row from `first`.
    fn scroll(list: &mut PageList, first: u32, lines: u32) {
        let bottom = list.rows() - 1;
        for n in first..first + lines {
            list.scroll_up();
            let (page, row) = list.screen_row_mut(bottom);
            page.set_cell(row, 0, numbered(n));
        }
    }

    /// The numbers of the rows of the list, oldest first; `None` for a row
    /// without one.
    fn numbers(list: &PageList) -> Vec<Option<u32>> {
        list.iter_rows()
            .map(|(page, row)| match page.cells(row)[0].content() {
                Content::BgRgb(Rgb { r, g, b }) => Some(u32::from_be_bytes([0, r, g, b])),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn a_new_list_is_an_empty_screen() {
        let list = PageList::new(80, 24, 4);
        assert_eq!(
            (list.total_rows(), list.scrollback_rows(), list.pages()),
            (24, 0, 1)
        );
        for y in 0..24 {
            let (page, row) = list.screen_row(y);
            assert_eq!(row, y);
            assert!(page.cells(row).iter().all(|cell| cell.is_empty()));
        }
    }

    #[test]
    fn scrolling_moves_rows_into_the_scrollback() {
        let mut list = PageList::new(80, 24, 4);
        scroll(&mut list, 0, 30);
        assert_eq!(list.scrollback_rows(), 30);
        let numbers = numbers(&list);
        assert_eq!(numbers[..24], [None; 24], "the first screen was empty");
        let scrolled: Vec<Option<u32>> = (0..30).map(Some).collect();
        assert_eq!(numbers[24..], scrolled);
        let (page, row) = list.screen_row(23);
        assert_eq!(page.cells(row)[0], numbered(29));
    }

    #[test]
    fn rows_go_on_into_new_pages() {
        let mut list = PageList::new(80, 24, 4);
        let page_rows = u32::from(Capacity::standard(80).rows);
        // Twelve rows short of a page more: the screen straddles two pages.
        let lines = page_rows - 12;
        scroll(&mut list, 0, lines);
        assert_eq!(list.pages(), 2);
        assert_eq!(list.total_rows(), 24 + lines as usize);
        let (first, _) = list.screen_row_id(0);
        let (last, _) = list.screen_row_id(23);
        assert_ne!(first, last);
        let expected: Vec<Option<u32>> = (lines - 24..lines).map(Some).collect();
        let screen: Vec<Option<u32>> = numbers(&list)[list.scrollback_rows()..].to_vec();
        assert_eq!(screen, expected);
    }

    #[test]
    fn the_oldest_page_leaves_for_the_pool_at_the_limit() {
        let mut list = PageList::new(80, 24, 3);
        let page_rows = usize::from(Capacity::standard(80).rows);
        let (oldest, _) = list.screen_row_id(0);
        // Fill three pages, then start a fourth.
        scroll(&mut list, 0, (3 * page_rows - 24 + 1) as u32);
        assert_eq!(list.pages(), 3);
        assert!(
            list.page(oldest).is_none(),
            "the oldest page's id went stale"
        );
        assert_eq!(list.total_rows(), 2 * page_rows + 1);
        // The pool hands the page back out, so it does not grow.
        assert_eq!(list.pool_len(), 0);
        let numbers = numbers(&list);
        let first = numbers[0].expect("the first page's rows are gone");
        let kept: Vec<Option<u32>> = (first..first + numbers.len() as u32).map(Some).collect();
        assert_eq!(numbers, kept, "the rows kept are the latest, in order");
    }

    #[test]
    fn a_screen_taller_than_a_page_spans_several() {
        // At 4000 columns, a page holds fewer rows than the screen has.
        let mut list = PageList::new(4000, 40, 1);
        assert!(list.max_pages() >= 4);
        scroll(&mut list, 0, 100);
        let expected: Vec<Option<u32>> = (60..100).map(Some).collect();
        let screen: Vec<Option<u32>> = (0..40)
            .map(|y| {
                let (page, row) = list.screen_row(y);
                match page.cells(row)[0].content() {
                    Content::BgRgb(Rgb { r, g, b }) => Some(u32::from_be_bytes([0, r, g, b])),
                    _ => None,
                }
            })
            .collect();
        assert_eq!(screen, expected);
    }

    #[test]
    fn a_grown_page_keeps_its_rows_and_id_and_leaves_the_pool_out() {
        let mut list = PageList::new(80, 24, 2);
        scroll(&mut list, 0, 10);
        let (id, _) = list.screen_row_id(23);
        list.grow_page(23, TableFull::Styles);
        assert_eq!(list.page(id).map(|page| page.capacity().styles), Some(512));
        let (page, row) = list.screen_row(23);
        assert_eq!(page.cells(row)[0], numbered(9));
        // When it leaves the list, a grown page is not pooled.
        let page_rows = usize::from(Capacity::standard(80).rows);
        scroll(&mut list, 10, (2 * page_rows) as u32);
        assert!(list.page(id).is_none());
        assert_eq!(list.pool_len(), 0);
    }

    #[test]
    #[should_panic(expected = "row 24 is past the screen")]
    fn a_row_past_the_screen_cannot_be_read() {
        PageList::new(80, 24, 4).screen_row(24);
    }
}
