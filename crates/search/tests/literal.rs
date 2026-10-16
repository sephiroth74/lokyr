//! Literal search over a terminal's rows, as programs print them.

use std::sync::atomic::AtomicBool;

use bolt_search::{Case, Direction, Match, Query};
use bolt_vt_core::page::{Content, Position};
use bolt_vt_core::parser::Parser;
use bolt_vt_core::terminal::{Options, Terminal};
use proptest::prelude::*;

/// Finds `query`, without cancelling.
fn find(
    list: &mut bolt_vt_core::page::PageList,
    query: &Query,
    from: Position,
    direction: Direction,
) -> Option<Match> {
    bolt_search::find(list, query, from, direction, &AtomicBool::new(false)).unwrap()
}

fn printed(cols: u16, rows: u16, text: &str) -> Terminal {
    let mut terminal = Terminal::new(cols, rows, Options::default());
    Parser::new().advance(&mut terminal, text.as_bytes());
    terminal
}

/// The cell at column `col` of row `y` of the screen.
fn at(terminal: &Terminal, y: u16, col: u16) -> Position {
    let (page, row) = terminal.screen_rows().screen_row_id(y);
    Position { page, row, col }
}

/// The screen's row and column of a pin.
fn place(terminal: &Terminal, pin: bolt_vt_core::page::PinId) -> Option<(u16, u16)> {
    let rows = terminal.screen_rows();
    let position = rows.pin_position(pin)?;
    rows.pin_on_screen(pin).map(|(col, y)| {
        assert_eq!(col, position.col);
        (y, col)
    })
}

fn search(
    terminal: &mut Terminal,
    text: &str,
    case: Case,
    from: Position,
    direction: Direction,
) -> Option<((u16, u16), (u16, u16))> {
    let literal = Query::literal(text, case).unwrap();
    let found = find(terminal.screen_rows_mut(), &literal, from, direction)?;
    let places = (place(terminal, found.start)?, place(terminal, found.end)?);
    found.release(terminal.screen_rows_mut());
    Some(places)
}

fn forward(
    terminal: &mut Terminal,
    text: &str,
    from: (u16, u16),
) -> Option<((u16, u16), (u16, u16))> {
    let from = at(terminal, from.0, from.1);
    search(terminal, text, Case::Sensitive, from, Direction::Forward)
}

fn backward(
    terminal: &mut Terminal,
    text: &str,
    from: (u16, u16),
) -> Option<((u16, u16), (u16, u16))> {
    let from = at(terminal, from.0, from.1);
    search(terminal, text, Case::Sensitive, from, Direction::Backward)
}

#[test]
fn matches_are_found_forward_and_backward() {
    let mut terminal = printed(20, 3, "one two one");
    assert_eq!(
        forward(&mut terminal, "one", (0, 0)),
        Some(((0, 0), (0, 2)))
    );
    assert_eq!(
        forward(&mut terminal, "one", (0, 1)),
        Some(((0, 8), (0, 10)))
    );
    assert_eq!(forward(&mut terminal, "one", (0, 9)), None);
    assert_eq!(
        backward(&mut terminal, "one", (2, 0)),
        Some(((0, 8), (0, 10)))
    );
    assert_eq!(
        backward(&mut terminal, "one", (0, 8)),
        Some(((0, 0), (0, 2)))
    );
    assert_eq!(backward(&mut terminal, "one", (0, 0)), None);
    assert_eq!(forward(&mut terminal, "three", (0, 0)), None);
    assert!(Query::literal("", Case::Sensitive).is_none());
}

