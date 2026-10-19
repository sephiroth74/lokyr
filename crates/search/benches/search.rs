//! The time that a search takes over a full scrollback: of literal text and
//! of a regex that match nowhere, so that every row is read, over 100 000
//! and 1 000 000 lines, and over one logical line of 1 000 000 cells, which
//! each search builds again; and the time that `find_all` takes to go
//! through 100 000 lines with a match each.

use std::hint::black_box;
use std::sync::atomic::AtomicBool;

use bolt_search::{Case, Direction, Query};
use bolt_vt_core::page::{Position, Scrollback};
use bolt_vt_core::parser::Parser;
use bolt_vt_core::terminal::{Options, Terminal};
use criterion::{Criterion, criterion_group, criterion_main};

/// The width of the terminal.
const COLS: u16 = 120;

/// A terminal of 120 by 40 that `lines` of text went through, varied lines
/// of 40 to 80 characters, the scrollback compacted as an idle terminal's
/// is, a hundred thousand lines at a time, so that a million lines fit in
/// some hundreds of MiB.
fn scrollback(lines: usize) -> Terminal {
    let options = Options {
        scrollback: Scrollback {
            limit: 4 << 30,
            budget: None,
        },
        ..Options::default()
    };
    let mut terminal = Terminal::new(COLS, 40, options);
    let mut parser = Parser::new();
    let words = [
        "quick",
        "brown",
        "lazy",
        "dog",
        "error",
        "warning",
        "build",
        "src/main.rs",
    ];
    for start in (0..lines).step_by(100_000) {
        let mut text = String::new();
        for n in start..(start + 100_000).min(lines) {
            let word = words[n % words.len()];
            let tail = "x".repeat(n % 41);
            text.push_str(&format!(
                "{n:08} the {word} fox jumps over line {} {tail}\r\n",
                n * 7
            ));
        }
        parser.advance(&mut terminal, text.as_bytes());
        terminal.compact_scrollback();
    }
    terminal
}

/// A terminal whose scrollback holds one logical line of `cells` cells.
fn long_line(cells: usize) -> Terminal {
    let options = Options {
        scrollback: Scrollback {
            limit: 1 << 30,
            budget: None,
        },
        ..Options::default()
    };
    let mut terminal = Terminal::new(COLS, 40, options);
    let text: String = (0..cells)
        .map(|n| char::from(b'a' + (n % 26) as u8))
        .collect();
    Parser::new().advance(&mut terminal, text.as_bytes());
    terminal
}

/// Searches `terminal`'s rows backward from the screen's last row, and
/// finds nothing.
fn search_back(terminal: &mut Terminal, query: &Query) {
    let (page, row) = terminal.screen_rows().screen_row_id(39);
    let from = Position { page, row, col: 0 };
    let found = bolt_search::find(
        terminal.screen_rows_mut(),
        query,
        from,
        Direction::Backward,
        &AtomicBool::new(false),
    );
    assert_eq!(black_box(found), Ok(None));
}

fn search(c: &mut Criterion) {
    let literal = Query::literal("needle", Case::Sensitive).unwrap();
    let regex = Query::regex(r"line \d+ needle", Case::Sensitive).unwrap();
    let mut group = c.benchmark_group("search");
    group.sample_size(10);
    for lines in [100_000, 1_000_000] {
        let mut terminal = scrollback(lines);
        group.bench_function(format!("literal, {lines} lines"), |b| {
            b.iter(|| search_back(&mut terminal, &literal));
        });
        group.bench_function(format!("regex, {lines} lines"), |b| {
            b.iter(|| search_back(&mut terminal, &regex));
        });
    }
    let mut terminal = long_line(1_000_000);
    group.bench_function("literal, a line of 1000000 cells", |b| {
        b.iter(|| search_back(&mut terminal, &literal));
    });
    group.bench_function("regex, a line of 1000000 cells", |b| {
        b.iter(|| search_back(&mut terminal, &regex));
    });
    // A match on every line, the first thousand pinned.
    let mut terminal = scrollback(100_000);
    let fox = Query::literal("fox", Case::Sensitive).unwrap();
    group.bench_function("find_all, 100000 lines", |b| {
        b.iter(|| {
            let rows = terminal.screen_rows_mut();
            let all = 0..rows.total_rows();
            let found = bolt_search::find_all(rows, &fox, all, 1000, &AtomicBool::new(false));
            let matches = found.unwrap();
            assert!(matches.count >= 100_000);
            for found in matches.pinned {
                found.release(terminal.screen_rows_mut());
            }
        });
    });
    group.finish();
}

criterion_group!(benches, search);
criterion_main!(benches);
