//! Differential tests of the VT core: the same bytes, fed to it and to
//! `alacritty_terminal`, whose screens are then compared, cell by cell, with
//! the cursor.
//!
//! A cell is compared as its text: its character with the zero-width ones
//! that it holds, an empty cell read as a space, as is the cell where
//! `alacritty_terminal` keeps a tab, and the second half of a wide
//! character, or the last cell of a row that a wide character left for the
//! next, as a token of its own. Each row's empty cells at the end are left
//! out. The screens' rows are counted, and the cursor's column, row and
//! pending wrap compared. Attributes, colors and the scrollback are left
//! out, as the two cores keep them in ways of their own.

#![forbid(unsafe_code)]

use alacritty_terminal::event::VoidListener;
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line};
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::test::TermSize;
use alacritty_terminal::term::{Config, Term};
use alacritty_terminal::vte::ansi::{Processor, Timeout};
use bolt_vt_core::page::{Content, Width};
use bolt_vt_core::parser::Parser;
use bolt_vt_core::terminal::{Options, Terminal};

/// The second cell of a wide character.
pub const TAIL: &str = "[tail]";

/// The last cell of a row, left empty for a wide character that did not fit
/// and went to the next row.
pub const HEAD: &str = "[head]";

/// A screen as the tests compare it: each row's cells, and the cursor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Screen {
    /// Each row's cells, from the left, without the empty ones at the end.
    pub rows: Vec<Vec<String>>,
    /// The cursor's column and row, from 0, and whether a wrap is pending.
    pub cursor: (usize, usize, bool),
}

/// Where two screens differ first.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Difference {
    /// The cell, the cursors or the rows' counts, with both sides: short,
    /// for the tests to keep.
    pub summary: String,
    /// The rows that differ, as text, to read when the summary is not
    /// enough.
    pub context: String,
}

impl Screen {
    /// This screen with row `y` alone, empty if it has none, and the cursor,
    /// for an input that writes a row at a time.
    pub fn only_row(&self, y: usize) -> Screen {
        Screen {
            rows: vec![self.rows.get(y).cloned().unwrap_or_default()],
            cursor: self.cursor,
        }
    }

    /// Where `self` differs from `other` first: the rows' counts, then the
    /// first cell, row by row, then the cursors.
    pub fn first_difference(&self, other: &Screen) -> Option<Difference> {
        if self.rows.len() != other.rows.len() {
            return Some(Difference {
                summary: format!("{} rows against {}", self.rows.len(), other.rows.len()),
                context: String::new(),
            });
        }
        for (y, (ours, theirs)) in self.rows.iter().zip(&other.rows).enumerate() {
            let width = ours.len().max(theirs.len());
            if let Some(x) = (0..width).find(|&x| cell(ours, x) != cell(theirs, x)) {
                return Some(Difference {
                    summary: format!(
                        "row {y}, column {x}: {:?} against {:?}",
                        cell(ours, x),
                        cell(theirs, x)
                    ),
                    context: format!("{:?} against {:?}", ours.concat(), theirs.concat()),
                });
            }
        }
        (self.cursor != other.cursor).then(|| Difference {
            summary: format!("cursor {:?} against {:?}", self.cursor, other.cursor),
            context: String::new(),
        })
    }
}

/// The VT core and `alacritty_terminal`, of one size, fed the same bytes.
pub struct Pair {
    parser: Parser,
    bolt: Terminal,
    processor: Processor,
    alacritty: Term<VoidListener>,
}

impl Pair {
    /// Both cores, of `cols` by `rows`, with their default options.
    pub fn new(cols: u16, rows: u16) -> Self {
        let size = TermSize::new(usize::from(cols), usize::from(rows));
        Self {
            parser: Parser::new(),
            bolt: Terminal::new(cols, rows, Options::default()),
            processor: Processor::new(),
            alacritty: Term::new(Config::default(), &size, VoidListener),
        }
    }

    /// Feeds `bytes` to both.
    pub fn feed(&mut self, bytes: &[u8]) {
        self.parser.advance(&mut self.bolt, bytes);
        self.processor.advance(&mut self.alacritty, bytes);
    }

    /// The VT core's screen.
    pub fn bolt(&self) -> Screen {
        let terminal = &self.bolt;
        let screen = terminal.screen_rows();
        let rows = (0..terminal.rows())
            .map(|y| {
                let (page, row) = screen.screen_row(y);
                let cells = (0..)
                    .zip(page.cells(row))
                    .map(|(x, cell)| match (cell.width(), cell.content()) {
                        (Width::SpacerTail, _) => TAIL.to_owned(),
                        (Width::SpacerHead, _) => HEAD.to_owned(),
                        (_, Content::Char('\0') | Content::BgPalette(_) | Content::BgRgb(_)) => {
                            " ".to_owned()
                        }
                        (_, Content::Char(c)) => c.to_string(),
                        (_, Content::Grapheme(c)) => std::iter::once(c)
                            .chain(page.grapheme(row, x).iter().copied())
                            .collect(),
                    })
                    .collect();
                trimmed(cells)
            })
            .collect();
        let cursor = terminal.cursor();
        Screen {
            rows,
            cursor: (
                usize::from(cursor.x),
                usize::from(cursor.y),
                cursor.pending_wrap,
            ),
        }
    }

