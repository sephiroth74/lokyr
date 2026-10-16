//! The terminal driven by the parser, as programs drive it.

use proptest::prelude::*;

use super::*;
use crate::page::{
    Capacity, Color, HyperlinkId, Page, PinId, Position, Rgb, Row, RowFlag, Semantic, StyleId,
    Underline,
};
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
fn a_wide_character_always_has_room() {
    // A terminal has two columns at least, and DECSLRM leaves two.
    let mut term = Term::new(1, 2);
    assert_eq!(term.terminal.cols(), 2);
    term.feed("\x1b[2G中a");
    assert_eq!(
        term.screen(),
        ["中", "a"],
        "the wide character went on in the next row"
    );
    term.feed("\x1b[H\x1b[2J\x1b[?69h\x1b[2;2s\x1b[1;2H中");
    assert_eq!(term.screen()[1], "中", "a margin of one column is refused");
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
fn el_erases_to_the_end_from_the_start_or_the_whole_row() {
    let mut term = Term::new(8, 1);
    term.feed("abcdef\x1b[3G\x1b[K");
    assert_eq!(term.screen(), ["ab"]);
    assert_eq!(term.cursor(), (2, 0, false));
    term.feed("\rabcdef\x1b[3G\x1b[1K");
    assert_eq!(term.screen(), ["   def"]);
    term.feed("\x1b[2K");
    assert_eq!(term.screen(), [""]);
    assert_eq!(term.cursor(), (2, 0, false), "the cursor stays");
}

#[test]
fn ed_erases_below_above_or_the_whole_screen() {
    let lines = "aaaa\r\nbbbb\r\ncccc\x1b[2;3H";
    let mut term = Term::new(4, 3);
    term.feed(lines).feed("\x1b[J");
    assert_eq!(term.screen(), ["aaaa", "bb", ""]);
    let mut term = Term::new(4, 3);
    term.feed(lines).feed("\x1b[1J");
    assert_eq!(term.screen(), ["", "   b", "cccc"]);
    let mut term = Term::new(4, 3);
    term.feed(lines).feed("\x1b[2J");
    assert_eq!(term.screen(), ["", "", ""]);
    assert_eq!(term.cursor(), (2, 1, false));
}

#[test]
fn ed_2_moves_the_primary_screen_into_the_scrollback() {
    let mut term = Term::new(4, 3);
    term.feed("a\r\nb\r\n\x1b[2J");
    assert_eq!(term.screen(), ["", "", ""]);
    assert_eq!(term.terminal.screen_rows().scrollback_rows(), 2, "up to b");
    term.feed("\x1b[2J");
    assert_eq!(
        term.terminal.screen_rows().scrollback_rows(),
        2,
        "nothing to move"
    );
    term.terminal.set_alternate_screen(true);
    term.feed("x\x1b[2J");
    assert_eq!(term.screen(), ["", "", ""]);
    assert_eq!(term.terminal.screen_rows().scrollback_rows(), 0);
}

#[test]
fn ed_3_clears_the_scrollback() {
    let mut term = Term::new(4, 2);
    term.feed("1\r\n2\r\n3\r\n4\x1b[3J");
    assert_eq!(term.terminal.screen_rows().scrollback_rows(), 0);
    assert_eq!(term.screen(), ["3", "4"]);
    term.feed("\r\n5\x1b[?3J");
    assert_eq!(
        term.terminal.screen_rows().scrollback_rows(),
        1,
        "DECSED has no 3"
    );
}

#[test]
fn ech_erases_cells_from_the_cursor() {
    let mut term = Term::new(8, 1);
    term.feed("abcdef\x1b[2G\x1b[2X");
    assert_eq!(term.screen(), ["a  def"]);
    term.feed("\x1b[X");
    assert_eq!(term.screen(), ["a  def"], "0 counts as 1");
    term.feed("\x1b[5G\x1b[99X");
    assert_eq!(term.screen(), ["a  d"]);
    assert_eq!(term.cursor(), (4, 0, false));
}

#[test]
fn erased_cells_take_the_background_color() {
    let mut term = Term::new(4, 2);
    term.terminal.set_pen(Style {
        bg: Color::Palette(4),
        bold: true,
        ..Style::DEFAULT
    });
    term.feed("ab\x1b[1G\x1b[X");
    assert_eq!(term.cell(0, 0).content(), Content::BgPalette(4));
    assert_eq!(term.cell(0, 0).style(), StyleId::DEFAULT, "only the color");
    term.terminal.set_pen(Style {
        bg: Color::Rgb(Rgb::new(1, 2, 3)),
        ..Style::DEFAULT
    });
    term.feed("\x1b[J");
    assert_eq!(term.cell(3, 1).content(), Content::BgRgb(Rgb::new(1, 2, 3)));
}

#[test]
fn erasing_half_of_a_wide_character_erases_it_whole() {
    let mut term = Term::new(6, 1);
    term.feed("a中b\x1b[3G\x1b[K");
    assert_eq!(term.screen(), ["a"]);
    term.feed("\ra中b\x1b[2G\x1b[1K");
    assert_eq!(term.screen(), ["   b"]);
}

#[test]
fn an_erase_to_the_end_of_a_row_ends_its_wrap() {
    let mut term = Term::new(5, 2);
    term.feed("abcdefg\x1b[1;3H\x1b[1K");
    assert!(
        term.row(0).has(RowFlag::SoftWrapped),
        "the end is still there"
    );
    term.feed("\x1b[K");
    assert!(!term.row(0).has(RowFlag::SoftWrapped));
    assert!(!term.row(1).has(RowFlag::Continuation));
}

#[test]
fn erasing_ends_the_cluster_and_the_wrap() {
    let mut term = Term::new(5, 1);
    term.feed("e\x1b[1K\u{301}");
    let (page, row) = term.terminal.screen_rows().screen_row(0);
    assert_eq!(
        page.grapheme(row, 0),
        [],
        "the mark stayed off the erased e"
    );
    term.feed("\rabcde\x1b[K");
    assert_eq!(term.cursor(), (4, 0, false), "no wrap pending");
}

#[test]
fn the_selective_erases_keep_protected_cells() {
    let mut term = Term::new(6, 2);
    term.feed("\x1b[1\"qab\x1b[0\"qcd\r\n\x1b[1\"q中\x1b[2\"qxy");
    assert!(term.cell(1, 0).is_protected());
    assert!(term.cell(1, 1).is_protected(), "the second half too");
    assert!(!term.cell(2, 0).is_protected());
    term.feed("\x1b[?2J");
    assert_eq!(term.screen(), ["ab", "中"]);
    term.feed("\x1b[1;1H\x1b[?K\x1b[K");
    assert_eq!(term.screen(), ["", "中"], "EL erases them all");
    term.feed("\x1b[2;1H\x1b[?2K");
    assert_eq!(term.screen(), ["", "中"]);
    term.feed("\x1b[2K");
    assert_eq!(term.screen(), ["", ""]);
}

#[test]
fn decsc_saves_the_protection() {
    let mut term = Term::new(4, 1);
    term.feed("\x1b[1\"q\x1b7\x1b[0\"q\x1b8a\x1b[3\"qb");
    assert!(term.cell(0, 0).is_protected());
    assert!(term.cell(1, 0).is_protected(), "3 does nothing");
}

/// A screen of 4 by 5 whose rows read 0 to 4.
fn numbered_rows() -> Term {
    let mut term = Term::new(4, 5);
    term.feed("0\r\n1\r\n2\r\n3\r\n4");
    term
}

fn blue_background() -> Style {
    Style {
        bg: Color::Palette(4),
        ..Style::DEFAULT
    }
}

#[test]
fn ich_inserts_erased_cells_at_the_cursor() {
    let mut term = Term::new(8, 1);
    term.feed("abcdef\x1b[2G\x1b[2@");
    assert_eq!(term.screen(), ["a  bcdef"]);
    assert_eq!(term.cursor(), (1, 0, false));
    term.terminal.set_pen(blue_background());
    term.feed("\x1b[@");
    assert_eq!(term.screen(), ["a   bcde"]);
    assert_eq!(term.cell(1, 0).content(), Content::BgPalette(4));
    term.feed("\x1b[99@");
    assert_eq!(term.screen(), ["a"]);
    let mut term = Term::new(6, 1);
    term.feed("a中b\x1b[3G\x1b[@");
    assert_eq!(term.screen(), ["a   b"], "the wide character went whole");
}

#[test]
fn dch_deletes_cells_at_the_cursor() {
    let mut term = Term::new(8, 1);
    term.feed("abcdef\x1b[2G\x1b[2P");
    assert_eq!(term.screen(), ["adef"]);
    term.terminal.set_pen(blue_background());
    term.feed("\x1b[P");
    assert_eq!(term.screen(), ["aef"]);
    assert_eq!(
        term.cell(7, 0).content(),
        Content::BgPalette(4),
        "at the end"
    );
    term.feed("\x1b[99P");
    assert_eq!(term.screen(), ["a"]);
    let mut term = Term::new(6, 1);
    term.feed("a中b\x1b[3G\x1b[P");
    assert_eq!(term.screen(), ["a b"]);
}

#[test]
fn il_and_dl_move_the_rows_below_the_cursor() {
    let mut term = numbered_rows();
    term.feed("\x1b[3;2H\x1b[L");
    assert_eq!(term.screen(), ["0", "1", "", "2", "3"]);
    assert_eq!(term.cursor(), (0, 2, false), "to the left margin");
    term.feed("\x1b[2;3H\x1b[2M");
    assert_eq!(term.screen(), ["0", "2", "3", "", ""]);
    term.feed("\x1b[99L");
    assert_eq!(term.screen(), ["0", "", "", "", ""]);
    assert_eq!(term.terminal.screen_rows().scrollback_rows(), 0);
}

#[test]
fn il_and_dl_stay_within_the_scroll_region() {
    let mut term = numbered_rows();
    term.terminal.margins.top = 1;
    term.terminal.margins.bottom = 3;
    term.feed("\x1b[3;1H\x1b[L");
    assert_eq!(term.screen(), ["0", "1", "", "2", "4"]);
    term.feed("\x1b[2;1H\x1b[M");
    assert_eq!(term.screen(), ["0", "", "2", "", "4"]);
    // Outside the region, they do nothing.
    term.feed("\x1b[5;3H\x1b[L\x1b[1;3H\x1b[M");
    assert_eq!(term.screen(), ["0", "", "2", "", "4"]);
    assert_eq!(term.cursor(), (2, 0, false));
}

#[test]
fn il_and_dl_bring_in_erased_rows() {
    let mut term = numbered_rows();
    term.terminal.set_pen(blue_background());
    term.feed("\x1b[2;1H\x1b[L\x1b[4;1H\x1b[M");
    assert_eq!(term.cell(3, 1).content(), Content::BgPalette(4));
    assert_eq!(term.cell(0, 4).content(), Content::BgPalette(4));
}

#[test]
fn su_and_sd_scroll_the_region() {
    let mut term = numbered_rows();
    term.feed("\x1b[2;2H\x1b[2S");
    assert_eq!(term.screen(), ["2", "3", "4", "", ""]);
    assert_eq!(
        term.terminal.screen_rows().scrollback_rows(),
        2,
        "the whole screen scrolled"
    );
    assert_eq!(term.cursor(), (1, 1, false), "the cursor stays");
    term.feed("\x1b[T");
    assert_eq!(term.screen(), ["", "2", "3", "4", ""]);
    let mut term = numbered_rows();
    term.terminal.margins.top = 1;
    term.terminal.margins.bottom = 3;
    term.feed("\x1b[S");
    assert_eq!(term.screen(), ["0", "2", "3", "", "4"]);
    assert_eq!(term.terminal.screen_rows().scrollback_rows(), 0);
    term.feed("\x1b[2T");
    assert_eq!(term.screen(), ["0", "", "", "2", "4"]);
    term.feed("\x1b[99S");
    assert_eq!(term.screen(), ["0", "", "", "", "4"]);
    // CSI T with five parameters is not SD.
    let mut term = numbered_rows();
    term.feed("\x1b[1;1;1;1;1T");
    assert_eq!(term.screen(), ["0", "1", "2", "3", "4"]);
}

#[test]
fn scrolling_brings_in_erased_rows() {
    let mut term = numbered_rows();
    term.terminal.set_pen(blue_background());
    term.feed("\x1b[S");
    assert_eq!(term.cell(2, 4).content(), Content::BgPalette(4));
    term.feed("\x1b[T");
    assert_eq!(term.cell(2, 0).content(), Content::BgPalette(4));
    term.feed("\x1b[H\x1bM");
    assert_eq!(term.cell(2, 0).content(), Content::BgPalette(4), "RI too");
}

#[test]
fn rep_repeats_the_last_character_printed() {
    let mut term = Term::new(10, 2);
    term.feed("hello\x1b[2b");
    assert_eq!(term.screen()[0], "hellooo");
    term.feed("\r\n\u{E9}\x1b[b");
    assert_eq!(term.screen()[1], "\u{E9}\u{E9}");
    // Through the character set in use, and after the cursor moved.
    term.feed("\x1b[2;1H\x1b(0q\x1b(B\r\x1b[3b");
    assert_eq!(term.screen()[1], "qqq");
    let mut term = Term::new(10, 1);
    term.feed("\x1b[3b");
    assert_eq!(term.screen(), [""], "nothing to repeat");
    term.feed("a\x1bc\x1b[3b");
    assert_eq!(term.screen(), [""], "RIS forgets it");
}

#[test]
fn editing_ends_the_cluster_and_the_wrap() {
    let mut term = Term::new(5, 2);
    // The b moved down a row: the mark must not land where it was.
    term.feed("ab\x1b[L\u{301}");
    let (page, row) = term.terminal.screen_rows().screen_row(0);
    assert_eq!(page.grapheme(row, 1), []);
    assert_eq!(term.screen(), ["", "ab"]);
    term.feed("\rabcde\x1b[P");
    assert_eq!(term.cursor(), (4, 0, false));
}

#[test]
fn decstbm_sets_the_scroll_region_and_puts_the_cursor_home() {
    let mut term = numbered_rows();
    term.feed("\x1b[3;3H\x1b[2;4r");
    assert_eq!(
        (term.terminal.margins.top, term.terminal.margins.bottom),
        (1, 3)
    );
    assert_eq!(term.cursor(), (0, 0, false));
    // A region of one row, or upside down, is ignored.
    term.feed("\x1b[3;3r\x1b[4;2r");
    assert_eq!(
        (term.terminal.margins.top, term.terminal.margins.bottom),
        (1, 3)
    );
    term.feed("\x1b[2;99r");
    assert_eq!(term.terminal.margins.bottom, 4, "the last row at most");
    term.feed("\x1b[r");
    assert_eq!(
        (term.terminal.margins.top, term.terminal.margins.bottom),
        (0, 4)
    );
    term.terminal.set_origin_mode(true);
    term.feed("\x1b[3;4r");
    assert_eq!(term.cursor(), (0, 2, false), "home of the region");
}

#[test]
fn lf_scrolls_the_region_at_its_bottom_margin() {
    let mut term = numbered_rows();
    term.feed("\x1b[2;4r\x1b[4;2H\n");
    assert_eq!(term.screen(), ["0", "2", "3", "", "4"]);
    assert_eq!(term.cursor(), (1, 3, false));
    assert_eq!(term.terminal.screen_rows().scrollback_rows(), 0);
    // IND and NEL too.
    term.feed("\x1bD\x1bE");
    assert_eq!(term.screen(), ["0", "", "", "", "4"]);
    assert_eq!(term.cursor(), (0, 3, false));
}

#[test]
fn lf_outside_the_region_moves_down_to_the_last_row() {
    let mut term = numbered_rows();
    term.feed("\x1b[2;3r\x1b[4;1H\n");
    assert_eq!(term.cursor(), (0, 4, false));
    term.feed("\n");
    assert_eq!(term.cursor(), (0, 4, false), "the last row stops it");
    assert_eq!(term.screen(), ["0", "1", "2", "3", "4"]);
    // From above, into the region.
    term.feed("\x1b[2;4r\x1b[1;1H\n\n\n\n");
    assert_eq!(term.cursor(), (0, 3, false));
    assert_eq!(term.screen(), ["0", "2", "3", "", "4"]);
}

#[test]
fn ri_scrolls_the_region_down_at_its_top_margin() {
    let mut term = numbered_rows();
    term.feed("\x1b[2;4r\x1b[2;1H\x1bM");
    assert_eq!(term.screen(), ["0", "", "1", "2", "4"]);
    assert_eq!(term.cursor(), (0, 1, false));
    // Above the region, it moves up to the first row.
    term.feed("\x1b[1;1H\x1bM");
    assert_eq!(term.cursor(), (0, 0, false));
    assert_eq!(term.screen(), ["0", "", "1", "2", "4"]);
    // Below it, up into it.
    term.feed("\x1b[5;1H\x1bM\x1bM");
    assert_eq!(term.cursor(), (0, 2, false));
}

#[test]
fn printing_wraps_within_the_scroll_region() {
    let mut term = numbered_rows();
    term.feed("\x1b[2;4r\x1b[4;1Habcdef");
    assert_eq!(term.screen(), ["0", "2", "abcd", "ef", "4"]);
}

#[test]
fn rows_scrolled_in_take_the_background_color() {
    let mut term = numbered_rows();
    term.terminal.set_pen(blue_background());
    term.feed("\x1b[5;1H\n");
    assert_eq!(term.cell(3, 4).content(), Content::BgPalette(4));
    term.feed("\x1b[2;3r\x1b[3;1H\n");
    assert_eq!(term.cell(3, 2).content(), Content::BgPalette(4));
}

#[test]
fn scosc_and_scorc_save_and_restore_the_cursor() {
    let mut term = Term::new(10, 3);
    term.feed("ab\x1b[s\r\n\x1b[u");
    assert_eq!(term.cursor(), (2, 0, false));
    // The kitty keyboard protocol's CSI u is not SCORC.
    term.feed("\r\n\x1b[?u\x1b[>1u\x1b[<u\x1b[=1;1u\x1b[1u");
    assert_eq!(term.cursor(), (0, 1, false));
}

#[test]
fn decaln_resets_the_margins() {
    let mut term = numbered_rows();
    term.feed("\x1b[2;3r\x1b#8");
    assert_eq!(
        (term.terminal.margins.top, term.terminal.margins.bottom),
        (0, 4)
    );
}

/// A screen of 10 by 5 whose rows are full of a to e, with left and right
/// margins at columns 2 to 5 and top and bottom ones at rows 1 to 3.
fn lettered_region() -> Term {
    let mut term = Term::new(10, 5);
    term.feed("aaaaaaaaaa\r\nbbbbbbbbbb\r\ncccccccccc\r\ndddddddddd\r\neeeeeeeeee");
    term.feed("\x1b[?69h\x1b[3;6s\x1b[2;4r");
    term
}

#[test]
fn declrmm_makes_csi_s_set_the_left_and_right_margins() {
    let mut term = Term::new(10, 3);
    term.feed("ab\x1b[3;6s\r\n\x1b[u");
    assert_eq!(
        term.cursor(),
        (2, 0, false),
        "without DECLRMM, CSI s is SCOSC"
    );
    assert_eq!(
        (term.terminal.margins.left, term.terminal.margins.right),
        (0, 9)
    );
    term.feed("\x1b[?69h\x1b[2;2H\x1b[3;6s");
    assert!(term.terminal.left_right_margin_mode());
    assert_eq!(
        (term.terminal.margins.left, term.terminal.margins.right),
        (2, 5)
    );
    assert_eq!(term.cursor(), (0, 0, false));
    term.feed("\x1b[5;5s\x1b[6;3s");
    assert_eq!(
        (term.terminal.margins.left, term.terminal.margins.right),
        (2, 5),
        "ignored"
    );
    term.feed("\x1b[2;99s");
    assert_eq!(
        (term.terminal.margins.left, term.terminal.margins.right),
        (1, 9)
    );
    term.terminal.set_origin_mode(true);
    term.feed("\x1b[3;6s");
    assert_eq!(term.cursor(), (2, 0, false), "home of the margins");
    term.feed("\x1b[?69l");
    assert!(!term.terminal.left_right_margin_mode());
    assert_eq!(
        (term.terminal.margins.left, term.terminal.margins.right),
        (0, 9)
    );
}

#[test]
fn printing_wraps_at_the_right_margin_to_the_left_one() {
    let sequences = "\x1b[?69h\x1b[3;6s\x1b[1;3Habcdefgh";
    let mut term = Term::new(10, 3);
    term.feed(sequences);
    assert_eq!(term.screen(), ["  abcd", "  efgh", ""]);
    assert_eq!(term.cursor(), (5, 1, true));
    assert!(!term.row(0).has(RowFlag::SoftWrapped), "not one line");
    assert!(!term.row(1).has(RowFlag::Continuation));
    // A byte at a time, through the print path of single characters.
    let mut slow = Term::new(10, 3);
    for byte in sequences.bytes() {
        slow.feed([byte]);
    }
    assert_eq!(slow.screen(), term.screen());
    assert_eq!(slow.cursor(), term.cursor());
}

#[test]
fn printing_right_of_the_right_margin_goes_to_the_edge() {
    let mut term = Term::new(10, 3);
    term.feed("\x1b[?69h\x1b[3;6s\x1b[1;8Hxyzw");
    assert_eq!(term.screen(), ["       xyz", "  w", ""]);
    assert!(
        !term.row(0).has(RowFlag::SoftWrapped),
        "the next line starts at the margin"
    );
}

#[test]
fn a_wide_character_at_the_right_margin_wraps_to_the_left_one() {
    let mut term = Term::new(10, 3);
    term.feed("\x1b[?69h\x1b[3;6s\x1b[1;6H中");
    assert_eq!(term.cell(5, 0).width(), Width::SpacerHead);
    assert_eq!(term.screen(), ["", "  中", ""]);
    // Without autowrap, it goes back a column.
    let mut term = Term::new(10, 3);
    term.terminal.set_autowrap(false);
    term.feed("\x1b[?69h\x1b[3;6s\x1b[1;6H中");
    assert_eq!(term.screen(), ["    中", "", ""]);
}

#[test]
fn insert_mode_pushes_cells_as_far_as_the_right_margin() {
    let mut term = Term::new(10, 1);
    term.feed("0123456789\x1b[?69h\x1b[3;6s\x1b[1;3H");
    term.terminal.set_insert_mode(true);
    term.feed("X");
    assert_eq!(term.screen(), ["01X2346789"]);
}

#[test]
fn lf_scrolls_the_cells_between_the_four_margins() {
    let mut term = lettered_region();
    term.feed("\x1b[4;3H\n");
    assert_eq!(
        term.screen(),
        [
            "aaaaaaaaaa",
            "bbccccbbbb",
            "ccddddcccc",
            "dd    dddd",
            "eeeeeeeeee"
        ]
    );
    assert_eq!(term.terminal.screen_rows().scrollback_rows(), 0);
    // Outside the left and right margins, the cursor stays at the bottom one.
    term.feed("\x1b[4;9H\n");
    assert_eq!(term.cursor(), (8, 3, false));
    assert_eq!(term.screen()[1], "bbccccbbbb");
}

#[test]
fn ri_scrolls_the_cells_between_the_four_margins_down() {
    let mut term = lettered_region();
    term.feed("\x1b[2;4H\x1bM");
    assert_eq!(
        term.screen(),
        [
            "aaaaaaaaaa",
            "bb    bbbb",
            "ccbbbbcccc",
            "ddccccdddd",
            "eeeeeeeeee"
        ]
    );
    let before = term.screen();
    term.feed("\x1b[2;2H\x1bM");
    assert_eq!(term.cursor(), (1, 1, false), "outside them, it stays");
    assert_eq!(term.screen(), before, "and nothing scrolls");
}

#[test]
fn il_and_dl_move_the_cells_between_the_margins() {
    let mut term = lettered_region();
    term.feed("\x1b[2;4H\x1b[L");
    assert_eq!(
        term.screen(),
        [
            "aaaaaaaaaa",
            "bb    bbbb",
            "ccbbbbcccc",
            "ddccccdddd",
            "eeeeeeeeee"
        ]
    );
    assert_eq!(term.cursor(), (2, 1, false));
    term.feed("\x1b[M\x1b[M");
    assert_eq!(
        term.screen(),
        [
            "aaaaaaaaaa",
            "bbccccbbbb",
            "cc    cccc",
            "dd    dddd",
            "eeeeeeeeee"
        ]
    );
    // Outside the left and right margins, they do nothing.
    let before = term.screen();
    term.feed("\x1b[2;8H\x1b[L");
    assert_eq!(term.screen(), before);
    assert_eq!(term.cursor(), (7, 1, false), "the cursor stays");
    term.feed("\x1b[M");
    assert_eq!(term.screen(), before);
    assert_eq!(term.cursor(), (7, 1, false));
}

#[test]
fn ich_and_dch_stay_between_the_margins() {
    let mut term = Term::new(10, 1);
    term.feed("0123456789\x1b[?69h\x1b[3;6s\x1b[1;4H\x1b[2@");
    assert_eq!(term.screen(), ["012  36789"]);
    // The cells past the right margin stay where they are.
    term.feed("\x1b[3P");
    assert_eq!(term.screen(), ["012   6789"]);
    term.feed("\x1b[1;8H\x1b[@\x1b[P\x1b[1;1H\x1b[@");
    assert_eq!(term.screen(), ["012   6789"], "outside them, nothing");
}

#[test]
fn su_and_sd_scroll_the_cells_between_the_margins() {
    let mut term = lettered_region();
    term.feed("\x1b[S");
    assert_eq!(
        term.screen()[1..4],
        ["bbccccbbbb", "ccddddcccc", "dd    dddd"]
    );
    assert_eq!(term.terminal.screen_rows().scrollback_rows(), 0);
    term.feed("\x1b[2T");
    assert_eq!(
        term.screen()[1..4],
        ["bb    bbbb", "cc    cccc", "ddccccdddd"]
    );
}

#[test]
fn tabs_stop_at_the_margins() {
    let mut term = Term::new(20, 1);
    term.feed("\x1b[?69h\x1b[3;6s");
    assert_eq!(moved(&mut term, 2, 0, "\t"), (5, 0, false));
    assert_eq!(
        moved(&mut term, 6, 0, "\t"),
        (8, 0, false),
        "from right of them"
    );
    assert_eq!(moved(&mut term, 2, 0, "\x1b[3I"), (5, 0, false));
    assert_eq!(moved(&mut term, 5, 0, "\x1b[Z"), (2, 0, false));
    assert_eq!(
        moved(&mut term, 1, 0, "\x1b[Z"),
        (0, 0, false),
        "from left of them"
    );
}

#[test]
fn a_wrap_left_pending_by_margins_that_moved_does_not_hold_the_cursor() {
    // The wrap is pending at column 3, the last one within the margins, which
    // then go back to the edges of the screen.
    let sequences = "\x1b[?69h\x1b[2;4s    \x1b[?69l ";
    let mut term = Term::new(5, 1);
    term.terminal.set_autowrap(false);
    term.feed(sequences);
    assert_eq!(term.cursor(), (4, 0, false));
    let mut slow = Term::new(5, 1);
    slow.terminal.set_autowrap(false);
    for byte in sequences.bytes() {
        slow.feed([byte]);
    }
    assert_eq!(slow.cursor(), term.cursor());
}

#[test]
fn a_cluster_at_the_right_margin_has_no_room_to_widen() {
    let mut term = Term::new(10, 1);
    term.feed("\x1b[?69h\x1b[3;6s\x1b[1;6H\u{2764}\u{FE0F}");
    assert_eq!(term.cell(5, 0).width(), Width::Narrow);
    assert_eq!(term.cell(6, 0), Cell::EMPTY);
    assert_eq!(term.cursor(), (5, 0, true));
}

#[test]
fn scrolling_empties_wide_characters_across_the_margins() {
    let mut term = Term::new(10, 3);
    term.feed("\r\n\x1b[2;6H中\x1b[?69h\x1b[3;6s\x1b[1;1H\x1b[S");
    assert_eq!(term.screen(), ["", "", ""]);
    assert!(
        (0..10).all(|x| term.cell(x, 1).width() == Width::Narrow),
        "no half of 中 is left"
    );
}

/// The pen after `sequences`, from the default one.
fn pen_after(sequences: &str) -> Style {
    let mut term = Term::new(4, 1);
    term.feed(sequences);
    *term.terminal.pen()
}

#[test]
fn sgr_turns_each_attribute_on_and_off() {
    type Field = fn(&Style) -> bool;
    let attributes: [(u16, u16, Field); 9] = [
        (1, 22, |style| style.bold),
        (2, 22, |style| style.faint),
        (3, 23, |style| style.italic),
        (5, 25, |style| style.blink),
        (6, 25, |style| style.blink),
        (7, 27, |style| style.inverse),
        (8, 28, |style| style.invisible),
        (9, 29, |style| style.strikethrough),
        (53, 55, |style| style.overline),
    ];
    for (on, off, field) in attributes {
        assert!(field(&pen_after(&format!("\x1b[{on}m"))), "{on}");
        assert!(
            !field(&pen_after(&format!("\x1b[{on}m\x1b[{off}m"))),
            "{off}"
        );
        assert!(
            !field(&pen_after(&format!("\x1b[{on}m\x1b[0m"))),
            "0 after {on}"
        );
    }
    let both = pen_after("\x1b[1;2m");
    assert!(both.bold && both.faint);
    assert_eq!(pen_after("\x1b[1;2;22m"), Style::DEFAULT, "22 ends both");
    assert_eq!(
        pen_after("\x1b[1;3;31m\x1b[m"),
        Style::DEFAULT,
        "an empty SGR resets"
    );
    assert_eq!(
        pen_after("\x1b[1;0;3m"),
        Style {
            italic: true,
            ..Style::DEFAULT
        }
    );
}

#[test]
fn sgr_chooses_the_underline() {
    let underline = |sequence: &str| pen_after(sequence).underline;
    assert_eq!(underline("\x1b[4m"), Underline::Single);
    assert_eq!(underline("\x1b[21m"), Underline::Double);
    for (style, expected) in [
        (0, Underline::None),
        (1, Underline::Single),
        (2, Underline::Double),
        (3, Underline::Curly),
        (4, Underline::Dotted),
        (5, Underline::Dashed),
    ] {
        assert_eq!(
            underline(&format!("\x1b[4m\x1b[4:{style}m")),
            expected,
            "4:{style}"
        );
    }
    assert_eq!(
        underline("\x1b[4:3m\x1b[4:9m"),
        Underline::Curly,
        "4:9 is unknown"
    );
    assert_eq!(underline("\x1b[4:3m\x1b[24m"), Underline::None);
}

#[test]
fn sgr_chooses_colors_of_the_palette() {
    for n in 0..8 {
        assert_eq!(
            pen_after(&format!("\x1b[{}m", 30 + n)).fg,
            Color::Palette(n as u8)
        );
        assert_eq!(
            pen_after(&format!("\x1b[{}m", 90 + n)).fg,
            Color::Palette(8 + n as u8)
        );
        assert_eq!(
            pen_after(&format!("\x1b[{}m", 40 + n)).bg,
            Color::Palette(n as u8)
        );
        assert_eq!(
            pen_after(&format!("\x1b[{}m", 100 + n)).bg,
            Color::Palette(8 + n as u8)
        );
    }
    assert_eq!(pen_after("\x1b[31;39m").fg, Color::Default);
    assert_eq!(pen_after("\x1b[41;49m").bg, Color::Default);
}

#[test]
fn sgr_reads_extended_colors_with_semicolons_and_with_colons() {
    let rgb = Color::Rgb(Rgb::new(10, 20, 30));
    for (which, field) in [(38, 0), (48, 1), (58, 2)] {
        let color = |sequence: String| {
            let pen = pen_after(&sequence);
            [pen.fg, pen.bg, pen.underline_color][field]
        };
        assert_eq!(color(format!("\x1b[{which};5;196m")), Color::Palette(196));
        assert_eq!(color(format!("\x1b[{which}:5:196m")), Color::Palette(196));
        assert_eq!(color(format!("\x1b[{which};2;10;20;30m")), rgb);
        assert_eq!(color(format!("\x1b[{which}:2::10:20:30m")), rgb);
        assert_eq!(color(format!("\x1b[{which}:2:0:10:20:30m")), rgb);
        assert_eq!(
            color(format!("\x1b[{which}:2:10:20:30m")),
            rgb,
            "without an id"
        );
    }
    assert_eq!(
        pen_after("\x1b[58;5;1m\x1b[59m").underline_color,
        Color::Default
    );
}

#[test]
fn sgr_goes_on_after_an_extended_color() {
    let pen = pen_after("\x1b[1;38;5;196;4m");
    assert!(pen.bold);
    assert_eq!(pen.fg, Color::Palette(196));
    assert_eq!(pen.underline, Underline::Single);
    let pen = pen_after("\x1b[38;2;1;2;3;3;48:5:4m");
    assert_eq!(pen.fg, Color::Rgb(Rgb::new(1, 2, 3)));
    assert!(pen.italic);
    assert_eq!(pen.bg, Color::Palette(4));
}

#[test]
fn sgr_skips_colors_it_cannot_read() {
    assert_eq!(pen_after("\x1b[38;5;300m").fg, Color::Default);
    assert_eq!(pen_after("\x1b[38:2::300:0:0m").fg, Color::Default);
    assert_eq!(pen_after("\x1b[38:3:1:2:3m").fg, Color::Default, "CMY");
    assert_eq!(pen_after("\x1b[38;2;1;2m").fg, Color::Default, "cut short");
    assert_eq!(pen_after("\x1b[38m").fg, Color::Default);
    // As in vte, the values after the one that fails count as attributes.
    let pen = pen_after("\x1b[38;2;300;1;3m");
    assert_eq!(pen.fg, Color::Default);
    assert!(pen.bold && pen.italic);
}

#[test]
fn sgr_with_a_private_marker_is_not_sgr() {
    // CSI > 4 ; 1 m is xterm's modifyOtherKeys.
    assert_eq!(pen_after("\x1b[>4;1m"), Style::DEFAULT);
    assert_eq!(pen_after("\x1b[?1m"), Style::DEFAULT);
}

#[test]
fn sgr_leaves_the_cluster_open_and_printed_cells_take_the_pen() {
    let mut term = Term::new(6, 1);
    term.feed("e\x1b[1;31m\u{301}x");
    let (page, row) = term.terminal.screen_rows().screen_row(0);
    assert_eq!(page.grapheme(row, 0), ['\u{301}']);
    assert_eq!(
        style_at(&term, 0, 0),
        Style::DEFAULT,
        "the e keeps its style"
    );
    assert_eq!(
        style_at(&term, 1, 0),
        Style {
            bold: true,
            fg: Color::Palette(1),
            ..Style::DEFAULT
        }
    );
}

/// What the terminal replied, as text.
fn replies(term: &mut Term) -> String {
    String::from_utf8(term.terminal.take_replies()).expect("replies are text")
}

#[test]
fn irm_and_lnm_are_the_ansi_modes() {
    let mut term = Term::new(6, 3);
    term.feed("abc\r\x1b[4hX\x1b[4$p\x1b[20$p");
    assert!(term.terminal.insert_mode());
    assert_eq!(term.screen()[0], "Xabc");
    assert_eq!(replies(&mut term), "\x1b[4;1$y\x1b[20;2$y");
    term.feed("\x1b[4l\x1b[20hz\n");
    assert!(!term.terminal.insert_mode());
    assert!(term.terminal.input_modes().newline);
    assert_eq!(term.cursor(), (0, 1, false), "LF went to a new line");
    term.feed("z\x0b");
    assert_eq!(term.cursor(), (0, 2, false), "VT too");
    term.feed("w\x1bD");
    assert_eq!(term.cursor(), (1, 2, false), "but not IND");
    term.feed("\x1b[20l\x1b[20$p\x1b[3$p");
    assert_eq!(replies(&mut term), "\x1b[20;2$y\x1b[3;0$y", "3 is unknown");
}

#[test]
fn dec_modes_set_reset_and_report() {
    type Get = fn(&Terminal) -> bool;
    let modes: [(u16, Get); 13] = [
        (1, |t| t.input_modes().cursor_keys_application),
        (5, |t| t.reverse_video()),
        (7, |t| t.autowrap()),
        (12, |t| t.cursor_blink()),
        (25, |t| t.cursor_visible()),
        (45, |t| t.reverse_wrap()),
        (69, |t| t.left_right_margin_mode()),
        (1004, |t| t.input_modes().focus_events),
        (1007, |t| t.input_modes().alternate_scroll),
        (1036, |t| t.input_modes().meta_sends_escape),
        (2004, |t| t.input_modes().bracketed_paste),
        (2026, |t| t.synchronized_output()),
        (2027, |t| t.widths().mode == Mode::Unicode),
    ];
    for (mode, get) in modes {
        let mut term = Term::new(4, 2);
        term.feed(format!("\x1b[?{mode}h\x1b[?{mode}$p"));
        assert!(get(&term.terminal), "{mode} set");
        assert_eq!(replies(&mut term), format!("\x1b[?{mode};1$y"));
        term.feed(format!("\x1b[?{mode}l\x1b[?{mode}$p"));
        assert!(!get(&term.terminal), "{mode} reset");
        assert_eq!(replies(&mut term), format!("\x1b[?{mode};2$y"));
    }
    let mut term = Term::new(4, 2);
    term.feed("\x1b[?1;2004h");
    assert!(term.terminal.input_modes().cursor_keys_application);
    assert!(term.terminal.input_modes().bracketed_paste);
    term.feed("\x1b[?9999$p");
    assert_eq!(replies(&mut term), "\x1b[?9999;0$y");
    assert!(
        term.terminal.cursor_visible(),
        "the cursor shows by default"
    );
}

#[test]
fn the_mouse_modes_choose_one_tracking_and_one_encoding() {
    let mut term = Term::new(4, 2);
    let modes = |term: &Term| {
        let input = term.terminal.input_modes();
        (input.mouse_tracking, input.mouse_encoding)
    };
    for (mode, tracking) in [
        (9, MouseTracking::X10),
        (1000, MouseTracking::Normal),
        (1002, MouseTracking::ButtonEvent),
        (1003, MouseTracking::AnyEvent),
    ] {
        term.feed(format!("\x1b[?{mode}h"));
        assert_eq!(modes(&term).0, tracking, "{mode}");
    }
    term.feed("\x1b[?1000$p\x1b[?1003$p");
    assert_eq!(replies(&mut term), "\x1b[?1000;2$y\x1b[?1003;1$y");
    // Resetting any of them turns tracking off, as in xterm.
    term.feed("\x1b[?1000l");
    assert_eq!(modes(&term).0, MouseTracking::None);
    term.feed("\x1b[?1006h");
    assert_eq!(modes(&term).1, MouseEncoding::Sgr);
    term.feed("\x1b[?1016h");
    assert_eq!(modes(&term).1, MouseEncoding::SgrPixels);
    term.feed("\x1b[?1016l");
    assert_eq!(modes(&term).1, MouseEncoding::X10);
}

#[test]
fn decom_and_decawm_are_modes_6_and_7() {
    let mut term = with_margins();
    term.feed("\x1b[3;3H\x1b[?6h");
    assert!(term.terminal.origin_mode());
    assert_eq!(term.cursor(), (2, 1, false), "home is the top left margin");
    term.feed("\x1b[?6$p\x1b[?6l\x1b[?6$p");
    assert_eq!(replies(&mut term), "\x1b[?6;1$y\x1b[?6;2$y");
    term.feed("\x1b[?7l");
    assert!(!term.terminal.autowrap());
}

/// The rows of the screen that is not active.
fn other_screen(term: &Term) -> Vec<String> {
    let screen = if term.terminal.is_alternate_screen() {
        &term.terminal.primary
    } else {
        &term.terminal.alternate
    };
    (0..term.terminal.rows())
        .map(|y| {
            let (page, row) = screen.rows.screen_row(y);
            page.cells(row)
                .iter()
                .map(|cell| match cell.content() {
                    Content::Char('\0') => ' ',
                    Content::Char(c) => c,
                    _ => '?',
                })
                .collect::<String>()
                .trim_end()
                .to_owned()
        })
        .collect()
}

#[test]
fn mode_47_switches_screens_and_the_cursor_goes_along() {
    let mut term = Term::new(6, 3);
    term.feed("main\x1b[2;3H\x1b[?47h");
    assert!(term.terminal.is_alternate_screen());
    assert_eq!(term.cursor(), (2, 1, false));
    assert_eq!(term.screen(), ["", "", ""]);
    term.feed("alt\x1b[?47$p\x1b[?47l");
    assert_eq!(replies(&mut term), "\x1b[?47;1$y");
    assert_eq!(term.screen(), ["main", "", ""]);
    assert_eq!(
        term.cursor(),
        (5, 1, false),
        "where it was on the other screen"
    );
    term.feed("\x1b[?47h");
    assert_eq!(term.screen(), ["", "  alt", ""], "47 erases nothing");
}

#[test]
fn mode_1047_erases_the_alternate_screen_on_the_way_out() {
    let mut term = Term::new(6, 3);
    term.feed("\x1b[?1047halt\x1b[?1047l");
    assert!(!term.terminal.is_alternate_screen());
    assert_eq!(other_screen(&term), ["", "", ""]);
}

#[test]
fn mode_1049_saves_the_cursor_and_erases_the_alternate_screen() {
    let mut term = Term::new(6, 3);
    term.feed("\x1b[?47hold\x1b[?47l\x1b[H");
    term.terminal.set_pen(red());
    term.feed("main\x1b[2;2H\x1b[?1049h");
    assert!(term.terminal.is_alternate_screen());
    assert_eq!(
        term.screen(),
        ["", "", ""],
        "the alternate screen is erased"
    );
    assert_eq!(term.cursor(), (1, 1, false));
    term.terminal.set_pen(Style::DEFAULT);
    term.feed("\x1b[3;3Hx\x1b[?1049l");
    assert!(!term.terminal.is_alternate_screen());
    assert_eq!(term.cursor(), (1, 1, false), "the saved cursor is back");
    assert_eq!(term.terminal.pen(), &red(), "with its pen");
    assert_eq!(term.screen()[0], "main");
}

#[test]
fn the_alternate_screen_saves_a_cursor_of_its_own() {
    let mut term = Term::new(6, 3);
    term.feed("\x1b[1;2H\x1b7\x1b[?47h\x1b[3;3H\x1b7\x1b[1;1H\x1b8");
    assert_eq!(term.cursor(), (2, 2, false), "the alternate screen's own");
    term.feed("\x1b[?47l\x1b8");
    assert_eq!(term.cursor(), (1, 0, false), "the primary screen's");
}

#[test]
fn mode_1048_saves_and_restores_the_cursor() {
    let mut term = Term::new(6, 3);
    term.feed("\x1b[?1048$p\x1b[2;4H\x1b[?1048h\x1b[H\x1b[?1048l\x1b[?1048$p");
    assert_eq!(term.cursor(), (3, 1, false));
    assert_eq!(replies(&mut term), "\x1b[?1048;2$y\x1b[?1048;1$y");
}

#[test]
fn mode_2027_switches_between_clusters_and_codepoints() {
    let mut term = Term::new(6, 1);
    term.feed("\x1b[?2027l\u{1F1EE}\u{1F1F9}");
    assert_eq!(
        term.cursor(),
        (2, 0, false),
        "a cell per regional indicator"
    );
    term.feed("\x1b[?2027h\r\u{1F1EE}\u{1F1F9}");
    assert_eq!(term.cursor(), (2, 0, false), "one wide cluster");
    assert_eq!(term.cell(0, 0).width(), Width::Wide);
    // A switch ends the cluster: half a flag on either side stays apart.
    term.feed("\r\x1b[K\u{1F1EE}\x1b[?2027l\x1b[?2027h\u{1F1F9}");
    assert_eq!(term.cell(0, 0).width(), Width::Narrow);
    assert_eq!(term.cursor(), (2, 0, false));
    // RIS brings back the mode the terminal was made with.
    let options = Options {
        widths: Widths {
            mode: Mode::Legacy,
            ..Widths::default()
        },
        ..Options::default()
    };
    let mut term = Term::with(6, 1, options);
    term.feed("\x1b[?2027h\x1bc");
    assert_eq!(term.terminal.widths().mode, Mode::Legacy);
}

#[test]
fn reverse_wraparound_moves_left_into_the_row_that_wraps() {
    let mut term = Term::new(5, 3);
    term.feed("abcdefg\x1b[?45h");
    assert_eq!(term.cursor(), (2, 1, false));
    term.feed("\x08\x08\x08");
    assert_eq!(term.cursor(), (4, 0, false));
    term.feed("\x1b[9D");
    assert_eq!(term.cursor(), (0, 0, false), "the top stops it");
    // Not into a row that does not wrap.
    term.feed("\x1b[3;3H\x1b[9D");
    assert_eq!(term.cursor(), (0, 2, false));
    // A pending wrap takes one of the moves.
    let mut term = Term::new(5, 2);
    term.feed("\x1b[?45habcde\x08");
    assert_eq!(term.cursor(), (4, 0, false));
    // Without autowrap, or without the mode, the row's start stops the cursor.
    let mut term = Term::new(5, 3);
    term.feed("abcdefg\x1b[?45h\x1b[?7l\x1b[9D");
    assert_eq!(term.cursor(), (0, 1, false));
    let mut term = Term::new(5, 3);
    term.feed("abcdefg\x1b[9D");
    assert_eq!(term.cursor(), (0, 1, false));
}

#[test]
fn the_kitty_keyboard_flags_are_pushed_popped_set_and_reported() {
    let mut term = Term::new(10, 2);
    let mut report = |sequence: &str| {
        term.feed(sequence).feed("\x1b[?u");
        replies(&mut term)
    };
    assert_eq!(report(""), "\x1b[?0u");
    assert_eq!(report("\x1b[>1u"), "\x1b[?1u");
    assert_eq!(report("\x1b[=10;2u"), "\x1b[?11u", "adds flags");
    assert_eq!(report("\x1b[=3;3u"), "\x1b[?8u", "removes flags");
    assert_eq!(report("\x1b[=5u"), "\x1b[?5u", "replaces them");
    assert_eq!(report("\x1b[=31;9u"), "\x1b[?5u", "an unknown mode");
    assert_eq!(report("\x1b[>255u"), "\x1b[?31u", "only the defined flags");
    assert_eq!(report("\x1b[>u"), "\x1b[?0u");
    assert_eq!(report("\x1b[<2u"), "\x1b[?5u");
    assert_eq!(report("\x1b[<0u"), "\x1b[?0u", "pops one");
    assert_eq!(report("\x1b[>3u\x1b[<9u"), "\x1b[?0u");
    term.feed("\x1b[>9u");
    assert_eq!(term.terminal.keyboard_flags(), KeyboardFlags::from_bits(9));
    term.feed("\x1b[2;3H\x1b7\x1b[H\x1b[u");
    assert_eq!(term.cursor(), (2, 1, false), "CSI u alone is still SCORC");
}

#[test]
fn each_screen_keeps_its_own_keyboard_flags() {
    let mut term = Term::new(10, 2);
    let flags = |term: &Term| term.terminal.keyboard_flags().bits();
    term.feed("\x1b[>1u\x1b[?1049h");
    assert_eq!(flags(&term), 0);
    term.feed("\x1b[>8u\x1b[>4u\x1b[?1049l");
    assert_eq!(flags(&term), 1);
    term.feed("\x1b[<u\x1b[?47h");
    assert_eq!(flags(&term), 4, "the alternate screen kept its stack");
    term.feed("\x1b[<u");
    assert_eq!(flags(&term), 8);
    term.feed("\x1bc");
    assert_eq!(flags(&term), 0);
    term.feed("\x1b[?47h");
    assert_eq!(flags(&term), 0, "RIS resets both stacks");
}

#[test]
fn synchronized_output_starts_ends_and_reports() {
    let mut term = Term::new(10, 2);
    let state = |term: &mut Term| {
        term.feed("\x1b[?2026$p");
        let terminal = &term.terminal;
        let state = (
            terminal.synchronized_output(),
            terminal.synchronized_updates(),
        );
        (state, replies(term))
    };
    assert_eq!(state(&mut term), ((false, 0), "\x1b[?2026;2$y".to_owned()));
    term.feed("\x1b[?2026h");
    assert_eq!(state(&mut term), ((true, 1), "\x1b[?2026;1$y".to_owned()));
    term.feed("\x1b[?2026h");
    assert_eq!(state(&mut term).0, (true, 2), "each start counts");
    term.feed("\x1b[?2026l");
    assert_eq!(state(&mut term), ((false, 2), "\x1b[?2026;2$y".to_owned()));
    term.feed("\x1b[?2026h");
    term.terminal.end_synchronized_output();
    assert_eq!(
        state(&mut term),
        ((false, 3), "\x1b[?2026;2$y".to_owned()),
        "the deadline passed"
    );
    term.feed("\x1b[?2026h\x1bc");
    assert_eq!(
        state(&mut term).0,
        (false, 4),
        "RIS ends it, and keeps count"
    );
}

/// Pins column `x` of row `y` of the primary screen.
fn pin(term: &mut Term, x: u16, y: u16) -> PinId {
    let rows = &mut term.terminal.primary.rows;
    let (page, row) = rows.screen_row_id(y);
    rows.pin(Position { page, row, col: x }).unwrap()
}

/// The character that `pin` is on, if the pin is not lost.
fn pinned_char(term: &Term, pin: PinId) -> Option<char> {
    let rows = &term.terminal.primary.rows;
    let position = rows.pin_position(pin)?;
    let page = rows.page(position.page)?;
    match page.cells(position.row)[usize::from(position.col)].content() {
        Content::Char(c) => Some(c),
        _ => None,
    }
}

#[test]
fn pins_follow_the_text_that_editing_moves() {
    let mut term = Term::new(10, 6);
    term.feed("a\r\nb\r\nc\r\nd\r\ne\r\nf");
    let [a, b, c, d, _, f] = [0, 1, 2, 3, 4, 5].map(|y| pin(&mut term, 0, y));
    let chars = |term: &Term, pins: &[PinId]| -> Vec<_> {
        pins.iter().map(|&pin| pinned_char(term, pin)).collect()
    };
    // A scroll region scrolls up: b leaves it, and c and d go up.
    term.feed("\x1b[2;5r\x1b[5H\n");
    assert_eq!(
        chars(&term, &[a, b, c, d]),
        [Some('a'), None, Some('c'), Some('d')]
    );
    let rows = &term.terminal.primary.rows;
    assert_eq!(
        rows.pin_position(c).map(|p| (p.page, p.row)),
        Some(rows.screen_row_id(1))
    );
    // IL pushes the last row out, and DL takes a out.
    term.feed("\x1b[r\x1b[H\x1b[L\x1b[2M");
    assert_eq!(
        chars(&term, &[a, c, d, f]),
        [None, Some('c'), Some('d'), None]
    );
    // Between the side margins, only the cells there move.
    term.feed("\x1b[3Hklmno\x1b[6Hfghij");
    let [m, g, h, i] = [(2, 2), (1, 5), (2, 5), (3, 5)].map(|(x, y)| pin(&mut term, x, y));
    term.feed("\x1b[?69h\x1b[3;4s\x1b[3;6r\x1b[6;3H\n");
    assert_eq!(term.screen()[4..], ["  hi", "fg  j"]);
    assert_eq!(
        chars(&term, &[m, g, h, i]),
        [None, Some('g'), Some('h'), Some('i')]
    );
    let rows = &term.terminal.primary.rows;
    let (page, row) = rows.screen_row_id(4);
    assert_eq!(rows.pin_position(h), Some(Position { page, row, col: 2 }));
    // ED 3 drops the scrollback, and the pins in it.
    term.feed("\x1b[?69l\x1b[r\x1b[6H\n\n\n\n\n\n");
    assert_eq!(
        chars(&term, &[c, g]),
        [Some('c'), Some('g')],
        "in the scrollback"
    );
    term.feed("\x1b[3J");
    assert_eq!(chars(&term, &[c, d, g, h, i]), [None; 5]);
    assert_eq!(term.terminal.primary.rows.pins(), 0);
}

#[test]
fn replies_wait_until_they_are_taken() {
    let mut term = Term::new(4, 1);
    term.feed("\x1b[?25$p");
    term.feed("\x1bc");
    assert_eq!(replies(&mut term), "\x1b[?25;1$y", "RIS keeps them");
    assert_eq!(replies(&mut term), "");
}

#[test]
fn device_attributes_reply() {
    let mut term = Term::new(10, 3);
    term.feed("\x1b[c\x1b[0c\x1b[>c\x1b[=c\x1b[1c\x1b[>1c");
    assert_eq!(
        replies(&mut term),
        "\x1b[?62;22c\x1b[?62;22c\x1b[>1;10;0c\x1bP!|00000000\x1b\\",
        "only the queries get replies"
    );
}

#[test]
fn dsr_reports_the_status_and_the_cursor() {
    let mut term = Term::new(10, 5);
    term.feed("\x1b[5n\x1b[3;4H\x1b[6n\x1b[1;10Hx\x1b[6n\x1b[7n");
    assert_eq!(replies(&mut term), "\x1b[0n\x1b[3;4R\x1b[1;10R");
    // In origin mode, from the top left of the margins.
    let mut term = with_margins();
    term.feed("\x1b[?6h\x1b[2;3H\x1b[6n");
    assert_eq!(replies(&mut term), "\x1b[2;3R");
    // DECRC brings back a cursor outside margins set after DECSC.
    let mut term = Term::new(10, 5);
    term.feed("\x1b[?6h\x1b[?69h\x1b7\x1b[2;4r\x1b[3;5s\x1b8\x1b[6n");
    assert_eq!(replies(&mut term), "\x1b[1;1R");
    assert_eq!(term.cursor(), (0, 0, false));
}

#[test]
fn xtversion_reports_the_name_and_version() {
    let mut term = Term::new(10, 1);
    term.feed("\x1b[>q\x1b[>1q");
    let version = env!("CARGO_PKG_VERSION");
    assert_eq!(replies(&mut term), format!("\x1bP>|saetta {version}\x1b\\"));
}

#[test]
fn xtwinops_reports_the_sizes_it_knows() {
    let mut term = Term::new(80, 24);
    term.feed("\x1b[18t\x1b[14t\x1b[16t");
    assert_eq!(
        replies(&mut term),
        "\x1b[8;24;80t",
        "the pixels are unknown yet"
    );
    term.terminal.set_cell_size(8, 16);
    term.feed("\x1b[14t\x1b[16t\x1b[3;10;10t");
    assert_eq!(replies(&mut term), "\x1b[4;384;640t\x1b[6;16;8t");
}

#[test]
fn the_title_is_reported_only_when_the_configuration_allows_it() {
    let mut term = Term::new(10, 1);
    term.terminal.title = "shell\x1b]evil".to_owned();
    term.feed("\x1b[21t\x1b[20t");
    assert_eq!(replies(&mut term), "");
    let options = Options {
        title_reports: true,
        ..Options::default()
    };
    let mut term = Term::with(10, 1, options);
    term.terminal.title = "shell\x1b]evil\u{9B}".to_owned();
    term.terminal.icon_title = "icon".to_owned();
    term.feed("\x1b[21t\x1b[20t");
    assert_eq!(
        replies(&mut term),
        "\x1b]lshell]evil\x1b\\\x1b]Licon\x1b\\",
        "without its controls"
    );
}

#[test]
fn the_title_stack_pushes_and_pops() {
    let mut term = Term::new(10, 1);
    let set = |term: &mut Term, title: &str, icon: &str| {
        term.terminal.title = title.to_owned();
        term.terminal.icon_title = icon.to_owned();
    };
    let titles = |term: &Term| {
        (
            term.terminal.title().to_owned(),
            term.terminal.icon_title().to_owned(),
        )
    };
    set(&mut term, "one", "1");
    term.feed("\x1b[22;0t");
    set(&mut term, "two", "2");
    term.feed("\x1b[22t");
    set(&mut term, "three", "3");
    term.feed("\x1b[23;2t");
    assert_eq!(
        titles(&term),
        ("two".into(), "3".into()),
        "the window's only"
    );
    term.feed("\x1b[23;1t");
    assert_eq!(titles(&term), ("two".into(), "1".into()), "the icon's only");
    term.feed("\x1b[23t");
    assert_eq!(titles(&term), ("two".into(), "1".into()), "an empty stack");
    for n in 0..12 {
        set(&mut term, &n.to_string(), "");
        term.feed("\x1b[22t");
    }
    for _ in 0..12 {
        term.feed("\x1b[23t");
    }
    assert_eq!(titles(&term).0, "2", "ten titles at most");
}

#[test]
fn the_title_stack_saves_and_brings_back_the_titles_asked_for() {
    let mut term = Term::new(10, 1);
    let titles = |term: &Term| {
        (
            term.terminal.title().to_owned(),
            term.terminal.icon_title().to_owned(),
        )
    };
    // The window's title alone is saved, so the icon's stays.
    term.feed("\x1b]1;icon-old\x07\x1b]2;win-old\x07\x1b[22;2t");
    term.feed("\x1b]1;icon-new\x07\x1b]2;win-new\x07");
    term.terminal.take_events();
    term.feed("\x1b[23;0t");
    assert_eq!(titles(&term), ("win-old".into(), "icon-new".into()));
    assert_eq!(term.terminal.take_events(), [Event::TitleChanged]);
    // Nothing to bring back: no event.
    term.feed("\x1b[22;1t\x1b[23;2t\x1b[22;7t\x1b[23t");
    assert_eq!(term.terminal.take_events(), []);
    assert_eq!(titles(&term), ("win-old".into(), "icon-new".into()));
    // As vim does: the shell's title comes back, and the embedder hears.
    term.feed("\x1b]2;shell\x07\x1b[22;0t\x1b]2;vim\x07");
    term.terminal.take_events();
    term.feed("\x1b[23;0t");
    assert_eq!(term.terminal.title(), "shell");
    assert_eq!(term.terminal.take_events(), [Event::TitleChanged]);
}

#[test]
fn nel_indexes_before_it_returns_the_carriage() {
    // Right of the right margin, at the bottom margin, IND does nothing,
    // so NEL does not scroll either.
    let mut term = Term::new(6, 3);
    term.feed("aaa\r\nbbb\r\nccc\x1b[?69h\x1b[1;3s\x1b[3;5H\x1bE");
    assert_eq!(term.screen(), ["aaa", "bbb", "ccc"]);
    assert_eq!(term.cursor(), (0, 2, false));
    // Within the margins, it scrolls.
    term.feed("\x1b[3;2H\x1bE");
    assert_eq!(term.screen(), ["bbb", "ccc", ""]);
}

#[test]
fn decrqss_reports_settings_as_the_sequences_that_set_them() {
    let mut term = Term::new(10, 5);
    term.feed("\x1b[1;4:3;38;5;196;48;2;1;2;3;58:5:9m\x1bP$qm\x1b\\");
    assert_eq!(
        replies(&mut term),
        "\x1bP1$r0;1;4:3;38;5;196;48;2;1;2;3;58:5:9m\x1b\\"
    );
    term.feed("\x1b[0;3;31;102m\x1bP$qm\x1b\\\x1b[m\x1bP$qm\x1b\\");
    assert_eq!(
        replies(&mut term),
        "\x1bP1$r0;3;31;102m\x1b\\\x1bP1$r0m\x1b\\"
    );
    term.feed("\x1b[2;4r\x1b[?69h\x1b[3;6s\x1b[4 q\x1b[1\"q");
    term.feed("\x1bP$qr\x1b\\\x1bP$qs\x1b\\\x1bP$q q\x1b\\\x1bP$q\"q\x1b\\");
    assert_eq!(
        replies(&mut term),
        "\x1bP1$r2;4r\x1b\\\x1bP1$r3;6s\x1b\\\x1bP1$r4 q\x1b\\\x1bP1$r1\"q\x1b\\"
    );
    // A query split across reads still gets its reply.
    term.feed("\x1bP$");
    term.feed("q\"");
    term.feed("q\x1b\\");
    assert_eq!(replies(&mut term), "\x1bP1$r1\"q\x1b\\");
}

#[test]
fn decrqss_does_not_echo_what_it_does_not_know() {
    let mut term = Term::new(10, 1);
    term.feed("\x1bP$qevil\x1b\\");
    assert_eq!(replies(&mut term), "\x1bP0$r\x1b\\");
    // Too long, cancelled, or another DCS: no reply.
    term.feed(format!("\x1bP$q{}\x1b\\", "m".repeat(2000)));
    term.feed("\x1bP$qm\x18\x1bPqm\x1b\\");
    assert_eq!(replies(&mut term), "");
}

#[test]
fn xtgettcap_reports_capabilities_by_name() {
    let options = Options {
        term: "xterm-saetta".to_owned(),
        ..Options::default()
    };
    let mut term = Term::with(10, 1, options);
    term.feed("\x1bP+q544E;436F;524742;5858\x1b\\");
    let hex = |text: &str| -> String { text.bytes().map(|byte| format!("{byte:02X}")).collect() };
    assert_eq!(
        replies(&mut term),
        format!(
            "\x1bP1+r544E={}\x1b\\\x1bP1+r436F={}\x1b\\\x1bP1+r524742\x1b\\\x1bP0+r5858\x1b\\",
            hex("xterm-saetta"),
            hex("256")
        )
    );
    // Names must be hexadecimal; the first that is not ends the reply.
    term.feed("\x1bP+q436F;zz;544E\x1b\\\x1bP+q5\x1b\\\x1bP+q+5\x1b\\");
    assert_eq!(
        replies(&mut term),
        format!("\x1bP1+r436F={}\x1b\\", hex("256"))
    );
    assert_eq!(Options::default().term, "xterm-256color");
}

#[test]
fn ris_keeps_what_the_embedder_configured() {
    let options = Options {
        term: "xterm-saetta".to_owned(),
        title_reports: true,
        ..Options::default()
    };
    let mut term = Term::with(10, 2, options);
    term.terminal.set_cell_size(8, 16);
    term.terminal.title = "old".to_owned();
    term.feed("\x1b[22t\x1bc\x1b[16t\x1b[21t\x1bP+q544E\x1b\\\x1b[23t");
    let reply = replies(&mut term);
    assert!(reply.starts_with("\x1b[6;16;8t\x1b]l\x1b\\"), "{reply:?}");
    assert!(reply.ends_with("=787465726D2D736165747461\x1b\\"));
    assert_eq!(term.terminal.title(), "", "RIS dropped the title stack too");
}

#[test]
fn osc_0_1_and_2_set_the_titles() {
    let mut term = Term::new(10, 1);
    term.feed("\x1b]0;both\x07");
    assert_eq!(
        (term.terminal.title(), term.terminal.icon_title()),
        ("both", "both")
    );
    term.feed("\x1b]2;window\x1b\\\x1b]1;icon\x07");
    assert_eq!(
        (term.terminal.title(), term.terminal.icon_title()),
        ("window", "icon")
    );
    assert_eq!(term.terminal.take_events(), vec![Event::TitleChanged; 3]);
    // UTF-8, without its controls, and split across reads.
    term.feed("\x1b]2;caf\u{E9} \u{9B}x");
    term.feed("y\x1b\\");
    assert_eq!(term.terminal.title(), "caf\u{E9} xy");
    // A string cancelled or too long changes nothing.
    term.feed("\x1b]2;gone\x18");
    term.feed(format!("\x1b]2;{}\x07", "x".repeat(70_000)));
    assert_eq!(term.terminal.title(), "caf\u{E9} xy");
    term.terminal.take_events();
    term.feed("\x1b]2;\x07");
    assert_eq!(term.terminal.title(), "", "an empty title");
    assert_eq!(term.terminal.take_events(), [Event::TitleChanged]);
}

#[test]
fn osc_7_gives_the_working_directory() {
    let mut term = Term::new(10, 1);
    term.feed("\x1b]7;file://mac.local/Users/me/My%20Files\x07");
    let expected = WorkingDirectory {
        host: "mac.local".to_owned(),
        path: "/Users/me/My Files".to_owned(),
    };
    assert_eq!(term.terminal.working_directory(), Some(&expected));
    assert_eq!(
        term.terminal.take_events(),
        [Event::WorkingDirectory(expected)]
    );
    term.feed("\x1b]7;file:///tmp/100%\x07");
    assert_eq!(term.terminal.working_directory().unwrap().host, "");
    assert_eq!(
        term.terminal.working_directory().unwrap().path,
        "/tmp/100%",
        "a lone %"
    );
    // Only file URLs, with a path.
    term.terminal.take_events();
    term.feed("\x1b]7;http://example.com/x\x07\x1b]7;file://host\x07");
    assert_eq!(term.terminal.take_events(), []);
    assert_eq!(term.terminal.working_directory().unwrap().path, "/tmp/100%");
    // A pair with a sign is not hexadecimal.
    term.feed("\x1b]7;file:///tmp/%+41\x07");
    assert_eq!(term.terminal.working_directory().unwrap().path, "/tmp/%+41");
    // RIS keeps the directory, which is the shell's.
    term.feed("\x1bc");
    assert_eq!(term.terminal.working_directory().unwrap().path, "/tmp/%+41");
}

#[test]
fn osc_9_and_777_send_notifications() {
    let mut term = Term::new(10, 1);
    term.feed("\x1b]9;Build done\x07\x1b]777;notify;Make;all; done\x1b\\");
    term.feed("\x1b]777;other;x\x07\x1b]9;a\x1bb\x07");
    assert_eq!(
        term.terminal.take_events(),
        [
            Event::Notification {
                title: String::new(),
                body: "Build done".to_owned()
            },
            Event::Notification {
                title: "Make".to_owned(),
                body: "all; done".to_owned()
            },
            Event::Notification {
                title: String::new(),
                body: "a".to_owned()
            },
        ],
        "777 knows only notify, and an ESC ends the string"
    );
}

#[test]
fn osc_9_4_reports_progress() {
    let mut term = Term::new(10, 1);
    term.feed("\x1b]9;4;1;42\x07\x1b]9;4;1;150\x07\x1b]9;4;2\x07\x1b]9;4;2;7\x07");
    term.feed("\x1b]9;4;3\x07\x1b]9;4;4;9\x07\x1b]9;4;0\x07\x1b]9;4\x07\x1b]9;4;5\x07");
    let progress: Vec<Event> = [
        Progress::Set(42),
        Progress::Set(100),
        Progress::Error(None),
        Progress::Error(Some(7)),
        Progress::Indeterminate,
        Progress::Paused(Some(9)),
        Progress::Remove,
        Progress::Remove,
    ]
    .into_iter()
    .map(Event::Progress)
    .collect();
    assert_eq!(term.terminal.take_events(), progress, "5 is no state");
    term.feed("\x1b]9;42\x07");
    assert!(matches!(
        &term.terminal.take_events()[..],
        [Event::Notification { .. }]
    ));
}

#[test]
fn osc_6_1_sets_the_tab_color_a_component_at_a_time() {
    let mut term = Term::new(10, 1);
    term.feed("\x1b]6;1;bg;red;brightness;255\x07\x1b]6;1;bg;blue;brightness;16\x07");
    assert_eq!(term.terminal.tab_color(), Some(Rgb::new(255, 0, 16)));
    term.feed("\x1b]6;1;bg;pink;brightness;1\x07\x1b]6;1;bg;red;brightness;300\x07");
    assert_eq!(
        term.terminal.tab_color(),
        Some(Rgb::new(255, 0, 16)),
        "ignored"
    );
    term.feed("\x1b]6;1;bg;*;default\x07");
    assert_eq!(term.terminal.tab_color(), None);
    assert_eq!(
        term.terminal.take_events(),
        [
            Event::TabColor(Some(Rgb::new(255, 0, 0))),
            Event::TabColor(Some(Rgb::new(255, 0, 16))),
            Event::TabColor(None),
        ]
    );
}

#[test]
fn ris_tells_the_embedder_what_it_dropped() {
    let mut term = Term::new(10, 1);
    term.feed("\x1b]2;t\x07\x1b]6;1;bg;red;brightness;1\x07\x1bc");
    assert_eq!(
        term.terminal.take_events(),
        [
            Event::TitleChanged,
            Event::TabColor(Some(Rgb::new(1, 0, 0))),
            Event::TitleChanged,
            Event::TabColor(None),
        ]
    );
    term.feed("\x1bc");
    assert_eq!(term.terminal.take_events(), [], "nothing left to drop");
}

#[test]
fn a_large_osc_string_does_not_keep_its_buffer() {
    let mut term = Term::new(10, 1);
    term.feed(format!("\x1b]52;c;{}\x07", "A".repeat(100_000)));
    assert!(term.terminal.osc.capacity() <= 4 * 1024);
    term.feed("\x1b]2;small\x07");
    assert!(term.terminal.osc.capacity() > 0, "a small one is kept");
}

#[test]
fn the_colors_start_as_xterm_s() {
    let colors = Colors::default();
    assert_eq!(colors.palette[1], Rgb::new(205, 0, 0));
    assert_eq!(colors.palette[12], Rgb::new(92, 92, 255));
    assert_eq!(colors.palette[16], Rgb::new(0, 0, 0));
    assert_eq!(colors.palette[196], Rgb::new(255, 0, 0));
    assert_eq!(colors.palette[231], Rgb::new(255, 255, 255));
    assert_eq!(colors.palette[232], Rgb::new(8, 8, 8));
    assert_eq!(colors.palette[255], Rgb::new(238, 238, 238));
}

#[test]
fn color_specs_follow_x11() {
    let color = |spec: &str| colors::parse_color(spec.as_bytes());
    assert_eq!(color("rgb:ff/80/00"), Some(Rgb::new(255, 128, 0)));
    assert_eq!(color("rgb:f/8/0"), Some(Rgb::new(255, 136, 0)), "scaled");
    assert_eq!(color("rgb:ffff/0000/8080"), Some(Rgb::new(255, 0, 128)));
    assert_eq!(
        color("#f80"),
        Some(Rgb::new(240, 128, 0)),
        "the high digits"
    );
    assert_eq!(color("#ff8000"), Some(Rgb::new(255, 128, 0)));
    assert_eq!(color("#fff888000"), Some(Rgb::new(255, 136, 0)));
    assert_eq!(color("#ffff88880000"), Some(Rgb::new(255, 136, 0)));
    for bad in [
        "red",
        "rgb:ff/80",
        "rgb:ff/80/00/00",
        "rgb:+f/0/0",
        "rgb:fffff/0/0",
        "#ff80",
        "#",
        "rgb://",
    ] {
        assert_eq!(color(bad), None, "{bad}");
    }
}

#[test]
fn osc_4_sets_and_reports_the_palette() {
    let mut term = Term::new(10, 1);
    term.feed("\x1b]4;1;rgb:12/34/56;300;#fff;2;red\x1b\\");
    assert_eq!(
        term.terminal.colors().palette[1],
        Rgb::new(0x12, 0x34, 0x56)
    );
    assert_eq!(
        term.terminal.colors().palette[2],
        Colors::default().palette[2],
        "a name"
    );
    assert_eq!(term.terminal.take_events(), [Event::ColorsChanged]);
    term.feed("\x1b]4;1;?;3;?\x07");
    assert_eq!(
        replies(&mut term),
        "\x1b]4;1;rgb:1212/3434/5656\x07\x1b]4;3;rgb:cdcd/cdcd/0000\x07",
        "each with the query's end"
    );
    term.feed("\x1b]4;5;#000\x07\x1b]104;1\x07");
    assert_eq!(
        term.terminal.colors().palette[1],
        Colors::default().palette[1]
    );
    assert_eq!(term.terminal.colors().palette[5], Rgb::new(0, 0, 0));
    term.feed("\x1b]104\x07");
    assert_eq!(term.terminal.colors(), &Colors::default());
}

#[test]
fn osc_10_to_12_set_and_report_the_dynamic_colors() {
    let mut term = Term::new(10, 1);
    term.feed("\x1b]10;#102030;#405060\x07\x1b]12;rgb:ff/00/00\x07");
    let colors = term.terminal.colors().clone();
    assert_eq!(
        (colors.foreground, colors.background, colors.cursor),
        (
            Rgb::new(16, 32, 48),
            Rgb::new(64, 80, 96),
            Rgb::new(255, 0, 0)
        ),
        "10 then 11"
    );
    term.feed("\x1b]11;?\x1b\\\x1b]10;?;?\x07");
    assert_eq!(
        replies(&mut term),
        "\x1b]11;rgb:4040/5050/6060\x1b\\\x1b]10;rgb:1010/2020/3030\x07\x1b]11;rgb:4040/5050/6060\x07"
    );
    term.feed("\x1b]110\x07\x1b]111\x07\x1b]112\x07");
    assert_eq!(term.terminal.colors(), &Colors::default());
}

#[test]
fn the_resets_bring_back_the_theme_and_so_does_ris() {
    let mut term = Term::new(10, 1);
    let mut theme = Colors::default();
    theme.palette[1] = Rgb::new(1, 1, 1);
    theme.foreground = Rgb::new(2, 2, 2);
    theme.background = Rgb::new(3, 3, 3);
    theme.cursor = Rgb::new(4, 4, 4);
    term.terminal.set_default_colors(theme.clone());
    assert_eq!(term.terminal.colors(), &theme);
    term.feed("\x1b]4;1;#fff\x07\x1b]10;#fff;#fff;#fff\x07");
    term.feed("\x1b]104;1\x07\x1b]110\x07\x1b]111\x07\x1b]112\x07");
    assert_eq!(term.terminal.colors(), &theme);
    term.feed("\x1b]4;1;#fff\x07");
    term.terminal.take_events();
    term.feed("\x1bc");
    assert_eq!(term.terminal.colors(), &theme);
    assert_eq!(term.terminal.take_events(), [Event::ColorsChanged]);
}

#[test]
fn osc_52_writes_the_clipboards() {
    let mut term = Term::new(10, 1);
    term.feed("\x1b]52;c;aGVsbG8=\x07\x1b]52;;d29ybGQ\x07\x1b]52;ps;eA==\x07\x1b]52;c;\x07");
    term.feed("\x1b]52;0;eA==\x07\x1b]52;c;!!\x07\x1b]52;c\x07");
    let clipboard = ClipboardTarget {
        clipboard: true,
        primary: false,
    };
    let primary = ClipboardTarget {
        clipboard: false,
        primary: true,
    };
    assert_eq!(
        term.terminal.take_events(),
        [
            Event::ClipboardWrite {
                target: clipboard,
                data: b"hello".to_vec()
            },
            Event::ClipboardWrite {
                target: clipboard,
                data: b"world".to_vec()
            },
            Event::ClipboardWrite {
                target: primary,
                data: b"x".to_vec()
            },
            Event::ClipboardWrite {
                target: clipboard,
                data: Vec::new()
            },
        ],
        "the cut buffers, bad base64 and no data at all are left out"
    );
}

#[test]
fn osc_52_reads_only_when_the_configuration_allows_them() {
    let mut term = Term::new(10, 1);
    term.feed("\x1b]52;c;?\x07");
    assert_eq!(term.terminal.take_events(), []);
    assert_eq!(replies(&mut term), "");
    let options = Options {
        clipboard_reads: true,
        ..Options::default()
    };
    let mut term = Term::with(10, 1, options);
    term.feed("\x1b]52;c;?\x07\x1b]52;p;?\x1b\\");
    let events = term.terminal.take_events();
    let [
        Event::ClipboardRead(clipboard),
        Event::ClipboardRead(primary),
    ] = events[..]
    else {
        panic!("two reads: {events:?}");
    };
    assert!(clipboard.target.clipboard && primary.target.primary);
    term.terminal.reply_clipboard(clipboard, b"hello");
    term.terminal.reply_clipboard(primary, b"");
    assert_eq!(
        replies(&mut term),
        "\x1b]52;c;aGVsbG8=\x07\x1b]52;p;\x1b\\",
        "each ends as its request did"
    );
}

/// The id and the URI of the link that the cell at column `x` of row `y`
/// is part of.
fn link_at(term: &Term, x: u16, y: u16) -> Option<(String, String)> {
    let (page, row) = term.terminal.screen_rows().screen_row(y);
    let link = page.hyperlinks().get(page.hyperlink(row, x)?);
    let text = |bytes: &[u8]| String::from_utf8(bytes.to_vec()).unwrap();
    Some((text(link.id), text(link.uri)))
}

fn link(id: &str, uri: &str) -> Option<(String, String)> {
    Some((id.to_owned(), uri.to_owned()))
}

#[test]
fn osc_8_links_the_text_printed_while_it_is_open() {
    let mut term = Term::new(10, 2);
    term.feed("a\x1b]8;;https://example.com\x1b\\bc\x1b]8;;\x1b\\d");
    assert_eq!(term.screen()[0], "abcd");
    let example = link("", "https://example.com");
    let links: Vec<_> = (0..5).map(|x| link_at(&term, x, 0)).collect();
    assert_eq!(links, [None, example.clone(), example, None, None]);
    assert_eq!(term.terminal.open_link(), None);
    let (page, row) = term.terminal.screen_rows().screen_row(0);
    let id = page.hyperlink(row, 1).unwrap();
    assert_eq!(page.hyperlinks().references(id), 2, "one for each cell");
    assert!(page.row(row).has(RowFlag::Hyperlinks));
    assert_tables_counted(&term.terminal.primary);
}

#[test]
fn osc_8_takes_the_id_from_its_parameters() {
    let mut term = Term::new(10, 2);
    term.feed("\x1b]8;a=1:id=one:b=2;http://x\x07a\x1b]8;id=one;http://y\x07b");
    term.feed("\x1b]8;id=one;http://x\x07c\x1b]8;;http://x\x07d");
    let links: Vec<_> = (0..4).map(|x| link_at(&term, x, 0)).collect();
    assert_eq!(
        links,
        [
            link("one", "http://x"),
            link("one", "http://y"),
            link("one", "http://x"),
            link("", "http://x"),
        ]
    );
    let (page, row) = term.terminal.screen_rows().screen_row(0);
    assert_eq!(
        page.hyperlink(row, 0),
        page.hyperlink(row, 2),
        "the same id and URI make the same link"
    );
    assert_eq!(term.terminal.open_link(), Some(&b"http://x"[..]));
    term.feed("\x1b]8\x07\x1b]8;id=two\x07");
    assert_eq!(
        term.terminal.open_link(),
        Some(&b"http://x"[..]),
        "no URI field"
    );
}

#[test]
fn osc_8_links_only_the_schemes_the_configuration_allows() {
    let mut term = Term::new(20, 2);
    for (uri, allowed) in [
        ("https://a", true),
        ("HTTP://a", true),
        ("mailto:a@b", true),
        ("file:///tmp", true),
        ("javascript:alert(1)", false),
        ("ssh://host", false),
        ("x-http://a", false),
        ("1http://a", false),
        (":a", false),
        ("https", false),
        ("ht\u{e9}tp://a", false),
    ] {
        term.feed(format!("\x1b]8;;https://open\x07\x1b]8;;{uri}\x07"));
        let open = term.terminal.open_link().map(<[u8]>::to_vec);
        assert_eq!(open, allowed.then(|| uri.as_bytes().to_vec()), "{uri}");
    }
    let options = Options {
        link_schemes: vec!["ssh".to_owned()],
        ..Options::default()
    };
    let mut term = Term::with(20, 2, options);
    term.feed("\x1b]8;;SSH://host\x07a\x1b]8;;https://a\x07b");
    assert_eq!(link_at(&term, 0, 0), link("", "SSH://host"));
    assert_eq!(link_at(&term, 1, 0), None);
    term.feed("\x1bc");
    assert_eq!(term.terminal.open_link(), None, "RIS closes the link");
    term.feed("\x1b]8;;ssh://host\x07");
    assert!(term.terminal.open_link().is_some(), "and keeps the schemes");
}

#[test]
fn osc_8_opens_no_link_with_control_characters() {
    let mut term = Term::new(10, 1);
    // DEL, CSI and NEL as C1 characters in UTF-8, and a byte that is not
    // UTF-8.
    for link in [
        &b"\x1b]8;;http://a\x7fb\x07"[..],
        b"\x1b]8;;http://a\xc2\x9bb\x07",
        b"\x1b]8;id=a\xc2\x85;http://a\x07",
        b"\x1b]8;;http://a\xffb\x07",
    ] {
        term.feed("\x1b]8;;http://open\x07").feed(link);
        assert_eq!(term.terminal.open_link(), None, "{link:?}");
    }
    term.feed("\x1b]8;;http://\u{e9}t\u{e9}.example\x07");
    assert!(term.terminal.open_link().is_some(), "letters beyond ASCII");
}

#[test]
fn osc_8_drops_links_longer_than_the_specification_allows() {
    let mut term = Term::new(10, 2);
    let uri = |len: usize| format!("http://{}", "a".repeat(len - 7));
    for (id, uri, kept) in [
        (String::new(), uri(2083), true),
        (String::new(), uri(2084), false),
        ("i".repeat(250), uri(10), true),
        ("i".repeat(251), uri(10), false),
        ("i".repeat(250), uri(2083), true),
    ] {
        term.feed(format!("\r\x1b]8;id={id};{uri}\x07x"));
        let expected = kept.then(|| (id.clone(), uri.clone()));
        assert_eq!(
            link_at(&term, 0, 0),
            expected,
            "{} and {}",
            id.len(),
            uri.len()
        );
    }
}

#[test]
fn links_grow_the_page_when_its_table_is_full() {
    let mut term = Term::new(80, 2);
    let uri = |n: usize| format!("http://{n}/{}", "a".repeat(1000));
    for n in 0..80 {
        term.feed(format!("\x1b]8;;{}\x07x", uri(n)));
    }
    for n in 0..80_u16 {
        let uri = uri(usize::from(n));
        assert_eq!(link_at(&term, n, 0), link("", &uri), "{n}");
    }
    assert_tables_counted(&term.terminal.primary);
}

#[test]
fn both_halves_of_a_wide_character_are_part_of_its_link() {
    let mut term = Term::new(10, 2);
    term.feed("\x1b]8;;http://a\x07\u{4E2D}\u{2764}\u{FE0F}\x1b]8;;\x07");
    for x in 0..4 {
        assert_eq!(link_at(&term, x, 0), link("", "http://a"), "{x}");
    }
    assert_eq!(
        term.cell(3, 0).width(),
        Width::SpacerTail,
        "the heart widened"
    );
    assert_eq!(link_at(&term, 4, 0), None);
    assert_tables_counted(&term.terminal.primary);
}

#[test]
fn writing_over_a_link_lets_it_go() {
    let mut term = Term::new(10, 2);
    term.feed("\x1b]8;;http://a\x07ab\u{4E2D}\x1b]8;;\x07\rxyz");
    term.feed("\x1b]8;;http://b\x07\u{2764}\x1b]8;;\x07\rw\x1b[3C\u{FE0F}");
    assert_eq!(term.screen()[0], "wyz\u{2764}\u{FE0F}");
    assert_eq!(link_at(&term, 3, 0), link("", "http://b"));
    assert_eq!(
        link_at(&term, 4, 0),
        link("", "http://b"),
        "the tail of the widened heart"
    );
    for x in [0, 1, 2, 5] {
        assert_eq!(link_at(&term, x, 0), None, "{x}");
    }
    assert_tables_counted(&term.terminal.primary);
    term.feed("\x1b[2K");
    let (page, _) = term.terminal.screen_rows().screen_row(0);
    assert!(page.hyperlinks().is_empty(), "erasing lets the links go");
}

#[test]
fn osc_133_marks_prompts_commands_and_their_output() {
    let mut term = Term::new(10, 3);
    term.feed("\x1b]133;A\x07$ \x1b]133;B\x07l\u{4E2D}\u{2764}\x1b]133;C\x07\u{FE0F}");
    term.feed("\r\nout\x1b]133;B\x07\x1b]133;D;0\x07!\r\n\x1b]133;A;aid=1\x07%");
    use Semantic::{Input, Output, Prompt};
    let zones: Vec<_> = (0..8).map(|x| term.cell(x, 0).semantic()).collect();
    assert_eq!(
        zones,
        [Prompt, Prompt, Input, Input, Input, Input, Input, Output],
        "the heart widened in its own zone, and an empty cell is output"
    );
    let zones: Vec<_> = (0..4).map(|x| term.cell(x, 1).semantic()).collect();
    assert_eq!(zones, [Output; 4]);
    assert_eq!(term.cell(0, 2).semantic(), Prompt);
    assert_eq!(term.terminal.semantic_zone(), Prompt);
    let starts: Vec<_> = (0..3)
        .map(|y| term.row(y).has(RowFlag::PromptStart))
        .collect();
    assert_eq!(starts, [true, false, true]);
    term.feed("\x1b]133;Z\x07\x1b]133\x07");
    assert_eq!(
        term.terminal.semantic_zone(),
        Prompt,
        "unknown marks change nothing"
    );
    term.feed("\x1bc");
    assert_eq!(term.terminal.semantic_zone(), Output);
}

/// The codepoints of the cluster at column `x` of row `y`.
fn cluster_at(term: &Term, x: u16, y: u16) -> Vec<char> {
    let (page, row) = term.terminal.screen_rows().screen_row(y);
    let (Content::Char(first) | Content::Grapheme(first)) =
        page.cells(row)[usize::from(x)].content()
    else {
        return Vec::new();
    };
    let mut cluster = vec![first];
    cluster.extend_from_slice(page.grapheme(row, x));
    cluster
}

#[test]
fn a_cluster_keeps_at_most_32_codepoints() {
    for normalization in [Normalization::None, Normalization::Nfc, Normalization::Nfd] {
        let options = Options {
            normalization,
            ..Options::default()
        };
        let mut term = Term::with(10, 2, options);
        term.feed(format!("a{}b", "\u{316}".repeat(100)));
        let cluster = cluster_at(&term, 0, 0);
        assert_eq!(cluster.len(), 32, "{normalization:?}");
        assert!(cluster[1..].iter().all(|&c| c == '\u{316}'));
        assert_eq!(
            term.screen()[0].chars().last(),
            Some('b'),
            "{normalization:?}"
        );
    }
    // A cluster far longer than a page could hold.
    let mut term = Term::new(10, 2);
    term.feed(format!("a{}", "\u{301}".repeat(70_000)));
    assert_eq!(cluster_at(&term, 0, 0).len(), 32);
}

/// Prints `count` links of the longest length allowed, each a cell of its
/// own, from the cursor.
fn print_long_links(term: &mut Term, count: usize, tag: &str) {
    let id = "i".repeat(250);
    for n in 0..count {
        let uri = format!("http://{tag}{n:06}/");
        let uri = format!("{uri}{}", "a".repeat(2083 - uri.len()));
        term.feed(format!("\x1b]8;id={id};{uri}\x07x"));
    }
    term.feed("\x1b]8;;\x07");
}

#[test]
fn links_that_a_page_has_no_room_for_leave_their_cells_out() {
    let mut term = Term::new(80, 24);
    print_long_links(&mut term, 900, "");
    let linked: Vec<bool> = (0..900)
        .map(|n| link_at(&term, n % 80, n / 80).is_some())
        .collect();
    let room = linked.iter().take_while(|&&linked| linked).count();
    assert!((800..900).contains(&room), "{room} links fit");
    assert!(linked[room..].iter().all(|&linked| !linked));
    let (_, uri) = link_at(&term, 7, 0).unwrap();
    assert!(uri.starts_with("http://000007/"));
    assert_tables_counted(&term.terminal.primary);
}

#[test]
fn a_row_moved_into_a_page_that_cannot_grow_keeps_its_text() {
    // At 100 columns, 900 links take rows 12 to 20, and the row that SD
    // drops below holds none to make room.
    let mut term = Term::new(100, 24);
    // The screen's first 12 rows end a page, the other 12 start the next.
    let page_rows = usize::from(Capacity::standard(100).rows);
    term.feed(format!("\x1b[24H{}", "\n".repeat(page_rows + 12 - 24)));
    {
        let rows = term.terminal.screen_rows();
        assert_ne!(rows.screen_row_id(11).0, rows.screen_row_id(12).0);
    }
    term.feed("\x1b[13H");
    print_long_links(&mut term, 900, "");
    // A long link in the first page, which the second has no room for.
    term.feed("\x1b[12H");
    print_long_links(&mut term, 1, "moved");
    assert!(link_at(&term, 0, 11).is_some());
    // SD in a region from row 12 moves row 11 down into the full page.
    term.feed("\x1b[12;24r\x1b[T");
    assert!(term.screen()[12].starts_with('x'));
    assert_eq!(
        link_at(&term, 0, 12),
        None,
        "the page had no room for the link"
    );
    assert!(
        link_at(&term, 0, 13).is_some(),
        "the rows within the page keep theirs"
    );
    assert_tables_counted(&term.terminal.primary);
}

#[test]
fn a_prepended_character_starts_the_cluster_of_the_next_one() {
    let mut term = Term::new(10, 2);
    term.feed("\r\u{600}123");
    assert_eq!(cluster_at(&term, 0, 0), ['\u{600}', '1']);
    assert_eq!(cluster_at(&term, 1, 0), ['2']);
    assert_eq!(term.cursor(), (3, 0, false));
    // After another character, and through both print paths.
    let mut term = Term::new(10, 2);
    term.feed("x\u{600}").feed("1");
    assert_eq!(cluster_at(&term, 1, 0), ['\u{600}', '1']);
    assert_eq!(term.cursor(), (2, 0, false));
    // A verse number: two digits after the sign.
    let mut term = Term::new(10, 2);
    term.feed("\u{6DD}\u{661}\u{662}");
    assert_eq!(cluster_at(&term, 0, 0), ['\u{6DD}', '\u{661}']);
    assert_eq!(cluster_at(&term, 1, 0), ['\u{662}']);
    // In legacy mode, a codepoint of no width joins the cell before it.
    let mut term = Term::new(10, 2);
    term.feed("\x1b[?2027l\r\u{600}123");
    assert_eq!(term.cursor(), (3, 0, false));
    assert_eq!(cluster_at(&term, 0, 0), ['1']);
}

#[test]
fn half_a_flag_and_a_mark_take_one_cell() {
    let mut term = Term::new(10, 2);
    term.feed("\u{1F1EE}\u{301}x");
    assert_eq!(term.cell(0, 0).width(), Width::Narrow);
    assert_eq!(term.cursor(), (2, 0, false));
}

#[test]
fn the_hangul_fillers_take_cells() {
    let mut term = Term::new(10, 2);
    term.feed("x\u{115F}\u{1161}y\u{3164}");
    assert_eq!(term.cursor(), (6, 0, false), "as wcwidth counts them");
}

#[test]
fn every_control_ends_the_cluster() {
    for control in ["\x07", "\x0f", "\x00", "\x1b[1m\x07"] {
        let mut term = Term::new(10, 2);
        term.feed(format!("\u{1F1EE}{control}\u{1F1F9}"));
        assert_eq!(term.cursor(), (2, 0, false), "{control:?}");
        assert_eq!(term.cell(0, 0).width(), Width::Narrow, "{control:?}");
    }
    // SGR alone leaves the cluster open.
    let mut term = Term::new(10, 2);
    term.feed("\u{1F1EE}\x1b[1m\u{1F1F9}");
    assert_eq!(term.cell(0, 0).width(), Width::Wide);
}

#[test]
fn a_wrap_that_stays_in_its_row_marks_no_line() {
    let flags = |term: &Term, y| {
        (
            term.row(y).has(RowFlag::SoftWrapped),
            term.row(y).has(RowFlag::Continuation),
        )
    };
    // Below the scroll region, the last row stops the text.
    let mut term = Term::new(5, 3);
    term.feed("\x1b[1;2r\x1b[3;1Habcdefg");
    assert_eq!(term.screen()[2], "fgcde");
    assert_eq!(flags(&term, 2), (false, false));
    // At the bottom margin, right of the right margin.
    let mut term = Term::new(5, 3);
    term.feed("\x1b[?69h\x1b[1;3s\x1b[1;2r\x1b[2;4Habc");
    assert_eq!(term.screen()[1], "c  ab");
    assert_eq!(flags(&term, 1), (false, false));
    // A wrap that moves on still joins the rows.
    let mut term = Term::new(5, 3);
    term.feed("abcdefg");
    assert_eq!(
        (flags(&term, 0), flags(&term, 1)),
        ((true, false), (false, true))
    );
}

#[test]
fn pins_follow_the_cells_that_insertion_and_deletion_move() {
    let mut term = Term::new(10, 2);
    term.feed("abcdefghij");
    let [c, f, i] = [2, 5, 8].map(|x| pin(&mut term, x, 0));
    // ICH 2 at column 1: c and f move right, i passes the edge.
    term.feed("\x1b[2G\x1b[2@");
    assert_eq!(term.screen()[0], "a  bcdefgh");
    assert_eq!(
        [c, f, i].map(|pin| pinned_char(&term, pin)),
        [Some('c'), Some('f'), None]
    );
    // DCH 3 at column 2: c goes, f moves left.
    term.feed("\x1b[4G\x1b[3P");
    assert_eq!(term.screen()[0], "a  efgh");
    assert_eq!([c, f].map(|pin| pinned_char(&term, pin)), [None, Some('f')]);
    // Printing in insert mode moves it right again.
    term.feed("\x1b[4h\x1b[1GXY");
    assert_eq!(term.screen()[0], "XYa  efgh");
    assert_eq!(pinned_char(&term, f), Some('f'));
    let rows = &term.terminal.primary.rows;
    assert_eq!(rows.pin_position(f).map(|p| p.col), Some(6));
}

#[test]
fn resizing_keeps_the_text_and_moves_the_cursor_with_it() {
    let mut term = Term::new(10, 4);
    term.feed("abc\r\ndef");
    term.terminal.resize(5, 4);
    assert_eq!(term.screen(), ["abc", "def", "", ""]);
    assert_eq!(term.cursor(), (3, 1, false));
    // Narrower, the lines wrap, and the empty rows below the cursor take
    // the new rows; shorter, the top rows go to the scrollback.
    term.terminal.resize(2, 2);
    assert_eq!(term.screen(), ["de", "f"]);
    assert_eq!(term.cursor(), (1, 1, false));
    assert_eq!((term.terminal.cols(), term.terminal.rows()), (2, 2));
    // Taller: the rows come back from the scrollback.
    term.terminal.resize(2, 3);
    assert_eq!(term.screen(), ["c", "de", "f"]);
    assert_eq!(term.cursor(), (1, 2, false));
    term.terminal.resize(0, 0);
    assert_eq!((term.terminal.cols(), term.terminal.rows()), (2, 1));
}

#[test]
fn a_wrap_pending_at_the_end_of_a_row_goes_on_after_it_when_it_widens() {
    let mut term = Term::new(5, 2);
    term.feed("abcde");
    assert_eq!(term.cursor(), (4, 0, true));
    term.terminal.resize(8, 2);
    assert_eq!(term.cursor(), (5, 0, false));
    term.feed("f");
    assert_eq!(term.screen()[0], "abcdef");
    // Narrower, the line wraps, and the cursor goes on after its text.
    let mut term = Term::new(5, 2);
    term.feed("abcde");
    term.terminal.resize(3, 2);
    assert_eq!(term.screen(), ["abc", "de"]);
    assert_eq!(term.cursor(), (2, 1, false));
    // On the alternate screen, cut, it stays pending in the last column.
    let mut term = Term::new(5, 2);
    term.feed("\x1b[?1049habcde");
    term.terminal.resize(3, 2);
    assert_eq!(term.cursor(), (2, 0, true));
    // Only taller, the wrap stays pending.
    let mut term = Term::new(5, 2);
    term.feed("abcde");
    term.terminal.resize(5, 3);
    assert_eq!(term.cursor(), (4, 0, true));
}

#[test]
fn resizing_brings_saved_cursors_back_and_resets_the_margins() {
    let mut term = Term::new(10, 6);
    term.feed("\x1b[5;9H\x1b7\x1b[2;3r\x1b[?69h\x1b[2;4s");
    term.terminal.resize(6, 4);
    term.feed("\x1b8");
    assert_eq!(
        term.cursor(),
        (5, 3, false),
        "the saved cursor came back within"
    );
    // The margins are the whole screen again: LF at the bottom row scrolls
    // the screen into the scrollback.
    term.feed("\x1b[4;1Hz\n");
    assert_eq!(term.terminal.screen_rows().scrollback_rows(), 1);
    assert_eq!(term.screen()[2], "z");
}

#[test]
fn resizing_keeps_the_tab_stops_and_sets_new_ones_every_eight_columns() {
    let mut term = Term::new(10, 2);
    term.feed("\x1b[4G\x1bH\x1b[3g\x1b[1G");
    term.feed("\x1b[6G\x1bH");
    term.terminal.resize(20, 2);
    let mut stops = Vec::new();
    term.feed("\r");
    for _ in 0..3 {
        term.feed("\t");
        stops.push(term.cursor().0);
    }
    assert_eq!(
        stops,
        [5, 16, 19],
        "the stop kept, a new one, then the last column"
    );
}

#[test]
fn each_screen_resizes_its_own_way() {
    let mut term = Term::new(10, 3);
    term.feed("1\r\n2\r\n3\r\n4\r\n5");
    term.feed("\x1b[?1049ha\r\nb\r\nc\r\nd");
    term.terminal.resize(10, 5);
    // The alternate screen grows at the bottom, without its rows that
    // scrolled away; the cursor stays.
    assert_eq!(term.screen(), ["b", "c", "d", "", ""]);
    assert_eq!(term.cursor(), (1, 2, false));
    term.feed("\x1b[?1049l");
    // The primary took its new rows from the scrollback.
    assert_eq!(term.screen(), ["1", "2", "3", "4", "5"]);
    assert_eq!(term.cursor(), (1, 4, false));
}

#[test]
fn resizing_ends_the_cluster_and_empties_a_cut_wide_character() {
    let mut term = Term::new(10, 2);
    term.feed("\u{1F1EE}");
    term.terminal.resize(12, 2);
    term.feed("\u{1F1F9}");
    assert_eq!(
        term.cell(0, 0).width(),
        Width::Narrow,
        "no flag across a resize"
    );
    let mut term = Term::new(10, 2);
    term.feed("abcdefgh\u{4E2D}");
    term.terminal.resize(9, 2);
    assert_eq!(term.screen()[0], "abcdefgh");
    assert!(term.cell(8, 0).is_empty());
    assert_tables_counted(&term.terminal.primary);
}

#[test]
fn reflow_rewraps_lines_as_the_width_changes() {
    let mut term = Term::new(10, 4);
    term.feed("abcdefghijklmnopqrstuvwxy\r\nshort");
    assert_eq!(
        term.screen(),
        ["abcdefghij", "klmnopqrst", "uvwxy", "short"]
    );
    term.terminal.resize(7, 4);
    assert_eq!(term.screen(), ["hijklmn", "opqrstu", "vwxy", "short"]);
    assert_eq!(term.cursor(), (5, 3, false));
    let flags: Vec<_> = (0..4)
        .map(|y| {
            (
                term.row(y).has(RowFlag::SoftWrapped),
                term.row(y).has(RowFlag::Continuation),
            )
        })
        .collect();
    assert_eq!(
        flags,
        [(true, true), (true, true), (false, true), (false, false)]
    );
    // Wider, the line comes back whole, the scrollback with it.
    term.terminal.resize(30, 4);
    assert_eq!(
        term.screen(),
        ["abcdefghijklmnopqrstuvwxy", "short", "", ""]
    );
    assert_eq!(term.cursor(), (5, 1, false));
    assert!(!term.row(0).has(RowFlag::SoftWrapped));
}

#[test]
fn reflow_moves_a_wide_character_that_the_edge_splits() {
    let mut term = Term::new(6, 4);
    term.feed("ab\u{4E2D}de\r\n");
    let tail = pin(&mut term, 3, 0);
    term.terminal.resize(3, 4);
    assert_eq!(term.screen(), ["ab", "\u{4E2D}d", "e", ""]);
    assert_eq!(term.cell(2, 0).width(), Width::SpacerHead);
    assert_eq!(term.cell(0, 1).width(), Width::Wide);
    let rows = &term.terminal.primary.rows;
    assert_eq!(rows.pin_on_screen(tail), Some((1, 1)), "on the second half");
    let head = pin(&mut term, 2, 0);
    term.terminal.resize(6, 4);
    assert_eq!(term.screen()[0], "ab\u{4E2D}de", "the spacer head goes");
    let rows = &term.terminal.primary.rows;
    assert_eq!(
        rows.pin_on_screen(head),
        Some((2, 0)),
        "the spacer head's pin"
    );
    assert_eq!(rows.pin_on_screen(tail), Some((3, 0)));
    // A spacer head's pin goes with the wide character, wherever it goes.
    let mut term = Term::new(3, 3);
    term.feed("xa\u{4E2D}");
    assert_eq!(term.cell(2, 0).width(), Width::SpacerHead);
    let head = pin(&mut term, 2, 0);
    term.terminal.resize(2, 3);
    assert_eq!(term.screen()[..2], ["xa", "\u{4E2D}"]);
    let rows = &term.terminal.primary.rows;
    assert_eq!(rows.pin_on_screen(head), Some((0, 1)));
}

#[test]
fn reflow_keeps_styles_graphemes_and_links() {
    let mut term = Term::new(8, 3);
    term.feed("abc\x1b[31mde\x1b]8;;http://a\x07fg\x1b]8;;\x07\x1b[me\u{301}xyz");
    term.terminal.resize(3, 3);
    assert_eq!(term.screen(), ["def", "ge\u{301}x", "yz"]);
    assert_eq!(style_at(&term, 0, 0).fg, Color::Palette(1));
    assert_eq!(link_at(&term, 1, 0), None);
    assert_eq!(link_at(&term, 2, 0), link("", "http://a"));
    assert_eq!(link_at(&term, 0, 1), link("", "http://a"));
    assert_eq!(cluster_at(&term, 1, 1), ['e', '\u{301}']);
    assert_tables_counted(&term.terminal.primary);
    term.terminal.resize(8, 3);
    assert_eq!(term.screen()[0], "abcdefge\u{301}");
    assert_eq!(cluster_at(&term, 7, 0), ['e', '\u{301}']);
    assert_tables_counted(&term.terminal.primary);
}

#[test]
fn reflow_keeps_the_cursor_where_it_was_in_its_line() {
    // In the empty cells after the text.
    let mut term = Term::new(10, 3);
    term.feed("ab\x1b[1;9H");
    term.terminal.resize(5, 3);
    assert_eq!(term.cursor(), (3, 1, false));
    assert_eq!(term.screen(), ["ab", "", ""]);
    // The empty rows below take the rows that the text needs.
    let mut term = Term::new(10, 6);
    term.feed("abcdefghijKLM");
    term.terminal.resize(5, 6);
    assert_eq!(term.screen(), ["abcde", "fghij", "KLM", "", "", ""]);
    assert_eq!(term.cursor(), (3, 2, false));
    assert_eq!(term.terminal.screen_rows().scrollback_rows(), 0);
}

#[test]
fn reflow_moves_the_saved_cursor_and_pins_with_their_text() {
    let mut term = Term::new(10, 4);
    term.feed("abcdefghijKLM\x1b[2;2H\x1b7\x1b[3;1H");
    let k = pin(&mut term, 0, 1);
    let trailing = pin(&mut term, 4, 1);
    term.terminal.resize(5, 4);
    assert_eq!(term.screen(), ["abcde", "fghij", "KLM", ""]);
    term.feed("\x1b8");
    assert_eq!(term.cursor(), (1, 2, false), "back on the L");
    let rows = &term.terminal.primary.rows;
    assert_eq!(rows.pin_on_screen(k), Some((0, 2)));
    assert_eq!(pinned_char(&term, k), Some('K'));
    assert_eq!(
        rows.pin_on_screen(trailing),
        Some((4, 2)),
        "as far after the text as it was"
    );
    // A saved cursor whose text went to the scrollback comes to the top.
    let mut term = Term::new(10, 3);
    term.feed("abcdefghijKLMNOPQRST\r\nxyz\x1b[2;2H\x1b7\x1b[3;3H");
    term.terminal.resize(3, 3);
    assert_eq!(term.screen(), ["PQR", "ST", "xyz"]);
    term.feed("\x1b8");
    assert_eq!(term.cursor(), (1, 0, false));
    // On the alternate screen, a saved cursor in cut columns comes back
    // to the last column.
    let mut term = Term::new(10, 3);
    term.feed("\x1b[?1049h\x1b[2;9H\x1b7\x1b[H");
    term.terminal.resize(5, 3);
    term.feed("\x1b8");
    assert_eq!(term.cursor(), (4, 1, false));
}

#[test]
fn reflow_keeps_the_prompt_mark_on_the_first_row_of_its_line() {
    let mut term = Term::new(6, 3);
    term.feed("\x1b]133;A\x07$ abcdefgh");
    term.terminal.resize(3, 3);
    let marks: Vec<_> = (0..3)
        .map(|y| term.row(y).has(RowFlag::PromptStart))
        .collect();
    assert_eq!(term.terminal.screen_rows().scrollback_rows(), 1);
    assert_eq!(marks, [false, false, false], "the line's first row went up");
    term.terminal.resize(12, 3);
    assert!(term.row(0).has(RowFlag::PromptStart));
    assert!(!term.row(1).has(RowFlag::PromptStart));
}

#[test]
fn narrowing_a_cleared_screen_keeps_it_clear() {
    let mut term = Term::new(10, 4);
    term.feed("ABCDEFGHIJ\r\nabcdefghij\r\nKLMNOPQRST\r\nklmnopqrst");
    term.feed("\x1b[H\x1b[2J$ ");
    assert_eq!(term.screen(), ["$", "", "", ""]);
    term.terminal.resize(5, 4);
    assert_eq!(
        term.screen(),
        ["$", "", "", ""],
        "the scrollback grew, not the screen"
    );
    assert_eq!(term.cursor(), (2, 0, false));
}

#[test]
fn rows_that_edits_moved_do_not_join_lines() {
    let line_up = |feed: &str, rows: u16| {
        let mut term = Term::new(10, rows);
        term.feed(feed);
        term.terminal.resize(20, rows);
        (term.screen(), term.cursor())
    };
    // DL takes the continuation away.
    assert_eq!(
        line_up("0123456789abc\r\nprompt\x1b[2;1H\x1b[M", 3),
        (
            ["0123456789", "prompt", ""].map(String::from).to_vec(),
            (0, 1, false)
        )
    );
    // IL puts a row between the line's rows.
    assert_eq!(
        line_up("0123456789abc\x1b[2;1H\x1b[L", 3),
        (
            ["0123456789", "", "abc"].map(String::from).to_vec(),
            (0, 1, false)
        )
    );
    // RI pushes the continuation off, and LF brings a new row in.
    let (screen, _) = line_up("x\r\n0123456789abc\x1b[H\x1bM\x1b[3;1H\r\n$ ls", 3);
    assert_eq!(screen, ["x", "0123456789", "$ ls"]);
    // Reverse wraparound does not go back into a row that is not its own.
    let mut term = Term::new(10, 3);
    term.feed("0123456789abc\x1b[2;1H\x1b[L\x1b[?45h\x08");
    assert_eq!(term.cursor(), (0, 1, false));
}

#[test]
fn rows_left_last_by_a_shorter_screen_do_not_wrap() {
    let mut term = Term::new(5, 6);
    term.feed("\x1b[3Habcdefghijklmnopqrst\x1b[H");
    term.terminal.resize(5, 4);
    term.feed("\x1b[4H\r\nNEW");
    term.terminal.resize(20, 4);
    assert!(
        term.screen().contains(&"NEW".to_owned()),
        "{:?}",
        term.screen()
    );
}

#[test]
fn a_cursor_on_a_spacer_head_goes_with_its_wide_character() {
    let mut term = Term::new(5, 3);
    term.feed("abcd\u{4E2D}\x1b[1;5H");
    assert_eq!(term.cell(4, 0).width(), Width::SpacerHead);
    term.terminal.resize(10, 3);
    assert_eq!(term.screen()[0], "abcd\u{4E2D}");
    assert_eq!(term.cursor(), (4, 0, false));
}

#[test]
fn reflow_keeps_the_text_below_the_cursor_and_the_cursor_on_screen() {
    let mut term = Term::new(6, 2);
    term.feed("abcdefghijk\x1b[H");
    term.terminal.resize(3, 2);
    assert_eq!(
        term.screen(),
        ["ghi", "jk"],
        "the text below the cursor stays"
    );
    assert_eq!(term.cursor(), (0, 0, false), "the cursor's row went up");
    assert_eq!(term.terminal.screen_rows().scrollback_rows(), 2);
    term.terminal.resize(6, 2);
    assert_eq!(term.screen(), ["abcdef", "ghijk"]);
}

#[test]
fn a_saved_cursor_in_a_cut_column_keeps_its_row() {
    let mut term = Term::new(10, 10);
    term.feed("\x1b[?1049h\x1b[6;1HROW5\x1b[6;9H\x1b7\x1b[10;1H");
    term.terminal.resize(5, 5);
    assert_eq!(term.screen()[0], "ROW5");
    term.feed("\x1b8");
    assert_eq!(term.cursor(), (4, 0, false));
}

#[test]
fn a_cluster_that_widens_in_insert_mode_moves_the_line_right() {
    let mut term = Term::new(10, 2);
    term.feed("abc\r\x1b[4h\u{2764}\u{FE0F}");
    assert_eq!(term.screen()[0], "\u{2764}\u{FE0F}abc");
    assert_eq!(term.cursor(), (2, 0, false));
    let mut term = Term::new(10, 2);
    term.feed("abc\r\x1b[4h\u{600}\u{4E2D}");
    assert_eq!(term.screen()[0], "\u{600}\u{4E2D}abc");
    assert_eq!(cluster_at(&term, 0, 0), ['\u{600}', '\u{4E2D}']);
}

#[test]
fn a_cluster_keeps_32_codepoints_after_normalization_too() {
    let options = Options {
        normalization: Normalization::Nfd,
        ..Options::default()
    };
    let mut term = Term::with(10, 2, options);
    term.feed(format!("a{}\u{344}x", "\u{301}".repeat(30)));
    assert_eq!(cluster_at(&term, 0, 0).len(), 32);
}

#[test]
fn a_flag_after_a_prepended_character_takes_two_cells() {
    let mut term = Term::new(10, 2);
    term.feed("\r\u{600}\u{1F1FA}\u{1F1F8}x");
    assert_eq!(term.cell(0, 0).width(), Width::Wide);
    assert_eq!(term.cursor(), (3, 0, false));
}

#[test]
fn compacting_the_scrollback_keeps_its_text() {
    let mut term = Term::new(80, 24);
    term.feed("\x1b[31mls -la\x1b[m\r\n".repeat(4000));
    let screen = term.screen();
    let before = term.terminal.screen_rows().memory();
    let saved = term.terminal.compact_scrollback();
    assert!(saved > before / 2, "{saved} of {before} bytes");
    assert_eq!(term.screen(), screen, "the screen stays");
    term.feed("more");
    // A resize brings the cold pages back, and every line with them.
    term.terminal.resize(40, 24);
    let lines = logical_lines(&term.terminal);
    assert_eq!(lines.len(), 4001);
    assert!(
        lines[..4000]
            .iter()
            .all(|line| line.len() == 6 && line[0].1.fg == Color::Palette(1))
    );
    assert_tables_counted(&term.terminal.primary);
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

/// The id and the URI of a link.
type LinkTarget = (Vec<u8>, Vec<u8>);

/// A cell as it reads, wherever its page keeps its style, grapheme and
/// link.
type CellValue = (Cell, Style, Vec<char>, Option<LinkTarget>);

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
                    let link = page.hyperlink(row, x).map(|id| {
                        let link = page.hyperlinks().get(id);
                        (link.id.to_vec(), link.uri.to_vec())
                    });
                    (
                        cell.with_style(StyleId::DEFAULT),
                        *page.styles().get(cell.style()),
                        page.grapheme(row, x).to_vec(),
                        link,
                    )
                })
                .collect();
            (page.row(row), cells)
        })
        .collect();
    (rows, screen.cursor)
}

