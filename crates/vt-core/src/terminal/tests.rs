//! The terminal driven by the parser, as programs drive it.

use super::*;
use crate::page::{Color, RowFlag, StyleId};
use crate::parser::Parser;
use crate::unicode::{Mode, Tables};

/// A terminal with the parser that feeds it.
struct Term {
    parser: Parser,
    terminal: Terminal,
}

impl Term {
    fn new(cols: u16, rows: u16) -> Self {
        Self::with(cols, rows, Options::default())
    }

    fn with(cols: u16, rows: u16, options: Options) -> Self {
        Self {
            parser: Parser::new(),
            terminal: Terminal::new(cols, rows, options),
        }
    }

    fn feed(&mut self, bytes: impl AsRef<[u8]>) -> &mut Self {
        self.parser.advance(&mut self.terminal, bytes.as_ref());
        self
    }

    /// The text of every row of the screen.
    fn screen(&self) -> Vec<String> {
        (0..self.terminal.rows())
            .map(|y| self.terminal.row_text(y))
            .collect()
    }

    fn cursor(&self) -> (u16, u16, bool) {
        let cursor = self.terminal.cursor();
        (cursor.x, cursor.y, cursor.pending_wrap)
    }

    fn cell(&self, x: u16, y: u16) -> Cell {
        self.terminal.cell(x, y)
    }

    fn row(&self, y: u16) -> crate::page::Row {
        let (page, row) = self.terminal.screen_rows().screen_row(y);
        page.row(row)
    }
}

#[test]
fn text_prints_at_the_cursor() {
    let mut term = Term::new(10, 3);
    term.feed("hello");
    assert_eq!(term.screen(), ["hello", "", ""]);
    assert_eq!(term.cursor(), (5, 0, false));
}

#[test]
fn the_last_column_leaves_the_wrap_pending() {
    let mut term = Term::new(5, 3);
    term.feed("abcde");
    assert_eq!(term.cursor(), (4, 0, true));
    term.feed("f");
    assert_eq!(term.screen(), ["abcde", "f", ""]);
    assert_eq!(term.cursor(), (1, 1, false));
    assert!(term.row(0).has(RowFlag::SoftWrapped));
    assert!(term.row(1).has(RowFlag::Continuation));
}

#[test]
fn wrapping_at_the_bottom_scrolls() {
    let mut term = Term::new(3, 2);
    term.feed("abcdefg");
    assert_eq!(term.screen(), ["def", "g"]);
    assert_eq!(term.terminal.screen_rows().scrollback_rows(), 1);
}

#[test]
fn without_autowrap_the_last_column_is_overwritten() {
    let mut term = Term::new(5, 2);
    term.terminal.set_autowrap(false);
    term.feed("abcdefg");
    assert_eq!(term.screen(), ["abcdg", ""]);
    assert_eq!(term.cursor(), (4, 0, true));
}

#[test]
fn a_wide_character_takes_two_cells() {
    let mut term = Term::new(6, 2);
    term.feed("a中b");
    assert_eq!(term.screen(), ["a中b", ""]);
    assert_eq!(term.cell(1, 0).width(), Width::Wide);
    assert_eq!(term.cell(2, 0).width(), Width::SpacerTail);
    assert_eq!(term.cursor(), (4, 0, false));
}

#[test]
fn a_wide_character_in_the_last_column_wraps() {
    let mut term = Term::new(5, 2);
    term.feed("abcd中");
    assert_eq!(term.screen(), ["abcd", "中"]);
    assert_eq!(term.cell(4, 0).width(), Width::SpacerHead);
    assert!(term.row(0).has(RowFlag::SoftWrapped));
    assert_eq!(term.cursor(), (2, 1, false));
}

#[test]
fn a_wide_character_without_autowrap_moves_back_a_column() {
    let mut term = Term::new(5, 2);
    term.terminal.set_autowrap(false);
    term.feed("abcd中");
    assert_eq!(term.screen(), ["abc中", ""]);
    assert_eq!(term.cursor(), (4, 0, true));
}

#[test]
fn a_wide_character_never_fits_one_column() {
    let mut term = Term::new(1, 2);
    term.feed("中a");
    assert_eq!(term.screen(), ["a", ""]);
}

#[test]
fn overwriting_half_of_a_wide_character_clears_the_other_half() {
    let mut term = Term::new(6, 2);
    // Over the second half of 文, which a backspace reaches.
    term.feed("中文\x08z");
    assert_eq!(term.screen(), ["中 z", ""]);
    assert_eq!(term.cell(2, 0), Cell::EMPTY, "the first half went");
    // Over the first half of 中.
    term.feed("\rx");
    assert_eq!(term.screen(), ["x  z", ""]);
    assert_eq!(term.cell(1, 0), Cell::EMPTY, "the second half went");
}