#[test]
fn a_match_crosses_rows_that_wrap_into_each_other() {
    let mut terminal = printed(10, 3, "0123456789abcdef");
    assert_eq!(
        forward(&mut terminal, "89ab", (0, 0)),
        Some(((0, 8), (1, 1)))
    );
    assert_eq!(
        backward(&mut terminal, "89ab", (2, 0)),
        Some(((0, 8), (1, 1)))
    );
    // Backward from inside the match's first row: the match goes on into
    // the next row of the line.
    assert_eq!(
        backward(&mut terminal, "89ab", (0, 9)),
        Some(((0, 8), (1, 1)))
    );
    // From the second row of the line, backward, the match starts before.
    assert_eq!(
        backward(&mut terminal, "89ab", (1, 0)),
        Some(((0, 8), (1, 1)))
    );
    assert_eq!(forward(&mut terminal, "89ab", (1, 0)), None);
    // Rows that a line break ended do not join.
    let mut terminal = printed(10, 3, "hello\r\nworld");
    assert_eq!(forward(&mut terminal, "helloworld", (0, 0)), None);
    assert_eq!(forward(&mut terminal, "o w", (0, 0)), None);
}

#[test]
fn wide_characters_and_clusters_are_found_whole() {
    let mut terminal = printed(10, 3, "a中文b");
    // The last cell is the first half of the second wide character.
    assert_eq!(
        forward(&mut terminal, "中文", (0, 0)),
        Some(((0, 1), (0, 3)))
    );
    assert_eq!(
        forward(&mut terminal, "文b", (0, 0)),
        Some(((0, 3), (0, 5)))
    );
    // A wide character that did not fit leaves a spacer head, and goes on
    // in the next row.
    let mut terminal = printed(5, 3, "abcd中");
    assert_eq!(
        forward(&mut terminal, "d中", (0, 0)),
        Some(((0, 3), (1, 0)))
    );
    let mut terminal = printed(10, 3, "cafe\u{301} ok");
    assert_eq!(
        forward(&mut terminal, "e\u{301} o", (0, 0)),
        Some(((0, 3), (0, 5)))
    );
    assert_eq!(
        forward(&mut terminal, "\u{301}", (0, 0)),
        Some(((0, 3), (0, 3)))
    );
}

#[test]
fn case_insensitive_search_folds_ascii() {
    let mut terminal = printed(20, 2, "Hello WORLD Été");
    let from = at(&terminal, 0, 0);
    assert_eq!(
        search(
            &mut terminal,
            "world",
            Case::Insensitive,
            from,
            Direction::Forward
        ),
        Some(((0, 6), (0, 10)))
    );
    assert_eq!(
        search(
            &mut terminal,
            "world",
            Case::Sensitive,
            from,
            Direction::Forward
        ),
        None
    );
    assert_eq!(
        search(
            &mut terminal,
            "HELLO",
            Case::Insensitive,
            from,
            Direction::Forward
        ),
        Some(((0, 0), (0, 4)))
    );
    // Only ASCII.
    assert_eq!(
        search(
            &mut terminal,
            "été",
            Case::Insensitive,
            from,
            Direction::Forward
        ),
        None
    );
    assert_eq!(
        search(
            &mut terminal,
            "Été",
            Case::Insensitive,
            from,
            Direction::Forward
        ),
        Some(((0, 12), (0, 14)))
    );
}

#[test]
fn empty_cells_are_spaces_but_not_at_the_end() {
    let mut terminal = printed(20, 2, "a\x1b[3Cb");
    assert_eq!(
        forward(&mut terminal, "a   b", (0, 0)),
        Some(((0, 0), (0, 4)))
    );
    assert_eq!(forward(&mut terminal, "b ", (0, 0)), None);
}

#[test]
fn matches_stay_on_their_cells_as_output_scrolls() {
    let mut parser = Parser::new();
    let mut terminal = Terminal::new(20, 4, Options::default());
    parser.advance(&mut terminal, b"find the needle\r\n");
    let literal = Query::literal("needle", Case::Sensitive).unwrap();
    let from = at(&terminal, 0, 0);
    let found: Match = find(
        terminal.screen_rows_mut(),
        &literal,
        from,
        Direction::Forward,
    )
    .unwrap();
    parser.advance(&mut terminal, "more\r\n".repeat(50).as_bytes());
    let rows = terminal.screen_rows();
    let start = rows.pin_position(found.start).unwrap();
    let page = rows.page(start.page).unwrap();
    assert_eq!(
        page.cells(start.row)[usize::from(start.col)].content(),
        Content::Char('n')
    );
    let end = rows.pin_position(found.end).unwrap();
    assert_eq!(
        rows.page(end.page).unwrap().cells(end.row)[usize::from(end.col)].content(),
        Content::Char('e')
    );
    // Backward from the screen, the needle is still there.
    let from = at(&terminal, 0, 0);
    let again = find(
        terminal.screen_rows_mut(),
        &literal,
        from,
        Direction::Backward,
    )
    .unwrap();
    assert_eq!(
        terminal.screen_rows().pin_position(again.start),
        terminal.screen_rows().pin_position(found.start)
    );
    again.release(terminal.screen_rows_mut());
    found.release(terminal.screen_rows_mut());
    assert_eq!(terminal.screen_rows().pins(), 0);
}