/// Checks that each page's styles and links have as many references as
/// cells that use them, and no others are in the tables; that the grapheme
/// table has an entry for each grapheme cell, and no other; and that each
/// row has the flags of what its cells hold.
fn assert_tables_counted(screen: &Screen) {
    use std::collections::HashMap;

    #[derive(Default)]
    struct Counts {
        styles: HashMap<StyleId, u32>,
        links: HashMap<HyperlinkId, u32>,
        graphemes: usize,
    }

    let mut pages: HashMap<*const Page, (&Page, Counts)> = HashMap::new();
    for (page, row) in screen.rows.iter_rows() {
        let (_, counts) = pages
            .entry(std::ptr::from_ref(page))
            .or_insert_with(|| (page, Counts::default()));
        let flags = page.row(row);
        for (x, cell) in (0..).zip(page.cells(row)) {
            if cell.style() != StyleId::DEFAULT {
                *counts.styles.entry(cell.style()).or_default() += 1;
                assert!(flags.has(RowFlag::Styles), "row {row}, column {x}");
            }
            if let Some(link) = page.hyperlink(row, x) {
                *counts.links.entry(link).or_default() += 1;
                assert!(flags.has(RowFlag::Hyperlinks), "row {row}, column {x}");
            }
            if let Content::Grapheme(_) = cell.content() {
                counts.graphemes += 1;
                assert!(!page.grapheme(row, x).is_empty(), "row {row}, column {x}");
                assert!(flags.has(RowFlag::Graphemes), "row {row}, column {x}");
            }
        }
    }
    for (page, counts) in pages.values() {
        assert_eq!(
            page.styles().len(),
            counts.styles.len(),
            "styles in the table"
        );
        for (&id, &count) in &counts.styles {
            assert_eq!(page.styles().references(id), count, "{id:?}");
        }
        assert_eq!(
            page.hyperlinks().len(),
            counts.links.len(),
            "links in the table"
        );
        assert_eq!(
            page.grapheme_entries(),
            counts.graphemes,
            "graphemes in the table"
        );
        for (&id, &count) in &counts.links {
            assert_eq!(page.hyperlinks().references(id), count, "{id:?}");
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
    Resize(u16, u16),
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
    "\x1b[K",
    "\x1b[1K",
    "\x1b[2K",
    "\x1b[J",
    "\x1b[1J",
    "\x1b[2J",
    "\x1b[3J",
    "\x1b[?K",
    "\x1b[?2J",
    "\x1b[2X",
    "\x1b[1\"q",
    "\x1b[0\"q",
    "\x1b[2@",
    "\x1b[P",
    "\x1b[L",
    "\x1b[2M",
    "\x1b[S",
    "\x1b[2T",
    "\x1b[3b",
    "\x1b[2;3r",
    "\x1b[r",
    "\x1b[s",
    "\x1b[u",
    "\x1b[?69h",
    "\x1b[4h",
    "\x1b[4l",
    "\x1b[20h",
    "\x1b[20l",
    "\x1b[?45h",
    "\x1b[?7l",
    "\x1b[?7h",
    "\x1b[?6h",
    "\x1b[?6l",
    "\x1b[?47h",
    "\x1b[?1047l",
    "\x1b[?1049h",
    "\x1b[?1049l",
    "\x1b[?1048h",
    "\x1b[?2027l",
    "\x1b[?2027h",
    "\x1b[6n",
    "\x1b[c",
    "\x1b[>1u",
    "\x1b[<u",
    "\x1b[=3;2u",
    "\x1b[?u",
    "\x1b[?2026h",
    "\x1b[?2026l",
    "\x1bP$qm\x1b\\",
    "\x1b[22t",
    "\x1b[23t",
    "\x1b]2;title\x07",
    "\x1b]7;file:///tmp\x1b\\",
    "\x1b]9;4;1;5\x07",
    "\x1b]4;1;#123\x07",
    "\x1b]11;?\x07",
    "\x1b]52;c;eA==\x07",
    "\x1b]8;;https://a\x07",
    "\x1b]8;id=1;file:///b\x1b\\",
    "\x1b]8;;\x07",
    "\x1b]133;A\x07",
    "\x1b]133;B\x07",
    "\x1b]133;C\x07",
    "\x1b]133;D;0\x07",
    "\x1b[0m",
    "\x1b[1;31m",
    "\x1b[44m",
    "\x1b[38;2;1;2;3m",
    "\x1b[48:5:9m",
    "\x1b[4:3m",
    "\x1b[7;9m",
    "\x1b[?69l",
    "\x1b[2;4s",
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
        1 => (1_u16..=10, 1_u16..=4).prop_map(|(cols, rows)| Op::Resize(cols, rows)),
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
            ..Options::default()
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
                    &Op::Resize(cols, rows) => terminal.resize(cols, rows),
                }
            }
            let (fast, slow) = (&fast.terminal, &slow.terminal);
            prop_assert_eq!(screen_state(&fast.primary), screen_state(&slow.primary), "after {:?}", op);
            prop_assert_eq!(screen_state(&fast.alternate), screen_state(&slow.alternate), "after {:?}", op);
            prop_assert_eq!(fast.cluster, slow.cluster, "after {:?}", op);
            prop_assert_eq!(fast.segmenter, slow.segmenter, "after {:?}", op);
        }
        for screen in [&fast.terminal.primary, &fast.terminal.alternate] {
            assert_tables_counted(screen);
        }
    }
}

