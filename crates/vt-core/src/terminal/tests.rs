//! The terminal driven by the parser, as programs drive it.

use proptest::prelude::*;

use super::*;
use crate::page::{Color, Page, Row, RowFlag, StyleId};
use crate::parser::{Params, Parser, StringEnd};
use crate::unicode::{Mode, Normalization, Tables};

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
    // Inserting in the middle of a wide character empties it too.
    let mut term = Term::new(6, 2);
    term.feed("中b");
    term.terminal.set_insert_mode(true);
    term.feed("x");
    assert_eq!(term.screen(), [" x b", ""]);
    assert_eq!(term.cell(0, 0), Cell::EMPTY);
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
    assert_eq!(term.terminal.charsets().shift(), Shift::G1);
    term.feed("\x0f");
    assert_eq!(term.terminal.charsets().shift(), Shift::G0);
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

fn normalizing(normalization: Normalization) -> Term {
    Term::with(
        10,
        2,
        Options {
            normalization,
            ..Options::default()
        },
    )
}

#[test]
fn nfc_composes_and_a_composed_character_takes_no_grapheme() {
    let mut term = normalizing(Normalization::Nfc);
    term.feed("e\u{301}a\u{308}\u{301}");
    assert_eq!(term.cell(0, 0).content(), Content::Char('\u{E9}'));
    let (page, row) = term.terminal.screen_rows().screen_row(0);
    assert_eq!(page.grapheme(row, 0), [], "no grapheme entry");
    assert_eq!(term.cell(1, 0).content(), Content::Grapheme('\u{E4}'));
    assert_eq!(page.grapheme(row, 1), ['\u{301}']);
    assert_eq!(term.cursor(), (2, 0, false));
}

#[test]
fn nfd_decomposes_even_a_single_character() {
    let mut term = normalizing(Normalization::Nfd);
    term.feed("\u{E9}x");
    assert_eq!(term.cell(0, 0).content(), Content::Grapheme('e'));
    assert_eq!(term.screen()[0], "e\u{301}x");
    assert_eq!(term.cursor(), (2, 0, false));
}

#[test]
fn hangul_composes_into_syllables_and_back() {
    let mut term = normalizing(Normalization::Nfc);
    term.feed("\u{1100}\u{1161}\u{11A8}!");
    assert_eq!(term.cell(0, 0).content(), Content::Char('\u{AC01}'));
    assert_eq!(term.cell(0, 0).width(), Width::Wide);
    assert_eq!(term.cursor(), (3, 0, false));

    let mut term = normalizing(Normalization::Nfd);
    term.feed("\u{AC01}!");
    assert_eq!(term.screen()[0], "\u{1100}\u{1161}\u{11A8}!");
    assert_eq!(term.cell(0, 0).width(), Width::Wide);
    assert_eq!(term.cursor(), (3, 0, false));
}

#[test]
fn hfs_plus_decomposes_but_leaves_its_ranges_alone() {
    let mut term = normalizing(Normalization::HfsPlus);
    term.feed("\u{E9}\u{2126}\u{F900}");
    assert_eq!(term.screen()[0], "e\u{301}\u{2126}\u{F900}");
    let mut term = normalizing(Normalization::Nfd);
    term.feed("\u{2126}");
    assert_eq!(term.screen()[0], "\u{3A9}");
}

#[test]
fn without_normalization_text_stays_as_written() {
    let mut term = normalizing(Normalization::None);
    term.feed("e\u{301}\u{E9}");
    assert_eq!(term.screen()[0], "e\u{301}\u{E9}");
}

#[test]
fn normalization_keeps_the_style_of_the_cell() {
    let mut term = normalizing(Normalization::Nfc);
    let red = Style {
        fg: Color::Palette(1),
        ..Style::DEFAULT
    };
    term.terminal.set_pen(red);
    term.feed("e\u{301}");
    let (page, _) = term.terminal.screen_rows().screen_row(0);
    assert_eq!(page.styles().get(term.cell(0, 0).style()), &red);
}

#[test]
fn ind_goes_down_and_scrolls_at_the_bottom() {
    let mut term = Term::new(5, 3);
    term.feed("ab\x1bD");
    assert_eq!(term.cursor(), (2, 1, false));
    term.feed("c\x1bDd");
    assert_eq!(term.screen(), ["ab", "  c", "   d"]);
    term.feed("\x1bD");
    assert_eq!(term.screen(), ["  c", "   d", ""]);
    assert_eq!(term.cursor(), (4, 2, false));
    assert_eq!(term.terminal.screen_rows().scrollback_rows(), 1);
}

