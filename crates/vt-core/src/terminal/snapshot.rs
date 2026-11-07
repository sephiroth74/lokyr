//! Snapshots: what a renderer draws, copied from the terminal under the
//! lock, so that it can draw without the lock.

use super::{
    Colors, Cursor, CursorStyle, Decorated, Highlighted, InputModes, KeyboardFlags, Origin,
    Selected, Terminal,
};
use crate::page::{Cell, Content, ListRow, Page, RowFlag, Style, StyleId};

/// The view as a renderer draws it: its rows, the screen's or, when the
/// view is scrolled up, those of the scrollback above it, the cursor and
/// the colors.
///
/// A renderer keeps one, and brings it up to date with
/// [`Terminal::snapshot`] before each frame: only the rows that changed
/// since, or came into the view, are copied, and the others stay, moved as
/// the screen scrolled or the view moved. A renderer that falls behind gets
/// the latest state, never the ones in between.
#[derive(Clone, Debug, Default)]
pub struct Snapshot {
    /// Where the rows were at the last update.
    origin: Option<Origin>,
    cols: u16,
    lines: Vec<Line>,
    /// Which rows the last update copied.
    changed: Vec<bool>,
    /// How many rows the last update moved its lines up by; down when
    /// negative.
    scrolled: i32,
    /// How many rows the view is above the screen's top.
    offset: usize,
    /// Whether the last update found other colors, or reverse video.
    colors_changed: bool,
    cursor: SnapshotCursor,
    reverse_video: bool,
    smooth_scroll: bool,
    /// Whether the alternate screen is the one shown.
    alternate: bool,
    /// What the program asked of the keys and the mouse.
    input_modes: InputModes,
    keyboard_flags: KeyboardFlags,
    colors: Colors,
    /// The selection, as the view shows it.
    selection: Option<Selected>,
    /// The decorations' spans that the view shows.
    decorations: Vec<Decorated>,
    /// The spans of the searches' matches that the view shows.
    highlights: Vec<Highlighted>,
}

/// The cursor, as a renderer draws it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SnapshotCursor {
    /// Where it is.
    pub position: Cursor,
    /// Whether it shows (DECTCEM).
    pub visible: bool,
    /// The shape that the program asked for, if it asked.
    pub style: Option<CursorStyle>,
    /// Whether the program asked it to blink (mode 12).
    pub blink: bool,
}

/// A row of the screen, with what its cells need from the page's tables.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Line {
    cells: Vec<Cell>,
    /// The styles that the cells use, sorted by id.
    styles: Vec<(StyleId, Style)>,
    /// The codepoints after the first of each cluster of several, one
    /// cluster after the other.
    codepoints: Vec<char>,
    /// The clusters of several codepoints: the column, where the cluster's
    /// codepoints start in `codepoints`, and how many there are.
    clusters: Vec<(u16, u32, u32)>,
    soft_wrapped: bool,
    prompt_start: bool,
}

impl Snapshot {
    /// The screen's columns.
    pub fn cols(&self) -> u16 {
        self.cols
    }

    /// The view's rows, as many as the screen's.
    pub fn rows(&self) -> u16 {
        // The screen has at most `u16::MAX` rows.
        self.lines.len() as u16
    }

    /// Row `y` of the view.
    pub fn line(&self, y: u16) -> &Line {
        &self.lines[usize::from(y)]
    }

    /// How many rows the view is above the screen's top, as
    /// [`Terminal::viewport_offset`] tells: 0 when it shows the screen.
    pub fn offset(&self) -> usize {
        self.offset
    }

    /// The selection, as the view shows it, if there is one.
    pub fn selection(&self) -> Option<Selected> {
        self.selection
    }

    /// The spans of the decorations that the view shows, as the last
    /// update found them, row by row.
    pub fn decorations(&self) -> &[Decorated] {
        &self.decorations
    }

    /// The spans of the searches' matches that the view shows, as the last
    /// update found them, each search's, its current match's last.
    pub fn highlights(&self) -> &[Highlighted] {
        &self.highlights
    }

    /// Whether the last update copied row `y`, which the renderer then has
    /// to draw again.
    pub fn changed(&self, y: u16) -> bool {
        self.changed[usize::from(y)]
    }

    /// How many rows the last update moved the lines up by, as the screen
    /// scrolled or the view moved down, and down by, as a negative number,
    /// as the view moved up: the renderer can move what it drew of them
    /// too. On the alternate screen, a scroll region that leaves fewer rows
    /// out than it holds scrolls as the whole screen does, and counts so:
    /// the rows outside it are marked changed, and drawn again.
    pub fn scrolled(&self) -> i32 {
        self.scrolled
    }

    /// Where the rows were at the last update, the drawn frame's: their
    /// epoch, and the number of the view's first row, which the lines
    /// start at; `None` before the first.
    pub fn origin(&self) -> Option<Origin> {
        self.origin
    }

    /// The cursor, in the view: below the screen's row, by the view's
    /// offset, and hidden when the view is above its row.
    pub fn cursor(&self) -> SnapshotCursor {
        self.cursor
    }

