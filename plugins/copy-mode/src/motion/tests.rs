//! The moves over rows of text, some of which wrap into the next.

use super::*;

/// The columns of the rows of the tests.
const COLUMNS: usize = 12;

/// Rows of text, each padded to the columns, from the row numbered
/// `first`, and whether each wraps into the next; the reads that they
/// took.
struct Rows {
    first: i64,
    rows: Vec<(String, bool)>,
    reads: usize,
}

impl Rows {
    /// Rows of `texts`, from the row numbered `first`: a text that ends
    /// with `>` wraps into the next row, without the `>`.
    fn new(first: i64, texts: &[&str]) -> Self {
        let rows = texts
            .iter()
            .map(|text| match text.strip_suffix('>') {
                Some(text) => (format!("{text:COLUMNS$}"), true),
                None => (format!("{text:COLUMNS$}"), false),
            })
            .collect();
        Self {
            first,
            rows,
            reads: 0,
        }
    }

    fn bounds(&self) -> Bounds {
        Bounds {
            oldest: self.first,
            last: self.first + self.rows.len() as i64 - 1,
        }
    }
}

impl Text for Rows {
    fn line(&mut self, row: i64, reach: usize) -> Option<LineText> {
        self.reads += 1;
        let index = usize::try_from(row - self.first).ok()?;
        if index >= self.rows.len() {
            return None;
        }
        let mut start = index;
        while start > 0 && self.rows[start - 1].1 && index - start < reach {
            start -= 1;
        }
        let mut end = index;
        while self.rows[end].1 && end + 1 < self.rows.len() && end - index < reach {
            end += 1;
        }
        let mut line = LineText {
            text: String::new(),
            cells: Vec::new(),
        };
        for at in start..=end {
            let number = self.first + at as i64;
            for (column, character) in self.rows[at].0.chars().enumerate() {
                line.text.push(character);
                line.cells.push((u16::try_from(column).unwrap(), number));
            }
        }
        Some(line)
    }
}

#[test]
fn a_word_s_move_goes_by_words_and_punctuation_across_the_rows() {
    let mut rows = Rows::new(10, &["foo-bar  baz", "", "  qux.quux"]);
    let bounds = rows.bounds();
    let mut at = (0, 10);
    let mut stops = Vec::new();
    for _ in 0..6 {
        at = word_next(&mut rows, bounds, at, false);
        stops.push(at);
    }
    // `-`, `bar`, `baz`, then past the blank row to `qux`, `.`, `quux`.
    assert_eq!(
        stops,
        [(3, 10), (4, 10), (9, 10), (2, 12), (5, 12), (6, 12)]
    );
    // At the last word, the move stays at its end.
    assert_eq!(word_next(&mut rows, bounds, (6, 12), false), (9, 12));
    // Back, the same stops the other way.
    let mut back = Vec::new();
    for _ in 0..6 {
        at = word_previous(&mut rows, bounds, at, false);
        back.push(at);
    }
    assert_eq!(back, [(5, 12), (2, 12), (9, 10), (4, 10), (3, 10), (0, 10)]);
    assert_eq!(word_previous(&mut rows, bounds, (0, 10), false), (0, 10));
}

#[test]
fn a_big_word_s_move_takes_punctuation_into_its_words() {
    let mut rows = Rows::new(0, &["foo-bar  baz", "  qux.quux"]);
    let bounds = rows.bounds();
    assert_eq!(word_next(&mut rows, bounds, (0, 0), true), (9, 0));
    assert_eq!(word_next(&mut rows, bounds, (9, 0), true), (2, 1));
    assert_eq!(word_previous(&mut rows, bounds, (2, 1), true), (9, 0));
    assert_eq!(word_previous(&mut rows, bounds, (9, 0), true), (0, 0));
    assert_eq!(word_end(&mut rows, bounds, (0, 0), true), (6, 0));
    assert_eq!(word_end(&mut rows, bounds, (6, 0), true), (11, 0));
    assert_eq!(word_end(&mut rows, bounds, (11, 0), true), (9, 1));
}

#[test]
fn a_word_s_end_is_its_last_character() {
    let mut rows = Rows::new(0, &["foo-bar  baz", "  qux"]);
    let bounds = rows.bounds();
    let mut at = (0, 0);
    let mut stops = Vec::new();
    for _ in 0..5 {
        at = word_end(&mut rows, bounds, at, false);
        stops.push(at);
    }
    assert_eq!(stops, [(2, 0), (3, 0), (6, 0), (11, 0), (4, 1)]);
    // Past the last word, none.
    assert_eq!(word_end(&mut rows, bounds, (4, 1), false), (4, 1));
}

