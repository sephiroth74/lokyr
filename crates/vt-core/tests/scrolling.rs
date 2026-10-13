//! Scrolling through the page list allocates nothing once its pool is warm,
//! as an allocator that counts shows.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell as Count;

use saetta_vt_core::page::{Capacity, Cell, Content, PageList, Rgb};

/// The system allocator, counting the allocations of each thread, so that
/// tests that run in parallel do not count each other's.
struct Counting;

thread_local! {
    static ALLOCATIONS: Count<usize> = const { Count::new(0) };
}

fn count() {
    ALLOCATIONS.with(|count| count.set(count.get() + 1));
}

fn allocations() -> usize {
    ALLOCATIONS.with(Count::get)
}

// SAFETY: every method forwards its arguments to `System`, which implements
// `GlobalAlloc` soundly; counting allocates nothing.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        count();
        // SAFETY: the caller upholds the contract of `alloc`.
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        count();
        // SAFETY: the caller upholds the contract of `alloc_zeroed`.
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        count();
        // SAFETY: the caller upholds the contract of `realloc`.
        unsafe { System.realloc(ptr, layout, new_size) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: the caller upholds the contract of `dealloc`.
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

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

#[test]
fn scrolling_a_million_lines_allocates_nothing_once_the_pool_is_warm() {
    let mut list = PageList::new(80, 24, 4);
    let page_rows = u32::from(Capacity::standard(80).rows);

    // Enough lines to fill every page the list can hold, and to recycle one.
    let warm = (list.max_pages() as u32 + 1) * page_rows;
    let start = allocations();
    scroll(&mut list, 0, warm);
    assert!(
        allocations() > start,
        "the counter sees the pages of the warm-up"
    );

    let before = allocations();
    let lines = 1_000_000;
    scroll(&mut list, warm, lines);
    assert_eq!(allocations() - before, 0, "scrolling allocated");

    // The scrollback keeps the latest lines, in order.
    let numbers: Vec<u32> = list
        .iter_rows()
        .map(|(page, row)| match page.cells(row)[0].content() {
            Content::BgRgb(Rgb { r, g, b }) => u32::from_be_bytes([0, r, g, b]),
            content => panic!("row without a number: {content:?}"),
        })
        .collect();
    let last = warm + lines - 1;
    let first = last + 1 - numbers.len() as u32;
    assert_eq!(numbers, (first..=last).collect::<Vec<u32>>());
    assert!(numbers.len() >= (list.max_pages() - 1) * page_rows as usize);
}