    /// Whether the screen shows in reverse video (DECSCNM).
    pub fn reverse_video(&self) -> bool {
        self.reverse_video
    }

    /// Whether the program asked for smooth scrolling (DECSCLM).
    pub fn smooth_scroll(&self) -> bool {
        self.smooth_scroll
    }

    /// Whether the alternate screen is the one shown, as a program such as
    /// vim shows its own.
    pub fn alternate_screen(&self) -> bool {
        self.alternate
    }

    /// The modes that the program set of what the keys, the mouse, the
    /// focus and a paste send.
    pub fn input_modes(&self) -> InputModes {
        self.input_modes
    }

    /// The kitty keyboard protocol's enhancements that the program asked
    /// for.
    pub fn keyboard_flags(&self) -> KeyboardFlags {
        self.keyboard_flags
    }

    /// The colors to draw with.
    pub fn colors(&self) -> &Colors {
        &self.colors
    }

    /// Whether the last update found the colors or reverse video changed
    /// since the update before: the renderer then draws every row again,
    /// those that the update did not copy too.
    pub fn colors_changed(&self) -> bool {
        self.colors_changed
    }
}

impl Line {
    /// The cells.
    pub fn cells(&self) -> &[Cell] {
        &self.cells
    }

    /// The styles that the line's cells use, each once.
    pub fn styles(&self) -> impl Iterator<Item = &Style> + '_ {
        self.styles.iter().map(|(_, style)| style)
    }

    /// The style that `id`, the style of one of the cells, stands for.
    pub fn style(&self, id: StyleId) -> Style {
        match self.styles.binary_search_by_key(&id, |&(id, _)| id) {
            Ok(at) => self.styles[at].1,
            Err(_) => Style::DEFAULT,
        }
    }

    /// The codepoints after the first of the cluster at column `x`: none
    /// for a cluster of one.
    pub fn grapheme(&self, x: u16) -> &[char] {
        match self.clusters.binary_search_by_key(&x, |&(x, ..)| x) {
            Ok(at) => {
                let (_, start, len) = self.clusters[at];
                &self.codepoints[start as usize..(start + len) as usize]
            }
            Err(_) => &[],
        }
    }

    /// Whether the row's text goes on to the next row, as it wrapped there.
    pub fn soft_wrapped(&self) -> bool {
        self.soft_wrapped
    }

    /// Whether a prompt starts in the row, as shell integration marks it
    /// (OSC 133), for a renderer to show.
    pub fn prompt_start(&self) -> bool {
        self.prompt_start
    }

    /// Makes the line a copy of row `row` of `page`.
    fn copy(&mut self, page: &Page, row: u16) {
        self.fill(page, row, page.cells(row), |x| page.grapheme(row, x));
    }

    /// Makes the line a copy of `row`, of a warm page or of a cold one,
    /// which keeps its graphemes' tables without its cells.
    fn copy_list_row(&mut self, row: ListRow<'_>) {
        let (page, cells) = row.parts();
        let y = row.row();
        self.fill(page, y, cells, |x| page.grapheme_of(y, x));
    }

    /// Makes the line row `row` of `page`, with `cells`, and empty cells
    /// after them to the page's width, and the codepoints that `grapheme`
    /// gives a cluster's column.
    fn fill<'p>(
        &mut self,
        page: &'p Page,
        row: u16,
        cells: &[Cell],
        grapheme: impl Fn(u16) -> &'p [char],
    ) {
        let metadata = page.row(row);
        self.cells.clear();
        self.cells.extend_from_slice(cells);
        self.cells.resize(usize::from(page.cols()), Cell::EMPTY);
        self.styles.clear();
        if metadata.has(RowFlag::Styles) {
            // The ids first, sorted and once each, then their styles: the
            // line's vector, kept from update to update, needs no other.
            let ids = self.cells.iter().map(|cell| cell.style());
            self.styles.extend(
                ids.filter(|&id| id != StyleId::DEFAULT)
                    .map(|id| (id, Style::DEFAULT)),
            );
            self.styles.sort_unstable_by_key(|&(id, _)| id);
            self.styles.dedup_by_key(|&mut (id, _)| id);
            for (id, style) in &mut self.styles {
                *style = *page.styles().get(*id);
            }
        }
        self.codepoints.clear();
        self.clusters.clear();
        if metadata.has(RowFlag::Graphemes) {
            for (x, cell) in self.cells.iter().enumerate() {
                // A row has `u16::MAX` cells at most.
                let x = x as u16;
                if let Content::Grapheme(_) = cell.content() {
                    let rest = grapheme(x);
                    // A row has fewer than 2^32 codepoints in its clusters.
                    let start = self.codepoints.len() as u32;
                    self.codepoints.extend_from_slice(rest);
                    self.clusters.push((x, start, rest.len() as u32));
                }
            }
        }
        self.soft_wrapped = metadata.has(RowFlag::SoftWrapped);
        self.prompt_start = metadata.has(RowFlag::PromptStart);
    }
}