#[test]
fn combining_characters_join_the_cell_before() {
    let mut term = Term::new(6, 2);
    term.feed("e\u{301}a\u{308}");
    assert_eq!(term.screen(), ["e\u{301}a\u{308}", ""]);
    assert_eq!(term.cursor(), (2, 0, false));
    assert_eq!(term.cell(0, 0).content(), Content::Grapheme('e'));
}

#[test]
fn a_combining_character_joins_the_last_column_while_the_wrap_is_pending() {
    let mut term = Term::new(3, 2);
    term.feed("abc\u{301}d");
    assert_eq!(term.screen(), ["abc\u{301}", "d"]);
}

#[test]
fn a_mark_after_the_cursor_moved_joins_the_character_left_of_it() {
    let mut term = Term::new(6, 2);
    term.feed("ab\x08x\u{301}");
    assert_eq!(term.screen(), ["ax\u{301}", ""]);
    // After a carriage return, nothing is left of the cursor: dropped.
    term.feed("\r\u{302}");
    assert_eq!(term.screen(), ["ax\u{301}", ""]);
    // After a tab to the last column and four backspaces, the cursor is
    // at column 1, so the character left of it takes the mark.
    term.feed("\x09\x08\x08\x08\x08\u{303}");
    assert_eq!(term.screen(), ["a\u{303}x\u{301}", ""]);
}

#[test]
fn vs16_widens_and_vs15_narrows_a_cluster() {
    let mut term = Term::new(6, 2);
    term.feed("\u{2764}");
    assert_eq!(term.cursor(), (1, 0, false));
    term.feed("\u{FE0F}x");
    assert_eq!(term.cell(0, 0).width(), Width::Wide);
    assert_eq!(term.cell(1, 0).width(), Width::SpacerTail);
    assert_eq!(term.screen(), ["\u{2764}\u{FE0F}x", ""]);
    assert_eq!(term.cursor(), (3, 0, false));

    term.feed("\r\n\u{231A}");
    assert_eq!(term.cursor(), (2, 1, false));
    term.feed("\u{FE0E}y");
    assert_eq!(term.cell(0, 1).width(), Width::Narrow);
    assert_eq!(term.screen()[1], "\u{231A}\u{FE0E}y");
}

#[test]
fn a_cluster_with_no_room_to_widen_stays_narrow() {
    let mut term = Term::new(3, 2);
    term.feed("ab\u{2764}\u{FE0F}");
    assert_eq!(term.cell(2, 0).width(), Width::Narrow);
    assert_eq!(term.cursor(), (2, 0, true));
}

#[test]
fn emoji_sequences_and_flags_take_one_wide_cell() {
    let mut term = Term::new(8, 2);
    term.feed("\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}\u{1F1EE}\u{1F1F9}!");
    assert_eq!(term.cursor(), (5, 0, false));
    assert_eq!(term.cell(2, 0).width(), Width::Wide, "the flag");
}

#[test]
fn a_flag_split_by_the_margin_stays_one_cluster() {
    let mut term = Term::new(3, 2);
    // The first indicator fills the last column; the second joins it.
    term.feed("ab\u{1F1EE}\u{1F1F9}c");
    assert_eq!(term.screen(), ["ab\u{1F1EE}\u{1F1F9}", "c"]);
}

#[test]
fn a_flag_that_wraps_stays_one_cluster() {
    let mut term = Term::new(3, 2);
    // The first indicator goes to the next row, and the second joins it.
    term.feed("abc\u{1F1EE}\u{1F1F9}");
    assert_eq!(term.screen(), ["abc", "\u{1F1EE}\u{1F1F9}"]);
    assert_eq!(term.cell(0, 1).width(), Width::Wide);
    assert_eq!(term.cursor(), (2, 1, false));
}

#[test]
fn legacy_mode_gives_each_codepoint_its_cell() {
    let options = Options {
        widths: Widths {
            mode: Mode::Legacy,
            ..Widths::default()
        },
        ..Options::default()
    };
    let mut term = Term::with(10, 2, options);
    term.feed("\u{1F468}\u{200D}\u{1F469}e\u{301}");
    assert_eq!(term.cursor(), (5, 0, false), "two wide emoji and a letter");
    assert_eq!(term.screen(), ["\u{1F468}\u{200D}\u{1F469}e\u{301}", ""]);
}

#[test]
fn unicode_8_tables_keep_emoji_narrow() {
    let options = Options {
        widths: Widths {
            tables: Tables::Unicode8,
            ..Widths::default()
        },
        ..Options::default()
    };
    let mut term = Term::with(10, 2, options);
    term.feed("\u{1F600}a");
    assert_eq!(term.cursor(), (2, 0, false));
}

