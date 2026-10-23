//! Whole words and clusters against an oracle of their own: the matches of
//! a literal that a search of each logical line's text finds by brute
//! force, the line read from its cells here, before its pages go cold.

use std::sync::atomic::AtomicBool;

use bolt_search::{Case, Direction, Query};
use bolt_vt_core::page::{Capacity, Content, PageList, Position, RowFlag, Width};
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
/// spaces, the second halves of wide characters and the spacer heads at a
/// row's end are left out, a spacer head inside a row reads as a space,
/// and the spaces that end a line go.
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
            let head = cell.width() == Width::SpacerHead;
            if cell.width() == Width::SpacerTail || (head && col + 1 == list.cols()) {
                continue;
            }
            line.cells
                .push((line.text.len(), Position { page: id, row, col }));
            match cell.content() {
                _ if head => line.text.push(' '),
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

/// The first and the last cell of each match of `needle` in `line`, by
/// brute force: from the line's start, the first place, then the first
/// after each match's end, where the needle starts, from the start of a
/// cell to the end of one, a whole word if `whole_word`.
fn expected(
    line: &Line,
    needle: &str,
    insensitive: bool,
    whole_word: bool,
) -> Vec<(Position, Position)> {
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
        let end = start + needle.len();
        // The cell that holds the match's last byte.
        let last = line
            .cells
            .iter()
            .rev()
            .find(|&&(cell_start, _)| cell_start < end)
            .unwrap();
        found.push((line.cells[cell(start).unwrap()].1, last.1));
        at = end;
    }
    found
}

/// The first and last cells of every match, going forward from each match's
/// next cell, going backward from each match's first cell, oldest first,
/// and in one pass over the rows `rows`.
type Cells = Vec<(Position, Position)>;