#[test]
fn a_word_that_a_row_wraps_stays_one_word_and_a_line_s_end_parts_two() {
    // `abcdefghijkl` wraps into `mno`; the next line starts with `pqr`.
    let mut rows = Rows::new(0, &["  abcdefghij>", "klmno", "pqr"]);
    let bounds = rows.bounds();
    // The wrapped word is one: from its start, its end is on the next row.
    assert_eq!(word_end(&mut rows, bounds, (2, 0), false), (4, 1));
    assert_eq!(word_next(&mut rows, bounds, (2, 0), false), (0, 2));
    assert_eq!(word_previous(&mut rows, bounds, (3, 1), false), (2, 0));
    // Rows that fill their columns and do not wrap part their words.
    let mut full = Rows::new(0, &["aaaaaaaaaaaa", "bbbbbbbbbbbb"]);
    let bounds = full.bounds();
    assert_eq!(word_next(&mut full, bounds, (0, 0), false), (0, 1));
    assert_eq!(word_end(&mut full, bounds, (0, 0), false), (11, 0));
    assert_eq!(word_previous(&mut full, bounds, (3, 1), false), (0, 1));
    assert_eq!(word_previous(&mut full, bounds, (0, 1), false), (0, 0));
}

#[test]
fn a_word_s_move_reads_a_long_line_a_few_rows_at_a_time() {
    // A line of 40 rows, of a word at its end.
    let mut texts = vec!["x>"];
    texts.extend(std::iter::repeat_n(">", 38));
    texts.push("       end");
    let mut rows = Rows::new(0, &texts);
    let bounds = rows.bounds();
    assert_eq!(word_next(&mut rows, bounds, (0, 0), false), (7, 39));
    // A read of each few rows, not of every row.
    assert!(rows.reads <= 8, "{} reads", rows.reads);
}

#[test]
fn a_line_s_edges_are_of_its_logical_line() {
    let mut rows = Rows::new(0, &["   foo bar  >", "baz   ", "", "x"]);
    assert_eq!(line_edge(&mut rows, (8, 1), Edge::Start), (0, 0));
    assert_eq!(line_edge(&mut rows, (8, 1), Edge::First), (3, 0));
    assert_eq!(line_edge(&mut rows, (4, 0), Edge::Last), (2, 1));
    // A blank line's edges are its first cell.
    assert_eq!(line_edge(&mut rows, (5, 2), Edge::Last), (0, 2));
    assert_eq!(line_edge(&mut rows, (5, 2), Edge::First), (0, 2));
}

#[test]
fn a_paragraph_s_move_stops_at_the_blank_line_after_or_before_it() {
    let mut rows = Rows::new(0, &["a", "b", "", "", "c", "d", "", "e"]);
    let bounds = rows.bounds();
    assert_eq!(paragraph(&mut rows, bounds, (0, 0), false), (0, 2));
    // From a blank line, past the blank lines and the paragraph after.
    assert_eq!(paragraph(&mut rows, bounds, (0, 2), false), (0, 6));
    assert_eq!(paragraph(&mut rows, bounds, (0, 6), false), (0, 7));
    assert_eq!(paragraph(&mut rows, bounds, (0, 7), true), (0, 6));
    assert_eq!(paragraph(&mut rows, bounds, (0, 5), true), (0, 3));
    assert_eq!(paragraph(&mut rows, bounds, (0, 3), true), (0, 0));
}

#[test]
fn the_bottom_is_the_last_row_that_shows_something_or_the_cursor_s() {
    let mut rows = Rows::new(0, &["a", "$ ls", "b", "", ""]);
    let bounds = rows.bounds();
    assert_eq!(bottom(&mut rows, bounds, 1), (0, 2));
    assert_eq!(bottom(&mut rows, bounds, 3), (0, 3));
}

/// Rows of which the second reads no cell, as a pane of one column whose
/// cell is a wide character's spacer.
struct Gap;

impl Text for Gap {
    fn line(&mut self, row: i64, _: usize) -> Option<LineText> {
        let text = if row == 1 { "" } else { "ab" };
        Some(LineText {
            text: text.to_owned(),
            cells: (0..text.len())
                .map(|column| (u16::try_from(column).unwrap(), row))
                .collect(),
        })
    }
}

#[test]
fn a_row_of_no_cells_leaves_the_walker_where_it_was() {
    let bounds = Bounds { oldest: 0, last: 2 };
    assert_eq!(word_next(&mut Gap, bounds, (0, 0), false), (1, 0));
    assert_eq!(word_end(&mut Gap, bounds, (1, 0), false), (1, 0));
    assert_eq!(word_previous(&mut Gap, bounds, (0, 2), false), (0, 2));
}
