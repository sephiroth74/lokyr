//! Regex search over a terminal's rows, and searches that another thread
//! cancels.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use bolt_search::{Cancelled, Case, Direction, Match, Query};
use bolt_vt_core::page::{PageList, PinId, Position, Scrollback};
use bolt_vt_core::parser::Parser;
use bolt_vt_core::terminal::{Options, Terminal};

fn printed(cols: u16, rows: u16, text: &str) -> Terminal {
    let mut terminal = Terminal::new(cols, rows, Options::default());
    Parser::new().advance(&mut terminal, text.as_bytes());
    terminal
}

fn at(terminal: &Terminal, y: u16, col: u16) -> Position {
    let (page, row) = terminal.screen_rows().screen_row_id(y);
    Position { page, row, col }
}

fn place(list: &PageList, pin: PinId) -> (u16, u16) {
    let (col, y) = list.pin_on_screen(pin).expect("on the screen");
    (y, col)
}

fn find(list: &mut PageList, query: &Query, from: Position, direction: Direction) -> Option<Match> {
    bolt_search::find(list, query, from, direction, &AtomicBool::new(false)).unwrap()
}

/// Where `query` matches, from row and column `from`, as two cells of the
/// screen.
fn search(
    terminal: &mut Terminal,
    query: &Query,
    from: (u16, u16),
    direction: Direction,
) -> Option<((u16, u16), (u16, u16))> {
    let from = at(terminal, from.0, from.1);
    let rows = terminal.screen_rows_mut();
    let found = find(rows, query, from, direction)?;
    let places = (place(rows, found.start), place(rows, found.end));
    found.release(rows);
    Some(places)
}

fn regex(pattern: &str) -> Query {
    Query::regex(pattern, Case::Sensitive).unwrap()
}

#[test]
fn a_regex_matches_across_wrapped_rows() {
    let mut terminal = printed(10, 3, "0123456789abcdef");
    let query = regex("8[0-9a-f]+c");
    assert_eq!(
        search(&mut terminal, &query, (0, 0), Direction::Forward),
        Some(((0, 8), (1, 2)))
    );
    assert_eq!(
        search(&mut terminal, &query, (2, 0), Direction::Backward),
        Some(((0, 8), (1, 2)))
    );
    assert_eq!(
        search(&mut terminal, &query, (0, 9), Direction::Forward),
        None
    );
}

#[test]
fn backward_finds_matches_that_overlap() {
    let mut terminal = printed(10, 2, "aaa");
    let query = regex("aa");
    assert_eq!(
        search(&mut terminal, &query, (1, 0), Direction::Backward),
        Some(((0, 1), (0, 2)))
    );
    assert_eq!(
        search(&mut terminal, &query, (0, 1), Direction::Backward),
        Some(((0, 0), (0, 1)))
    );
    assert_eq!(
        search(&mut terminal, &query, (0, 1), Direction::Forward),
        Some(((0, 1), (0, 2)))
    );
}

#[test]
fn an_insensitive_regex_folds_unicode() {
    let mut terminal = printed(20, 2, "Un ÉTÉ chaud");
    let unicode = Query::regex("été", Case::Insensitive).unwrap();
    assert_eq!(
        search(&mut terminal, &unicode, (0, 0), Direction::Forward),
        Some(((0, 3), (0, 5)))
    );
    // A literal folds ASCII only.
    let literal = Query::literal("été", Case::Insensitive).unwrap();
    assert_eq!(
        search(&mut terminal, &literal, (0, 0), Direction::Forward),
        None
    );
    // A pattern with capitals still sees the text as it is.
    let capitals = regex("[A-Z]+ ");
    assert_eq!(
        search(&mut terminal, &capitals, (0, 1), Direction::Forward),
        None
    );
    assert_eq!(
        search(&mut terminal, &regex("U[a-z]"), (0, 0), Direction::Forward),
        Some(((0, 0), (0, 1)))
    );
}

#[test]
fn whole_words_are_a_filter_on_the_matches() {
    let mut terminal = printed(30, 2, "cat concat cat_x cat.");
    for query in [
        Query::literal("cat", Case::Sensitive)
            .unwrap()
            .whole_word(true),
        regex("c.t").whole_word(true),
    ] {
        assert_eq!(
            search(&mut terminal, &query, (0, 0), Direction::Forward),
            Some(((0, 0), (0, 2)))
        );
        assert_eq!(
            search(&mut terminal, &query, (0, 1), Direction::Forward),
            Some(((0, 17), (0, 19)))
        );
        assert_eq!(
            search(&mut terminal, &query, (1, 0), Direction::Backward),
            Some(((0, 17), (0, 19)))
        );
        assert_eq!(
            search(&mut terminal, &query, (0, 17), Direction::Backward),
            Some(((0, 0), (0, 2)))
        );
    }
    let any = Query::literal("cat", Case::Sensitive).unwrap();
    assert_eq!(
        search(&mut terminal, &any, (0, 1), Direction::Forward),
        Some(((0, 7), (0, 9)))
    );
    // Backward, a whole word can start a character before a match that is
    // not one, and overlap it.
    let mut terminal = printed(10, 2, "ab");
    let query = regex("b|ab").whole_word(true);
    assert_eq!(
        search(&mut terminal, &query, (1, 0), Direction::Backward),
        Some(((0, 0), (0, 1)))
    );
    // Words in other scripts.
    let mut terminal = printed(30, 2, "été étés");
    let query = Query::literal("été", Case::Sensitive)
        .unwrap()
        .whole_word(true);
    assert_eq!(
        search(&mut terminal, &query, (0, 1), Direction::Forward),
        None
    );
}

