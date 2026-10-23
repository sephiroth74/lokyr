//! Scrolling through the page list, and printing text that scrolls,
//! allocate nothing once the pool of pages is warm, the pages take the
//! memory that they count, and a title costs no allocation a byte, as an
//! allocator that counts shows.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell as Count;

use bolt_vt_core::page::{Capacity, Cell, Content, Page, PageList, Rgb, Scrollback, TableFull};
use bolt_vt_core::parser::Parser;
use bolt_vt_core::terminal::{Options, Terminal};
use bolt_vt_core::unicode::Normalization;

/// The system allocator, counting the allocations of each thread and the
/// bytes that they hold, so that tests that run in parallel do not count
/// each other's.
struct Counting;

thread_local! {
    static ALLOCATIONS: Count<usize> = const { Count::new(0) };
    static LIVE: Count<isize> = const { Count::new(0) };
}

fn count(bytes: isize) {
    ALLOCATIONS.with(|count| count.set(count.get() + 1));
    held(bytes);
}

fn held(bytes: isize) {
    LIVE.with(|live| live.set(live.get() + bytes));
}

fn allocations() -> usize {
    ALLOCATIONS.with(Count::get)
}

/// The bytes that this thread's allocations hold now.
fn live() -> isize {
    LIVE.with(Count::get)
}

/// A size of an allocation, as the counts take it.
fn size(bytes: usize) -> isize {
    isize::try_from(bytes).expect("an allocation fits in an isize")
}

