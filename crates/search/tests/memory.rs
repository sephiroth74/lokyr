//! The memory that a search takes for a long line, counted by an
//! allocator of its own: the test is the only one in its binary, so that
//! nothing else allocates meanwhile.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use bolt_search::{Case, Direction, Query};
use bolt_vt_core::page::{Position, Scrollback};
use bolt_vt_core::parser::Parser;
use bolt_vt_core::terminal::{Options, Terminal};

/// The system's allocator, counting the bytes in use and their peak.
struct Counting;

static IN_USE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

// SAFETY: every call goes to the system's allocator, with the same layout;
// the counts change nothing that it does.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let in_use = IN_USE.fetch_add(layout.size(), Ordering::Relaxed) + layout.size();
        PEAK.fetch_max(in_use, Ordering::Relaxed);
        // SAFETY: the caller's contract, passed on.
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        IN_USE.fetch_sub(layout.size(), Ordering::Relaxed);
        // SAFETY: the caller's contract, passed on.
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

#[test]
fn a_long_line_takes_a_few_bytes_a_cell() {
    // One logical line of 400 000 cells, which the scrollback keeps.
    let options = Options {
        scrollback: Scrollback {
            limit: 64 << 20,
            budget: None,
        },
        ..Options::default()
    };
    let mut terminal = Terminal::new(80, 24, options);
    let cells = 400_000;
    Parser::new().advance(&mut terminal, "x".repeat(cells).as_bytes());
    // The scrollback keeps the line whole.
    assert!(terminal.screen_rows().total_rows() >= cells / 80);
    let (page, row) = terminal.screen_rows().screen_row_id(23);
    let from = Position { page, row, col: 0 };
    let query = Query::literal("needle", Case::Sensitive).unwrap();
    let before = IN_USE.load(Ordering::Relaxed);
    PEAK.store(before, Ordering::Relaxed);
    let rows = terminal.screen_rows_mut();
    let found = bolt_search::find(
        rows,
        &query,
        from,
        Direction::Backward,
        &AtomicBool::new(false),
    );
    assert_eq!(found, Ok(None));
    let taken = PEAK.load(Ordering::Relaxed) - before;
    // The text's byte, where the cell's text starts and its column, about 7
    // bytes, as the vectors grow: a layout twice as large fails.
    assert!(taken < 16 * cells, "{} bytes a cell", taken / cells);
}