#[test]
fn unicode_word_boundaries_work_on_text_that_is_not_ascii() {
    let mut terminal = printed(20, 2, "un étés été chaud");
    let query = regex(r"\bété\b");
    assert_eq!(
        search(&mut terminal, &query, (0, 0), Direction::Forward),
        Some(((0, 8), (0, 10)))
    );
}

#[test]
fn empty_matches_do_not_count_and_anchors_hold_to_the_line() {
    let mut terminal = printed(10, 3, "aaxx\r\nxxabc\r\nabc");
    assert_eq!(
        search(&mut terminal, &regex("x*"), (0, 0), Direction::Forward),
        Some(((0, 2), (0, 3)))
    );
    assert_eq!(
        search(&mut terminal, &regex("^abc"), (0, 0), Direction::Forward),
        Some(((2, 0), (2, 2)))
    );
    assert_eq!(
        search(&mut terminal, &regex("x$"), (0, 0), Direction::Forward),
        Some(((0, 3), (0, 3)))
    );
    // Searching from the middle of a line does not make it its start.
    assert_eq!(
        search(&mut terminal, &regex("^abc"), (1, 2), Direction::Forward),
        Some(((2, 0), (2, 2)))
    );
}

#[test]
fn a_pattern_that_does_not_compile_is_an_error() {
    let error = Query::regex("(unclosed", Case::Sensitive).unwrap_err();
    assert!(error.to_string().contains("does not compile"), "{error}");
}

#[test]
fn another_thread_cancels_a_running_search() {
    // A long scrollback without a match, searched again and again until
    // the flag stops a search.
    let options = Options {
        scrollback: Scrollback {
            limit: 64 << 20,
            budget: None,
        },
        ..Options::default()
    };
    let mut terminal = Terminal::new(120, 24, options);
    let line = format!("{}\r\n", "lorem ipsum dolor sit amet ".repeat(4));
    Parser::new().advance(&mut terminal, line.repeat(20_000).as_bytes());
    let query = Query::regex("[0-9]{3}x", Case::Sensitive).unwrap();
    // How long a whole search takes, in this build.
    let whole = {
        let from = at(&terminal, 23, 0);
        let started = Instant::now();
        let rows = terminal.screen_rows_mut();
        let found = bolt_search::find(
            rows,
            &query,
            from,
            Direction::Backward,
            &AtomicBool::new(false),
        );
        assert_eq!(found, Ok(None));
        started.elapsed()
    };
    let cancel = Arc::new(AtomicBool::new(false));
    let searcher = {
        let cancel = Arc::clone(&cancel);
        std::thread::spawn(move || {
            loop {
                let from = at(&terminal, 23, 0);
                let rows = terminal.screen_rows_mut();
                match bolt_search::find(rows, &query, from, Direction::Backward, &cancel) {
                    Ok(None) => {}
                    Ok(Some(_)) => panic!("nothing to find"),
                    Err(Cancelled) => return Instant::now(),
                }
            }
        })
    };
    // Halfway through a search, as likely as not.
    std::thread::sleep(whole / 2);
    let asked = Instant::now();
    cancel.store(true, Ordering::Relaxed);
    let stopped = searcher.join().unwrap();
    // A search that did not stop would have gone on to its end.
    let took = stopped.duration_since(asked);
    assert!(
        took < whole / 4,
        "stopped {took:?} after the flag; a search takes {whole:?}"
    );
    assert_eq!(Cancelled.to_string(), "the search was cancelled");
}

/// The first cell of every match, going forward from each match's next
/// cell.
fn starts(terminal: &mut Terminal, query: &Query, cols: u16) -> Vec<Position> {
    let mut from = {
        let (page, row) = terminal.screen_rows().row_at(0).unwrap();
        Position { page, row, col: 0 }
    };
    let mut found = Vec::new();
    while let Some(hit) = find(terminal.screen_rows_mut(), query, from, Direction::Forward) {
        let rows = terminal.screen_rows_mut();
        let start = rows.pin_position(hit.start).unwrap();
        hit.release(rows);
        found.push(start);
        from = if start.col + 1 < cols {
            Position {
                col: start.col + 1,
                ..start
            }
        } else {
            let index = rows.row_index(start.page, start.row).unwrap() + 1;
            match rows.row_at(index) {
                Some((page, row)) => Position { page, row, col: 0 },
                None => break,
            }
        };
    }
    found
}

