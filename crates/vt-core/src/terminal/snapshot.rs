//! Snapshots: what a renderer draws, copied from the terminal under the
//! lock, so that it can draw without the lock.

use super::{Colors, Cursor, CursorStyle, Terminal};
use crate::page::{Cell, Content, Page, RowFlag, Style, StyleId};

/// The screen as a renderer draws it: its rows, the cursor and the colors.
///
/// A renderer keeps one, and brings it up to date with
/// [`Terminal::snapshot`] before each frame: only the rows that changed
/// since are copied, and the others stay, moved up when the screen
/// scrolled. A renderer that falls behind gets the latest state, never the
/// ones in between.
#[derive(Clone, Debug, Default)]
pub struct Snapshot {
    /// The epoch of the screen's rows and how many times they had
    /// scrolled, at the last update.
    origin: Option<(u64, u64)>,
    cols: u16,
    lines: Vec<Line>,
    /// Which rows the last update copied.
    changed: Vec<bool>,
    /// How many rows the last update moved its lines up by.
    scrolled: u16,
    /// Whether the last update found other colors, or reverse video.
    colors_changed: bool,
    cursor: SnapshotCursor,
    reverse_video: bool,
    smooth_scroll: bool,
    colors: Colors,
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

    /// The screen's rows.
    pub fn rows(&self) -> u16 {
        // The screen has at most `u16::MAX` rows.
        self.lines.len() as u16
    }

    /// Row `y` of the screen.
    pub fn line(&self, y: u16) -> &Line {
        &self.lines[usize::from(y)]
    }

    /// Whether the last update copied row `y`, which the renderer then has
    /// to draw again.
    pub fn changed(&self, y: u16) -> bool {
        self.changed[usize::from(y)]
    }

    /// How many rows the last update moved the lines up by, as the screen
    /// scrolled: the renderer can move what it drew of them too.
    pub fn scrolled(&self) -> u16 {
        self.scrolled
    }

    /// The cursor.
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
        let metadata = page.row(row);
        self.cells.clear();
        self.cells.extend_from_slice(page.cells(row));
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
            for (x, cell) in (0..).zip(&self.cells) {
                if let Content::Grapheme(_) = cell.content() {
                    let rest = page.grapheme(row, x);
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
    /// Brings `snapshot` up to date with the screen, and returns how many
    /// rows it copied: those that changed since the last update, as the
    /// rows mark it, and those that scrolling brought in. The rows that
    /// it copies are marked as drawn, so that the next update skips them
    /// until they change again. A snapshot of another screen, or of rows
    /// laid out again by a resize, copies every row.
    pub fn snapshot(&mut self, snapshot: &mut Snapshot) -> usize {
        snapshot.cursor = SnapshotCursor {
            position: self.cursor(),
            visible: self.cursor_visible(),
            style: self.cursor_style(),
            blink: self.cursor_blink(),
        };
        let reverse_video = self.reverse_video();
        snapshot.colors_changed =
            snapshot.reverse_video != reverse_video || snapshot.colors != self.colors;
        snapshot.reverse_video = reverse_video;
        snapshot.smooth_scroll = self.smooth_scroll();
        if snapshot.colors != self.colors {
            snapshot.colors.clone_from(&self.colors);
        }
        let rows = &mut self.screen_mut().rows;
        let origin = (rows.epoch(), rows.scrolled());
        let (cols, height) = (rows.cols(), rows.rows());
        let lines = usize::from(height);
        // How far the lines that the snapshot has move up; past the screen,
        // none of them stays.
        let shift = match snapshot.origin {
            Some((epoch, scrolled))
                if epoch == origin.0 && snapshot.cols == cols && snapshot.lines.len() == lines =>
            {
                usize::try_from(origin.1 - scrolled).unwrap_or(usize::MAX)
            }
            _ => usize::MAX,
        };
        snapshot.origin = Some(origin);
        snapshot.cols = cols;
        snapshot.lines.resize_with(lines, Line::default);
        snapshot.changed.clear();
        snapshot.changed.resize(lines, false);
        let kept = lines.saturating_sub(shift);
        snapshot.lines.rotate_left(lines - kept);
        // The screen has at most `u16::MAX` rows.
        snapshot.scrolled = (lines - kept) as u16;
        let mut copied = 0;
        for y in 0..height {
            let fresh = usize::from(y) >= kept;
            let (page, row) = rows.screen_row_mut(y);
            if fresh || page.row(row).has(RowFlag::Dirty) {
                snapshot.lines[usize::from(y)].copy(page, row);
                snapshot.changed[usize::from(y)] = true;
                page.row_mut(row).set(RowFlag::Dirty, false);
                copied += 1;
            }
        }
        copied
    }
}

/// A snapshot of `rows`, copied whole, without marking anything, for tests
/// to compare an updated snapshot with.
#[cfg(test)]
pub(super) fn copy_of(terminal: &Terminal) -> Snapshot {
    let rows = terminal.screen_rows();
    let lines = (0..rows.rows())
        .map(|y| {
            let (page, row) = rows.screen_row(y);
            let mut line = Line::default();
            line.copy(page, row);
            line
        })
        .collect::<Vec<_>>();
    Snapshot {
        origin: None,
        cols: rows.cols(),
        changed: vec![true; lines.len()],
        lines,
        scrolled: 0,
        colors_changed: false,
        cursor: SnapshotCursor {
            position: terminal.cursor(),
            visible: terminal.cursor_visible(),
            style: terminal.cursor_style(),
            blink: terminal.cursor_blink(),
        },
        reverse_video: terminal.reverse_video(),
        smooth_scroll: terminal.smooth_scroll(),
        colors: terminal.colors().clone(),
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;
    use crate::page::Color;
    use crate::parser::Parser;
    use crate::terminal::Options;

    fn terminal(cols: u16, rows: u16) -> (Terminal, Parser) {
        (Terminal::new(cols, rows, Options::default()), Parser::new())
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
    ];

    #[derive(Clone, Debug)]
    enum Op {
        Feed(usize),
        Resize(u16, u16),
        Snapshot,
    }

    fn op() -> impl Strategy<Value = Op> {
        prop_oneof![
            8 => (0..PIECES.len()).prop_map(Op::Feed),
            1 => (2_u16..14, 1_u16..8).prop_map(|(cols, rows)| Op::Resize(cols, rows)),
            3 => Just(Op::Snapshot),
        ]
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(512))]

        /// An updated snapshot shows what a copy of the screen shows, however
        /// the screen changed between two updates.
        #[test]
        fn an_updated_snapshot_matches_a_copy_of_the_screen(ops in prop::collection::vec(op(), 1..80)) {
            let (mut terminal, mut parser) = terminal(10, 5);
            let mut snapshot = Snapshot::default();
            for op in ops {
                match op {
                    Op::Feed(piece) => feed(&mut terminal, &mut parser, PIECES[piece]),
                    Op::Resize(cols, rows) => terminal.resize(cols, rows),
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
