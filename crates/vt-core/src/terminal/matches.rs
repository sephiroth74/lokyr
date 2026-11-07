//! Matches: the spans that a search found, which the terminal keeps by the
//! search's id, a part at a time, each part's in reading order and the
//! parts by the number of their first row, so that all of them are in
//! reading order; and which the view shows as highlights, as the embedder
//! asks, the current one apart. Their cells are pins, which keep their text
//! as output scrolls it into the scrollback and a resize reflows it, and go
//! with it. However many they are, a snapshot finds those of the view by
//! a binary search.

use std::collections::BTreeMap;

use super::Terminal;
use super::selection::CellPin;

/// A span of a search's matches, as the view shows it: the cells of a row
/// of the view from a column to another, both included.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Highlighted {
    /// The row of the view.
    pub row: u16,
    /// The first column.
    pub first: u16,
    /// The last column.
    pub last: u16,
    /// Whether it is of the current match.
    pub current: bool,
}

/// A match's first and last cells, by their columns and their rows'
/// numbers.
pub type MatchCells = ((u16, i64), (u16, i64));

/// A match's first and last cells' pins.
type Span = (CellPin, CellPin);

/// The matches of a search that the terminal keeps.
#[derive(Debug)]
pub(super) struct Matches {
    id: u64,
    /// Each part's matches, in reading order, by the number of the part's
    /// first row as the search found it.
    parts: BTreeMap<i64, Vec<Span>>,
    /// Whether the view shows them.
    shown: bool,
    /// The current match, which the view shows apart, shown or not.
    current: Option<Span>,
}

impl Terminal {
    /// Keeps `found`, the matches of a part of the search `id`, each its
    /// first and last cells' pins, in reading order, whose first row was
    /// numbered `part` as the search found it, beside the search's others;
    /// [`Terminal::drop_matches`] lets them go. The view does not show
    /// them until [`Terminal::show_matches`].
    pub fn add_matches(&mut self, id: u64, part: i64, found: Vec<(CellPin, CellPin)>) {
        let at = match self.matches.iter().position(|kept| kept.id == id) {
            Some(at) => at,
            None => {
                self.matches.push(Matches {
                    id,
                    parts: BTreeMap::new(),
                    shown: false,
                    current: None,
                });
                self.matches.len() - 1
            }
        };
        self.matches[at]
            .parts
            .entry(part)
            .or_default()
            .extend(found);
    }

    /// Lets the matches of the search `id` go, and its current match.
    pub fn drop_matches(&mut self, id: u64) {
        let Some(at) = self.matches.iter().position(|kept| kept.id == id) else {
            return;
        };
        let gone = self.matches.swap_remove(at);
        for (start, end) in gone.parts.into_values().flatten().chain(gone.current) {
            self.unpin_cell(&start);
            self.unpin_cell(&end);
        }
    }

    /// The matches of the search `id`, in reading order, from the one at
    /// `first`, `count` at most, where their cells are now, those whose
    /// cells left gone.
    pub fn matches_of(&self, id: u64, first: usize, count: usize) -> Vec<MatchCells> {
        let Some(kept) = self.matches.iter().find(|kept| kept.id == id) else {
            return Vec::new();
        };
        kept.parts
            .values()
            .flatten()
            .filter_map(|span| self.span_cells(span))
            .skip(first)
            .take(count)
            .collect()
    }

    /// Shows the matches of the search `id`, or hides them, as `all` says,
    /// and `current`'s cells, from its first to its last, apart, as the
    /// current match, if it gives one, which the terminal pins, as they
    /// keep their text. Whether the terminal keeps the search's matches.
    pub fn show_matches(&mut self, id: u64, all: bool, current: Option<MatchCells>) -> bool {
        let Some(at) = self.matches.iter().position(|kept| kept.id == id) else {
            return false;
        };
        if let Some((start, end)) = self.matches[at].current.take() {
            self.unpin_cell(&start);
            self.unpin_cell(&end);
        }
        let pinned = current
            .filter(|&(from, to)| (from.1, from.0) <= (to.1, to.0))
            .and_then(|(from, to)| {
                let start = self.pin_cell(from)?;
                match self.pin_cell(to) {
                    Some(end) => Some((start, end)),
                    None => {
                        self.unpin_cell(&start);
                        None
                    }
                }
            });
        let kept = &mut self.matches[at];
        kept.shown = all;
        kept.current = pinned;
        true
    }