    /// `alacritty_terminal`'s screen, read as the VT core's is. A
    /// synchronized update under way ends first, as the processor holds its
    /// bytes back until then, and the VT core has them.
    pub fn alacritty(&mut self) -> Screen {
        if self.processor.sync_timeout().pending_timeout() {
            self.processor.stop_sync(&mut self.alacritty);
        }
        let grid = self.alacritty.grid();
        let rows = (0..grid.screen_lines())
            .map(|y| {
                // A screen has fewer rows than an i32 holds.
                let row = &grid[Line(y as i32)];
                let cells = (0..grid.columns())
                    .map(|x| {
                        let cell = &row[Column(x)];
                        if cell.flags.contains(Flags::LEADING_WIDE_CHAR_SPACER) {
                            return HEAD.to_owned();
                        }
                        if cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
                            return TAIL.to_owned();
                        }
                        // An empty cell, or the one where a tab started,
                        // which keeps the tab for copies and shows a space.
                        let c = if matches!(cell.c, '\0' | '\t') {
                            ' '
                        } else {
                            cell.c
                        };
                        std::iter::once(c)
                            .chain(cell.zerowidth().into_iter().flatten().copied())
                            .collect()
                    })
                    .collect();
                trimmed(cells)
            })
            .collect();
        let cursor = &grid.cursor;
        Screen {
            rows,
            cursor: (
                cursor.point.column.0,
                cursor.point.line.0.max(0) as usize,
                cursor.input_needs_wrap,
            ),
        }
    }

    /// Where the two screens differ first, if they do.
    pub fn difference(&mut self) -> Option<Difference> {
        let alacritty = self.alacritty();
        self.bolt().first_difference(&alacritty)
    }
}

/// Cell `x` of `row`, empty past its end.
fn cell(row: &[String], x: usize) -> &str {
    row.get(x).map_or(" ", String::as_str)
}

/// `cells` without the empty ones at the end.
fn trimmed(mut cells: Vec<String>) -> Vec<String> {
    while cells.last().is_some_and(|cell| cell == " ") {
        cells.pop();
    }
    cells
}

#[cfg(test)]
mod tests {
    use super::*;

    fn screen(rows: &[&[&str]], cursor: (usize, usize, bool)) -> Screen {
        Screen {
            rows: rows
                .iter()
                .map(|row| row.iter().map(|&cell| cell.to_owned()).collect())
                .collect(),
            cursor,
        }
    }

    #[test]
    fn screens_differ_in_their_rows_count_then_a_cell_then_the_cursor() {
        let ours = screen(&[&["a", "b"], &[]], (0, 0, false));
        assert_eq!(ours.first_difference(&ours), None);
        let fewer = screen(&[&["a", "b"]], (0, 0, false));
        assert_eq!(
            ours.first_difference(&fewer).unwrap().summary,
            "2 rows against 1"
        );
        // A cell past a row's end is empty.
        let shorter = screen(&[&["a"], &[" ", "c"]], (0, 0, true));
        let difference = ours.first_difference(&shorter).unwrap();
        assert_eq!(difference.summary, "row 0, column 1: \"b\" against \" \"");
        assert_eq!(difference.context, "\"ab\" against \"a\"");
        let pending = screen(&[&["a", "b"], &[]], (0, 0, true));
        assert_eq!(
            ours.first_difference(&pending).unwrap().summary,
            "cursor (0, 0, false) against (0, 0, true)"
        );
    }

    #[test]
    fn the_cores_are_read_a_cell_at_a_time() {
        // A character wide in one core and narrow in the other shows after
        // the cursor has left its row.
        let mut pair = Pair::new(10, 2);
        pair.feed("x\u{2600}\u{FE0F}y\r\n".as_bytes());
        assert_eq!(pair.bolt().rows[0], ["x", "\u{2600}\u{FE0F}", TAIL, "y"]);
        assert_eq!(pair.alacritty().rows[0], ["x", "\u{2600}\u{FE0F}", "y"]);
        // A wide character that does not fit leaves the same cell in both.
        let mut pair = Pair::new(4, 2);
        pair.feed("abc\u{4E2D}".as_bytes());
        assert_eq!(pair.bolt().rows[0], ["a", "b", "c", HEAD]);
        assert_eq!(pair.bolt().rows[1], ["\u{4E2D}", TAIL]);
        assert_eq!(pair.difference(), None);
    }

    #[test]
    fn a_synchronized_update_ends_before_the_screens_are_read() {
        let mut pair = Pair::new(10, 2);
        pair.feed(b"\x1b[?2026hX");
        assert_eq!(pair.alacritty().rows[0], ["X"]);
        pair.feed(b"Y\x1b[?2026lZ");
        assert_eq!(pair.difference(), None);
        assert_eq!(pair.bolt().rows[0], ["X", "Y", "Z"]);
    }
}
