//! Scrolling through the page list, and printing text that scrolls,
//! allocate nothing once the pool of pages is warm, as an allocator that
//! counts shows.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell as Count;

use bolt_vt_core::page::{Capacity, Cell, Content, PageList, Rgb, Scrollback};
use bolt_vt_core::parser::Parser;
use bolt_vt_core::terminal::{Options, Terminal};

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
    // Room for four pages.
    let pages = 4;
    let scrollback = Scrollback {
        limit: pages * Capacity::standard(80).bytes(),
        budget: None,
    };
    let mut list = PageList::new(80, 24, scrollback);
    let page_rows = u32::from(Capacity::standard(80).rows);

    // Enough lines to fill every page the list can hold, and to recycle one.
    let warm = (pages as u32 + 1) * page_rows;
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
    assert!(numbers.len() >= (pages - 1) * page_rows as usize);
}

#[test]
fn printing_colored_text_allocates_nothing_once_the_pool_is_warm() {
    let scrollback = Scrollback {
        limit: 4 * Capacity::standard(80).bytes(),
        budget: None,
    };
    let mut terminal = Terminal::new(
        80,
        24,
        Options {
            scrollback,
            ..Options::default()
        },
    );
    let mut parser = Parser::new();
    // Colored words, as `ls --color` prints them, wide characters and an
    // emoji, then the end of the line. No mark: a page whose grapheme table
    // fills up grows, and a grown page does not go back to the pool.
    let line = "\x1b[1;34msrc\x1b[0m  \x1b[38;5;208mmain.rs\x1b[0m  \x1b[38:2::1:2:3m日本語\x1b[m \u{1F600}\x1b[K\r\n";
    let page_rows = usize::from(Capacity::standard(80).rows);
    let warm = line.repeat(5 * page_rows);
    parser.advance(&mut terminal, warm.as_bytes());
    let lines = line.repeat(20_000);
    let before = allocations();
    parser.advance(&mut terminal, lines.as_bytes());
    assert_eq!(allocations() - before, 0, "printing allocated");
    assert_eq!(terminal.row_text(22), "src  main.rs  日本語 \u{1F600}");
}