// SAFETY: every method forwards its arguments to `System`, which implements
// `GlobalAlloc` soundly; counting allocates nothing.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        count(size(layout.size()));
        // SAFETY: the caller upholds the contract of `alloc`.
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        count(size(layout.size()));
        // SAFETY: the caller upholds the contract of `alloc_zeroed`.
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        count(size(new_size) - size(layout.size()));
        // SAFETY: the caller upholds the contract of `realloc`.
        unsafe { System.realloc(ptr, layout, new_size) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        held(-size(layout.size()));
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
    // emoji, then the end of the line.
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

/// A terminal of 80 by 24 whose scrollback takes four standard pages at
/// most, with `normalization`.
fn small(normalization: Normalization) -> Terminal {
    let scrollback = Scrollback {
        limit: 4 * Capacity::standard(80).bytes(),
        budget: None,
    };
    Terminal::new(
        80,
        24,
        Options {
            scrollback,
            normalization,
            ..Options::default()
        },
    )
}

/// How many times printing 20 000 rows of `text`, over and over,
/// allocates, once five pages of them warmed the pool.
fn allocations_printing(terminal: &mut Terminal, text: &str) -> usize {
    let mut parser = Parser::new();
    let page_rows = usize::from(Capacity::standard(80).rows);
    let rows = text.matches('\n').count().max(1);
    let warm = text.repeat((5 * page_rows).div_ceil(rows));
    parser.advance(terminal, warm.as_bytes());
    let lines = text.repeat(20_000_usize.div_ceil(rows));
    let before = allocations();
    parser.advance(terminal, lines.as_bytes());
    allocations() - before
}

#[test]
fn printing_text_with_combining_marks_allocates_nothing_once_the_pool_is_warm() {
    // Decomposed text, as macOS writes file names: a mark on each row
    // fills the table of a page's clusters, which grows.
    let line = format!("{}e\u{301}\r\n", "x".repeat(70));
    let mut terminal = small(Normalization::None);
    assert_eq!(allocations_printing(&mut terminal, &line), 0);
    assert!(terminal.row_text(22).ends_with("e\u{301}"));
    // With normalization, clusters of four marks at most, which the
    // reordering keeps without allocating.
    let line = format!("{}a\u{316}\u{301}\u{302}\u{303}\r\n", "x".repeat(70));
    let mut terminal = small(Normalization::Nfc);
    assert_eq!(allocations_printing(&mut terminal, &line), 0);
    // A cluster of six marks on one row in eight: fewer clusters than a
    // page has room for, whose codepoints take more of its arena.
    let plain = format!("{}\r\n", "x".repeat(79));
    let text = format!(
        "{}{}e\u{301}\u{302}\u{303}\u{304}\u{305}\u{306}\r\n",
        plain.repeat(7),
        "x".repeat(70)
    );
    let mut terminal = small(Normalization::None);
    assert_eq!(allocations_printing(&mut terminal, &text), 0);
}

#[test]
fn printing_text_of_more_styles_than_a_page_has_allocates_nothing_once_the_pool_is_warm() {
    // 300 colors, one a row, over and over, which grow the table of each
    // page's styles.
    let text: String = (0..300u32)
        .map(|n| {
            format!(
                "\x1b[38;2;{};{};9m{}\x1b[m\r\n",
                n % 256,
                n / 256,
                "x".repeat(79)
            )
        })
        .collect();
    let mut terminal = small(Normalization::None);
    assert_eq!(allocations_printing(&mut terminal, &text), 0);
}

#[test]
fn printing_linked_text_allocates_nothing_once_the_pool_is_warm() {
    // Two names a row, as `ls --hyperlink` prints them, ten linked cells
    // that fill the table of a page's links, which grows.
    let line = "\x1b]8;;file:///home/a.txt\x1b\\a.txt\x1b]8;;\x1b\\  \
                \x1b]8;;file:///home/b.txt\x1b\\b.txt\x1b]8;;\x1b\\\r\n";
    let mut terminal = small(Normalization::None);
    assert_eq!(allocations_printing(&mut terminal, line), 0);
    assert_eq!(terminal.row_text(22), "a.txt  b.txt");
    // A link on one row in ten, of 40 targets: more links than a page has
    // room for, and few linked cells.
    let linked = |targets: u32, uri: &dyn Fn(u32) -> String| -> String {
        (0..targets * 10)
            .map(|n| match n % 10 {
                0 => format!("\x1b]8;;{}\x1b\\a\x1b]8;;\x1b\\\r\n", uri(n / 10)),
                _ => "x\r\n".to_owned(),
            })
            .collect()
    };
    let text = linked(40, &|n| format!("file:///{n}"));
    let mut terminal = small(Normalization::None);
    assert_eq!(allocations_printing(&mut terminal, &text), 0);
    // Of 20 targets of 200 bytes: fewer links than a page has room for,
    // whose targets take more of its arena.
    let text = linked(20, &|n| format!("file:///{n}{}", "a".repeat(190)));
    let mut terminal = small(Normalization::None);
    assert_eq!(allocations_printing(&mut terminal, &text), 0);
}

#[test]
fn a_long_title_costs_no_allocation_a_byte() {
    // The report of a title in hexadecimal took an allocation a byte, as
    // the title changed, whether reports were on or off.
    for title_reports in [false, true] {
        let options = Options {
            title_reports,
            ..Options::default()
        };
        let mut terminal = Terminal::new(80, 24, options);
        let mut parser = Parser::new();
        parser.advance(&mut terminal, b"\x1b[>1t");
        let title = format!("\x1b]2;{}\x07", "t".repeat(60_000));
        let before = allocations();
        parser.advance(&mut terminal, title.as_bytes());
        let made = allocations() - before;
        assert!(
            made < 100,
            "{made} allocations, with reports {title_reports}"
        );
        assert_eq!(terminal.title().len(), 60_000, "the title was set");
    }
}

#[test]
fn a_page_takes_about_8_bytes_a_cell_and_what_it_counts() {
    for cols in [20, 80, 132, 200, 500] {
        let capacity = Capacity::standard(cols);
        let cells = usize::from(capacity.rows) * usize::from(cols);
        let per_cell = capacity.bytes() as f64 / cells as f64;
        // A cell's 8 bytes, with its share of its row's and of the tables
        // of styles, clusters and links: five bytes a row, which weigh most
        // on narrow rows.
        assert!((8.0..8.7).contains(&per_cell), "{cols} columns: {per_cell}");
        let before = live();
        let page = Page::new(capacity);
        let held = live() - before;
        // Its header is here on the stack, and in a list's slab.
        let header = size(size_of::<Page>());
        assert_eq!(held + header, size(page.memory()), "{cols} columns");
        assert_eq!(page.memory(), capacity.bytes(), "{cols} columns");
        // A page whose tables grew too.
        let grown = capacity.grown_for(TableFull::Styles).unwrap();
        let before = live();
        let page = Page::new(grown);
        assert_eq!(
            live() - before + header,
            size(page.memory()),
            "{cols} columns"
        );
        assert_eq!(page.memory(), grown.bytes(), "{cols} columns");
    }
}

#[test]
fn a_burst_of_styled_text_leaves_the_scrollback_its_rows_once_plain_text_follows() {
    // A style a cell, as image viewers and gradients print, which grows the
    // pages' tables of styles as far as they go.
    let burst: String = (0..4_000u32)
        .map(|line| {
            let cells: String = (0..79u32)
                .map(|x| {
                    let n = line * 79 + x;
                    format!(
                        "\x1b[38;2;{};{};{}mx",
                        n % 256,
                        n / 256 % 256,
                        n / 65536 % 256
                    )
                })
                .collect();
            format!("{cells}\x1b[m\r\n")
        })
        .collect();
    let plain = format!("{}\r\n", "x".repeat(79)).repeat(100_000);
    let rows = |text: &[&str]| {
        let mut terminal = Terminal::new(80, 24, Options::default());
        let mut parser = Parser::new();
        for part in text {
            parser.advance(&mut terminal, part.as_bytes());
        }
        terminal.screen_rows().scrollback_rows()
    };
    let fresh = rows(&[&plain]);
    let after = rows(&[&burst, &plain]);
    assert!(
        after + 2 * usize::from(Capacity::standard(80).rows) >= fresh,
        "{after} rows against {fresh}"
    );
}

#[test]
fn a_full_scrollback_stays_within_its_limit_and_5_percent() {
    let limit = 8 * Capacity::standard(80).bytes();
    // Plain text, colored text, and text with marks and links, which grow
    // the pages' tables.
    let plain = format!("{}\r\n", "x".repeat(79));
    let colored: String = (0..8u8)
        .map(|n| format!("\x1b[38;2;{n};{};9m{}", n * 3, "y".repeat(9)))
        .chain(["\x1b[m\r\n".to_owned()])
        .collect();
    let marked = format!("{}e\u{301}\r\n", "x".repeat(70));
    let linked =
        "\x1b]8;;file:///a\x1b\\a\x1b]8;;\x1b\\ \x1b]8;;file:///b\x1b\\b\x1b]8;;\x1b\\\r\n";
    for (kind, line) in [
        ("plain", plain.as_str()),
        ("colored", &colored),
        ("marked", &marked),
        ("linked", linked),
    ] {
        let before = live();
        let mut terminal = Terminal::new(
            80,
            24,
            Options {
                scrollback: Scrollback {
                    limit,
                    budget: None,
                },
                ..Options::default()
            },
        );
        let mut parser = Parser::new();
        parser.advance(&mut terminal, line.repeat(100_000).as_bytes());
        let list = terminal.screen_rows();
        assert!(list.memory() <= limit, "{kind}: {} bytes", list.memory());
        // What the terminal holds, but for the alternate screen's page.
        let alternate = size(terminal.inactive_screen_rows().memory());
        let held = live() - before - alternate;
        assert!(
            held as f64 <= limit as f64 * 1.05,
            "{kind}: {held} bytes held against a limit of {limit}"
        );
        assert!(list.scrollback_rows() > 0, "{kind}");
    }
}