impl Terminal {
    /// Brings `snapshot` up to date with the view, and returns how many
    /// rows it copied: those that changed since the last update, as the
    /// rows mark it, those of the screen and those that left it for the
    /// scrollback once changed, and those that scrolling, of the screen or
    /// of the view, brought in. The rows that it copies are marked as
    /// drawn, so that the next update skips them until they change again. A
    /// snapshot of another screen, or of rows laid out again by a resize,
    /// copies every row.
    pub fn snapshot(&mut self, snapshot: &mut Snapshot) -> usize {
        let offset = self.settle_viewport();
        let cursor = self.cursor();
        let rows_shown = usize::from(self.rows);
        let in_view = usize::from(cursor.y) + offset;
        snapshot.cursor = SnapshotCursor {
            position: Cursor {
                y: u16::try_from(in_view).unwrap_or(u16::MAX),
                ..cursor
            },
            visible: self.cursor_visible() && in_view < rows_shown,
            style: self.cursor_style(),
            blink: self.cursor_blink(),
        };
        let reverse_video = self.reverse_video();
        snapshot.colors_changed =
            snapshot.reverse_video != reverse_video || snapshot.colors != self.colors;
        snapshot.reverse_video = reverse_video;
        snapshot.smooth_scroll = self.smooth_scroll();
        snapshot.alternate = self.alternate_active;
        snapshot.input_modes = self.input_modes();
        snapshot.keyboard_flags = self.keyboard_flags();
        if snapshot.colors != self.colors {
            snapshot.colors.clone_from(&self.colors);
        }
        let origin = self.origin();
        let rows = &mut self.screen_mut().rows;
        let (cols, height) = (rows.cols(), rows.rows());
        let lines = usize::from(height);
        let scrollback = rows.scrollback_rows();
        // How far the view's first row moved down since the last update,
        // as the lines that the snapshot has move up; up when negative.
        let moved = match snapshot.origin {
            Some(before)
                if before.epoch == origin.epoch
                    && snapshot.cols == cols
                    && snapshot.lines.len() == lines =>
            {
                origin.top.saturating_sub(before.top)
            }
            _ => i64::try_from(lines).unwrap_or(i64::MAX),
        };
        snapshot.origin = Some(origin);
        snapshot.offset = offset;
        snapshot.cols = cols;
        snapshot.lines.resize_with(lines, Line::default);
        snapshot.changed.clear();
        snapshot.changed.resize(lines, false);
        // At most the view's rows.
        let shift = moved.unsigned_abs().min(lines as u64) as usize;
        let down = moved < 0;
        if down {
            snapshot.lines.rotate_right(shift);
        } else {
            snapshot.lines.rotate_left(shift);
        }
        // The view has at most `u16::MAX` rows.
        snapshot.scrolled = if down { -(shift as i32) } else { shift as i32 };
        // The lines that came into the view: at its top when it moved up,
        // at its bottom otherwise.
        let fresh = |y: usize| if down { y < shift } else { y >= lines - shift };
        let mut copied = 0;
        // The rows of the scrollback in the view, at its top, and those of
        // them that changed on the screen before they left it.
        let above = offset.min(lines);
        if above > 0 {
            let first = scrollback - offset;
            let mut changed = Vec::new();
            for (y, row) in rows.rows_from(first).take(above).enumerate() {
                let dirty = row.metadata().has(RowFlag::Dirty);
                if fresh(y) || dirty {
                    snapshot.lines[y].copy_list_row(row);
                    snapshot.changed[y] = true;
                    copied += 1;
                }
                if dirty {
                    changed.push(first + y);
                }
            }
            for index in changed {
                rows.mark_drawn(index);
            }
        }
        for y in above..lines {
            // A row of the screen, `offset` rows down from the view's.
            let (page, row) = rows.screen_row_mut((y - offset) as u16);
            if fresh(y) || page.row(row).has(RowFlag::Dirty) {
                snapshot.lines[y].copy(page, row);
                snapshot.changed[y] = true;
                page.row_mut(row).set(RowFlag::Dirty, false);
                copied += 1;
            }
        }
        snapshot.selection = self.selection_in_snapshot(&snapshot.changed);
        snapshot.decorations = self.decorated_view();
        snapshot.highlights = self.highlighted_view();
        copied
    }
}

