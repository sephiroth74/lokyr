//! Matches: the spans that a search found, which the terminal keeps by the
//! search's id, a part at a time, each part's in reading order and the
//! parts by the number of their first row, so that all of them are in
//! reading order; and which the view shows as highlights, as the embedder
//! asks, the current one apart. Their cells are pins, which keep their text
//! as output scrolls it into the scrollback and a resize reflows it, and go
//! with it; a match whose cells a program writes other text over where they
//! are, as a full-screen program redraws, is one no more. However many they
//! are, a snapshot finds those of the view by a binary search.

use std::collections::BTreeMap;
use std::hash::{DefaultHasher, Hash, Hasher};

use super::Terminal;
use super::selection::{CellPin, row_at};
use crate::page::{Content, Width};

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

/// A match's first and last cells' pins, and the print of its text as the
/// search found it.
type Span = (CellPin, CellPin, u64);

/// The first place in reading order of a match that keeps its text, after
/// those whose text left.
const KEPT: (i64, u16) = (i64::MIN, 1);

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
        let mut spans = Vec::with_capacity(found.len());
        for (start, end) in found {
            match self.span_print(&start, &end) {
                Some(print) => spans.push((start, end, print)),
                None => {
                    self.unpin_cell(&start);
                    self.unpin_cell(&end);
                }
            }
        }
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
            .extend(spans);
    }

    /// Lets the matches of the search `id` go, and its current match.
    pub fn drop_matches(&mut self, id: u64) {
        let Some(at) = self.matches.iter().position(|kept| kept.id == id) else {
            return;
        };
        let gone = self.matches.swap_remove(at);
        for (start, end, _) in gone.parts.into_values().flatten().chain(gone.current) {
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
        // Those whose text left come first, as the oldest rows leave first.
        let gone = self.spans_before(kept, KEPT);
        kept.parts
            .values()
            .flatten()
            .skip(gone)
            .filter_map(|span| self.kept_cells(span))
            .skip(first)
            .take(count)
            .collect()
    }

    /// The current match of the search `id`, where its cells are now, and
    /// its place among the search's matches now, from 0, those whose text
    /// left not counted; `None` of none, or of one whose text left. And how
    /// many of the search's matches keep their text.
    pub fn current_match(&self, id: u64) -> (Option<(usize, MatchCells)>, usize) {
        let Some(kept) = self.matches.iter().find(|kept| kept.id == id) else {
            return (None, 0);
        };
        let gone = self.spans_before(kept, KEPT);
        let total: usize = kept.parts.values().map(Vec::len).sum();
        let current = kept
            .current
            .as_ref()
            .and_then(|span| self.kept_cells(span))
            .map(|cells| {
                let before = self.spans_before(kept, (cells.0.1, cells.0.0));
                (before.saturating_sub(gone), cells)
            });
        (current, total - gone)
    }

    /// Where a match starts in reading order, its first cell's row then its
    /// column, of one whose text left before every other's.
    fn order(&self, span: &Span) -> (i64, u16) {
        self.pinned_cell(&span.0)
            .map_or((i64::MIN, 0), |(column, row)| (row, column))
    }

    /// How many of `kept`'s matches start before `at` in reading order, by
    /// a binary search of its parts, then of the part's matches.
    fn spans_before(&self, kept: &Matches, at: (i64, u16)) -> usize {
        let parts: Vec<&Vec<Span>> = kept
            .parts
            .values()
            .filter(|part| !part.is_empty())
            .collect();
        let whole =
            parts.partition_point(|part| part.last().is_some_and(|span| self.order(span) < at));
        let within = parts
            .get(whole)
            .map_or(0, |part| part.partition_point(|span| self.order(span) < at));
        parts[..whole].iter().map(|part| part.len()).sum::<usize>() + within
    }

    /// Shows the matches of the search `id`, or hides them, as `all` says,
    /// and `current`'s cells, from its first to its last, apart, as the
    /// current match, if it gives one, which the terminal pins, as they
    /// keep their text. Whether the terminal keeps the search's matches.
    pub fn show_matches(&mut self, id: u64, all: bool, current: Option<MatchCells>) -> bool {
        let Some(at) = self.matches.iter().position(|kept| kept.id == id) else {
            return false;
        };
        if let Some((start, end, _)) = self.matches[at].current.take() {
            self.unpin_cell(&start);
            self.unpin_cell(&end);
        }
        let pinned = current
            .filter(|&(from, to)| (from.1, from.0) <= (to.1, to.0))
            .and_then(|(from, to)| {
                let print = self.text_print((from, to));
                let start = self.pin_cell(from)?;
                match (self.pin_cell(to), print) {
                    (Some(end), Some(print)) => Some((start, end, print)),
                    (end, _) => {
                        self.unpin_cell(&start);
                        if let Some(end) = end {
                            self.unpin_cell(&end);
                        }
                        None
                    }
                }
            });
        let holds = pinned.is_some();
        let kept = &mut self.matches[at];
        kept.shown = all;
        kept.current = pinned;
        // The view stays where it is as the program prints on, at the
        // screen too, until it is scrolled or the search ends.
        if holds {
            self.pin_viewport();
        }
        true
    }

    /// Whether a search shows its current match, which holds the view where
    /// it is.
    pub(super) fn holds_view(&self) -> bool {
        self.matches.iter().any(|kept| kept.current.is_some())
    }

    /// Whether output brings the view back to the screen now: as
    /// [`Terminal::scroll_on_output`] says, but while a search shows its
    /// current match, as a find bar does, whose view stays where the search
    /// put it as the program prints on.
    pub fn output_scrolls_view(&self) -> bool {
        self.scroll_on_output && !self.holds_view()
    }

    /// The spans that the view shows of the matches shown, row by row, and
    /// of the current matches, apart, of those whose cells hold the text
    /// that the search found; nothing, and no cost, without them. Those of
    /// the view are found by a binary search, as their matches lie in
    /// reading order: of their last cells' rows, of those whose text left
    /// before the others, as the oldest rows leave first.
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
            let current = kept.current.as_ref().and_then(|span| self.kept_cells(span));
            if kept.shown {
                for (part, index, cells) in self.in_view(kept, (top, bottom)) {
                    let span = &kept.parts[&part][index];
                    if Some(cells) != current && self.span_print(&span.0, &span.1) == Some(span.2) {
                        self.spans_of(cells, (top, bottom), false, &mut spans);
                    }
                }
            }
            if let Some(cells) = current {
                self.spans_of(cells, (top, bottom), true, &mut spans);
            }
        }
        spans
    }

    /// Where `span`'s cells are now, if they are still there, its first
    /// not after its last, as a move of the rows between them, as a scroll
    /// region's, can have put them.
    fn span_cells(&self, (start, end, _): &Span) -> Option<MatchCells> {
        self.pins_cells(start, end)
    }

    /// Where the cells of the pins `start` and `end` are now, as
    /// [`Terminal::span_cells`] tells them.
    fn pins_cells(&self, start: &CellPin, end: &CellPin) -> Option<MatchCells> {
        let (start, end) = (self.pinned_cell(start)?, self.pinned_cell(end)?);
        ((start.1, start.0) <= (end.1, end.0)).then_some((start, end))
    }

    /// Where `span`'s cells are now, if they still hold the text that the
    /// search found there.
    fn kept_cells(&self, span: &Span) -> Option<MatchCells> {
        let cells = self.span_cells(span)?;
        (self.span_print(&span.0, &span.1)? == span.2).then_some(cells)
    }

    /// The print of the text of the cells from the pin `start` to `end`:
    /// of a match on a row, read where the pins are, without a walk of the
    /// list; of one across rows, as [`Terminal::text_print`] reads it.
    fn span_print(&self, start: &CellPin, end: &CellPin) -> Option<u64> {
        let (from, to) = (self.pin_place(start)?, self.pin_place(end)?);
        if (from.page, from.row) == (to.page, to.row) && from.col <= to.col {
            let row = self.screen().rows.row_of(from.page, from.row)?;
            let mut hasher = DefaultHasher::new();
            for x in from.col..=to.col.min(row.cols() - 1) {
                hash_cell(&mut hasher, row.cell(x).content(), row.grapheme(x));
            }
            return Some(hasher.finish());
        }
        self.text_print(self.pins_cells(start, end)?)
    }

    /// The print of the text of the cells from `start` to `end`, in reading
    /// order, a cell without a character a space; `None` of a row that the
    /// active screen does not hold.
    fn text_print(&self, (start, end): MatchCells) -> Option<u64> {
        let mut hasher = DefaultHasher::new();
        let rows = &self.screen().rows;
        for number in start.1..=end.1 {
            let row = row_at(rows, self.row_index(number)?)?;
            let first = if number == start.1 { start.0 } else { 0 };
            let last = if number == end.1 {
                end.0
            } else {
                row.cols() - 1
            };
            for x in first..=last.min(row.cols() - 1) {
                hash_cell(&mut hasher, row.cell(x).content(), row.grapheme(x));
            }
        }
        Some(hasher.finish())
    }

    /// The last column of the character whose first cell is `at`: the next
    /// of a wide one, within the row.
    fn character_end(&self, (column, row): (u16, i64)) -> u16 {
        let wide = self
            .row_index(row)
            .and_then(|index| row_at(&self.screen().rows, index))
            .is_some_and(|list_row| {
                column < list_row.cols() && list_row.cell(column).width() == Width::Wide
            });
        if wide && column + 1 < self.cols {
            column + 1
        } else {
            column
        }
    }

    /// Lets go the matches of the view whose cells hold other text than the
    /// search found there, as a program writes over them where they are,
    /// and a current match's, their pins going, so that the matches' reads
    /// and places leave them out; nothing, and no cost, without matches
    /// shown.
    pub(super) fn drop_rewritten_matches(&mut self) {
        if self
            .matches
            .iter()
            .all(|kept| !kept.shown && kept.current.is_none())
        {
            return;
        }
        let top = self.origin().top;
        let bottom = top + i64::from(self.rows) - 1;
        let mut gone = Vec::new();
        for at in 0..self.matches.len() {
            let kept = &self.matches[at];
            let rewritten: Vec<(i64, usize)> = self
                .in_view(kept, (top, bottom))
                .into_iter()
                .filter(|&(part, index, _)| {
                    let span = &kept.parts[&part][index];
                    self.span_print(&span.0, &span.1) != Some(span.2)
                })
                .map(|(part, index, _)| (part, index))
                .collect();
            let current_rewritten = kept.current.as_ref().is_some_and(|span| {
                self.span_cells(span).is_some() && self.span_print(&span.0, &span.1) != Some(span.2)
            });
            let kept = &mut self.matches[at];
            for (part, index) in rewritten.into_iter().rev() {
                if let Some(spans) = kept.parts.get_mut(&part) {
                    gone.push(spans.remove(index));
                }
            }
            if current_rewritten {
                gone.extend(kept.current.take());
            }
        }
        for (start, end, _) in gone {
            self.unpin_cell(&start);
            self.unpin_cell(&end);
        }
    }

    /// The matches of `kept` on the rows of the view, from the row numbered
    /// `top` to `bottom`: each's part, its place in the part, and its
    /// cells, found by a binary search of the parts, then of the first
    /// one's matches, as they lie in reading order.
    fn in_view(&self, kept: &Matches, (top, bottom): (i64, i64)) -> Vec<(i64, usize, MatchCells)> {
        let parts: Vec<(i64, &Vec<Span>)> = kept
            .parts
            .iter()
            .filter(|(_, part)| !part.is_empty())
            .map(|(part, spans)| (*part, spans))
            .collect();
        let row = |span: &Span| self.pinned_cell(&span.1).map_or(i64::MIN, |at| at.1);
        let first_part =
            parts.partition_point(|(_, part)| part.last().is_some_and(|span| row(span) < top));
        let mut first = parts
            .get(first_part)
            .map_or(0, |(_, part)| part.partition_point(|span| row(span) < top));
        let mut found = Vec::new();
        'parts: for (part, spans) in parts.iter().skip(first_part) {
            for (index, span) in spans.iter().enumerate().skip(first) {
                let Some(cells) = self.span_cells(span) else {
                    continue;
                };
                if cells.0.1 > bottom {
                    break 'parts;
                }
                found.push((*part, index, cells));
            }
            first = 0;
        }
        found
    }

    /// Pushes onto `spans` those of the match of `cells` on the rows of the
    /// view, from the row numbered `top` to `bottom`, its last character's
    /// both cells of a wide one, whose first half its last cell is.
    fn spans_of(
        &self,
        (start, end): MatchCells,
        (top, bottom): (i64, i64),
        current: bool,
        spans: &mut Vec<Highlighted>,
    ) {
        let last_column = self.cols - 1;
        let end = (self.character_end(end), end.1);
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

/// Adds to `hasher` a cell of `content` and `grapheme`, the codepoints of
/// its cluster after the first, a cell without a character a space.
fn hash_cell(hasher: &mut DefaultHasher, content: Content, grapheme: &[char]) {
    match content {
        Content::Char('\0') | Content::BgPalette(_) | Content::BgRgb(_) => ' '.hash(hasher),
        Content::Char(character) => character.hash(hasher),
        Content::Grapheme(character) => {
            character.hash(hasher);
            for codepoint in grapheme {
                codepoint.hash(hasher);
            }
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
    fn output_leaves_the_view_where_a_search_shows_its_current_match() {
        let options = Options {
            scroll_on_output: true,
            ..Options::default()
        };
        let mut terminal = Terminal::new(10, 3, options);
        feed(&mut terminal, "ab\r\nnone");
        add(&mut terminal, 1, 0, &[((0, 0), (1, 0))]);
        assert!(terminal.output_scrolls_view());
        // Shown without a current match, output scrolls the view still.
        terminal.show_matches(1, true, None);
        assert!(terminal.output_scrolls_view());
        terminal.show_matches(1, true, Some(((0, 0), (1, 0))));
        assert!(!terminal.output_scrolls_view());
        // Its end gives the view back to the output.
        terminal.drop_matches(1);
        assert!(terminal.output_scrolls_view());
        terminal.set_scroll_on_output(false);
        assert!(!terminal.output_scrolls_view());
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

    #[test]
    fn the_current_match_is_told_where_its_text_is_now_and_its_place() {
        let mut terminal = Terminal::new(20, 3, Options::default());
        feed(&mut terminal, "ab ab\r\nxx ab\r\nnone");
        let first = terminal.row_number(0);
        let found = [
            ((0, first), (1, first)),
            ((3, first), (4, first)),
            ((3, first + 1), (4, first + 1)),
        ];
        add(&mut terminal, 1, first, &found);
        assert_eq!(terminal.current_match(1), (None, 3));
        terminal.show_matches(1, true, Some(found[2]));
        assert_eq!(terminal.current_match(1), (Some((2, found[2])), 3));
        // A resize that wraps every row moves the matches' text, and the
        // current one is told where it is now, its place the same.
        terminal.resize(3, 3);
        let (current, kept) = terminal.current_match(1);
        let (place, (start, end)) = current.unwrap();
        assert_eq!((place, kept), (2, 3));
        assert_eq!(terminal.matches_of(1, 2, 1), [(start, end)]);
        assert_ne!((start, end), found[2]);
        assert_eq!(terminal.matches_of(1, 0, 3).len(), 3);
        // Of no search, nothing.
        assert_eq!(terminal.current_match(2), (None, 0));
    }

    #[test]
    fn a_current_match_whose_text_left_is_none_and_the_others_places_go_down() {
        let options = Options {
            scrollback: Scrollback {
                limit: 64 << 20,
                ..Scrollback::default()
            },
            ..Options::default()
        };
        let mut terminal = Terminal::new(500, 4, options);
        let text: Vec<String> = (0..2000).map(|n| format!("m{n}")).collect();
        feed(&mut terminal, &text.join("\r\n"));
        let first = terminal.row_number(0);
        let cells: Vec<MatchCells> = (0..2000)
            .map(|n| ((0, first + n), (1, first + n)))
            .collect();
        add(&mut terminal, 1, first, &cells);
        // The current one among the oldest, then the last.
        terminal.show_matches(1, true, Some(cells[1]));
        assert_eq!(terminal.current_match(1), (Some((1, cells[1])), 2000));
        terminal.set_scrollback_limit(0);
        let kept = terminal.matches_of(1, 0, usize::MAX);
        assert!((4..2000).contains(&kept.len()), "{} kept", kept.len());
        assert_eq!(terminal.current_match(1), (None, kept.len()));
        terminal.show_matches(1, true, Some(cells[1999]));
        let (current, count) = terminal.current_match(1);
        assert_eq!(current, Some((kept.len() - 1, cells[1999])));
        assert_eq!(count, kept.len());
        // The reads skip those whose text left alike.
        assert_eq!(terminal.matches_of(1, kept.len() - 1, 5), [cells[1999]]);
    }

    #[test]
    fn a_match_whose_text_a_program_writes_over_is_one_no_more() {
        let mut terminal = Terminal::new(20, 3, Options::default());
        feed(&mut terminal, "needle here\r\nneedle too");
        let found = [((0, 0), (5, 0)), ((0, 1), (5, 1))];
        add(&mut terminal, 1, 0, &found);
        terminal.show_matches(1, true, Some(found[1]));
        assert_eq!(shown(&mut terminal), [(0, 0, 5, false), (1, 0, 5, true)]);
        // The same text written again where it is keeps it.
        feed(&mut terminal, "\x1b[1;1Hneedle");
        assert_eq!(shown(&mut terminal), [(0, 0, 5, false), (1, 0, 5, true)]);
        // Other text over the first, an erase and a write as a full-screen
        // program redraws: it goes, from the reads and the places too.
        feed(&mut terminal, "\x1b[1;1H\x1b[2Kother text");
        assert_eq!(shown(&mut terminal), [(1, 0, 5, true)]);
        assert_eq!(terminal.matches_of(1, 0, 10), [found[1]]);
        assert_eq!(terminal.current_match(1), (Some((0, found[1])), 1));
        // The current one written over likewise.
        feed(&mut terminal, "\x1b[2;1H\x1b[2Knothing");
        assert_eq!(shown(&mut terminal), []);
        assert_eq!(terminal.current_match(1), (None, 0));
        let pins = terminal.screen_rows().pins();
        terminal.drop_matches(1);
        assert_eq!(terminal.screen_rows().pins(), pins);
    }

    #[test]
    fn the_alternate_screen_s_matches_show_not_over_another_program_s_text() {
        let mut terminal = Terminal::new(20, 3, Options::default());
        feed(&mut terminal, "\x1b[?1049hneedle");
        let found = [((0, terminal.row_number(0)), (5, terminal.row_number(0)))];
        add(&mut terminal, 1, found[0].0.1, &found);
        terminal.show_matches(1, true, None);
        assert_eq!(shown(&mut terminal), [(0, 0, 5, false)]);
        // Left, then a program of other text on it.
        feed(&mut terminal, "\x1b[?1049l");
        assert_eq!(shown(&mut terminal), []);
        feed(&mut terminal, "\x1b[?1049h");
        assert_eq!(shown(&mut terminal), []);
        feed(&mut terminal, "something");
        assert_eq!(shown(&mut terminal), []);
        assert!(terminal.matches_of(1, 0, 10).is_empty());
    }

    #[test]
    fn a_match_that_ends_in_a_wide_character_covers_both_its_cells() {
        let mut terminal = Terminal::new(20, 3, Options::default());
        feed(&mut terminal, "ab日本語cd");
        // `日本`, its last cell the first half of `本`.
        add(&mut terminal, 1, 0, &[((2, 0), (4, 0))]);
        terminal.show_matches(1, true, None);
        assert_eq!(shown(&mut terminal), [(0, 2, 5, false)]);
        // The current one alike.
        terminal.show_matches(1, false, Some(((2, 0), (4, 0))));
        assert_eq!(shown(&mut terminal), [(0, 2, 5, true)]);
    }

    #[test]
    fn output_leaves_the_view_at_a_current_match_on_the_screen() {
        let options = Options {
            scroll_on_output: true,
            ..Options::default()
        };
        let mut terminal = Terminal::new(10, 3, options);
        feed(&mut terminal, "one\r\nab\r\nthree");
        let row = terminal.row_number(terminal.screen_rows().scrollback_rows() + 1);
        add(&mut terminal, 1, row, &[((0, row), (1, row))]);
        terminal.show_matches(1, true, Some(((0, row), (1, row))));
        assert_eq!(shown(&mut terminal), [(1, 0, 1, true)]);
        // More rows than the screen's: the view stays at the match.
        feed(&mut terminal, "\r\nfour\r\nfive\r\nsix\r\nseven");
        assert_eq!(shown(&mut terminal), [(1, 0, 1, true)]);
        assert_eq!(terminal.origin().row(1), row);
        // Scrolled to the screen, the view follows it again.
        terminal.scroll_viewport(Scroll::Bottom);
        feed(&mut terminal, "\r\neight");
        assert_eq!(terminal.viewport_offset(), 0);
        assert!(shown(&mut terminal).is_empty());
        // The search's end lets the view go.
        terminal.show_matches(1, true, Some(((0, row), (1, row))));
        terminal.drop_matches(1);
        feed(&mut terminal, "\r\nnine");
        shown(&mut terminal);
        terminal.scroll_viewport(Scroll::Bottom);
        assert_eq!(terminal.viewport_offset(), 0);
    }
}