/// A cell of a line as it reads, wherever its page keeps its style,
/// grapheme and link: its cluster, its style and its link's target.
type LineCell = (Vec<char>, Style, Option<LinkTarget>);

/// The logical lines of the primary screen and its scrollback, oldest
/// first, as their cells read: without the spacers of wide characters, the
/// empty cells that end each line, and the empty lines at the end.
fn logical_lines(terminal: &Terminal) -> Vec<Vec<LineCell>> {
    let mut lines: Vec<Vec<LineCell>> = Vec::new();
    let mut wrapped = false;
    for (page, row) in terminal.primary.rows.iter_rows() {
        // A row goes on from the one before it only when that one
        // soft-wraps and this one is its continuation.
        if !(wrapped && page.row(row).has(RowFlag::Continuation)) {
            lines.push(Vec::new());
        }
        let line = lines.last_mut().expect("a line was started");
        for (x, &cell) in (0..).zip(page.cells(row)) {
            if matches!(cell.width(), Width::SpacerHead | Width::SpacerTail) {
                continue;
            }
            let cluster = match cell.content() {
                Content::Char('\0') | Content::BgPalette(_) | Content::BgRgb(_) => Vec::new(),
                Content::Char(c) | Content::Grapheme(c) => {
                    let mut cluster = vec![c];
                    cluster.extend_from_slice(page.grapheme(row, x));
                    cluster
                }
            };
            let link = page.hyperlink(row, x).map(|id| {
                let link = page.hyperlinks().get(id);
                (link.id.to_vec(), link.uri.to_vec())
            });
            line.push((cluster, *page.styles().get(cell.style()), link));
        }
        wrapped = page.row(row).has(RowFlag::SoftWrapped);
    }
    for line in &mut lines {
        while line
            .last()
            .is_some_and(|(cluster, _, link)| cluster.is_empty() && link.is_none())
        {
            line.pop();
        }
    }
    while lines.last().is_some_and(Vec::is_empty) {
        lines.pop();
    }
    lines
}