/// A snapshot of the view, copied whole, without marking anything, for
/// tests to compare an updated snapshot with.
#[cfg(test)]
pub(super) fn copy_of(terminal: &Terminal) -> Snapshot {
    let rows = terminal.screen_rows();
    let offset = terminal.viewport_offset();
    let lines = rows
        .rows_from(rows.scrollback_rows() - offset)
        .take(usize::from(rows.rows()))
        .map(|row| {
            let mut line = Line::default();
            line.copy_list_row(row);
            line
        })
        .collect::<Vec<_>>();
    let cursor = terminal.cursor();
    let y = usize::from(cursor.y) + offset;
    Snapshot {
        origin: Some(terminal.origin()),
        cols: rows.cols(),
        changed: vec![true; lines.len()],
        scrolled: 0,
        offset,
        colors_changed: false,
        cursor: SnapshotCursor {
            position: Cursor {
                y: u16::try_from(y).unwrap_or(u16::MAX),
                ..cursor
            },
            visible: terminal.cursor_visible() && y < lines.len(),
            style: terminal.cursor_style(),
            blink: terminal.cursor_blink(),
        },
        lines,
        reverse_video: terminal.reverse_video(),
        smooth_scroll: terminal.smooth_scroll(),
        alternate: terminal.is_alternate_screen(),
        input_modes: terminal.input_modes(),
        keyboard_flags: terminal.keyboard_flags(),
        colors: terminal.colors().clone(),
        selection: None,
        decorations: Vec::new(),
        highlights: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;
    use crate::page::{Capacity, Color, Scrollback};
    use crate::parser::Parser;
    use crate::terminal::{Options, Scroll};

    fn terminal(cols: u16, rows: u16) -> (Terminal, Parser) {
        (Terminal::new(cols, rows, Options::default()), Parser::new())
    }

    #[test]
    fn a_snapshot_tells_the_alternate_screen() {
        let (mut terminal, mut parser) = terminal(10, 3);
        let mut snapshot = Snapshot::default();
        terminal.snapshot(&mut snapshot);
        assert!(!snapshot.alternate_screen());
        feed(&mut terminal, &mut parser, "\x1b[?1049h");
        terminal.snapshot(&mut snapshot);
        assert!(snapshot.alternate_screen());
        assert!(copy_of(&terminal).alternate_screen());
        feed(&mut terminal, &mut parser, "\x1b[?1049l");
        terminal.snapshot(&mut snapshot);
        assert!(!snapshot.alternate_screen());
    }

    #[test]
    fn a_snapshot_tells_the_keys_and_the_mouse_s_modes() {
        let (mut terminal, mut parser) = terminal(10, 3);
        let mut snapshot = Snapshot::default();
        terminal.snapshot(&mut snapshot);
        assert_eq!(snapshot.input_modes(), terminal.input_modes());
        assert_eq!(snapshot.keyboard_flags(), KeyboardFlags::NONE);
        // Bracketed paste, the mouse's tracking, and the kitty protocol's
        // first flag.
        feed(&mut terminal, &mut parser, "\x1b[?2004h\x1b[?1003h\x1b[>1u");
        terminal.snapshot(&mut snapshot);
        assert!(snapshot.input_modes().bracketed_paste);
        assert_eq!(snapshot.input_modes(), terminal.input_modes());
        assert_eq!(snapshot.keyboard_flags(), terminal.keyboard_flags());
        assert_ne!(snapshot.keyboard_flags(), KeyboardFlags::NONE);
        assert_eq!(copy_of(&terminal).input_modes(), terminal.input_modes());
    }

    fn feed(terminal: &mut Terminal, parser: &mut Parser, text: &str) {
        parser.advance(terminal, text.as_bytes());
    }

    /// Whether `snapshot` shows what `copy`, a copy of the screen, shows.
    fn assert_same(snapshot: &Snapshot, copy: &Snapshot) {
        assert_eq!(
            (snapshot.cols(), snapshot.rows()),
            (copy.cols(), copy.rows())
        );
        for y in 0..copy.rows() {
            assert_eq!(snapshot.line(y), copy.line(y), "row {y}");
        }
        assert_eq!(snapshot.cursor(), copy.cursor());
        assert_eq!(snapshot.reverse_video(), copy.reverse_video());
        assert_eq!(snapshot.alternate_screen(), copy.alternate_screen());
        assert_eq!(snapshot.colors(), copy.colors());
    }

    fn changed_rows(snapshot: &Snapshot) -> Vec<u16> {
        (0..snapshot.rows())
            .filter(|&y| snapshot.changed(y))
            .collect()
    }

    #[test]
    fn an_unchanged_screen_copies_nothing() {
        let (mut terminal, mut parser) = terminal(10, 4);
        feed(&mut terminal, &mut parser, "one\r\ntwo");
        let mut snapshot = Snapshot::default();
        assert_eq!(
            terminal.snapshot(&mut snapshot),
            4,
            "the first copies every row"
        );
        assert_eq!(terminal.snapshot(&mut snapshot), 0);
        assert!(changed_rows(&snapshot).is_empty());
        // The cursor comes along without a row.
        feed(&mut terminal, &mut parser, "\x1b[3;5H\x1b[?25l");
        assert_eq!(terminal.snapshot(&mut snapshot), 0);
        let cursor = snapshot.cursor();
        assert_eq!(
            (cursor.position.x, cursor.position.y, cursor.visible),
            (4, 2, false)
        );
        assert_same(&snapshot, &copy_of(&terminal));
    }

    #[test]
    fn a_change_copies_its_row() {
        let (mut terminal, mut parser) = terminal(10, 4);
        let mut snapshot = Snapshot::default();
        terminal.snapshot(&mut snapshot);
        feed(&mut terminal, &mut parser, "\x1b[3;1Hthree");
        assert_eq!(terminal.snapshot(&mut snapshot), 1);
        assert_eq!(changed_rows(&snapshot), [2]);
        assert_eq!(snapshot.line(2).cells()[0].content(), Content::Char('t'));
    }

    #[test]
    fn scrolling_moves_the_lines_and_copies_those_that_come_in() {
        let (mut terminal, mut parser) = terminal(10, 4);
        feed(&mut terminal, &mut parser, "a\r\nb\r\nc\r\nd");
        let mut snapshot = Snapshot::default();
        terminal.snapshot(&mut snapshot);
        feed(&mut terminal, &mut parser, "\r\ne\r\nf");
        assert_eq!(terminal.snapshot(&mut snapshot), 2);
        assert_eq!(snapshot.scrolled(), 2);
        assert_eq!(changed_rows(&snapshot), [2, 3]);
        let first = |snapshot: &Snapshot, y| snapshot.line(y).cells()[0].content();
        assert_eq!(first(&snapshot, 0), Content::Char('c'));
        assert_eq!(first(&snapshot, 3), Content::Char('f'));
        assert_same(&snapshot, &copy_of(&terminal));
        // More than a screen: every line is new.
        feed(&mut terminal, &mut parser, &"\r\nx".repeat(9));
        assert_eq!(terminal.snapshot(&mut snapshot), 4);
        assert_same(&snapshot, &copy_of(&terminal));
    }

    #[test]
    fn another_screen_or_a_resize_copies_every_row() {
        let (mut terminal, mut parser) = terminal(10, 4);
        let mut snapshot = Snapshot::default();
        terminal.snapshot(&mut snapshot);
        feed(&mut terminal, &mut parser, "\x1b[?1049h");
        assert_eq!(terminal.snapshot(&mut snapshot), 4);
        feed(&mut terminal, &mut parser, "\x1b[?1049l");
        assert_eq!(terminal.snapshot(&mut snapshot), 4);
        terminal.resize(12, 5);
        assert_eq!(terminal.snapshot(&mut snapshot), 5);
        assert_eq!((snapshot.cols(), snapshot.rows()), (12, 5));
        assert_eq!(terminal.snapshot(&mut snapshot), 0);
        // A resize to the same size lays nothing out again.
        terminal.resize(12, 5);
        assert_eq!(terminal.snapshot(&mut snapshot), 0);
    }

    /// The text of row `y` of `snapshot`, without its spaces at the end.
    fn text(snapshot: &Snapshot, y: u16) -> String {
        let text: String = snapshot
            .line(y)
            .cells()
            .iter()
            .map(|cell| match cell.content() {
                Content::Char(c) if c != '\0' => c,
                _ => ' ',
            })
            .collect();
        text.trim_end().to_owned()
    }

    /// The first character of each row of `snapshot`.
    fn firsts(snapshot: &Snapshot) -> String {
        (0..snapshot.rows())
            .map(|y| text(snapshot, y).chars().next().unwrap_or(' '))
            .collect()
    }

    /// A terminal of 10 by 4 with `a` to `t` printed, a row each: `q` to
    /// `t` on the screen, and the others in the scrollback.
    fn lettered() -> (Terminal, Parser, Snapshot) {
        let (mut terminal, mut parser) = terminal(10, 4);
        let letters: Vec<String> = ('a'..='t').map(String::from).collect();
        feed(&mut terminal, &mut parser, &letters.join("\r\n"));
        let mut snapshot = Snapshot::default();
        terminal.snapshot(&mut snapshot);
        assert_eq!(firsts(&snapshot), "qrst");
        (terminal, parser, snapshot)
    }

    #[test]
    fn the_view_scrolls_through_the_scrollback_copying_the_rows_that_come_in() {
        let (mut terminal, _, mut snapshot) = lettered();
        terminal.scroll_viewport(Scroll::Up(2));
        assert_eq!(terminal.viewport_offset(), 2);
        assert_eq!(terminal.snapshot(&mut snapshot), 2);
        assert_eq!((snapshot.scrolled(), snapshot.offset()), (-2, 2));
        assert_eq!(changed_rows(&snapshot), [0, 1]);
        assert_eq!(firsts(&snapshot), "opqr");
        assert_same(&snapshot, &copy_of(&terminal));
        // The cursor, on the screen's last row, is below the view.
        assert!(!snapshot.cursor().visible);
        terminal.scroll_viewport(Scroll::PageUp);
        assert_eq!(terminal.snapshot(&mut snapshot), 4);
        assert_eq!(firsts(&snapshot), "klmn");
        terminal.scroll_viewport(Scroll::Down(1));
        assert_eq!(terminal.snapshot(&mut snapshot), 1);
        assert_eq!((snapshot.scrolled(), changed_rows(&snapshot)), (1, vec![3]));
        assert_eq!(firsts(&snapshot), "lmno");
        // The top, past which it goes no further.
        terminal.scroll_viewport(Scroll::Top);
        assert_eq!(terminal.viewport_offset(), 16);
        terminal.scroll_viewport(Scroll::Up(1));
        assert_eq!(terminal.viewport_offset(), 16);
        terminal.snapshot(&mut snapshot);
        assert_eq!(firsts(&snapshot), "abcd");
        terminal.scroll_viewport(Scroll::PageDown);
        terminal.snapshot(&mut snapshot);
        assert_eq!(firsts(&snapshot), "efgh");
        assert_same(&snapshot, &copy_of(&terminal));
        // Back at the screen, with the cursor.
        terminal.scroll_viewport(Scroll::Bottom);
        assert_eq!(terminal.viewport_offset(), 0);
        terminal.snapshot(&mut snapshot);
        assert_eq!(firsts(&snapshot), "qrst");
        let cursor = snapshot.cursor();
        assert!(cursor.visible);
        assert_eq!((cursor.position.x, cursor.position.y), (1, 3));
        assert_same(&snapshot, &copy_of(&terminal));
        assert_eq!(terminal.screen_rows().pins(), 0, "the view's pin went");
    }

    #[test]
    fn a_view_above_the_screen_keeps_its_rows_as_output_comes() {
        let (mut terminal, mut parser, mut snapshot) = lettered();
        terminal.scroll_viewport(Scroll::Up(2));
        terminal.snapshot(&mut snapshot);
        // `q` and `r`, the screen's first rows, are in the view: a change
        // of one copies it; a change below the view copies nothing.
        feed(&mut terminal, &mut parser, "\x1b[1;1HQ");
        assert_eq!(terminal.snapshot(&mut snapshot), 1);
        assert_eq!(changed_rows(&snapshot), [2]);
        feed(&mut terminal, &mut parser, "\x1b[4;1HT");
        assert_eq!(terminal.snapshot(&mut snapshot), 0);
        assert_eq!(firsts(&snapshot), "opQr");
        // Output scrolls the screen under the view, which keeps its rows
        // and copies nothing.
        feed(&mut terminal, &mut parser, "\r\nu\r\nv\r\nw");
        assert_eq!(terminal.viewport_offset(), 5);
        assert_eq!(terminal.snapshot(&mut snapshot), 0);
        assert_eq!(firsts(&snapshot), "opQr");
        assert_same(&snapshot, &copy_of(&terminal));
        // Back at the screen, the rows that come in, the one changed
        // below the view among them.
        terminal.scroll_viewport(Scroll::Bottom);
        assert_eq!(terminal.snapshot(&mut snapshot), 4);
        assert_eq!(firsts(&snapshot), "Tuvw");
        assert_same(&snapshot, &copy_of(&terminal));
    }

    #[test]
    fn the_alternate_screen_and_a_cleared_scrollback_bring_the_view_to_the_screen() {
        let (mut terminal, mut parser, mut snapshot) = lettered();
        terminal.scroll_viewport(Scroll::Up(2));
        feed(&mut terminal, &mut parser, "\x1b[?1049h");
        assert_eq!(terminal.viewport_offset(), 0);
        // The alternate screen has no scrollback to scroll.
        terminal.scroll_viewport(Scroll::Up(2));
        assert_eq!(terminal.viewport_offset(), 0);
        feed(&mut terminal, &mut parser, "\x1b[?1049l");
        terminal.snapshot(&mut snapshot);
        assert_eq!(firsts(&snapshot), "qrst");
        terminal.scroll_viewport(Scroll::Up(3));
        feed(&mut terminal, &mut parser, "\x1b[3J");
        assert_eq!(terminal.viewport_offset(), 0);
        terminal.snapshot(&mut snapshot);
        assert_eq!(firsts(&snapshot), "qrst");
        assert_same(&snapshot, &copy_of(&terminal));
        // ED 3 and output in one feed, as `clear` and a program's output
        // after it: the view follows the screen.
        feed(&mut terminal, &mut parser, "\r\nu\r\nv\r\nw");
        terminal.scroll_viewport(Scroll::Up(2));
        assert_eq!(terminal.viewport_offset(), 2);
        feed(&mut terminal, &mut parser, "\x1b[3J\r\nx\r\ny");
        assert_eq!(terminal.viewport_offset(), 0);
        terminal.snapshot(&mut snapshot);
        assert_eq!(firsts(&snapshot), "vwxy");
        assert_eq!(terminal.screen_rows().pins(), 0);
    }

    #[test]
    fn a_resize_keeps_the_view_on_its_rows() {
        let (mut terminal, _, mut snapshot) = lettered();
        terminal.scroll_viewport(Scroll::Up(2));
        for (cols, rows) in [(12, 4), (6, 3)] {
            terminal.resize(cols, rows);
            terminal.snapshot(&mut snapshot);
            assert_eq!(text(&snapshot, 0), "o", "at {cols} by {rows}");
            assert_same(&snapshot, &copy_of(&terminal));
        }
        // A taller screen that takes the view's first row shows the screen,
        // which the view follows from then on.
        terminal.resize(6, 8);
        assert_eq!(terminal.screen_rows().pins(), 0, "let go at once");
        terminal.snapshot(&mut snapshot);
        assert_eq!(terminal.viewport_offset(), 0);
        assert_eq!(firsts(&snapshot), "mnopqrst");
        assert_eq!(terminal.screen_rows().pins(), 0);
        assert_same(&snapshot, &copy_of(&terminal));
    }

    #[test]
    fn output_after_a_resize_that_takes_the_view_leaves_it_at_the_screen() {
        let (mut terminal, mut parser, mut snapshot) = lettered();
        terminal.scroll_viewport(Scroll::Up(2));
        // A taller screen, and output before the next snapshot.
        terminal.resize(10, 8);
        feed(&mut terminal, &mut parser, "\r\nu\r\nv\r\nw\r\nx");
        assert_eq!(terminal.viewport_offset(), 0);
        terminal.snapshot(&mut snapshot);
        assert_eq!(firsts(&snapshot), "qrstuvwx");
    }

    #[test]
    fn a_row_that_changed_before_it_left_the_screen_comes_into_the_view_as_it_is() {
        let (mut terminal, mut parser, mut snapshot) = lettered();
        // `q`, changed on the screen, then scrolled up into the
        // scrollback, with no update between.
        feed(&mut terminal, &mut parser, "\x1b[1;1HQ\x1b[4;2H\r\nu");
        terminal.scroll_viewport(Scroll::Up(1));
        terminal.snapshot(&mut snapshot);
        assert_eq!(firsts(&snapshot), "Qrst");
        assert_same(&snapshot, &copy_of(&terminal));
        assert_eq!(terminal.snapshot(&mut snapshot), 0, "drawn once");
    }

    #[test]
    fn a_view_whose_rows_leave_the_scrollback_keeps_its_top() {
        let page = Capacity::standard(10);
        let options = Options {
            scrollback: Scrollback {
                limit: 2 * page.bytes(),
                budget: None,
            },
            ..Options::default()
        };
        let (mut terminal, mut parser) = (Terminal::new(10, 4, options), Parser::new());
        let lines = |from: usize, count: usize| -> String {
            (from..from + count).map(|n| format!("{n}\r\n")).collect()
        };
        let rows = usize::from(page.rows);
        feed(&mut terminal, &mut parser, &lines(0, rows));
        terminal.scroll_viewport(Scroll::Top);
        let mut snapshot = Snapshot::default();
        terminal.snapshot(&mut snapshot);
        assert_eq!(text(&snapshot, 0), "0");
        // The oldest page leaves, with the view's first row.
        feed(&mut terminal, &mut parser, &lines(rows, 2 * rows));
        let scrollback = terminal.screen_rows().scrollback_rows();
        assert_eq!(terminal.viewport_offset(), scrollback);
        terminal.snapshot(&mut snapshot);
        assert_same(&snapshot, &copy_of(&terminal));
        let top = text(&snapshot, 0);
        assert_ne!(top, "0");
        // It stays at the top as output comes.
        feed(&mut terminal, &mut parser, &lines(3 * rows, 3));
        assert_eq!(terminal.snapshot(&mut snapshot), 0);
        assert_eq!(text(&snapshot, 0), top);
        assert_eq!(terminal.viewport_offset(), scrollback + 3);
    }

    #[test]
    fn an_update_tells_when_the_colors_changed() {
        let (mut terminal, mut parser) = terminal(10, 4);
        let mut snapshot = Snapshot::default();
        terminal.snapshot(&mut snapshot);
        for change in [
            "\x1b]4;1;rgb:00/ff/00\x07",
            "\x1b[?5h",
            "\x1b]11;rgb:10/20/30\x07",
        ] {
            feed(&mut terminal, &mut parser, change);
            assert_eq!(terminal.snapshot(&mut snapshot), 0, "{change:?}: no row");
            assert!(snapshot.colors_changed(), "{change:?}");
            assert_eq!(terminal.snapshot(&mut snapshot), 0);
            assert!(!snapshot.colors_changed(), "{change:?}, once");
        }
        // The same colors again change nothing.
        feed(
            &mut terminal,
            &mut parser,
            "\x1b]4;1;rgb:00/ff/00\x07\x1b[?5h",
        );
        terminal.snapshot(&mut snapshot);
        assert!(!snapshot.colors_changed());
    }

    #[test]
    fn a_line_has_its_styles_and_clusters() {
        let (mut terminal, mut parser) = terminal(10, 2);
        feed(
            &mut terminal,
            &mut parser,
            "\x1b[31mr\x1b[1;42mg\x1b[me\u{301}",
        );
        let mut snapshot = Snapshot::default();
        terminal.snapshot(&mut snapshot);
        let line = snapshot.line(0);
        let style = |x: usize| line.style(line.cells()[x].style());
        assert_eq!(style(0).fg, Color::Palette(1));
        assert_eq!((style(1).bg, style(1).bold), (Color::Palette(2), true));
        assert_eq!(style(2), Style::DEFAULT);
        assert_eq!(line.cells()[2].content(), Content::Grapheme('e'));
        assert_eq!(line.grapheme(2), ['\u{301}']);
        assert_eq!(line.grapheme(0), [] as [char; 0]);
        assert!(!line.soft_wrapped());
        // Each style once, however many cells use it.
        feed(
            &mut terminal,
            &mut parser,
            "\r\n\x1b[31mrrr\x1b[32mggg\x1b[31mrrr",
        );
        terminal.snapshot(&mut snapshot);
        assert_eq!(snapshot.line(1).styles.len(), 2);
        // A line that wrapped.
        feed(&mut terminal, &mut parser, "\x1b[m\r\n0123456789wrapped");
        terminal.snapshot(&mut snapshot);
        assert!(snapshot.line(0).soft_wrapped());
        assert!(!snapshot.line(1).soft_wrapped());
        // A prompt's start, which marks its row to draw.
        assert!(!snapshot.line(1).prompt_start());
        feed(&mut terminal, &mut parser, "\x1b]133;A\x07");
        assert_eq!(terminal.snapshot(&mut snapshot), 1);
        assert!(snapshot.line(1).prompt_start());
    }

    /// Pieces of output that change the screen in every way they can.
    const PIECES: &[&str] = &[
        "a",
        "hello world",
        "\r\n",
        "\n",
        "\r",
        "\t",
        "\x1b[H",
        "\x1b[4;3H",
        "\x1b[2J",
        "\x1b[3J",
        "\x1b[J",
        "\x1b[K",
        "\x1b[1K",
        "\x1b[2L",
        "\x1b[2M",
        "\x1b[2S",
        "\x1b[T",
        "\x1b[2;5r",
        "\x1b[r",
        "\x1b[3@",
        "\x1b[2P",
        "\x1b[2X",
        "\x1b[31m",
        "\x1b[1;42m",
        "\x1b[m",
        "中文",
        "e\u{301}",
        "\u{231A}\u{FE0E}",
        "\u{2764}\u{FE0F}",
        "\x1b[?1049h",
        "\x1b[?1049l",
        "\x1b[?5h",
        "\x1b[?5l",
        "\x1b]4;1;rgb:12/34/56\x07",
        "\x1b]10;rgb:ff/00/00\x07",
        "\x1b7",
        "\x1b8",
        "\x1bM",
        "\x1bD",
        "\x1bE",
        "\x1b[4h",
        "\x1b[4l",
        "\x1b[?7l",
        "\x1b[?7h",
        "\x1b[?25l",
        "\x1b[?25h",
        "\x1b[2 q",
        "\x1b#8",
        "\x1b[?69h\x1b[2;6s",
        "\x1b[?69l",
        "\x1b]8;;http://x\x07link\x1b]8;;\x07",
        "\x1b]133;A\x07",
        "\x1b]11;rgb:10/20/30\x07",
        "\x1bc",
        "\x1b[3J",
        "line\r\nline\r\nline\r\n",
    ];

    #[derive(Clone, Debug)]
    enum Op {
        Feed(usize),
        Resize(u16, u16),
        Scroll(Scroll),
        Snapshot,
    }

    fn scroll() -> impl Strategy<Value = Scroll> {
        prop_oneof![
            (1_usize..6).prop_map(Scroll::Up),
            (1_usize..6).prop_map(Scroll::Down),
            Just(Scroll::PageUp),
            Just(Scroll::PageDown),
            Just(Scroll::Top),
            Just(Scroll::Bottom),
        ]
    }

    fn op() -> impl Strategy<Value = Op> {
        prop_oneof![
            8 => (0..PIECES.len()).prop_map(Op::Feed),
            1 => (2_u16..14, 1_u16..8).prop_map(|(cols, rows)| Op::Resize(cols, rows)),
            2 => scroll().prop_map(Op::Scroll),
            3 => Just(Op::Snapshot),
        ]
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(512))]

        /// An updated snapshot shows what a copy of the view shows, however
        /// the screen changed and the view moved between two updates.
        #[test]
        fn an_updated_snapshot_matches_a_copy_of_the_screen(ops in prop::collection::vec(op(), 1..80)) {
            let (mut terminal, mut parser) = terminal(10, 5);
            let mut snapshot = Snapshot::default();
            for op in ops {
                match op {
                    Op::Feed(piece) => feed(&mut terminal, &mut parser, PIECES[piece]),
                    Op::Resize(cols, rows) => terminal.resize(cols, rows),
                    Op::Scroll(scroll) => terminal.scroll_viewport(scroll),
                    Op::Snapshot => {
                        let before = (snapshot.colors().clone(), snapshot.reverse_video());
                        let copied = terminal.snapshot(&mut snapshot);
                        let after = (terminal.colors().clone(), terminal.reverse_video());
                        prop_assert_eq!(snapshot.colors_changed(), before != after);
                        prop_assert!(copied <= usize::from(snapshot.rows()));
                        assert_same(&snapshot, &copy_of(&terminal));
                        prop_assert_eq!(terminal.snapshot(&mut snapshot), 0, "nothing changed since");
                    }
                }
            }
            terminal.snapshot(&mut snapshot);
            assert_same(&snapshot, &copy_of(&terminal));
        }
    }
}