#[test]
fn a_match_whose_cells_left_the_scrollback_is_lost() {
    let mut parser = Parser::new();
    let options = Options {
        scrollback: bolt_vt_core::page::Scrollback {
            limit: 1,
            budget: None,
        },
        ..Options::default()
    };
    // At 80 columns, a page holds 778 rows.
    let mut terminal = Terminal::new(80, 4, options);
    parser.advance(&mut terminal, b"needle\r\n");
    let literal = Query::literal("needle", Case::Sensitive).unwrap();
    let from = at(&terminal, 0, 0);
    let found = find(
        terminal.screen_rows_mut(),
        &literal,
        from,
        Direction::Forward,
    )
    .unwrap();
    parser.advance(&mut terminal, "more\r\n".repeat(5000).as_bytes());
    assert_eq!(terminal.screen_rows().pin_position(found.start), None);
    let from = at(&terminal, 0, 0);
    assert!(
        find(
            terminal.screen_rows_mut(),
            &literal,
            from,
            Direction::Backward
        )
        .is_none()
    );
}

#[test]
fn cold_pages_are_searched() {
    let mut parser = Parser::new();
    let mut terminal = Terminal::new(80, 24, Options::default());
    parser.advance(&mut terminal, b"the needle is old\r\n");
    parser.advance(&mut terminal, "line\r\n".repeat(3000).as_bytes());
    assert!(terminal.compact_scrollback() > 0);
    let literal = Query::literal("needle", Case::Sensitive).unwrap();
    let from = at(&terminal, 0, 0);
    let found = find(
        terminal.screen_rows_mut(),
        &literal,
        from,
        Direction::Backward,
    )
    .unwrap();
    let rows = terminal.screen_rows();
    assert_eq!(
        rows.row_index(rows.pin_position(found.start).unwrap().page, 0),
        Some(0)
    );
}

/// The logical lines of `lines` printed at `cols` columns: each line,
/// without the spaces that end it.
fn logical(lines: &[String]) -> Vec<String> {
    lines
        .iter()
        .map(|line| line.trim_end().to_owned())
        .collect()
}