fn found(
    terminal: &mut Terminal,
    query: &Query,
    rows: std::ops::Range<usize>,
) -> (Cells, Cells, Cells) {
    let cancel = AtomicBool::new(false);
    let list = terminal.screen_rows_mut();
    let mut stepped = Vec::new();
    let (page, row) = list.row_at(0).unwrap();
    let mut from = Position { page, row, col: 0 };
    while let Some(hit) = bolt_search::find(list, query, from, Direction::Forward, &cancel).unwrap()
    {
        let start = list.pin_position(hit.start).unwrap();
        let end = list.pin_position(hit.end).unwrap();
        hit.release(list);
        stepped.push((start, end));
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
    // Backward from the last cell, in the last row, which is empty, as the
    // text ends with a line break.
    let mut back = Vec::new();
    let (page, row) = list.row_at(list.total_rows() - 1).unwrap();
    let mut from = Position {
        page,
        row,
        col: list.cols() - 1,
    };
    while let Some(hit) =
        bolt_search::find(list, query, from, Direction::Backward, &cancel).unwrap()
    {
        let start = list.pin_position(hit.start).unwrap();
        let end = list.pin_position(hit.end).unwrap();
        hit.release(list);
        back.push((start, end));
        from = start;
    }
    back.reverse();
    let all = bolt_search::find_all(list, query, rows, usize::MAX, &cancel).unwrap();
    let all = all
        .pinned
        .into_iter()
        .map(|hit| {
            let cells = (
                list.pin_position(hit.start).unwrap(),
                list.pin_position(hit.end).unwrap(),
            );
            hit.release(list);
            cells
        })
        .collect();
    (stepped, back, all)
}

#[test]
fn a_spacer_head_reads_as_a_space_inside_a_row_alone() {
    // A wrap within left and right margins leaves a head at the right
    // margin, between `a` and `b`; at the row's end, after `cd`, the head
    // that a full-width wrap leaves has no text.
    let mut terminal = Terminal::new(10, 4, Options::default());
    let output =
        "\x1b[?69h\x1b[3;8s\x1b[1;8H\u{4e2d}\x1b[1;7Ha\x1b[1;9Hb\x1b[?69l\x1b[3;8Hcd\u{4e2d}\r\n";
    Parser::new().advance(&mut terminal, output.as_bytes());
    for needle in ["a b", "cd\u{4e2d}"] {
        let expected: Cells = lines(terminal.screen_rows())
            .iter()
            .flat_map(|line| expected(line, needle, false, false))
            .collect();
        assert_eq!(expected.len(), 1, "{needle}");
        let query = Query::literal(needle, Case::Sensitive).unwrap();
        let rows = 0..terminal.screen_rows().total_rows();
        let (stepped, back, all) = found(&mut terminal, &query, rows);
        assert_eq!(
            (&stepped, &back, &all),
            (&expected, &expected, &expected),
            "{needle}"
        );
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    /// A literal finds what the oracle finds, whole words or not, either
    /// case, with wide characters and clusters, stepping either way, or in
    /// one pass over a range of rows, its matches' first and last cells
    /// alike, with lines that cross from one page into the next, and the
    /// lines' pages gone cold.
    #[test]
    fn a_literal_finds_what_the_oracle_finds(
        lines_in in prop::collection::vec("[aAb _.\u{4e2d}\u{e9}\u{301}]{0,12}", 1..10),
        needle in "[aAb_.\u{4e2d}\u{e9}\u{301}]{1,3}",
        cols in 3_u16..12,
        insensitive in any::<bool>(),
        whole_word in any::<bool>(),
        first_row in 0_usize..40,
        row_count in 0_usize..40,
        crossing in any::<bool>(),
        cold in any::<bool>(),
        margins in prop::option::of((0_u16..12, 0_u16..12)),
    ) {
        // Empty lines up to the first page's last row, and there the needle
        // in the last column, so that it goes on in the next page.
        let filler = if crossing {
            usize::from(Capacity::standard(cols).rows) - 1
        } else {
            0
        };
        let mut text = "\r\n".repeat(filler);
        if crossing {
            text.push_str(&" ".repeat(usize::from(cols) - 1));
            text.push_str(&needle);
            text.push_str("\r\n");
        }
        // At times, the lines go within left and right margins, the right
        // one left of the last column, so that a wide character that does
        // not fit at it leaves a spacer head inside the row; the cursor
        // stays in its row, at the left margin.
        if let Some((left, width)) = margins {
            let left = left % (cols - 2);
            let right = left + 1 + width % (cols - 2 - left);
            text.push_str(&format!(
                "\x1b7\x1b[?69h\x1b[{};{}s\x1b8\x1b[{}G",
                left + 1,
                right + 1,
                left + 1
            ));
        }
        for line in &lines_in {
            text.push_str(line);
            text.push_str("\r\n");
        }
        if margins.is_some() {
            text.push_str("\x1b[?69l");
        }
        let mut terminal = Terminal::new(cols, 4, Options::default());
        Parser::new().advance(&mut terminal, text.as_bytes());
        // The row of the last line, which a page of rows after it, and some,
        // sends to a page that goes cold.
        let last_line = terminal.screen_rows().scrollback_rows() + usize::from(terminal.cursor().y) - 1;
        if cold {
            let rows = usize::from(Capacity::standard(cols).rows) + 16;
            Parser::new().advance(&mut terminal, "\r\n".repeat(rows).as_bytes());
        }
        let list = terminal.screen_rows();
        let expected: Cells = lines(list)
            .iter()
            .flat_map(|line| expected(line, &needle, insensitive, whole_word))
            .collect();
        let rows = filler + first_row..filler + first_row + row_count;
        let in_rows: Cells = expected
            .iter()
            .copied()
            .filter(|(start, _)| rows.contains(&list.row_index(start.page, start.row).unwrap()))
            .collect();
        let case = if insensitive { Case::Insensitive } else { Case::Sensitive };
        let query = Query::literal(&needle, case).unwrap().whole_word(whole_word);
        if cold {
            terminal.compact_scrollback();
            let rows = terminal.screen_rows();
            let (page, _) = rows.row_at(last_line).unwrap();
            prop_assert!(rows.page(page).is_none(), "the lines are cold");
        }
        let (stepped, back, all) = found(&mut terminal, &query, rows);
        prop_assert_eq!(&stepped, &expected);
        prop_assert_eq!(&back, &expected);
        prop_assert_eq!(&all, &in_rows);
        prop_assert_eq!(terminal.screen_rows().pins(), 0);
    }
}
