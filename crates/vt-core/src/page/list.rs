//! The page list: the pages of a screen and of its scrollback, oldest
//! first, with a pool of spare pages, within limits in bytes.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use super::{Capacity, Page, TableFull};

/// No node, at either end of the list.
const NONE: u32 = u32::MAX;

/// The bytes that the pages of a pane take by default.
pub const SCROLLBACK_BYTES: usize = 10 * 1024 * 1024;

/// The bytes that the pages of every pane together take by default.
pub const SCROLLBACK_BUDGET_BYTES: usize = 100 * 1024 * 1024;

/// Memory that the page lists of several panes share, from any thread.
///
/// Each list counts its pages in it. A list that needs a page past the
/// budget makes room by letting its own oldest pages go: a busy pane never
/// takes the scrollback of an idle one.
///
/// Only the pages that screens need can pass the budget, since a screen
/// always gets its rows. A list that adds a page while the budget is passed
/// gives back its own scrollback first, until the budget is met again.
#[derive(Debug)]
pub struct Budget {
    limit: usize,
    used: AtomicUsize,
}

impl Budget {
    /// A budget of `limit` bytes, none of them used.
    pub fn new(limit: usize) -> Self {
        Self {
            limit,
            used: AtomicUsize::new(0),
        }
    }

    /// The bytes that the lists may take together.
    pub fn limit(&self) -> usize {
        self.limit
    }

    /// The bytes that the lists take now.
    pub fn used(&self) -> usize {
        self.used.load(Ordering::Relaxed)
    }

    /// Takes `bytes` if they fit.
    fn try_take(&self, bytes: usize) -> bool {
        self.used
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |used| {
                used.checked_add(bytes).filter(|&total| total <= self.limit)
            })
            .is_ok()
    }

    /// Takes `bytes` whether they fit or not, for the pages of a screen.
    fn take(&self, bytes: usize) {
        self.used.fetch_add(bytes, Ordering::Relaxed);
    }

    fn give(&self, bytes: usize) {
        self.used.fetch_sub(bytes, Ordering::Relaxed);
    }

    fn is_over(&self) -> bool {
        self.used() > self.limit
    }
}

/// What the pages of a list may take.
#[derive(Clone, Debug)]
pub struct Scrollback {
    /// The bytes of the list's pages, those of its screen and of its pool
    /// included: [`SCROLLBACK_BYTES`] by default.
    pub limit: usize,
    /// A budget that the lists of several panes share, if any.
    pub budget: Option<Arc<Budget>>,
}

impl Default for Scrollback {
    fn default() -> Self {
        Self {
            limit: SCROLLBACK_BYTES,
            budget: None,
        }
    }
}

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
/// are the scrollback. Scrolling adds an empty row at the bottom.
///
/// The pages stay within the [`Scrollback`] limits. When a new page would
/// pass them, the oldest page leaves the list first: it is emptied and kept
/// in a pool, from which the new page then comes. A page that grew past the
/// standard size is freed instead. The screen never leaves, even when its
/// pages alone pass the limits.
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
    scrollback: Scrollback,
    /// The bytes of the pages, in the list and in the pool.
    memory: usize,
}