#[test]
fn insert_mode_pushes_the_rest_of_the_row() {
    let mut term = Term::new(6, 2);
    term.feed("abcdef\r");
    term.terminal.set_insert_mode(true);
    term.feed("x中");
    assert_eq!(term.screen(), ["x中abc", ""]);
}

#[test]
fn insert_mode_never_leaves_half_a_wide_character() {
    let mut term = Term::new(5, 2);
    term.feed("abc中\r");
    term.terminal.set_insert_mode(true);
    term.feed("x");
    // 中 lost its second half off the row, so it went whole.
    assert_eq!(term.screen(), ["xabc", ""]);
}

#[test]
fn printed_cells_take_the_pen() {
    let mut term = Term::new(6, 2);
    let red = Style {
        fg: Color::Palette(1),
        ..Style::DEFAULT
    };
    term.terminal.set_pen(red);
    term.feed("a中");
    term.terminal.set_pen(Style::DEFAULT);
    term.feed("b");
    let (page, _) = term.terminal.screen_rows().screen_row(0);
    for x in 0..3 {
        assert_eq!(page.styles().get(term.cell(x, 0).style()), &red, "{x}");
    }
    assert_eq!(term.cell(3, 0).style(), StyleId::DEFAULT);
    // The cells hold the references; the pen holds none between prints.
    assert_eq!(page.styles().references(term.cell(0, 0).style()), 3);
}

#[test]
fn bel_rings() {
    let mut term = Term::new(4, 2);
    term.feed("\x07a\x07");
    assert_eq!(term.terminal.bells(), 2);
    assert_eq!(term.screen(), ["a", ""]);
}

#[test]
fn backspace_moves_left_and_stops_at_the_first_column() {
    let mut term = Term::new(4, 2);
    term.feed("ab\x08");
    assert_eq!(term.cursor(), (1, 0, false));
    term.feed("\x08\x08\x08");
    assert_eq!(term.cursor(), (0, 0, false));
    // With the wrap pending, it leaves the last column.
    term.feed("abcd\x08");
    assert_eq!(term.cursor(), (2, 0, false));
}

#[test]
fn tab_goes_to_the_next_stop_or_the_last_column() {
    let mut term = Term::new(20, 2);
    term.feed("\x09");
    assert_eq!(term.cursor(), (8, 0, false));
    term.feed("ab\x09");
    assert_eq!(term.cursor(), (16, 0, false));
    term.feed("\x09\x09");
    assert_eq!(term.cursor(), (19, 0, false));
}

#[test]
fn line_feeds_go_down_and_scroll_at_the_bottom() {
    for control in [b'\n', 0x0B, 0x0C] {
        let mut term = Term::new(4, 2);
        term.feed("ab").feed([control]);
        assert_eq!(term.cursor(), (2, 1, false), "{control:#04x}");
        term.feed("c").feed([control]);
        assert_eq!(term.screen(), ["  c", ""], "{control:#04x}");
        assert_eq!(term.cursor(), (3, 1, false));
    }
}

#[test]
fn carriage_return_goes_to_the_first_column() {
    let mut term = Term::new(4, 2);
    term.feed("abcd\r");
    assert_eq!(term.cursor(), (0, 0, false));
    term.feed("x");
    assert_eq!(term.screen(), ["xbcd", ""]);
}

#[test]
fn so_and_si_switch_the_character_set() {
    let mut term = Term::new(4, 2);
    term.feed("\x0e");
    assert_eq!(term.terminal.shift(), Shift::G1);
    term.feed("\x0f");
    assert_eq!(term.terminal.shift(), Shift::G0);
}

#[test]
fn other_c0_controls_do_nothing() {
    let mut term = Term::new(4, 2);
    term.feed("a\x00\x01\x05\x1fb");
    assert_eq!(term.screen(), ["ab", ""]);
}

#[test]
fn the_screens_keep_their_rows_and_cursors() {
    let mut term = Term::new(4, 2);
    term.feed("main");
    term.terminal.set_alternate_screen(true);
    assert_eq!(term.screen(), ["", ""]);
    term.feed("\r\nalt");
    assert_eq!(term.cursor(), (3, 1, false));
    term.terminal.set_alternate_screen(false);
    assert_eq!(term.screen(), ["main", ""]);
    assert_eq!(term.cursor(), (3, 0, true));
    // The alternate screen keeps no scrollback: only the pages its rows
    // need.
    term.terminal.set_alternate_screen(true);
    term.feed("\n".repeat(2000));
    assert!(term.terminal.screen_rows().pages() <= 2);
    term.terminal.set_alternate_screen(false);
    assert_eq!(term.screen(), ["main", ""]);
}
