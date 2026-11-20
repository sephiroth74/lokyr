//! The gutter's marks, which a pane shows at the left of its cells: a row's
//! mark of a color that an embedder lays, as a plugin's, and the mark of
//! each prompt of the commands that the shell marks, of the command's
//! state. A laid mark holds a pin of its row's first cell, so that it keeps
//! its row as output scrolls it into the scrollback and a resize reflows
//! it, and goes with it, as the scrollback's limit or a clearing takes the
//! row. The snapshot carries the marks of the view's rows, and the blocks
//! of the commands that the view shows, of their rows and states, which an
//! embedder draws along.

use super::Terminal;
use super::folds::ViewMap;
use super::selection::CellPin;
use crate::page::Rgb;

/// The most marks that a terminal keeps laid, of both screens: each
/// snapshot looks at every one where it is.
pub const MOST_GUTTER_MARKS: usize = 4096;

/// What a mark of the gutter shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GutterKind {
    /// A mark that an embedder laid, of its color.
    Laid(Rgb),
    /// A command's prompt, of a command whose output started and which has
    /// not ended.
    Running,
    /// A command's prompt, of a command that ended of the status 0, or of
    /// none that the shell told.
    Succeeded,
    /// A command's prompt, of a command that ended of another status.
    Failed,
}

/// A mark of the gutter, as the view shows it: its row of the view, and
/// what it shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GutterMark {
    /// The row of the view.
    pub row: u16,
    /// What it shows.
    pub kind: GutterKind,
}

/// A command's block, as the view shows it: its command's id, its first
/// row and its last,
/// by the view's rows from its top, either past the view where the block
/// goes past it, and its state, none of a command whose output has not
/// started, as the prompt where the user types.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BlockMark {
    /// Its command's id.
    pub id: u64,
    /// Its first row, its prompt's, of the view's rows.
    pub first: i32,
    /// Its last row, its output's, of the view's rows.
    pub last: i32,
    /// Its state, as its prompt's mark shows it.
    pub kind: Option<GutterKind>,
    /// Whether the row before its first is the blank row that the terminal
    /// put before its prompt, as [`Terminal::set_block_spacing`] asks.
    pub room_above: bool,
    /// Whether the row after its last is the blank row that the terminal
    /// put before the next command's prompt.
    pub room_below: bool,
}

/// A mark that a terminal keeps laid.
#[derive(Debug)]
pub(super) struct Laid {
    id: u64,
    pin: CellPin,
    color: Rgb,
}

impl Terminal {
    /// Lays a mark of `color` in the gutter at the row numbered `row` of
    /// the active screen, as the mark `id`, which the caller numbers, in
    /// place of one of that id. Whether it took it: the active screen holds
    /// the row, and the terminal keeps fewer than [`MOST_GUTTER_MARKS`].
    pub fn mark_gutter(&mut self, id: u64, row: i64, color: Rgb) -> bool {
        self.unmark_gutter(id);
        if self.gutter.len() >= MOST_GUTTER_MARKS {
            return false;
        }
        let Some(pin) = self.pin_cell((0, row)) else {
            return false;
        };
        self.gutter.push(Laid { id, pin, color });
        true
    }

    /// Takes the gutter's mark `id` away, if the terminal keeps it.
    pub fn unmark_gutter(&mut self, id: u64) {
        if let Some(at) = self.gutter.iter().position(|laid| laid.id == id) {
            let laid = self.gutter.swap_remove(at);
            self.unpin_cell(&laid.pin);
        }
    }

    /// The ids of the gutter's marks that the terminal keeps laid, of both
    /// screens, but those whose rows left.
    pub fn gutter_marks(&self) -> Vec<u64> {
        self.gutter
            .iter()
            .filter(|laid| !self.is_lost(&laid.pin))
            .map(|laid| laid.id)
            .collect()
    }