#[test]
fn nel_goes_to_the_start_of_the_next_row() {
    let mut term = Term::new(5, 2);
    term.feed("abcde\x1bE");
    assert_eq!(term.cursor(), (0, 1, false));
    term.feed("f\x1bE");
    assert_eq!(term.screen(), ["f", ""]);
    assert_eq!(term.cursor(), (0, 1, false));
}

#[test]
fn ri_goes_up_and_scrolls_down_at_the_top() {
    let mut term = Term::new(5, 3);
    term.feed("1\r\n2\r\n3");
    term.feed("\x1bM");
    assert_eq!(term.cursor(), (1, 1, false));
    term.feed("\x1bM\x1bM");
    assert_eq!(term.screen(), ["", "1", "2"], "3 scrolled off the bottom");
    assert_eq!(term.cursor(), (1, 0, false));
    assert_eq!(term.terminal.screen_rows().scrollback_rows(), 0);
    term.feed("x");
    assert_eq!(term.screen(), [" x", "1", "2"]);
}

#[test]
fn ri_moves_the_wrap_flags_with_the_rows() {
    let mut term = Term::new(3, 3);
    term.feed("abcd\x1bM\x1bM");
    assert_eq!(term.screen(), ["", "abc", "d"]);
    assert!(term.row(1).has(RowFlag::SoftWrapped));
    assert!(term.row(2).has(RowFlag::Continuation));
    assert!(!term.row(0).has(RowFlag::SoftWrapped));
}

#[test]
fn ri_on_a_single_row_clears_it() {
    let mut term = Term::new(3, 1);
    term.feed("abc\x1bM");
    assert_eq!(term.screen(), [""]);
    assert_eq!(
        term.cursor(),
        (2, 0, false),
        "the wrap is no longer pending"
    );
}

#[test]
fn hts_sets_a_tab_stop_at_the_cursor() {
    let mut term = Term::new(20, 2);
    term.feed("abc\x1bH\r\t");
    assert_eq!(term.cursor(), (3, 0, false));
    term.feed("\t");
    assert_eq!(term.cursor(), (8, 0, false));
}

#[test]
fn a_mark_after_an_index_does_not_join_the_character_before() {
    let mut term = Term::new(5, 3);
    term.feed("\x1bDe\x1bM\u{301}");
    let (page, row) = term.terminal.screen_rows().screen_row(1);
    assert_eq!(page.grapheme(row, 0), [], "the mark stayed off the e");
    term.feed("\x1bDx\x1bD\u{301}");
    let (page, row) = term.terminal.screen_rows().screen_row(1);
    assert_eq!(page.grapheme(row, 1), []);
    // After DECRC, a mark joins the character left of the cursor.
    let mut term = Term::new(5, 1);
    term.feed("e\x1b7x\x1b8\u{301}");
    let (page, row) = term.terminal.screen_rows().screen_row(0);
    assert_eq!(page.grapheme(row, 0), ['\u{301}']);
    assert_eq!(page.grapheme(row, 1), []);
}

#[test]
fn dec_special_graphics_draws_lines_and_boxes() {
    let mut term = Term::new(10, 3);
    term.feed("\x1b(0lqqk\r\nx  x\r\nmqqj\x1b(B ok");
    assert_eq!(term.screen(), ["┌──┐", "│  │", "└──┘ ok"]);
    // The same, a byte at a time, through the print path of single
    // characters.
    let mut slow = Term::new(10, 3);
    for byte in b"\x1b(0lqqk\r\nx  x\r\nmqqj\x1b(B ok" {
        slow.feed([*byte]);
    }
    assert_eq!(slow.screen(), term.screen());
    let mut every = Term::new(40, 1);
    every.feed("\x1b(0_`abcdefghijklmnopqrstuvwxyz{|}~");
    assert_eq!(
        every.screen(),
        [" ◆▒\u{2409}\u{240C}\u{240D}\u{240A}°±\u{2424}\u{240B}┘┐┌└┼⎺⎻─⎼⎽├┤┴┬│≤≥π≠£·"]
    );
    every.feed("\x1b(0AZ09");
    assert!(every.screen()[0].ends_with("AZ09"), "only _ to ~ change");
}

#[test]
fn so_and_si_switch_between_g0_and_g1() {
    let mut term = Term::new(10, 1);
    term.feed("\x1b)0q\x0eq\x0fq");
    assert_eq!(term.screen(), ["q─q"]);
}