/// Text to fill a screen with: words and spaces, wide characters, emoji,
/// combining marks, colors, links, and new lines.
fn reflow_text() -> impl Strategy<Value = String> {
    let piece = prop_oneof![
        4 => "[a-z]{1,8}",
        2 => Just(" ".to_owned()),
        2 => Just("\u{4E2D}".to_owned()),
        1 => Just("\u{1F600}".to_owned()),
        1 => Just("e\u{301}".to_owned()),
        1 => Just("\x1b[31m".to_owned()),
        1 => Just("\x1b[1;44m".to_owned()),
        1 => Just("\x1b[m".to_owned()),
        1 => Just("\x1b]8;;http://a\x07".to_owned()),
        1 => Just("\x1b]8;;\x07".to_owned()),
        2 => Just("\r\n".to_owned()),
        1 => prop::sample::select(vec![
            "\x1b[H", "\x1b[2;3H", "\x1b[A", "\x1b[3D", "\x1b[L", "\x1b[M", "\x1bM", "\x1b[2;3r",
            "\x1b[r", "\x1b[S", "\x1b[T", "\x1b[2J", "\x1b[K",
        ])
        .prop_map(str::to_owned),
    ];
    // The cursor ends wherever the text leaves it: no resize drops text.
    prop::collection::vec(piece, 1..80).prop_map(|pieces| pieces.concat())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    /// Shrinking a screen, then widening it back, keeps every line, cell
    /// by cell, with its styles, graphemes and links, and so do the sizes
    /// between, narrower or wider, shorter or taller.
    #[test]
    fn reflow_round_trips_keep_every_line(
        cols in 4_u16..=16,
        rows in 2_u16..=6,
        narrower in 2_u16..=15,
        sizes in prop::collection::vec((2_u16..=20, 1_u16..=8), 0..4),
        text in reflow_text(),
    ) {
        let narrower = narrower.min(cols - 1);
        let mut term = Term::new(cols, rows);
        term.feed(&text);
        let before = logical_lines(&term.terminal);
        for (cols, rows) in std::iter::once((narrower, rows)).chain(sizes) {
            term.terminal.resize(cols, rows);
            prop_assert_eq!(&logical_lines(&term.terminal), &before, "at {}x{}", cols, rows);
        }
        term.terminal.resize(cols, rows);
        prop_assert_eq!(logical_lines(&term.terminal), before);
        assert_tables_counted(&term.terminal.primary);
    }
}