impl PageList {
    /// A list of `cols` columns, whose screen has `rows` empty rows, with
    /// pages within `scrollback`.
    pub fn new(cols: u16, rows: u16, scrollback: Scrollback) -> Self {
        let capacity = Capacity::standard(cols);
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
            scrollback,
            memory: 0,
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

    /// The limits that the pages stay within.
    pub fn scrollback(&self) -> &Scrollback {
        &self.scrollback
    }

    /// The bytes that the pages take, in the list and in the pool.
    pub fn memory(&self) -> usize {
        self.memory
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
    /// the same. If the larger page passes the limits, the oldest pages of
    /// the scrollback make room.
    pub fn grow_page(&mut self, y: u16, full: TableFull) {
        let (index, _) = self.locate(y);
        let page = self.page_at(index);
        let grown = page.grown(page.capacity().grown_for(full));
        let added = grown.memory() - page.memory();
        self.nodes[index as usize].page = Some(grown);
        self.take(added);
        self.enforce();
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
    /// full.
    fn push_row(&mut self) {
        let needs_page = self.tail == NONE || self.page_at(self.tail).is_full();
        if needs_page {
            let page = self.new_page();
            self.link(page);
        }
        let tail = self.tail;
        self.page_at_mut(tail).push_row();
        self.total_rows += 1;
        // Another list's screen may have passed the shared budget.
        if needs_page && self.is_over() {
            self.enforce();
        }
    }

    /// A page for the end of the list: from the pool if it has one, or a
    /// new one if the limits allow it. Otherwise the oldest page leaves for
    /// the pool first, which then hands it out again.
    fn new_page(&mut self) -> Page {
        loop {
            if let Some(page) = self.pool.pop() {
                return page;
            }
            let bytes = self.capacity.bytes();
            // The new page will hold a row of the screen.
            if !self.can_evict(1) {
                // The screen needs the page, whatever the limits.
                self.take(bytes);
                return Page::new(self.capacity);
            }
            if self.memory + bytes <= self.scrollback.limit && self.try_take(bytes) {
                return Page::new(self.capacity);
            }
            self.evict_head();
        }
    }

    /// Links `page` at the end of the list.
    fn link(&mut self, page: Page) {
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

    /// Whether the oldest page can leave without taking part of the screen,
    /// with `coming` more rows about to be added.
    fn can_evict(&self, coming: usize) -> bool {
        self.head != NONE
            && self.total_rows + coming - usize::from(self.page_at(self.head).len())
                >= usize::from(self.rows)
    }

    /// Unlinks the oldest page, and keeps it in the pool if it has the
    /// standard capacity; frees it otherwise.
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
        } else {
            self.give(page.memory());
        }
    }

    /// Frees spare pages, then the oldest pages of the scrollback, until the
    /// pages are within the limits or only the screen is left.
    fn enforce(&mut self) {
        while self.is_over() {
            if let Some(page) = self.pool.pop() {
                self.give(page.memory());
            } else if self.can_evict(0) {
                self.evict_head();
            } else {
                break;
            }
        }
    }

    fn is_over(&self) -> bool {
        self.memory > self.scrollback.limit
            || self
                .scrollback
                .budget
                .as_ref()
                .is_some_and(|budget| budget.is_over())
    }

    /// Counts `bytes` more, whether the budget has room or not.
    fn take(&mut self, bytes: usize) {
        self.memory += bytes;
        if let Some(budget) = &self.scrollback.budget {
            budget.take(bytes);
        }
    }

    /// Counts `bytes` more if the budget has room for them.
    fn try_take(&mut self, bytes: usize) -> bool {
        let taken = self
            .scrollback
            .budget
            .as_ref()
            .is_none_or(|budget| budget.try_take(bytes));
        if taken {
            self.memory += bytes;
        }
        taken
    }

    /// Counts `bytes` less.
    fn give(&mut self, bytes: usize) {
        self.memory -= bytes;
        if let Some(budget) = &self.scrollback.budget {
            budget.give(bytes);
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

/// The pages leave the budget with the list.
impl Drop for PageList {
    fn drop(&mut self) {
        if let Some(budget) = &self.scrollback.budget {
            budget.give(self.memory);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::page::{Cell, Color, Content, Rgb, Style};

    /// Limits that hold `pages` standard pages of `cols` columns.
    fn pages(cols: u16, pages: usize) -> Scrollback {
        Scrollback {
            limit: pages * Capacity::standard(cols).bytes(),
            budget: None,
        }
    }

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
        let list = PageList::new(80, 24, pages(80, 4));
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
        let mut list = PageList::new(80, 24, pages(80, 4));
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
        let mut list = PageList::new(80, 24, pages(80, 4));
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
        let mut list = PageList::new(80, 24, pages(80, 3));
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
        let mut list = PageList::new(4000, 40, pages(4000, 1));
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
        assert!(list.memory() > list.scrollback().limit, "the screen stays");
    }

    #[test]
    fn a_grown_page_keeps_its_rows_and_id_and_leaves_the_pool_out() {
        let mut list = PageList::new(80, 24, pages(80, 2));
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
        PageList::new(80, 24, pages(80, 4)).screen_row(24);
    }

    /// The bytes of the pages, counted one by one.
    fn counted(list: &PageList) -> usize {
        list.iter_pages().map(Page::memory).sum::<usize>()
            + list.pool.iter().map(Page::memory).sum::<usize>()
    }

    /// Writes a row in a style of its own, `n`, growing the page when its
    /// style table is full, as the terminal will.
    fn styled_line(list: &mut PageList, n: u32) {
        let bottom = list.rows() - 1;
        list.scroll_up();
        let [_, r, g, b] = n.to_be_bytes();
        let style = Style {
            fg: Color::Rgb(Rgb::new(r, g, b)),
            ..Style::DEFAULT
        };
        let id = loop {
            let (page, _) = list.screen_row_mut(bottom);
            match page.styles_mut().acquire(&style) {
                Ok(id) => break id,
                Err(full) => list.grow_page(bottom, full),
            }
        };
        let (page, row) = list.screen_row_mut(bottom);
        page.set_cell(row, 0, numbered(n).with_style(id));
        page.styles_mut().release(id);
    }

    #[test]
    fn a_flood_of_output_stays_within_the_limit() {
        let limit = Scrollback::default().limit;
        let mut list = PageList::new(80, 24, Scrollback::default());
        for n in 0..100_000 {
            scroll(&mut list, n, 1);
            assert!(
                list.memory() <= limit,
                "{} bytes after {n} lines",
                list.memory()
            );
        }
        assert_eq!(list.memory(), counted(&list));
        // It holds as much as the limit allows.
        assert!(list.memory() + Capacity::standard(80).bytes() > limit);
        let numbers: Vec<u32> = numbers(&list).into_iter().map(Option::unwrap).collect();
        let first = numbers[0];
        assert_eq!(numbers, (first..100_000).collect::<Vec<u32>>());
    }

    #[test]
    fn pages_that_grow_stay_within_the_limit_too() {
        let limit = Scrollback::default().limit;
        let mut list = PageList::new(80, 24, Scrollback::default());
        for n in 0..50_000 {
            styled_line(&mut list, n);
            assert!(
                list.memory() <= limit,
                "{} bytes after {n} lines",
                list.memory()
            );
        }
        assert_eq!(list.memory(), counted(&list));
        // Every row still has its own style.
        let (page, row) = list.screen_row(23);
        let style = page.styles().get(page.cells(row)[0].style());
        assert_eq!(style.fg, Color::Rgb(Rgb::new(0, 0xC3, 0x4F)));
    }

    #[test]
    fn a_page_that_grows_past_the_limit_makes_room() {
        let mut list = PageList::new(80, 24, pages(80, 3));
        let page_rows = u32::from(Capacity::standard(80).rows);
        // Three full pages: exactly at the limit.
        scroll(&mut list, 0, 3 * page_rows - 24);
        assert_eq!(list.memory(), list.scrollback().limit);
        list.grow_page(23, TableFull::Styles);
        assert!(list.memory() <= list.scrollback().limit);
        assert_eq!(list.pages(), 2, "the oldest page made room");
        assert_eq!(list.memory(), counted(&list));
        let (page, row) = list.screen_row(23);
        assert_eq!(page.cells(row)[0], numbered(3 * page_rows - 25));
    }

    #[test]
    fn panes_share_a_budget() {
        let budget = Arc::new(Budget::new(6 * Capacity::standard(80).bytes()));
        let shared = Scrollback {
            budget: Some(Arc::clone(&budget)),
            ..Scrollback::default()
        };
        let mut busy = PageList::new(80, 24, shared.clone());
        let mut idle = PageList::new(80, 24, shared);
        scroll(&mut idle, 0, 1000);
        let idle_memory = idle.memory();
        for n in 0..20_000 {
            scroll(&mut busy, n, 1);
            assert!(
                budget.used() <= budget.limit(),
                "{} bytes after {n} lines",
                budget.used()
            );
            assert_eq!(budget.used(), busy.memory() + idle.memory());
        }
        // The busy pane made room with its own pages, not the idle one's.
        assert_eq!(idle.memory(), idle_memory);
        assert_eq!(numbers(&idle).last(), Some(&Some(999)));
        drop(busy);
        assert_eq!(budget.used(), idle.memory());
    }

    #[test]
    fn a_pane_gives_back_its_scrollback_when_a_screen_passes_the_budget() {
        let page = Capacity::standard(80).bytes();
        let budget = Arc::new(Budget::new(3 * page));
        let shared = Scrollback {
            budget: Some(Arc::clone(&budget)),
            ..Scrollback::default()
        };
        let mut first = PageList::new(80, 24, shared.clone());
        let page_rows = u32::from(Capacity::standard(80).rows);
        scroll(&mut first, 0, 3 * page_rows - 24);
        assert_eq!(budget.used(), budget.limit());
        // A new pane's screen gets its page past the budget.
        let second = PageList::new(80, 24, shared);
        assert_eq!(budget.used(), 4 * page);
        // The first pane gives back its own scrollback when it next needs
        // a page.
        scroll(&mut first, 3 * page_rows, 1);
        assert_eq!(budget.used(), budget.limit());
        assert_eq!(budget.used(), first.memory() + second.memory());
    }

    #[test]
    fn panes_on_several_threads_share_a_budget() {
        let budget = Arc::new(Budget::new(8 * Capacity::standard(80).bytes()));
        std::thread::scope(|scope| {
            for _ in 0..4 {
                let scrollback = Scrollback {
                    budget: Some(Arc::clone(&budget)),
                    ..Scrollback::default()
                };
                let budget = &budget;
                scope.spawn(move || {
                    let mut list = PageList::new(80, 24, scrollback);
                    // A screen of 24 rows needs at most two pages, which can
                    // pass the budget while other panes hold it.
                    let screens = 4 * 2 * Capacity::standard(80).bytes();
                    for n in 0..20_000 {
                        scroll(&mut list, n, 1);
                        assert!(budget.used() <= budget.limit() + screens);
                    }
                });
            }
        });
        assert_eq!(budget.used(), 0, "every list gave its pages back");
    }

    #[test]
    fn the_screen_stays_whatever_the_limits() {
        let mut list = PageList::new(80, 24, pages(80, 0));
        scroll(&mut list, 0, 2000);
        let screen: Vec<Option<u32>> = numbers(&list)[list.scrollback_rows()..].to_vec();
        assert_eq!(screen, (1976..2000).map(Some).collect::<Vec<_>>());
        assert!(list.pages() <= 2);
    }
}