#[test]
fn ls2_and_ls3_put_g2_and_g3_in_use() {
    let mut term = Term::new(10, 1);
    term.feed("\x1b*0\x1b+A\x1bnq#\x1bo#q\x0fq#");
    assert_eq!(term.screen(), ["─#£qq#"]);
    assert_eq!(term.terminal.charsets().shift(), Shift::G0);
}

#[test]
fn ss2_and_ss3_take_one_character_from_g2_or_g3() {
    let mut term = Term::new(10, 1);
    term.feed("\x1b*0\x1b+A\x1bNqq\x1bO##");
    assert_eq!(term.screen(), ["─q£#"]);
    // A character that is not ASCII uses up the shift too.
    term.feed("\x1bN\u{E9}q");
    assert_eq!(term.screen(), ["─q£#\u{E9}q"]);
    assert_eq!(term.terminal.charsets().single_shift(), None);
}

#[test]
fn the_british_set_has_a_pound_sign() {
    let mut term = Term::new(10, 1);
    term.feed("\x1b(A#3\x1b(B#");
    assert_eq!(term.screen(), ["£3#"]);
}

#[test]
fn an_unknown_set_leaves_the_slot_as_it_was() {
    let mut term = Term::new(10, 1);
    term.feed("\x1b(0\x1b(Zq\x1b(%5q");
    assert_eq!(term.screen(), ["──"]);
    assert_eq!(
        term.terminal.charsets().get(Shift::G0),
        Charset::DecSpecialGraphics
    );
}

#[test]
fn line_characters_are_wide_when_ambiguous_ones_are() {
    let options = Options {
        widths: Widths {
            ambiguous_wide: true,
            ..Widths::default()
        },
        ..Options::default()
    };
    let mut term = Term::with(10, 1, options);
    term.feed("\x1b(0qx\x1b(Bq");
    assert_eq!(term.screen(), ["─│q"]);
    assert_eq!(term.cell(0, 0).width(), Width::Wide);
    assert_eq!(term.cursor(), (5, 0, false));
}

fn red() -> Style {
    Style {
        fg: Color::Palette(1),
        ..Style::DEFAULT
    }
}

/// The style of the cell at column `x` of row `y`.
fn style_at(term: &Term, x: u16, y: u16) -> Style {
    let (page, _) = term.terminal.screen_rows().screen_row(y);
    *page.styles().get(term.cell(x, y).style())
}

#[test]
fn decrc_brings_back_the_cursor_pen_and_character_sets_of_decsc() {
    let mut term = Term::new(10, 3);
    term.feed("\r\n   ");
    term.terminal.set_pen(red());
    term.feed("\x1b(0\x1b7");
    term.terminal.set_pen(Style::DEFAULT);
    term.feed("\x1b(B\r\nab");
    term.feed("\x1b8q");
    assert_eq!(term.screen(), ["", "   ─", "ab"]);
    assert_eq!(style_at(&term, 3, 1), red());
    assert_eq!(term.cursor(), (4, 1, false));
    // What was saved stays saved.
    term.feed("\x1b8");
    assert_eq!(term.cursor(), (3, 1, false));
}

#[test]
fn decrc_brings_back_a_pending_wrap() {
    let mut term = Term::new(5, 2);
    term.feed("abcde\x1b7\r\x1b8");
    assert_eq!(term.cursor(), (4, 0, true));
    term.feed("f");
    assert_eq!(term.screen(), ["abcde", "f"]);
}

#[test]
fn decrc_without_decsc_goes_home_with_the_defaults() {
    let mut term = Term::new(10, 2);
    term.terminal.set_pen(red());
    term.feed("\x1b(0\x0e\r\nabc\x1b8");
    assert_eq!(term.cursor(), (0, 0, false));
    assert_eq!(term.terminal.pen(), &Style::DEFAULT);
    assert_eq!(term.terminal.charsets(), Charsets::default());
}

#[test]
fn each_screen_saves_its_own_cursor() {
    let mut term = Term::new(10, 3);
    term.feed("ab\x1b7");
    term.terminal.set_alternate_screen(true);
    term.feed("\r\n\r\nxyz\x1b7\r\x1b8");
    assert_eq!(term.cursor(), (3, 2, false));
    term.terminal.set_alternate_screen(false);
    term.feed("\r\n\x1b8");
    assert_eq!(term.cursor(), (2, 0, false));
}

#[test]
fn decaln_fills_the_screen_with_e() {
    let mut term = Term::new(4, 3);
    term.terminal.set_pen(red());
    term.feed("abcdefg\x1b#8");
    assert_eq!(term.screen(), ["EEEE"; 3]);
    assert_eq!(term.cursor(), (0, 0, false));
    assert_eq!(style_at(&term, 1, 1), Style::DEFAULT);
    assert!(!term.row(0).has(RowFlag::SoftWrapped));
    assert!(!term.row(1).has(RowFlag::Continuation));
}