/// How many times `needle` starts in `text`, overlaps included.
fn occurrences(text: &str, needle: &str) -> usize {
    (0..text.len())
        .filter(|&at| text[at..].starts_with(needle))
        .count()
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// Going forward from each match's next cell, and backward from each
    /// match's first, finds every occurrence, overlaps included, as a
    /// search of the lines' text does.
    #[test]
    fn every_occurrence_is_found_either_way(
        lines in prop::collection::vec("[ab ]{0,30}", 1..20),
        needle in "[ab]{1,3}",
        cols in 3_u16..12,
    ) {
        let mut text = String::new();
        for line in &lines {
            text.push_str(line);
            text.push_str("\r\n");
        }
        let mut terminal = printed(cols, 4, &text);
        let expected: usize = logical(&lines).iter().map(|line| occurrences(line, &needle)).sum();
        let literal = Query::literal(&needle, Case::Sensitive).unwrap();
        let first = {
            let (page, row) = terminal.screen_rows().row_at(0).unwrap();
            Position { page, row, col: 0 }
        };
        let mut found = 0;
        let mut from = first;
        while let Some(hit) = find(terminal.screen_rows_mut(), &literal, from, Direction::Forward) {
            let start = terminal.screen_rows().pin_position(hit.start).unwrap();
            hit.release(terminal.screen_rows_mut());
            found += 1;
            prop_assert!(found <= expected, "more than {} found", expected);
            // The next cell, or the next row's first.
            from = if start.col + 1 < cols {
                Position { col: start.col + 1, ..start }
            } else {
                let rows = terminal.screen_rows();
                let index = rows.row_index(start.page, start.row).unwrap() + 1;
                match rows.row_at(index) {
                    Some((page, row)) => Position { page, row, col: 0 },
                    None => break,
                }
            };
        }
        prop_assert_eq!(found, expected);
        let last = {
            let rows = terminal.screen_rows();
            let (page, row) = rows.row_at(rows.total_rows() - 1).unwrap();
            Position { page, row, col: cols - 1 }
        };
        let mut backward = 0;
        let mut from = last;
        // The last cell itself can start a match of one character.
        let tail = find(terminal.screen_rows_mut(), &literal, from, Direction::Forward);
        if let Some(hit) = tail {
            hit.release(terminal.screen_rows_mut());
            backward += 1;
        }
        while let Some(hit) = find(terminal.screen_rows_mut(), &literal, from, Direction::Backward) {
            from = terminal.screen_rows().pin_position(hit.start).unwrap();
            hit.release(terminal.screen_rows_mut());
            backward += 1;
            prop_assert!(backward <= expected);
        }
        prop_assert_eq!(backward, expected);
        prop_assert_eq!(terminal.screen_rows().pins(), 0);
    }
}

#[test]
fn a_match_crosses_from_one_page_into_the_next() {
    use bolt_vt_core::page::Capacity;
    let cols = 2000;
    let per_page = usize::from(Capacity::standard(cols).rows);
    // Lines up to the last row of the first page, then one that wraps from
    // it into the second page's first row.
    let text = format!("{}{}needle", "x\r\n".repeat(per_page - 1), "a".repeat(1997));
    let mut terminal = printed(cols, 4, &text);
    let rows = terminal.screen_rows();
    let (first_page, _) = rows.row_at(0).unwrap();
    let (last_page, last_row) = rows.row_at(per_page - 1).unwrap();
    let (next_page, next_row) = rows.row_at(per_page).unwrap();
    assert_eq!((last_page, last_row), (first_page, (per_page - 1) as u16));
    assert_ne!(next_page, first_page, "the line goes on in another page");
    let literal = Query::literal("needle", Case::Sensitive).unwrap();
    let start = Position {
        page: first_page,
        row: 0,
        col: 0,
    };
    let found = find(
        terminal.screen_rows_mut(),
        &literal,
        start,
        Direction::Forward,
    )
    .unwrap();
    let rows = terminal.screen_rows();
    assert_eq!(
        rows.pin_position(found.start),
        Some(Position {
            page: last_page,
            row: last_row,
            col: 1997
        })
    );
    assert_eq!(
        rows.pin_position(found.end),
        Some(Position {
            page: next_page,
            row: next_row,
            col: 2
        })
    );
    found.release(terminal.screen_rows_mut());
    let end = Position {
        page: next_page,
        row: next_row,
        col: 10,
    };
    let found = find(
        terminal.screen_rows_mut(),
        &literal,
        end,
        Direction::Backward,
    )
    .unwrap();
    assert_eq!(
        terminal.screen_rows().pin_position(found.start),
        Some(Position {
            page: last_page,
            row: last_row,
            col: 1997
        })
    );
}

#[test]
fn a_row_that_soft_wraps_joins_only_its_continuation() {
    // IL puts a row of its own between a row that wrapped and the one that
    // continues it: the three are three lines.
    let mut terminal = printed(10, 4, "0123456789abc\x1b[2;1H\x1b[LXY");
    assert_eq!(forward(&mut terminal, "9X", (0, 0)), None);
    assert_eq!(forward(&mut terminal, "Yab", (0, 0)), None);
    assert_eq!(
        forward(&mut terminal, "abc", (0, 0)),
        Some(((2, 0), (2, 2)))
    );
}