    /// The marks of the gutter that the view `map` shows, by their rows: the
    /// prompts of the primary screen's commands that ran, and the laid ones
    /// of the active screen, those whose rows left going, after a prompt's
    /// of their row, so that they show over it; and the blocks of the
    /// primary screen's commands that the view shows, oldest first. A mark
    /// of a row that a fold takes shows on its placeholder's row, as a
    /// block's end in a fold ends on it.
    pub(super) fn gutter_view(&mut self, map: &ViewMap) -> (Vec<GutterMark>, Vec<BlockMark>) {
        let (top, bottom) = (map.top(), map.end());
        let row_of = |row: i64| map.showing(row);
        let of_view = |row: i64| {
            let place = map.place(row);
            i32::try_from(place).unwrap_or(if place < 0 { i32::MIN } else { i32::MAX })
        };
        let mut marks = Vec::new();
        let mut blocks = Vec::new();
        if !self.is_alternate_screen() {
            for block in self.blocks_in(top, bottom) {
                if let (Some(kind), Some(row)) = (block.kind, block.prompt.and_then(row_of)) {
                    marks.push(GutterMark { row, kind });
                }
                blocks.push(BlockMark {
                    id: block.id,
                    first: of_view(block.first),
                    last: of_view(block.last),
                    kind: block.kind,
                    room_above: block.room_above,
                    room_below: block.room_below,
                });
            }
        }
        if !self.gutter.is_empty() {
            let gone: Vec<u64> = self
                .gutter
                .iter()
                .filter(|laid| self.is_lost(&laid.pin))
                .map(|laid| laid.id)
                .collect();
            for id in gone {
                self.unmark_gutter(id);
            }
            for laid in &self.gutter {
                let row = self.pinned_cell(&laid.pin).and_then(|(_, row)| row_of(row));
                if let Some(row) = row {
                    marks.push(GutterMark {
                        row,
                        kind: GutterKind::Laid(laid.color),
                    });
                }
            }
        }
        marks.sort_by_key(|mark| mark.row);
        (marks, blocks)
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

    /// The gutter's marks of a snapshot updated now, as `(row, kind)`.
    fn shown(terminal: &mut Terminal) -> Vec<(u16, GutterKind)> {
        let mut snapshot = Snapshot::default();
        terminal.snapshot(&mut snapshot);
        snapshot
            .gutter()
            .iter()
            .map(|mark| (mark.row, mark.kind))
            .collect()
    }

    /// The commands' blocks of a snapshot updated now, as `(first, last,
    /// kind)`.
    fn blocks(terminal: &mut Terminal) -> Vec<(i32, i32, Option<GutterKind>)> {
        let mut snapshot = Snapshot::default();
        terminal.snapshot(&mut snapshot);
        snapshot
            .blocks()
            .iter()
            .map(|block| (block.first, block.last, block.kind))
            .collect()
    }

    #[test]
    fn the_view_s_blocks_run_from_their_prompts_to_their_ends() {
        let mut terminal = Terminal::new(20, 8, Options::default());
        // `ls` of two rows, `false` of none, then `seq`, which runs.
        feed(
            &mut terminal,
            concat!(
                "\x1b]133;A\x07$ \x1b]133;B\x07ls\r\n\x1b]133;C\x07a\r\nb\r\n\x1b]133;D;0\x07",
                "\x1b]133;A\x07$ \x1b]133;B\x07false\r\n\x1b]133;C\x07\x1b]133;D;1\x07",
                "\x1b]133;A\x07$ \x1b]133;B\x07seq\r\n\x1b]133;C\x071\r\n2",
            ),
        );
        use GutterKind::{Failed, Running, Succeeded};
        assert_eq!(
            blocks(&mut terminal),
            [
                (0, 2, Some(Succeeded)),
                (3, 3, Some(Failed)),
                // To the cursor's row.
                (4, 6, Some(Running)),
            ]
        );
        // It ends, and the prompt where the user types has no state.
        feed(
            &mut terminal,
            "\r\n\x1b]133;D;0\x07\x1b]133;A\x07$ \x1b]133;B\x07",
        );
        assert_eq!(
            blocks(&mut terminal)[2..],
            [(4, 6, Some(Succeeded)), (7, 7, None)]
        );
        // Scrolled by three rows: `ls`' block goes, and `false`'s first row
        // is the view's.
        feed(&mut terminal, "\r\n\r\n\r\n");
        assert_eq!(terminal.origin().top, 3);
        assert_eq!(
            blocks(&mut terminal),
            [(0, 0, Some(Failed)), (1, 3, Some(Succeeded)), (4, 4, None)]
        );
        // The view two rows up: `ls`' block, whose prompt is above the view,
        // starts above it.
        terminal.scroll_viewport(Scroll::Up(2));
        assert_eq!(
            blocks(&mut terminal),
            [
                (-1, 1, Some(Succeeded)),
                (2, 2, Some(Failed)),
                (3, 5, Some(Succeeded)),
                (6, 6, None)
            ]
        );
        // The alternate screen shows none.
        terminal.scroll_viewport(Scroll::Bottom);
        feed(&mut terminal, "\x1b[?1049h");
        assert_eq!(blocks(&mut terminal), []);
    }

    #[test]
    fn a_block_starts_at_its_prompt_s_mark_and_its_bar_at_its_first_text() {
        let mut terminal = Terminal::new(20, 6, Options::default());
        // A theme's prompt of an empty line first, as Starship's.
        feed(
            &mut terminal,
            "\x1b]133;A\x07\r\n$ \x1b]133;B\x07ls\r\n\x1b]133;C\x07a\r\n\x1b]133;D;0\x07",
        );
        assert_eq!(blocks(&mut terminal), [(0, 2, Some(GutterKind::Succeeded))]);
        assert_eq!(shown(&mut terminal), [(1, GutterKind::Succeeded)]);
    }

    const RED: Rgb = Rgb { r: 255, g: 0, b: 0 };
    const LAID: GutterKind = GutterKind::Laid(RED);

    #[test]
    fn a_laid_mark_keeps_its_row_as_output_scrolls_it_and_the_view_moves() {
        let mut terminal = Terminal::new(20, 3, Options::default());
        feed(&mut terminal, "first\r\nsecond");
        assert!(terminal.mark_gutter(7, 1, RED));
        assert_eq!(shown(&mut terminal), [(1, LAID)]);
        // Three rows of output: the view shows it no more, and keeps it.
        feed(&mut terminal, "\r\na\r\nb\r\nc");
        assert_eq!(terminal.origin().top, 2);
        assert_eq!(shown(&mut terminal), []);
        assert_eq!(terminal.gutter_marks(), [7]);
        // The view up shows it at its row.
        terminal.scroll_viewport(Scroll::Up(2));
        assert_eq!(shown(&mut terminal), [(1, LAID)]);
        terminal.scroll_viewport(Scroll::Bottom);
        // Taken away, its pin goes.
        let pins = terminal.screen_rows().pins();
        terminal.unmark_gutter(7);
        assert_eq!(terminal.screen_rows().pins(), pins - 1);
        assert!(terminal.gutter_marks().is_empty());
    }

    #[test]
    fn a_laid_mark_follows_its_line_through_a_reflow() {
        let mut terminal = Terminal::new(10, 4, Options::default());
        // `abcdefghij` and `klmn`, one line wrapped, then a line below.
        feed(&mut terminal, "abcdefghijklmn\r\nnext");
        assert!(terminal.mark_gutter(1, 2, RED));
        assert_eq!(shown(&mut terminal), [(2, LAID)]);
        // Wider, the wrapped line is one row, and the mark's line moves up.
        terminal.resize(20, 4);
        assert_eq!(shown(&mut terminal), [(1, LAID)]);
        // Narrower again, it moves down.
        terminal.resize(10, 4);
        assert_eq!(shown(&mut terminal), [(2, LAID)]);
    }

    #[test]
    fn a_laid_mark_goes_with_its_row_and_stays_with_its_screen() {
        let options = Options {
            scrollback: Scrollback {
                limit: 0,
                ..Scrollback::default()
            },
            ..Options::default()
        };
        let mut terminal = Terminal::new(10, 3, options);
        feed(&mut terminal, "x");
        assert!(terminal.mark_gutter(1, 0, RED));
        // The other screen does not show it; its own does again.
        feed(&mut terminal, "\x1b[?1049h");
        assert_eq!(shown(&mut terminal), []);
        feed(&mut terminal, "\x1b[?1049l");
        assert_eq!(shown(&mut terminal), [(0, LAID)]);
        // The scrollback's limit takes its row, and it goes, its pin too.
        let mut lines = 0;
        while !terminal.gutter_marks().is_empty() {
            feed(&mut terminal, "\r\nmore");
            lines += 1;
            assert!(lines < 100_000, "the scrollback keeps every row");
        }
        assert_eq!(shown(&mut terminal), []);
        assert!(terminal.gutter.is_empty());
    }

    #[test]
    fn clearing_the_scrollback_takes_its_marks_and_a_reset_takes_all() {
        let mut terminal = Terminal::new(10, 2, Options::default());
        feed(&mut terminal, "a\r\nb\r\nc\r\nd");
        assert!(terminal.mark_gutter(1, 0, RED));
        assert!(terminal.mark_gutter(2, 3, RED));
        feed(&mut terminal, "\x1b[3J");
        assert_eq!(terminal.gutter_marks(), [2]);
        assert_eq!(shown(&mut terminal), [(1, LAID)]);
        feed(&mut terminal, "\x1bc");
        assert!(terminal.gutter_marks().is_empty());
        assert_eq!(shown(&mut terminal), []);
    }

    #[test]
    fn a_laid_mark_needs_its_row_and_a_room_among_the_most() {
        let mut terminal = Terminal::new(10, 3, Options::default());
        assert!(!terminal.mark_gutter(1, -1, RED));
        assert!(!terminal.mark_gutter(1, 3, RED));
        // One of an id takes the place of the one before.
        assert!(terminal.mark_gutter(1, 0, RED));
        assert!(terminal.mark_gutter(1, 2, RED));
        assert_eq!(shown(&mut terminal), [(2, LAID)]);
        for id in 2..=u64::try_from(MOST_GUTTER_MARKS).unwrap() {
            assert!(terminal.mark_gutter(id, 0, RED));
        }
        assert!(!terminal.mark_gutter(0, 0, RED));
    }

    /// A command that printed a line and ended of `status`, or of none told,
    /// as a shell's integration marks it.
    fn command(status: Option<i32>) -> String {
        let end = status.map_or("\x1b]133;D\x07".to_owned(), |status| {
            format!("\x1b]133;D;{status}\x07")
        });
        format!("\x1b]133;A\x07$ \x1b]133;B\x07run\r\n\x1b]133;C\x07out\r\n{end}")
    }

    #[test]
    fn the_prompts_show_their_commands_ends() {
        let mut terminal = Terminal::new(20, 10, Options::default());
        feed(&mut terminal, &command(Some(0)));
        feed(&mut terminal, &command(Some(2)));
        feed(&mut terminal, &command(None));
        // An empty line, which the shell ends without its output's mark.
        feed(
            &mut terminal,
            "\x1b]133;A\x07$ \x1b]133;B\x07\r\n\x1b]133;D;0\x07",
        );
        // A command that runs.
        feed(
            &mut terminal,
            "\x1b]133;A\x07$ \x1b]133;B\x07sleep\r\n\x1b]133;C\x07",
        );
        assert_eq!(
            shown(&mut terminal),
            [
                (0, GutterKind::Succeeded),
                (2, GutterKind::Failed),
                (4, GutterKind::Succeeded),
                (7, GutterKind::Running),
            ]
        );
        // It ends.
        feed(
            &mut terminal,
            "\x1b]133;D;130\x07\x1b]133;A\x07$ \x1b]133;B\x07",
        );
        assert_eq!(shown(&mut terminal)[3], (7, GutterKind::Failed));
        // The alternate screen shows none.
        feed(&mut terminal, "\x1b[?1049h");
        assert_eq!(shown(&mut terminal), []);
        feed(&mut terminal, "\x1b[?1049l");
        assert_eq!(shown(&mut terminal).len(), 4);
    }

    /// A command whose prompt starts with an empty line, as a theme's
    /// prompt prints one, then its two rows, and that failed.
    const SPACED: &str = "\x1b]133;A\x07\r\nuser ~/src\r\n$ \x1b]133;B\x07run\r\n\x1b]133;C\x07out\r\n\x1b]133;D;1\x07";

    #[test]
    fn a_prompt_s_mark_is_on_its_first_row_of_text() {
        let mut terminal = Terminal::new(20, 14, Options::default());
        feed(&mut terminal, SPACED);
        // A prompt of text on its start's row keeps it.
        feed(&mut terminal, &command(Some(0)));
        // A prompt of no text up to its input keeps its start's row.
        feed(
            &mut terminal,
            "\x1b]133;A\x07\r\n\x1b]133;B\x07\r\n\x1b]133;C\x07x\r\n\x1b]133;D;0\x07",
        );
        // A first line of spaces alone is as empty.
        feed(
            &mut terminal,
            "\x1b]133;A\x07   \r\n$ \x1b]133;B\x07ls\r\n\x1b]133;C\x07\x1b]133;D;0\x07",
        );
        assert_eq!(
            shown(&mut terminal),
            [
                (1, GutterKind::Failed),
                (4, GutterKind::Succeeded),
                (6, GutterKind::Succeeded),
                (10, GutterKind::Succeeded)
            ]
        );
        // A prompt whose start is above the view and whose text is in it
        // shows its mark.
        let mut short = Terminal::new(20, 3, Options::default());
        feed(&mut short, SPACED);
        assert_eq!(short.origin().top, 2);
        short.scroll_viewport(Scroll::Up(1));
        assert_eq!(shown(&mut short), [(0, GutterKind::Failed)]);
        // The jumps go to the prompts' starts, as before.
        assert_eq!(terminal.prompt_indexes()[0], 0);
    }

    #[test]
    fn the_view_shows_the_prompts_of_its_rows_alone() {
        let mut terminal = Terminal::new(20, 4, Options::default());
        for status in 0..20 {
            feed(&mut terminal, &command(Some(status % 2)));
        }
        // Each command takes two rows, the last one's prompt not there.
        let top = terminal.origin().top;
        assert_eq!(top, 40 - 4 + 1);
        let shown_now = shown(&mut terminal);
        assert_eq!(shown_now, [(1, GutterKind::Failed)]);
        // The view up, at the third command's prompt.
        terminal.scroll_viewport(Scroll::Top);
        assert_eq!(
            shown(&mut terminal),
            [(0, GutterKind::Succeeded), (2, GutterKind::Failed)]
        );
        // A laid mark on a prompt's row shows both, the laid one last.
        assert!(terminal.mark_gutter(1, 2, RED));
        assert_eq!(
            shown(&mut terminal),
            [
                (0, GutterKind::Succeeded),
                (2, GutterKind::Failed),
                (2, LAID)
            ]
        );
    }
}