#[test]
fn ris_puts_the_terminal_back_as_it_was_made() {
    let mut term = Term::new(10, 2);
    term.feed("one\r\ntwo\r\nthree\x1b7\x1b(0\x1b)0\x0e\x1b=\x1bH\x07");
    term.terminal.set_pen(red());
    term.terminal.set_autowrap(false);
    term.terminal.set_insert_mode(true);
    term.terminal.set_alternate_screen(true);
    term.feed("alt\x1bc");
    assert!(!term.terminal.is_alternate_screen());
    assert_eq!(term.screen(), ["", ""]);
    assert_eq!(term.terminal.screen_rows().scrollback_rows(), 0);
    assert_eq!(term.cursor(), (0, 0, false));
    assert_eq!(term.terminal.pen(), &Style::DEFAULT);
    assert_eq!(term.terminal.charsets(), Charsets::default());
    assert!(term.terminal.autowrap());
    assert!(!term.terminal.insert_mode());
    assert!(!term.terminal.keypad_application());
    assert_eq!(term.terminal.bells(), 1, "the bells stay counted");
    // The tab stop at column 5 is gone, and so is the saved cursor.
    term.feed("\t");
    assert_eq!(term.cursor(), (8, 0, false));
    term.feed("\r\n\x1b8");
    assert_eq!(term.cursor(), (0, 0, false));
    term.terminal.set_alternate_screen(true);
    assert_eq!(term.screen(), ["", ""], "the alternate screen is empty too");
}

#[test]
fn ris_keeps_the_configuration() {
    let options = Options {
        widths: Widths {
            mode: Mode::Legacy,
            ..Widths::default()
        },
        normalization: Normalization::Nfc,
        ..Options::default()
    };
    let mut term = Term::with(6, 2, options);
    term.feed("\x1bc");
    assert_eq!(term.terminal.widths().mode, Mode::Legacy);
    assert_eq!(term.terminal.normalization(), Normalization::Nfc);
    assert_eq!((term.terminal.cols(), term.terminal.rows()), (6, 2));
}

#[test]
fn deckpam_and_deckpnm_switch_the_keypad() {
    let mut term = Term::new(4, 1);
    assert!(!term.terminal.keypad_application());
    term.feed("\x1b=");
    assert!(term.terminal.keypad_application());
    term.feed("\x1b>");
    assert!(!term.terminal.keypad_application());
}

/// Where the cursor is after `sequences`, from column `x` of row `y`.
fn moved(term: &mut Term, x: u16, y: u16, sequences: &str) -> (u16, u16, bool) {
    term.feed(format!("\x1b[{};{}H{sequences}", y + 1, x + 1));
    term.cursor()
}

/// Margins at rows 1 to 3 and columns 2 to 7 of a screen of 10 by 5.
fn with_margins() -> Term {
    let mut term = Term::new(10, 5);
    term.terminal.margins = Margins {
        top: 1,
        bottom: 3,
        left: 2,
        right: 7,
    };
    term
}

#[test]
fn relative_moves_stop_at_the_edges_of_the_screen() {
    let mut term = Term::new(10, 5);
    assert_eq!(moved(&mut term, 5, 2, "\x1b[A"), (5, 1, false));
    assert_eq!(
        moved(&mut term, 5, 2, "\x1b[0A"),
        (5, 1, false),
        "0 counts as 1"
    );
    assert_eq!(moved(&mut term, 5, 2, "\x1b[9A"), (5, 0, false));
    assert_eq!(moved(&mut term, 5, 2, "\x1b[2B"), (5, 4, false));
    assert_eq!(moved(&mut term, 5, 2, "\x1b[99B"), (5, 4, false));
    assert_eq!(moved(&mut term, 5, 2, "\x1b[3C"), (8, 2, false));
    assert_eq!(moved(&mut term, 5, 2, "\x1b[65535C"), (9, 2, false));
    assert_eq!(moved(&mut term, 5, 2, "\x1b[3D"), (2, 2, false));
    assert_eq!(moved(&mut term, 5, 2, "\x1b[99D"), (0, 2, false));
    // VPR and HPR move as CUD and CUF.
    assert_eq!(moved(&mut term, 5, 2, "\x1b[2e"), (5, 4, false));
    assert_eq!(moved(&mut term, 5, 2, "\x1b[2a"), (7, 2, false));
}

