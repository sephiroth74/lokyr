//! Whole words and clusters against an oracle of their own: the matches of
//! a literal that a search of each logical line's text finds by brute
//! force, the line read from its cells here.

use std::sync::atomic::AtomicBool;

use bolt_search::{Case, Direction, Query};
use bolt_vt_core::page::{Content, PageList, Position, RowFlag, Width};
use bolt_vt_core::parser::Parser;
use bolt_vt_core::terminal::{Options, Terminal};
use proptest::prelude::*;

/// A logical line's text, and its cells that show something: where each
/// one's text starts, and the cell.
struct Line {
    text: String,
    cells: Vec<(usize, Position)>,
}

/// The logical lines of `list`, from their cells: empty ones read as
/// spaces, the halves that show nothing are left out, and the spaces that
/// end a line go.
fn lines(list: &PageList) -> Vec<Line> {
    let mut lines = Vec::new();
    let mut line = Line {
        text: String::new(),
        cells: Vec::new(),
    };
    for index in 0..list.total_rows() {
        let (id, row) = list.row_at(index).unwrap();
        let page = list.page(id).unwrap();
        for (col, cell) in (0..).zip(page.cells(row)) {
            if matches!(cell.width(), Width::SpacerTail | Width::SpacerHead) {
                continue;
            }
            line.cells
                .push((line.text.len(), Position { page: id, row, col }));
            match cell.content() {
                Content::Char(c) if c != '\0' => line.text.push(c),
                Content::Grapheme(c) => {
                    line.text.push(c);
                    line.text.extend(page.grapheme(row, col));
                }
                _ => line.text.push(' '),
            }
        }
        let goes_on = page.row(row).has(RowFlag::SoftWrapped)
            && list.row_at(index + 1).is_some_and(|(next, next_row)| {
                list.page(next)
                    .unwrap()
                    .row(next_row)
                    .has(RowFlag::Continuation)
            });
        if !goes_on {
            let len = line.text.trim_end_matches(' ').len();
            line.text.truncate(len);
            line.cells.retain(|&(start, _)| start < len);
            lines.push(std::mem::replace(
                &mut line,
                Line {
                    text: String::new(),
                    cells: Vec::new(),
                },
            ));
        }
    }
    lines
}

/// The first cell of each match of `needle` in `line`, by brute force: from
/// the line's start, the first place, then the first after each match's
/// end, where the needle starts, from the start of a cell to the end of
/// one, a whole word if `whole_word`.
fn expected(line: &Line, needle: &str, insensitive: bool, whole_word: bool) -> Vec<Position> {
    let (text, needle) = if insensitive {
        (line.text.to_ascii_lowercase(), needle.to_ascii_lowercase())
    } else {
        (line.text.clone(), needle.to_owned())
    };
    let cell = |at: usize| line.cells.iter().position(|&(start, _)| start == at);
    let word = |c: char| c.is_alphanumeric() || c == '_';
    let counts = |at: usize| {
        let end = at + needle.len();
        let Some(first) = cell(at) else {
            return false;
        };
        if !text[at..].starts_with(&needle) || (end != text.len() && cell(end).is_none()) {
            return false;
        }
        let before = first
            .checked_sub(1)
            .and_then(|before| text[line.cells[before].0..].chars().next());
        let after = text[end..].chars().next();
        !whole_word || (!before.is_some_and(word) && !after.is_some_and(word))
    };
    let mut found = Vec::new();
    let mut at = 0;
    while let Some(start) = (at..text.len()).find(|&start| counts(start)) {
        found.push(line.cells[cell(start).unwrap()].1);
        at = start + needle.len();
    }
    found
}

/// The first cell of every match, going forward from each match's next
/// cell, and in one pass.
fn found(terminal: &mut Terminal, query: &Query) -> (Vec<Position>, Vec<Position>) {
    let cancel = AtomicBool::new(false);
    let list = terminal.screen_rows_mut();
    let mut stepped = Vec::new();
    let (page, row) = list.row_at(0).unwrap();
    let mut from = Position { page, row, col: 0 };
    while let Some(hit) = bolt_search::find(list, query, from, Direction::Forward, &cancel).unwrap()
    {
        let start = list.pin_position(hit.start).unwrap();
        hit.release(list);
        stepped.push(start);
        from = if start.col + 1 < list.cols() {
            Position {
                col: start.col + 1,
                ..start
            }
        } else {
            let index = list.row_index(start.page, start.row).unwrap() + 1;
            match list.row_at(index) {
                Some((page, row)) => Position { page, row, col: 0 },
                None => break,
            }
        };
    }
    let all =
        bolt_search::find_all(list, query, 0..list.total_rows(), usize::MAX, &cancel).unwrap();
    let all = all
        .pinned
        .into_iter()
        .map(|hit| {
            let start = list.pin_position(hit.start).unwrap();
            hit.release(list);
            start
        })
        .collect();
    (stepped, all)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    /// A literal finds what the oracle finds, whole words or not, either
    /// case, with wide characters and clusters, stepping or in one pass.
    #[test]
    fn a_literal_finds_what_the_oracle_finds(
        lines_in in prop::collection::vec("[aAb _.\u{4e2d}\u{e9}\u{301}]{0,12}", 1..10),
        needle in "[aAb_.\u{4e2d}\u{e9}\u{301}]{1,3}",
        cols in 3_u16..12,
        insensitive in any::<bool>(),
        whole_word in any::<bool>(),
    ) {
        let mut text = String::new();
        for line in &lines_in {
            text.push_str(line);
            text.push_str("\r\n");
        }
        let mut terminal = Terminal::new(cols, 4, Options::default());
        Parser::new().advance(&mut terminal, text.as_bytes());
        let expected: Vec<Position> = lines(terminal.screen_rows())
            .iter()
            .flat_map(|line| expected(line, &needle, insensitive, whole_word))
            .collect();
        let case = if insensitive { Case::Insensitive } else { Case::Sensitive };
        let query = Query::literal(&needle, case).unwrap().whole_word(whole_word);
        let (stepped, all) = found(&mut terminal, &query);
        prop_assert_eq!(&stepped, &expected);
        prop_assert_eq!(&all, &expected);
        prop_assert_eq!(terminal.screen_rows().pins(), 0);
    }
}