/// The first cell of every match, going backward from each match's start,
/// from the last row, which is empty, in the order of the rows.
fn starts_backward(terminal: &mut Terminal, query: &Query) -> Vec<Position> {
    let rows = terminal.screen_rows();
    let (page, row) = rows.row_at(rows.total_rows() - 1).unwrap();
    let mut from = Position { page, row, col: 0 };
    let mut found = Vec::new();
    while let Some(hit) = find(terminal.screen_rows_mut(), query, from, Direction::Backward) {
        let rows = terminal.screen_rows_mut();
        from = rows.pin_position(hit.start).unwrap();
        hit.release(rows);
        found.push(from);
    }
    found.reverse();
    found
}

proptest::proptest! {
    #![proptest_config(proptest::prelude::ProptestConfig::with_cases(256))]

    /// A regex of plain letters finds what the same literal finds, whole
    /// words or not, either way.
    #[test]
    fn a_regex_of_letters_finds_what_the_literal_finds(
        lines in proptest::collection::vec("[ab .]{0,30}", 1..16),
        needle in "[ab]{1,3}",
        cols in 3_u16..12,
        whole_word in proptest::bool::ANY,
    ) {
        let mut text = String::new();
        for line in &lines {
            text.push_str(line);
            text.push_str("\r\n");
        }
        let mut terminal = printed(cols, 4, &text);
        let literal = Query::literal(&needle, Case::Sensitive).unwrap().whole_word(whole_word);
        let regex = Query::regex(&needle, Case::Sensitive).unwrap().whole_word(whole_word);
        let by_literal = starts(&mut terminal, &literal, cols);
        let by_regex = starts(&mut terminal, &regex, cols);
        proptest::prop_assert_eq!(&by_literal, &by_regex);
        proptest::prop_assert_eq!(&starts_backward(&mut terminal, &literal), &by_literal);
        proptest::prop_assert_eq!(&starts_backward(&mut terminal, &regex), &by_literal);
        proptest::prop_assert_eq!(terminal.screen_rows().pins(), 0);
    }
}

#[test]
fn an_insensitive_regex_sees_the_text_as_it_is() {
    // A group that asks for case again matches the text's own.
    let mut terminal = printed(10, 2, "Un un");
    let query = Query::regex("(?-i:U)N", Case::Insensitive).unwrap();
    assert_eq!(
        search(&mut terminal, &query, (0, 0), Direction::Forward),
        Some(((0, 0), (0, 1)))
    );
    assert_eq!(
        search(&mut terminal, &query, (0, 1), Direction::Forward),
        None
    );
}

#[test]
fn a_search_stops_inside_a_long_line() {
    // One logical line of 64 000 letters, on a screen that holds it: tried
    // from each of its starts, the matches of `a+` take minutes to go
    // through.
    let mut terminal = printed(80, 801, &format!("{}b", "a".repeat(63_999)));
    let searches = [
        // Forward, a whole word that no match is, as a letter follows each.
        (regex("a+").whole_word(true), (0, 0), Direction::Forward),
        // Backward, every match from the line's start, for the last one.
        (regex("a+"), (800, 0), Direction::Backward),
    ];
    for (query, from, direction) in searches {
        let from = at(&terminal, from.0, from.1);
        let cancel = Arc::new(AtomicBool::new(false));
        let flag = {
            let cancel = Arc::clone(&cancel);
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(100));
                cancel.store(true, Ordering::Relaxed);
                Instant::now()
            })
        };
        let rows = terminal.screen_rows_mut();
        let found = bolt_search::find(rows, &query, from, direction, &cancel);
        let stopped = Instant::now();
        let asked = flag.join().unwrap();
        assert_eq!(found, Err(Cancelled), "{direction:?}");
        let took = stopped.duration_since(asked);
        assert!(
            took < Duration::from_secs(2),
            "{direction:?}: stopped {took:?} after the flag"
        );
    }
}

#[test]
fn a_cancelled_search_stops_either_way() {
    let mut terminal = printed(10, 2, "abc");
    let from = at(&terminal, 0, 0);
    let query = regex("b");
    let cancelled = AtomicBool::new(true);
    for direction in [Direction::Forward, Direction::Backward] {
        let rows = terminal.screen_rows_mut();
        assert_eq!(
            bolt_search::find(rows, &query, from, direction, &cancelled),
            Err(Cancelled)
        );
    }
}