#[test]
fn relative_moves_stop_at_the_margins_from_within_them() {
    let mut term = with_margins();
    assert_eq!(moved(&mut term, 4, 2, "\x1b[9A"), (4, 1, false));
    assert_eq!(moved(&mut term, 4, 2, "\x1b[9B"), (4, 3, false));
    assert_eq!(moved(&mut term, 4, 2, "\x1b[9C"), (7, 2, false));
    assert_eq!(moved(&mut term, 4, 2, "\x1b[9D"), (2, 2, false));
    // From outside, the far edge of the screen stops them.
    assert_eq!(moved(&mut term, 4, 0, "\x1b[9A"), (4, 0, false));
    assert_eq!(moved(&mut term, 4, 4, "\x1b[9B"), (4, 4, false));
    assert_eq!(moved(&mut term, 9, 2, "\x1b[9C"), (9, 2, false));
    assert_eq!(moved(&mut term, 0, 2, "\x1b[9D"), (0, 2, false));
    // Toward the margins, from outside, they stop at them.
    assert_eq!(moved(&mut term, 4, 0, "\x1b[9B"), (4, 3, false));
    assert_eq!(moved(&mut term, 4, 4, "\x1b[9A"), (4, 1, false));
    assert_eq!(moved(&mut term, 0, 2, "\x1b[9C"), (7, 2, false));
    assert_eq!(moved(&mut term, 9, 2, "\x1b[9D"), (2, 2, false));
}

#[test]
fn cnl_and_cpl_go_to_the_left_margin() {
    let mut term = Term::new(10, 5);
    assert_eq!(moved(&mut term, 5, 1, "\x1b[2E"), (0, 3, false));
    assert_eq!(moved(&mut term, 5, 3, "\x1b[F"), (0, 2, false));
    let mut term = with_margins();
    assert_eq!(moved(&mut term, 5, 1, "\x1b[E"), (2, 2, false));
    assert_eq!(
        moved(&mut term, 1, 3, "\x1b[F"),
        (0, 2, false),
        "from left of it"
    );
}

#[test]
fn absolute_moves_count_from_the_top_left_of_the_screen() {
    let mut term = Term::new(10, 5);
    term.feed("\x1b[2;3H");
    assert_eq!(term.cursor(), (2, 1, false));
    term.feed("\x1b[H");
    assert_eq!(term.cursor(), (0, 0, false));
    term.feed("\x1b[99;99f");
    assert_eq!(term.cursor(), (9, 4, false));
    term.feed("\x1b[;5H");
    assert_eq!(term.cursor(), (4, 0, false));
    term.feed("\x1b[4G");
    assert_eq!(term.cursor(), (3, 0, false));
    term.feed("\x1b[6`");
    assert_eq!(term.cursor(), (5, 0, false));
    term.feed("\x1b[G");
    assert_eq!(term.cursor(), (0, 0, false));
    term.feed("\x1b[2C\x1b[3d");
    assert_eq!(term.cursor(), (2, 2, false));
    term.feed("\x1b[99d");
    assert_eq!(term.cursor(), (2, 4, false));
    // Outside origin mode, margins do not hold the cursor.
    let mut term = with_margins();
    term.feed("\x1b[5;10H");
    assert_eq!(term.cursor(), (9, 4, false));
}

#[test]
fn origin_mode_counts_from_the_margins_and_stays_within_them() {
    let mut term = with_margins();
    term.feed("\x1b[5;5H");
    term.terminal.set_origin_mode(true);
    assert_eq!(term.cursor(), (2, 1, false), "home is the top left margin");
    term.feed("\x1b[2;2H");
    assert_eq!(term.cursor(), (3, 2, false));
    term.feed("\x1b[99;99H");
    assert_eq!(term.cursor(), (7, 3, false));
    term.feed("\x1b[1G");
    assert_eq!(term.cursor(), (2, 3, false));
    term.feed("\x1b[1d");
    assert_eq!(term.cursor(), (2, 1, false));
    term.feed("\x1b[4C\r");
    assert_eq!(term.cursor(), (2, 1, false), "CR goes to the left margin");
    term.terminal.set_origin_mode(false);
    assert_eq!(term.cursor(), (0, 0, false));
}

#[test]
fn cr_goes_to_the_left_margin_from_right_of_it() {
    let mut term = with_margins();
    assert_eq!(moved(&mut term, 5, 2, "\r"), (2, 2, false));
    assert_eq!(moved(&mut term, 1, 2, "\r"), (0, 2, false));
}