    /// The spans that the view shows of the matches shown, row by row, and
    /// of the current matches, apart; nothing, and no cost, without them.
    /// Those of the view are found by a binary search, as their matches lie
    /// in reading order: of their last cells' rows, of those whose text
    /// left before the others, as the oldest rows leave first.
    pub(super) fn highlighted_view(&self) -> Vec<Highlighted> {
        if self
            .matches
            .iter()
            .all(|kept| !kept.shown && kept.current.is_none())
        {
            return Vec::new();
        }
        let top = self.origin().top;
        let bottom = top + i64::from(self.rows) - 1;
        let mut spans = Vec::new();
        for kept in &self.matches {
            let current = kept.current.as_ref().and_then(|span| self.span_cells(span));
            if kept.shown {
                let parts: Vec<&Vec<Span>> = kept
                    .parts
                    .values()
                    .filter(|part| !part.is_empty())
                    .collect();
                // The row of a match's last cell; before every row, of a
                // match whose text left.
                let row = |span: &Span| self.pinned_cell(&span.1).map_or(i64::MIN, |at| at.1);
                let first_part =
                    parts.partition_point(|part| part.last().is_some_and(|span| row(span) < top));
                let mut first = parts
                    .get(first_part)
                    .map_or(0, |part| part.partition_point(|span| row(span) < top));
                'parts: for part in parts.iter().skip(first_part) {
                    for span in &part[first..] {
                        let Some(cells) = self.span_cells(span) else {
                            continue;
                        };
                        if cells.0.1 > bottom {
                            break 'parts;
                        }
                        if Some(cells) != current {
                            self.spans_of(cells, (top, bottom), false, &mut spans);
                        }
                    }
                    first = 0;
                }
            }
            if let Some(cells) = current {
                self.spans_of(cells, (top, bottom), true, &mut spans);
            }
        }
        spans
    }

    /// Where `span`'s cells are now, if its text is still there, its first
    /// not after its last, as a move of the rows between them, as a scroll
    /// region's, can have put them.
    fn span_cells(&self, (start, end): &Span) -> Option<MatchCells> {
        let (start, end) = (self.pinned_cell(start)?, self.pinned_cell(end)?);
        ((start.1, start.0) <= (end.1, end.0)).then_some((start, end))
    }

    /// Pushes onto `spans` those of the match of `cells` on the rows of the
    /// view, from the row numbered `top` to `bottom`.
    fn spans_of(
        &self,
        (start, end): MatchCells,
        (top, bottom): (i64, i64),
        current: bool,
        spans: &mut Vec<Highlighted>,
    ) {
        let last_column = self.cols - 1;
        for row in start.1.max(top)..=end.1.min(bottom) {
            spans.push(Highlighted {
                // Within the view's rows, which a `u16` holds.
                row: (row - top) as u16,
                first: if row == start.1 { start.0 } else { 0 },
                last: if row == end.1 { end.0 } else { last_column },
                current,
            });
        }
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

    /// The highlights of a snapshot updated now, as `(row, first, last,
    /// current)`.
    fn shown(terminal: &mut Terminal) -> Vec<(u16, u16, u16, bool)> {
        let mut snapshot = Snapshot::default();
        terminal.snapshot(&mut snapshot);
        snapshot
            .highlights()
            .iter()
            .map(|span| (span.row, span.first, span.last, span.current))
            .collect()
    }

    /// Adds, as a part of the search `id` of the row numbered `part`, the
    /// matches of `cells`.
    fn add(terminal: &mut Terminal, id: u64, part: i64, cells: &[MatchCells]) {
        let pins = cells
            .iter()
            .map(|&(from, to)| {
                (
                    terminal.pin_cell(from).unwrap(),
                    terminal.pin_cell(to).unwrap(),
                )
            })
            .collect();
        terminal.add_matches(id, part, pins);
    }

    #[test]
    fn the_view_shows_its_matches_once_asked_and_the_current_one_apart() {
        let mut terminal = Terminal::new(20, 3, Options::default());
        feed(&mut terminal, "ab ab\r\nab\r\nnone");
        // Of two parts, the second's rows first, as the search goes.
        add(&mut terminal, 1, 1, &[((0, 1), (1, 1))]);
        add(&mut terminal, 1, 0, &[((0, 0), (1, 0)), ((3, 0), (4, 0))]);
        assert_eq!(shown(&mut terminal), []);
        assert_eq!(
            terminal.matches_of(1, 0, 10),
            [((0, 0), (1, 0)), ((3, 0), (4, 0)), ((0, 1), (1, 1))]
        );
        assert_eq!(terminal.matches_of(1, 1, 1), [((3, 0), (4, 0))]);
        // Shown, the second the current one.
        assert!(terminal.show_matches(1, true, Some(((3, 0), (4, 0)))));
        assert_eq!(
            shown(&mut terminal),
            [(0, 0, 1, false), (1, 0, 1, false), (0, 3, 4, true)]
        );
        // The current one alone.
        assert!(terminal.show_matches(1, false, Some(((0, 1), (1, 1)))));
        assert_eq!(shown(&mut terminal), [(1, 0, 1, true)]);
        // Dropped, its pins go, the current one's too.
        let pins = terminal.screen_rows().pins();
        terminal.drop_matches(1);
        assert_eq!(terminal.screen_rows().pins(), pins - 8);
        assert_eq!(shown(&mut terminal), []);
        assert!(terminal.matches_of(1, 0, 10).is_empty());
        assert!(!terminal.show_matches(1, true, None));
    }

    #[test]
    fn the_view_finds_its_matches_among_many_as_it_scrolls_and_their_text_leaves() {
        let options = Options {
            scrollback: Scrollback {
                limit: 64 << 20,
                ..Scrollback::default()
            },
            ..Options::default()
        };
        // Wide, so that the rows take many pages, which the scrollback's
        // limit lets go one at a time.
        let mut terminal = Terminal::new(500, 4, options);
        // A match at the start of every row of 2000, in parts of 100, the
        // last ones first.
        let text: Vec<String> = (0..2000).map(|n| format!("m{n}")).collect();
        feed(&mut terminal, &text.join("\r\n"));
        let first = terminal.row_number(0);
        for part in (0..20).rev() {
            let rows = (part * 100..part * 100 + 100).map(|n| first + n);
            let cells: Vec<MatchCells> = rows.map(|row| ((0, row), (1, row))).collect();
            add(&mut terminal, 1, first + part * 100, &cells);
        }
        terminal.show_matches(1, true, None);
        let screen = [0, 1, 2, 3].map(|row| (row, 0, 1, false));
        assert_eq!(shown(&mut terminal), screen);
        // Up into the scrollback, its rows' matches.
        terminal.scroll_viewport(Scroll::Up(1500));
        assert_eq!(shown(&mut terminal), screen);
        // The scrollback's limit takes the oldest rows, whose matches go;
        // the view's still show, at the bottom and at the top.
        terminal.scroll_viewport(Scroll::Bottom);
        terminal.set_scrollback_limit(0);
        let kept = terminal.matches_of(1, 0, usize::MAX).len();
        assert!((4..2000).contains(&kept), "{kept} kept");
        assert_eq!(shown(&mut terminal), screen);
        terminal.scroll_viewport(Scroll::Top);
        assert_eq!(shown(&mut terminal), screen);
    }

    #[test]
    fn a_match_across_rows_shows_a_span_on_each_and_none_on_the_other_screen() {
        let mut terminal = Terminal::new(10, 4, Options::default());
        // `abcdefghij` and `klmn`, one line wrapped.
        feed(&mut terminal, "abcdefghijklmn");
        add(&mut terminal, 1, 0, &[((8, 0), (1, 1))]);
        terminal.show_matches(1, true, None);
        assert_eq!(shown(&mut terminal), [(0, 8, 9, false), (1, 0, 1, false)]);
        feed(&mut terminal, "\x1b[?1049h");
        assert_eq!(shown(&mut terminal), []);
        feed(&mut terminal, "\x1b[?1049l");
        assert_eq!(shown(&mut terminal), [(0, 8, 9, false), (1, 0, 1, false)]);
    }
}
