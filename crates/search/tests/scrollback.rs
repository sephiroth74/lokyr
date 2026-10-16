//! A search of cold pages leaves the scrollback as it was: its rows, its
//! memory, and the share of a budget that other panes count on.

use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use bolt_search::{Case, Direction, Query};
use bolt_vt_core::page::{Budget, Capacity, Position, Scrollback};
use bolt_vt_core::parser::Parser;
use bolt_vt_core::terminal::{Options, Terminal};

struct Pane {
    terminal: Terminal,
    parser: Parser,
}

impl Pane {
    fn new(scrollback: Scrollback) -> Self {
        let options = Options {
            scrollback,
            ..Options::default()
        };
        Self {
            terminal: Terminal::new(80, 24, options),
            parser: Parser::new(),
        }
    }

    /// Prints `pages` pages of short lines, which compacting shrinks.
    fn print(&mut self, pages: usize) {
        let lines = "x\r\n".repeat(pages * page_rows());
        self.parser.advance(&mut self.terminal, lines.as_bytes());
    }

    fn rows(&self) -> usize {
        self.terminal.screen_rows().total_rows()
    }

    fn memory(&self) -> usize {
        self.terminal.screen_rows().memory()
    }

    /// Searches every row, backward from the screen's last, for text that
    /// is not there.
    fn search_everything(&mut self) {
        let (page, row) = self.terminal.screen_rows().screen_row_id(23);
        let from = Position { page, row, col: 0 };
        let query = Query::literal("needle", Case::Sensitive).unwrap();
        let rows = self.terminal.screen_rows_mut();
        let found = bolt_search::find(
            rows,
            &query,
            from,
            Direction::Backward,
            &AtomicBool::new(false),
        );
        assert_eq!(found, Ok(None));
    }
}

fn page_bytes() -> usize {
    Capacity::standard(80).bytes()
}

fn page_rows() -> usize {
    usize::from(Capacity::standard(80).rows)
}

#[test]
fn a_searched_scrollback_keeps_its_rows() {
    let scrollback = Scrollback {
        limit: 40 * page_bytes(),
        budget: None,
    };
    let mut panes = [Pane::new(scrollback.clone()), Pane::new(scrollback)];
    for pane in &mut panes {
        // A full scrollback, compacted, then the room that this made, used.
        pane.print(40);
        assert!(pane.terminal.compact_scrollback() > 0);
        pane.print(20);
    }
    let [control, searched] = &mut panes;
    let memory = searched.memory();
    searched.search_everything();
    assert_eq!(searched.memory(), memory, "the search took no memory");
    control.print(1);
    searched.print(1);
    assert_eq!(searched.rows(), control.rows(), "the search lost no rows");
}

#[test]
fn a_search_leaves_other_panes_their_scrollback() {
    let budget = Arc::new(Budget::new(60 * page_bytes()));
    let shared = Scrollback {
        limit: 100 * page_bytes(),
        budget: Some(Arc::clone(&budget)),
    };
    let mut searched = Pane::new(shared.clone());
    let mut other = Pane::new(shared);
    searched.print(50);
    assert!(searched.terminal.compact_scrollback() > 0);
    other.print(40);
    let used = budget.used();
    searched.search_everything();
    assert_eq!(
        budget.used(),
        used,
        "the search took nothing from the budget"
    );
    // Within the budget, the other pane's next page costs it no rows.
    let rows = other.rows();
    other.print(1);
    assert_eq!(other.rows(), rows + page_rows());
}