#[test]
fn cursor_moves_end_the_wrap_and_the_cluster() {
    let mut term = Term::new(5, 2);
    term.feed("abcde\x1b[D");
    assert_eq!(term.cursor(), (3, 0, false));
    term.feed("\x1b[2;1Hxy\x1b[1G\u{301}");
    let (page, row) = term.terminal.screen_rows().screen_row(1);
    assert_eq!(page.grapheme(row, 1), [], "the mark stayed off the y");
}

#[test]
fn cht_and_cbt_move_by_tab_stops() {
    let mut term = Term::new(20, 1);
    assert_eq!(moved(&mut term, 0, 0, "\x1b[2I"), (16, 0, false));
    assert_eq!(moved(&mut term, 0, 0, "\x1b[99I"), (19, 0, false));
    assert_eq!(moved(&mut term, 17, 0, "\x1b[Z"), (16, 0, false));
    assert_eq!(moved(&mut term, 16, 0, "\x1b[Z"), (8, 0, false));
    assert_eq!(moved(&mut term, 17, 0, "\x1b[2Z"), (8, 0, false));
    assert_eq!(
        moved(&mut term, 5, 0, "\x1b[Z"),
        (0, 0, false),
        "no stop before"
    );
    assert_eq!(moved(&mut term, 19, 0, "\x1b[65535Z"), (0, 0, false));
}

#[test]
fn tbc_clears_a_tab_stop_or_all_of_them() {
    let mut term = Term::new(20, 1);
    term.feed("\x1b[9G\x1b[g\r\t");
    assert_eq!(term.cursor(), (16, 0, false));
    term.feed("\x1b[5g\r\t");
    assert_eq!(term.cursor(), (16, 0, false), "other values do nothing");
    term.feed("\x1b[3g\r\t");
    assert_eq!(term.cursor(), (19, 0, false));
}

#[test]
fn decscusr_sets_the_cursor_style() {
    let mut term = Term::new(4, 1);
    assert_eq!(term.terminal.cursor_style(), None);
    for (value, shape, blinking) in [
        (1, CursorShape::Block, true),
        (2, CursorShape::Block, false),
        (3, CursorShape::Underline, true),
        (4, CursorShape::Underline, false),
        (5, CursorShape::Bar, true),
        (6, CursorShape::Bar, false),
    ] {
        term.feed(format!("\x1b[{value} q"));
        assert_eq!(
            term.terminal.cursor_style(),
            Some(CursorStyle { shape, blinking }),
            "{value}"
        );
    }
    term.feed("\x1b[7 q");
    assert!(term.terminal.cursor_style().is_some(), "7 does nothing");
    term.feed("\x1b[ q");
    assert_eq!(term.terminal.cursor_style(), None);
}

#[test]
fn decsc_saves_origin_mode() {
    let mut term = with_margins();
    term.terminal.set_origin_mode(true);
    term.feed("\x1b7");
    term.terminal.set_origin_mode(false);
    term.feed("\x1b8");
    assert!(term.terminal.origin_mode());
    term.feed("\x1b[H");
    assert_eq!(term.cursor(), (2, 1, false));
}

#[test]
fn a_run_of_ascii_holds_a_reference_to_its_style_in_each_cell() {
    let mut term = Term::new(10, 2);
    let red = Style {
        fg: Color::Palette(1),
        ..Style::DEFAULT
    };
    term.terminal.set_pen(red);
    term.feed("abcdef");
    let (page, _) = term.terminal.screen_rows().screen_row(0);
    assert_eq!(page.styles().get(term.cell(5, 0).style()), &red);
    assert_eq!(page.styles().references(term.cell(0, 0).style()), 6);
    assert!(term.row(0).has(RowFlag::Styles));
    term.terminal.set_pen(Style::DEFAULT);
    term.feed("\rxyz");
    let (page, _) = term.terminal.screen_rows().screen_row(0);
    assert_eq!(page.styles().references(term.cell(5, 0).style()), 3);
}

#[test]
fn a_run_over_halves_of_wide_characters_clears_their_other_halves() {
    let mut term = Term::new(8, 2);
    // Back to the second half of the second 中.
    term.feed("中中中\x08\x08\x08ab");
    assert_eq!(term.screen(), ["中 ab", ""]);
    assert_eq!(term.cell(5, 0), Cell::EMPTY);
    assert_eq!(term.cursor(), (5, 0, false));
}

#[test]
fn ascii_after_a_prepended_character_joins_its_cluster() {
    let mut term = Term::new(6, 2);
    term.feed("\u{D4E}ab");
    assert_eq!(term.screen(), ["\u{D4E}ab", ""]);
    let (page, row) = term.terminal.screen_rows().screen_row(0);
    assert_eq!(page.grapheme(row, 0), ['a']);
    assert_eq!(term.cell(1, 0).content(), Content::Char('b'));
    assert_eq!(term.cursor(), (2, 0, false));
}

