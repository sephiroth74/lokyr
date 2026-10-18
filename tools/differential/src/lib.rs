//! Differential tests of the VT core: the same bytes, fed to it and to
//! `alacritty_terminal`, whose screens are then compared, row by row, with
//! the cursor.
//!
//! A screen is compared as its text: each row's characters, an empty cell
//! read as a space, as is the cell where `alacritty_terminal` keeps a tab,
//! and the second half of a wide character left out, with no spaces at the
//! end, and the cursor's column and row. Attributes,
//! colors and the scrollback are left out, as the two cores keep them in
//! ways of their own.

#![forbid(unsafe_code)]

use alacritty_terminal::event::VoidListener;
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line};
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::test::TermSize;
use alacritty_terminal::term::{Config, Term};
use alacritty_terminal::vte::ansi::Processor;
use bolt_vt_core::parser::Parser;
use bolt_vt_core::terminal::{Options, Terminal};

/// A screen as the tests compare it: its rows' text, and the cursor's
/// column and row, from 0.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Screen {
    /// Each row's text.
    pub rows: Vec<String>,
    /// The cursor's column and row.
    pub cursor: (usize, usize),
}

impl Screen {
    /// The first row where `self` and `other` differ, with both, or the
    /// cursors, if only they do.
    pub fn first_difference(&self, other: &Screen) -> Option<String> {
        let rows = self.rows.iter().zip(&other.rows).enumerate();
        if let Some((y, (ours, theirs))) = rows.into_iter().find(|(_, (a, b))| a != b) {
            return Some(format!("row {y}: {ours:?} against {theirs:?}"));
        }
        (self.cursor != other.cursor)
            .then(|| format!("cursor {:?} against {:?}", self.cursor, other.cursor))
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
        let cursor = terminal.cursor();
        Screen {
            rows: (0..terminal.rows()).map(|y| terminal.row_text(y)).collect(),
            cursor: (usize::from(cursor.x), usize::from(cursor.y)),
        }
    }

    /// `alacritty_terminal`'s screen, read as the VT core's is.
    pub fn alacritty(&self) -> Screen {
        let grid = self.alacritty.grid();
        let rows = (0..grid.screen_lines())
            .map(|y| {
                // A screen has fewer rows than an i32 holds.
                let row = &grid[Line(y as i32)];
                let mut text = String::new();
                for x in 0..grid.columns() {
                    let cell = &row[Column(x)];
                    if cell
                        .flags
                        .intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER)
                    {
                        continue;
                    }
                    // An empty cell, or the one where a tab started, which
                    // keeps the tab for copies and shows a space.
                    text.push(if matches!(cell.c, '\0' | '\t') {
                        ' '
                    } else {
                        cell.c
                    });
                    text.extend(cell.zerowidth().into_iter().flatten());
                }
                text.truncate(text.trim_end_matches(' ').len());
                text
            })
            .collect();
        let point = grid.cursor.point;
        Screen {
            rows,
            cursor: (point.column.0, point.line.0.max(0) as usize),
        }
    }

    /// The first difference between the two screens, if any.
    pub fn difference(&self) -> Option<String> {
        self.bolt().first_difference(&self.alacritty())
    }
}
