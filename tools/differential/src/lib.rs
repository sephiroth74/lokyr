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
//!
//! [`explore`] feeds an input a byte at a time and compares the screens
//! after each, as the differential fuzz target does: the first point where
//! they part must have one of the known [`Cause`]s, after which they are
//! compared no more.
//!
//! [`vtebench`] rebuilds vtebench's scenarios, which `bolt-throughput`
//! times against `alacritty_terminal`'s, and whose instructions the
//! `instructions` benchmark counts.

#![forbid(unsafe_code)]

mod cause;
mod explore;
pub mod vtebench;

pub use cause::Cause;
pub use explore::{Before, Divergence, Step, explore, explore_after, fuzz_input};

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

/// DEL, which vte's parser prints as a character outside escape sequences
/// and strings, while DEC's and the VT core's take it for nothing:
/// `alacritty_terminal` shows nothing of it, but its REP then repeats it,
/// rather than the character before. [`Pair`] gives `alacritty_terminal`
/// [`NUL`] in its place, so that REP repeats the same character in both.
pub(crate) const DEL: u8 = 0x7F;

/// What `alacritty_terminal` gets for DEL outside escape sequences and
/// strings: a control that does nothing, and that ends a character whose
/// bytes did not all come, as DEL does in the VT core.
pub(crate) const NUL: u8 = 0x00;

/// The VT core and `alacritty_terminal`, of one size, fed the same bytes,
/// but for the DEL that vte prints, for which `alacritty_terminal` gets
/// NUL.
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

    /// Feeds `bytes` to both, but each DEL that the parser meets outside
    /// escape sequences and strings, for which `alacritty_terminal` gets
    /// NUL.
    pub fn feed(&mut self, bytes: &[u8]) {
        let mut rest = bytes;
        while let Some(at) = rest.iter().position(|&byte| byte == DEL) {
            let (before, after) = rest.split_at(at);
            self.parser.advance(&mut self.bolt, before);
            self.processor.advance(&mut self.alacritty, before);
            let ground = self.parser.in_ground();
            self.parser.advance(&mut self.bolt, &[DEL]);
            let theirs = if ground { NUL } else { DEL };
            self.processor.advance(&mut self.alacritty, &[theirs]);
            rest = &after[1..];
        }
        self.parser.advance(&mut self.bolt, rest);
        self.processor.advance(&mut self.alacritty, rest);
    }

    /// The VT core's terminal, for the state that the screens do not show.
    pub fn terminal(&self) -> &Terminal {
        &self.bolt
    }

    /// The VT core's screen.
    pub fn bolt(&self) -> Screen {
        let terminal = &self.bolt;
        let rows = (0..terminal.rows())
            .map(|y| trimmed(bolt_cells(terminal, y).map(Cell::text).collect()))
            .collect();
        Screen {
            rows,
            cursor: bolt_cursor(terminal),
        }
    }

    /// `alacritty_terminal`'s screen, read as the VT core's is. A
    /// synchronized update under way ends first, as the processor holds its
    /// bytes back until then, and the VT core has them.
    pub fn alacritty(&mut self) -> Screen {
        self.end_sync();
        let grid = self.alacritty.grid();
        let rows = (0..grid.screen_lines())
            .map(|y| {
                trimmed(
                    alacritty_cells(&self.alacritty, y)
                        .map(Cell::text)
                        .collect(),
                )
            })
            .collect();
        Screen {
            rows,
            cursor: alacritty_cursor(&self.alacritty),
        }
    }

    /// Where the two screens differ first, if they do.
    pub fn difference(&mut self) -> Option<Difference> {
        let alacritty = self.alacritty();
        self.bolt().first_difference(&alacritty)
    }

    /// Whether the two screens are the same, as [`Pair::difference`] finds
    /// them, without reading them into text: for a check after every byte.
    pub fn same(&mut self) -> bool {
        self.end_sync();
        let (bolt, alacritty) = (&self.bolt, &self.alacritty);
        let rows = usize::from(bolt.rows());
        rows == alacritty.grid().screen_lines()
            && bolt_cursor(bolt) == alacritty_cursor(alacritty)
            && (0..bolt.rows()).all(|y| {
                // A screen has fewer rows than an i32 holds.
                bolt_cells(bolt, y).eq(alacritty_cells(alacritty, usize::from(y)))
            })
    }

    /// Ends `alacritty_terminal`'s synchronized update, if one is under way,
    /// so that its screen has the bytes that the processor held back.
    fn end_sync(&mut self) {
        if self.processor.sync_timeout().pending_timeout() {
            self.processor.stop_sync(&mut self.alacritty);
        }
    }
}