#[test]
fn a_mark_after_a_run_joins_its_last_character() {
    let mut term = Term::new(6, 2);
    term.feed("abc\u{301}d");
    let (page, row) = term.terminal.screen_rows().screen_row(0);
    assert_eq!(page.grapheme(row, 2), ['\u{301}']);
    assert_eq!(term.cursor(), (4, 0, false));
}

/// Passes everything to the terminal, but prints runs of ASCII one
/// character at a time, as [`Handler::print_ascii`] does by default.
struct OneByOne<'a>(&'a mut Terminal);

impl Handler for OneByOne<'_> {
    fn print(&mut self, c: char) {
        self.0.print(c);
    }

    fn execute(&mut self, byte: u8) {
        self.0.execute(byte);
    }

    fn esc_dispatch(&mut self, intermediates: &[u8], final_byte: u8) {
        self.0.esc_dispatch(intermediates, final_byte);
    }

    fn csi_dispatch(&mut self, params: &Params, intermediates: &[u8], final_byte: u8) {
        self.0.csi_dispatch(params, intermediates, final_byte);
    }

    fn osc_start(&mut self) {
        self.0.osc_start();
    }

    fn osc_put(&mut self, data: &[u8]) {
        self.0.osc_put(data);
    }

    fn osc_end(&mut self, end: StringEnd) {
        self.0.osc_end(end);
    }

    fn dcs_start(&mut self, params: &Params, intermediates: &[u8], final_byte: u8) {
        self.0.dcs_start(params, intermediates, final_byte);
    }

    fn dcs_put(&mut self, data: &[u8]) {
        self.0.dcs_put(data);
    }

    fn dcs_end(&mut self, end: StringEnd) {
        self.0.dcs_end(end);
    }

    fn apc_start(&mut self) {
        self.0.apc_start();
    }

    fn apc_put(&mut self, data: &[u8]) {
        self.0.apc_put(data);
    }

    fn apc_end(&mut self, end: StringEnd) {
        self.0.apc_end(end);
    }
}

/// A cell as it reads, wherever its page keeps its style and grapheme.
type CellValue = (Cell, Style, Vec<char>);

/// What printing can change in a screen, as values that do not depend on
/// where the tables keep them.
fn screen_state(screen: &Screen) -> (Vec<(Row, Vec<CellValue>)>, Cursor) {
    let rows = screen
        .rows
        .iter_rows()
        .map(|(page, row)| {
            let cells = (0..)
                .zip(page.cells(row))
                .map(|(x, &cell)| {
                    (
                        cell.with_style(StyleId::DEFAULT),
                        *page.styles().get(cell.style()),
                        page.grapheme(row, x).to_vec(),
                    )
                })
                .collect();
            (page.row(row), cells)
        })
        .collect();
    (rows, screen.cursor)
}

/// Checks that each page's styles have as many references as cells that
/// use them, and no other style is in the table.
fn assert_styles_counted(screen: &Screen) {
    use std::collections::HashMap;

    let mut pages: HashMap<*const Page, (&Page, HashMap<StyleId, u32>)> = HashMap::new();
    for (page, row) in screen.rows.iter_rows() {
        let (_, counts) = pages
            .entry(std::ptr::from_ref(page))
            .or_insert_with(|| (page, HashMap::new()));
        for cell in page.cells(row) {
            if cell.style() != StyleId::DEFAULT {
                *counts.entry(cell.style()).or_default() += 1;
            }
        }
    }
    for (page, counts) in pages.values() {
        assert_eq!(page.styles().len(), counts.len(), "styles in the table");
        for (&id, &count) in counts {
            assert_eq!(page.styles().references(id), count, "{id:?}");
        }
    }
}

/// Something that the property does to both terminals.
#[derive(Clone, Debug)]
enum Op {
    /// The bytes of one read.
    Feed(Vec<u8>),
    Pen(Style),
    Autowrap(bool),
    Insert(bool),
    Alternate(bool),
}

