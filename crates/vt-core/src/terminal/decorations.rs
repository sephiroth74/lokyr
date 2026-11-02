//! Decorations: spans of a look over a range of cells, which an embedder
//! lays over the text, as a search's matches or a link under the pointer.
//! A decoration holds two pins, its first and its last cell, so that it
//! keeps its text as output scrolls it into the scrollback and a resize
//! reflows it, and goes with it, as the scrollback's limit or a clearing
//! takes its rows. The snapshot carries the spans that the view shows.

use super::Terminal;
use super::selection::CellPin;
use crate::page::{Rgb, Underline};

/// The most decorations that a terminal keeps, of both screens: each
/// snapshot looks at every one where they are, which costs with their
/// number.
pub const MOST_DECORATIONS: usize = 4096;

/// How a decoration looks.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Look {
    /// The line under its cells, if any.
    pub underline: Underline,
    /// The underline's color; the text's without one.
    pub underline_color: Option<Rgb>,
    /// The cells' background, in place of the text's, if any.
    pub background: Option<Rgb>,
}

/// A span of a decoration, as the view shows it: the cells of a row of the
/// view from a column to another, both included.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Decorated {
    /// The decoration's id.
    pub id: u64,
    /// The row of the view.
    pub row: u16,
    /// The first column.
    pub first: u16,
    /// The last column.
    pub last: u16,
    /// How it looks.
    pub look: Look,
}

/// A decoration that a terminal keeps.
#[derive(Debug)]
pub(super) struct Decoration {
    id: u64,
    start: CellPin,
    end: CellPin,
    look: Look,
}

impl Terminal {
    /// Decorates the cells from `from` to `to`, in reading order, each by
    /// its column and its row's number, the first not after the last, with
    /// `look`, as the decoration `id`, which the caller numbers, in place of
    /// one of that id. Whether it took it: the active screen holds both
    /// cells, and the terminal keeps fewer than [`MOST_DECORATIONS`].
    pub fn decorate(&mut self, id: u64, from: (u16, i64), to: (u16, i64), look: Look) -> bool {
        self.undecorate(id);
        if (to.1, to.0) < (from.1, from.0) || self.decorations.len() >= MOST_DECORATIONS {
            return false;
        }
        let Some(start) = self.pin_cell(from) else {
            return false;
        };
        let Some(end) = self.pin_cell(to) else {
            self.unpin_cell(&start);
            return false;
        };
        self.decorations.push(Decoration {
            id,
            start,
            end,
            look,
        });
        true
    }

    /// Takes the decoration `id` away, if the terminal keeps it.
    pub fn undecorate(&mut self, id: u64) {
        if let Some(at) = self
            .decorations
            .iter()
            .position(|decoration| decoration.id == id)
        {
            let decoration = self.decorations.swap_remove(at);
            self.unpin_cell(&decoration.start);
            self.unpin_cell(&decoration.end);
        }
    }

    /// The ids of the decorations that the terminal keeps, of both
    /// screens, but those whose text left.
    pub fn decorations(&self) -> Vec<u64> {
        self.decorations
            .iter()
            .filter(|decoration| !self.is_lost(&decoration.start) && !self.is_lost(&decoration.end))
            .map(|decoration| decoration.id)
            .collect()
    }