/// A cell as the screens are compared by.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Cell<'a> {
    /// The second half of a wide character.
    Tail,
    /// The last cell of a row, left for a wide character.
    Head,
    /// A character, a space for an empty cell, with the zero-width ones
    /// that the cell holds.
    Text(char, &'a [char]),
}

impl Cell<'_> {
    fn text(self) -> String {
        match self {
            Self::Tail => TAIL.to_owned(),
            Self::Head => HEAD.to_owned(),
            Self::Text(c, rest) => std::iter::once(c).chain(rest.iter().copied()).collect(),
        }
    }
}

/// The text of the cell at column `x` of row `y` of the VT core's screen,
/// as the screens are compared by.
pub(crate) fn bolt_cell_text(terminal: &Terminal, x: u16, y: u16) -> String {
    bolt_cells(terminal, y)
        .nth(usize::from(x))
        .map_or_else(|| " ".to_owned(), Cell::text)
}

/// The cells of row `y` of the VT core's screen.
fn bolt_cells(terminal: &Terminal, y: u16) -> impl Iterator<Item = Cell<'_>> {
    let (page, row) = terminal.screen_rows().screen_row(y);
    (0..)
        .zip(page.cells(row))
        .map(move |(x, cell)| match (cell.width(), cell.content()) {
            (Width::SpacerTail, _) => Cell::Tail,
            (Width::SpacerHead, _) => Cell::Head,
            (_, Content::Char('\0') | Content::BgPalette(_) | Content::BgRgb(_)) => {
                Cell::Text(' ', &[])
            }
            (_, Content::Char(c)) => Cell::Text(c, &[]),
            (_, Content::Grapheme(c)) => Cell::Text(c, page.grapheme(row, x)),
        })
}

/// The VT core's cursor, as [`Screen::cursor`] has it.
fn bolt_cursor(terminal: &Terminal) -> (usize, usize, bool) {
    let cursor = terminal.cursor();
    (
        usize::from(cursor.x),
        usize::from(cursor.y),
        cursor.pending_wrap,
    )
}

/// The cells of row `y` of `alacritty_terminal`'s screen.
fn alacritty_cells(term: &Term<VoidListener>, y: usize) -> impl Iterator<Item = Cell<'_>> {
    let grid = term.grid();
    // A screen has fewer rows than an i32 holds.
    let row = &grid[Line(y as i32)];
    (0..grid.columns()).map(move |x| {
        let cell = &row[Column(x)];
        if cell.flags.contains(Flags::LEADING_WIDE_CHAR_SPACER) {
            Cell::Head
        } else if cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
            Cell::Tail
        } else {
            // An empty cell, or the one where a tab started, which keeps
            // the tab for copies and shows a space.
            let c = if matches!(cell.c, '\0' | '\t') {
                ' '
            } else {
                cell.c
            };
            Cell::Text(c, cell.zerowidth().unwrap_or_default())
        }
    })
}

/// `alacritty_terminal`'s cursor, as [`Screen::cursor`] has it.
fn alacritty_cursor(term: &Term<VoidListener>) -> (usize, usize, bool) {
    let cursor = &term.grid().cursor;
    (
        cursor.point.column.0,
        cursor.point.line.0.max(0) as usize,
        cursor.input_needs_wrap,
    )
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
    fn the_fuzz_input_has_characters_for_bytes_past_ascii_and_no_osc() {
        assert_eq!(fuzz_input(b"a\x80\x81"), "a\u{4e2d}\u{e9}".as_bytes());
        // OSC, after controls and DEL too, which an escape sequence passes
        // over.
        for osc in [&b"\x1b]"[..], b"\x1b\x01]", b"\x1b\x7f]"] {
            assert_eq!(fuzz_input(osc).last(), Some(&b'^'), "{osc:?}");
        }
        // Not after CAN, nor as the final byte of another escape sequence.
        assert_eq!(
            fuzz_input(b"\x1b\x80]").last(),
            Some(&b'^'),
            "after a byte past ASCII"
        );
        for other in [&b"\x1b\x18]"[..], b"\x1b(]", b"]"] {
            assert_eq!(fuzz_input(other).last(), Some(&b']'), "{other:?}");
        }
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