/// Text that is not a run of ASCII: controls, marks, prepended and wide
/// characters, the pieces of emoji sequences, and a sequence the terminal
/// ignores.
const OTHER_TEXT: &[&str] = &[
    "\r",
    "\n",
    "\r\n",
    "\x08",
    "\t",
    "\x07",
    "\x0e",
    "\x0f",
    "\x7f",
    "\x1b[1m",
    "\x1b(0",
    "\x1b(B",
    "\x1b)0",
    "\x1b*A",
    "\x1bN",
    "\x1bO",
    "\x1bn",
    "\x1bD",
    "\x1bE",
    "\x1bM",
    "\x1b7",
    "\x1b8",
    "\x1bc",
    "\x1b#8",
    "\x1b[A",
    "\x1b[2B",
    "\x1b[C",
    "\x1b[3D",
    "\x1b[2;3H",
    "\x1b[G",
    "\x1b[2d",
    "\x1b[I",
    "\x1b[Z",
    "\x1b[3g",
    "\u{AD}",
    "\u{E9}",
    "\u{301}",
    "\u{600}",
    "\u{D4E}",
    "\u{93E}",
    "\u{1100}",
    "\u{1161}",
    "\u{2764}",
    "\u{FE0E}",
    "\u{FE0F}",
    "\u{200D}",
    "\u{1F1EE}",
    "中",
    "\u{1F600}",
];

fn read() -> impl Strategy<Value = Vec<u8>> {
    let piece = prop_oneof![
        6 => prop::collection::vec(0x20_u8..=0x7E, 1..24),
        4 => prop::sample::select(OTHER_TEXT).prop_map(|text| text.as_bytes().to_vec()),
        1 => any::<u8>().prop_map(|byte| vec![byte]),
    ];
    prop::collection::vec(piece, 1..8).prop_map(|pieces| pieces.concat())
}

fn op() -> impl Strategy<Value = Op> {
    let styles = [
        Style::DEFAULT,
        Style {
            fg: Color::Palette(1),
            ..Style::DEFAULT
        },
        Style {
            bold: true,
            bg: Color::Palette(4),
            ..Style::DEFAULT
        },
    ];
    prop_oneof![
        12 => read().prop_map(Op::Feed),
        2 => prop::sample::select(styles.to_vec()).prop_map(Op::Pen),
        1 => any::<bool>().prop_map(Op::Autowrap),
        1 => any::<bool>().prop_map(Op::Insert),
        1 => any::<bool>().prop_map(Op::Alternate),
    ]
}

fn widths() -> impl Strategy<Value = Widths> {
    (
        prop::sample::select(vec![Mode::Unicode, Mode::Legacy]),
        prop::sample::select(vec![Tables::Current, Tables::Unicode8]),
        any::<bool>(),
    )
        .prop_map(|(mode, tables, ambiguous_wide)| Widths {
            mode,
            tables,
            ambiguous_wide,
        })
}

fn normalization() -> impl Strategy<Value = Normalization> {
    prop::sample::select(vec![
        Normalization::None,
        Normalization::Nfc,
        Normalization::Nfd,
        Normalization::HfsPlus,
    ])
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1024))]

    #[test]
    fn runs_of_ascii_print_as_their_characters_do(
        cols in 1_u16..=10,
        rows in 1_u16..=4,
        scrollback in any::<bool>(),
        widths in widths(),
        normalization in normalization(),
        ops in prop::collection::vec(op(), 1..24),
    ) {
        let options = || Options {
            scrollback: if scrollback {
                Scrollback::default()
            } else {
                Scrollback { limit: 0, ..Scrollback::default() }
            },
            widths,
            normalization,
        };
        let mut fast = Term::with(cols, rows, options());
        let mut slow = Term::with(cols, rows, options());
        for op in &ops {
            for (term, one_by_one) in [(&mut fast, false), (&mut slow, true)] {
                let terminal = &mut term.terminal;
                match op {
                    Op::Feed(bytes) if one_by_one => {
                        term.parser.advance(&mut OneByOne(terminal), bytes);
                    }
                    Op::Feed(bytes) => term.parser.advance(terminal, bytes),
                    Op::Pen(style) => terminal.set_pen(*style),
                    &Op::Autowrap(on) => terminal.set_autowrap(on),
                    &Op::Insert(on) => terminal.set_insert_mode(on),
                    &Op::Alternate(on) => terminal.set_alternate_screen(on),
                }
            }
            let (fast, slow) = (&fast.terminal, &slow.terminal);
            prop_assert_eq!(screen_state(&fast.primary), screen_state(&slow.primary), "after {:?}", op);
            prop_assert_eq!(screen_state(&fast.alternate), screen_state(&slow.alternate), "after {:?}", op);
            prop_assert_eq!(fast.cluster, slow.cluster, "after {:?}", op);
            prop_assert_eq!(fast.segmenter, slow.segmenter, "after {:?}", op);
        }
        for screen in [&fast.terminal.primary, &fast.terminal.alternate] {
            assert_styles_counted(screen);
        }
    }
}