    /// The spans that the view shows of the active screen's decorations,
    /// row by row; the decorations whose text left go. Nothing, and no
    /// cost, without decorations.
    pub(super) fn decorated_view(&mut self) -> Vec<Decorated> {
        if self.decorations.is_empty() {
            return Vec::new();
        }
        let gone: Vec<u64> = self
            .decorations
            .iter()
            .filter(|decoration| self.is_lost(&decoration.start) || self.is_lost(&decoration.end))
            .map(|decoration| decoration.id)
            .collect();
        for id in gone {
            self.undecorate(id);
        }
        let top = self.origin().top;
        let bottom = top + i64::from(self.rows) - 1;
        let last_column = self.cols - 1;
        let mut spans = Vec::new();
        for decoration in &self.decorations {
            let (Some(start), Some(end)) = (
                self.pinned_cell(&decoration.start),
                self.pinned_cell(&decoration.end),
            ) else {
                continue;
            };
            // A move of the rows between its ends, as a scroll region's, can
            // have put them out of order.
            if (end.1, end.0) < (start.1, start.0) {
                continue;
            }
            for row in start.1.max(top)..=end.1.min(bottom) {
                spans.push(Decorated {
                    id: decoration.id,
                    // Within the view's rows, which a `u16` holds.
                    row: (row - top) as u16,
                    first: if row == start.1 { start.0 } else { 0 },
                    last: if row == end.1 { end.0 } else { last_column },
                    look: decoration.look,
                });
            }
        }
        spans
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::page::Scrollback;
    use crate::parser::Parser;
    use crate::terminal::{Options, Scroll, Snapshot};

    fn feed(terminal: &mut Terminal, text: &str) {
        Parser::new().advance(terminal, text.as_bytes());
    }

    /// The decorations' spans of a snapshot updated now, as `(row, first,
    /// last)`.
    fn shown(terminal: &mut Terminal) -> Vec<(u16, u16, u16)> {
        let mut snapshot = Snapshot::default();
        terminal.snapshot(&mut snapshot);
        snapshot
            .decorations()
            .iter()
            .map(|span| (span.row, span.first, span.last))
            .collect()
    }

    const UNDERLINED: Look = Look {
        underline: Underline::Single,
        underline_color: None,
        background: None,
    };

    #[test]
    fn a_decoration_keeps_its_text_as_output_scrolls_it_and_the_view_moves() {
        let mut terminal = Terminal::new(20, 3, Options::default());
        feed(&mut terminal, "find the match here\r\nnext");
        // `match`, on the row numbered 0.
        assert!(terminal.decorate(7, (9, 0), (13, 0), UNDERLINED));
        assert_eq!(shown(&mut terminal), [(0, 9, 13)]);
        // Two rows of output: the view shows it no more, and keeps it.
        feed(&mut terminal, "\r\na\r\nb\r\nc");
        assert_eq!(terminal.origin().top, 2);
        assert_eq!(shown(&mut terminal), []);
        assert_eq!(terminal.decorations(), [7]);
        // The view up shows it at its row.
        terminal.scroll_viewport(Scroll::Up(2));
        assert_eq!(shown(&mut terminal), [(0, 9, 13)]);
        terminal.scroll_viewport(Scroll::Bottom);
        // Undecorated, its pins go.
        let pins = terminal.screen_rows().pins();
        terminal.undecorate(7);
        assert_eq!(terminal.screen_rows().pins(), pins - 2);
        assert!(terminal.decorations().is_empty());
    }

    #[test]
    fn a_decoration_across_rows_shows_a_span_on_each_and_follows_a_reflow() {
        let mut terminal = Terminal::new(10, 4, Options::default());
        // `abcdefghij` and `klmn`, one line wrapped.
        feed(&mut terminal, "abcdefghijklmn");
        assert!(terminal.decorate(1, (8, 0), (1, 1), UNDERLINED));
        assert_eq!(shown(&mut terminal), [(0, 8, 9), (1, 0, 1)]);
        // Wider, the line is one row, and so is the decoration.
        terminal.resize(20, 4);
        assert_eq!(shown(&mut terminal), [(0, 8, 11)]);
    }

    #[test]
    fn a_decoration_goes_with_its_text_and_stays_with_its_screen() {
        let options = Options {
            scrollback: Scrollback {
                limit: 0,
                ..Scrollback::default()
            },
            ..Options::default()
        };
        let mut terminal = Terminal::new(10, 3, options);
        feed(&mut terminal, "x");
        assert!(terminal.decorate(1, (0, 0), (0, 0), UNDERLINED));
        // The other screen does not show it; its own does again.
        feed(&mut terminal, "\x1b[?1049h");
        assert_eq!(shown(&mut terminal), []);
        feed(&mut terminal, "\x1b[?1049l");
        assert_eq!(shown(&mut terminal), [(0, 0, 0)]);
        // The scrollback's limit takes its row, and it goes.
        let mut lines = 0;
        while !terminal.decorations().is_empty() {
            feed(&mut terminal, "\r\nmore");
            lines += 1;
            assert!(lines < 100_000, "the scrollback keeps every row");
        }
        assert_eq!(shown(&mut terminal), []);
    }

    #[test]
    fn a_decoration_needs_its_cells_in_order_and_a_room_among_the_most() {
        let mut terminal = Terminal::new(10, 3, Options::default());
        feed(&mut terminal, "abc");
        assert!(!terminal.decorate(1, (2, 0), (1, 0), UNDERLINED));
        assert!(!terminal.decorate(1, (0, 0), (0, 9), UNDERLINED));
        assert!(!terminal.decorate(1, (0, -1), (0, 0), UNDERLINED));
        // One of an id takes the place of the one before.
        assert!(terminal.decorate(1, (0, 0), (0, 0), UNDERLINED));
        assert!(terminal.decorate(1, (1, 0), (2, 0), UNDERLINED));
        assert_eq!(shown(&mut terminal), [(0, 1, 2)]);
        for id in 2..=u64::try_from(MOST_DECORATIONS).unwrap() {
            assert!(terminal.decorate(id, (0, 0), (0, 0), UNDERLINED));
        }
        assert!(!terminal.decorate(0, (0, 0), (0, 0), UNDERLINED));
    }
}
