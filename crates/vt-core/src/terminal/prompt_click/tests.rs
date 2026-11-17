//! A click in the command line: the arrows to the clicked character, or
//! the click itself, as the shell's prompt announced.

use super::*;
use crate::parser::Parser;
use crate::terminal::Options;

/// A terminal of `cols` columns fed `bytes`.
fn fed(cols: u16, bytes: &str) -> Terminal {
    let mut terminal = Terminal::new(cols, 6, Options::default());
    Parser::new().advance(&mut terminal, bytes.as_bytes());
    terminal
}

/// The click at column `x` of the screen's row `y` of `terminal`.
fn click(terminal: &Terminal, x: u16, y: u16) -> Option<PromptClick> {
    terminal.prompt_click(x, terminal.origin().row(y))
}

/// A prompt of `options`, `$ `, and the input `typed` after it.
fn prompt(options: &str, typed: &str) -> String {
    format!("\x1b]133;A{options}\x07$ \x1b]133;B\x07{typed}")
}

#[test]
fn nothing_moves_of_a_shell_that_announced_nothing() {
    let terminal = fed(40, &prompt("", "echo hello"));
    assert_eq!(terminal.prompt_clicks(), None);
    assert_eq!(click(&terminal, 4, 0), None);
}

#[test]
fn a_click_sends_the_arrows_to_its_character_and_none_past_the_end() {
    // `echo hello` from column 2, the cursor at its end, column 12.
    let mut terminal = fed(40, &prompt(";cl=line", "echo hello"));
    assert_eq!(terminal.prompt_clicks(), Some(PromptClicks::Line));
    assert_eq!(click(&terminal, 4, 0), Some(PromptClick::Left(8)));
    // Past the end: at the end already.
    assert_eq!(click(&terminal, 30, 0), None);
    // On the prompt, and below the command line: nothing.
    assert_eq!(click(&terminal, 0, 0), None);
    assert_eq!(click(&terminal, 4, 2), None);
    // The cursor moved back to the `h`: a click past the end takes it to
    // the end.
    Parser::new().advance(&mut terminal, b"\x1b[8D");
    assert_eq!(click(&terminal, 30, 0), Some(PromptClick::Right(8)));
    assert_eq!(click(&terminal, 6, 0), Some(PromptClick::Right(2)));
}

#[test]
fn a_wide_character_is_one_arrow_its_second_half_too() {
    let terminal = fed(40, &prompt(";cl=line", "a漢b"));
    // `a` at 2, `漢` at 3 and 4, `b` at 5, the cursor at 6.
    assert_eq!(click(&terminal, 4, 0), Some(PromptClick::Left(2)));
    assert_eq!(click(&terminal, 3, 0), Some(PromptClick::Left(2)));
    assert_eq!(click(&terminal, 5, 0), Some(PromptClick::Left(1)));
}

#[test]
fn a_long_line_wrapped_over_rows_is_one_line() {
    // Ten columns: the prompt and 15 characters wrap into a second row.
    let terminal = fed(10, &prompt(";cl=line", "abcdefghijklmno"));
    let cursor = terminal.cursor();
    assert_eq!((cursor.x, cursor.y), (7, 1));
    // From the end, 15 characters in, to the `b`, the second.
    assert_eq!(click(&terminal, 3, 0), Some(PromptClick::Left(14)));
}

#[test]
fn the_line_s_arrows_stay_in_its_line_and_the_lines_arrows_cross_them() {
    // Two lines, the cursor at the second's end.
    let typed = "one\r\ntwo";
    let terminal = fed(40, &prompt(";cl=line", typed));
    assert_eq!(click(&terminal, 3, 0), None);
    assert_eq!(click(&terminal, 1, 1), Some(PromptClick::Left(2)));
    // Of `cl=m`, the line's end one arrow: from 7 steps in to 1.
    let terminal = fed(40, &prompt(";cl=m", typed));
    assert_eq!(terminal.prompt_clicks(), Some(PromptClicks::Lines));
    assert_eq!(click(&terminal, 3, 0), Some(PromptClick::Left(6)));
}

#[test]
fn click_events_hand_the_click_itself_before_the_arrows() {
    let terminal = fed(
        40,
        &format!("\r\n{}", prompt(";click_events=1;cl=line", "ls")),
    );
    assert_eq!(
        terminal.prompt_clicks(),
        Some(PromptClicks::Events { relative: false })
    );
    assert_eq!(
        click(&terminal, 3, 1),
        Some(PromptClick::Event { col: 4, row: 2 })
    );
    // Below the command line too, as the shell takes it; above the prompt,
    // nothing.
    assert_eq!(
        click(&terminal, 0, 3),
        Some(PromptClick::Event { col: 1, row: 4 })
    );
    assert_eq!(click(&terminal, 3, 0), None);
    // Of its row from the prompt's first.
    let terminal = fed(40, &format!("\r\n{}", prompt(";click_events=2", "ls")));
    assert_eq!(
        click(&terminal, 3, 1),
        Some(PromptClick::Event { col: 4, row: 1 })
    );
}

#[test]
fn nothing_moves_out_of_the_command_line_s_editing() {
    // Its command runs.
    let terminal = fed(40, &format!("{}\x1b]133;C\x07", prompt(";cl=line", "ls")));
    assert_eq!(click(&terminal, 2, 0), None);
    // A full-screen program's screen.
    let terminal = fed(40, &format!("{}\x1b[?1049h", prompt(";cl=line", "ls")));
    assert_eq!(click(&terminal, 2, 0), None);
}

#[test]
fn a_secondary_prompt_keeps_the_options_and_a_new_prompt_takes_its_own() {
    let mut terminal = fed(40, &prompt(";cl=line", "ls"));
    Parser::new().advance(&mut terminal, b"\x1b]133;A;k=s\x07");
    assert_eq!(terminal.prompt_clicks(), Some(PromptClicks::Line));
    Parser::new().advance(&mut terminal, b"\r\n\x1b]133;A\x07");
    assert_eq!(terminal.prompt_clicks(), None);
    Parser::new().advance(&mut terminal, b"\x1b]133;A;cl=other\x07");
    assert_eq!(terminal.prompt_clicks(), None);
}

/// What replaces the selection of `terminal` from column `from` to column
/// `to`, both of the screen's row `y`, as a key deletes or types over it.
fn replaced(
    terminal: &mut Terminal,
    (from, to): (u16, u16),
    y: u16,
) -> Option<(Option<PromptClick>, usize)> {
    let row = terminal.origin().row(y);
    terminal.select(
        (from, row),
        (to, row),
        crate::terminal::SelectionShape::Cells,
        crate::terminal::SelectionUnit::Cell,
        "",
    );
    terminal.command_line_selection()
}

#[test]
fn a_selection_in_the_command_line_goes_of_the_arrows_to_its_end_and_as_many_backspaces() {
    // `hel` of `echo hello`: two left arrows from the end, then three
    // Backspaces.
    let mut terminal = fed(40, &prompt(";cl=line", "echo hello"));
    assert_eq!(
        replaced(&mut terminal, (7, 9), 0),
        Some((Some(PromptClick::Left(2)), 3))
    );
    // To past the command's end: its characters alone, from the cursor.
    assert_eq!(replaced(&mut terminal, (7, 20), 0), Some((None, 5)));
    // A wide character one Backspace.
    let mut terminal = fed(40, &prompt(";cl=line", "a漢b"));
    assert_eq!(
        replaced(&mut terminal, (3, 4), 0),
        Some((Some(PromptClick::Left(1)), 1))
    );
    // Of a shell that takes the clicks too.
    let mut terminal = fed(40, &prompt(";click_events=1", "echo hello"));
    assert_eq!(
        replaced(&mut terminal, (2, 5), 0),
        Some((Some(PromptClick::Left(6)), 4))
    );
}

#[test]
fn a_selection_of_the_prompt_or_of_another_line_is_not_the_command_line_s() {
    let mut terminal = fed(40, &prompt(";cl=line", "echo hello"));
    assert_eq!(replaced(&mut terminal, (0, 4), 0), None);
    let typed = "one\r\ntwo";
    let mut terminal = fed(40, &prompt(";cl=line", typed));
    assert_eq!(replaced(&mut terminal, (2, 3), 0), None);
    // Of the lines' arrows: from 7 steps in to 2, one deleted.
    let mut terminal = fed(40, &prompt(";cl=m", typed));
    assert_eq!(
        replaced(&mut terminal, (3, 3), 0),
        Some((Some(PromptClick::Left(5)), 1))
    );
    // Of a shell that announced nothing, or a rectangle.
    let mut terminal = fed(40, &prompt("", "echo hello"));
    assert_eq!(replaced(&mut terminal, (7, 9), 0), None);
    let mut terminal = fed(40, &prompt(";cl=line", "echo hello"));
    let row = terminal.origin().row(0);
    terminal.select(
        (7, row),
        (9, row),
        crate::terminal::SelectionShape::Rectangle,
        crate::terminal::SelectionUnit::Cell,
        "",
    );
    assert_eq!(terminal.command_line_selection(), None);
}

/// `echo hello` after its prompt, then a right prompt, `main`, at column
/// 35 of 40, of the input's mark after it or not, and the cursor back at
/// the line's end, column 12, as zsh draws them.
fn with_right_prompt(input_after: bool) -> Terminal {
    let after = if input_after { "\x1b]133;B\x07" } else { "" };
    fed(
        40,
        &format!(
            "{}\x1b[23C\x1b]133;P;k=r\x07main{after}\x1b[27D",
            prompt(";cl=line", "echo hello")
        ),
    )
}

#[test]
fn a_right_prompt_is_no_part_of_the_command_line() {
    let mut terminal = with_right_prompt(true);
    assert_eq!(terminal.cursor().x, 12);
    for x in 35..39 {
        assert_eq!(terminal.cell(x, 0).semantic(), Semantic::Prompt, "{x}");
    }
    // The command line starts where its first `B` marked it.
    let row = terminal.origin().row(0);
    assert_eq!(terminal.commands().last().unwrap().input, Some((2, row)));
    // A click on it is a click past the input's end: at its end already.
    assert_eq!(click(&terminal, 36, 0), None);
    // A selection of it is no command line's; one past the text's end, of
    // blank cells alone, is.
    assert_eq!(replaced(&mut terminal, (35, 38), 0), None);
    assert_eq!(replaced(&mut terminal, (30, 38), 0), None);
    assert_eq!(replaced(&mut terminal, (9, 20), 0), Some((None, 3)));
    // What the user types goes on as input.
    Parser::new().advance(&mut terminal, b"!");
    assert_eq!(terminal.cell(12, 0).semantic(), Semantic::Input);
    // The cursor back at the `h`: a click on the right prompt takes it to
    // the end.
    Parser::new().advance(&mut terminal, b"\x1b[9D");
    assert_eq!(click(&terminal, 36, 0), Some(PromptClick::Right(9)));
}

#[test]
fn the_spaces_that_zsh_erases_with_are_no_part_of_the_command_line() {
    // zsh erases a character with `\b \b`: `echo hello` and three
    // Backspaces leave `echo he` and three spaces, the cursor at 9.
    let mut terminal = fed(
        40,
        &prompt(";cl=line", "echo hello\x08 \x08\x08 \x08\x08 \x08"),
    );
    assert_eq!(terminal.cursor().x, 9);
    // `he` to past the end: its two characters, from the cursor.
    assert_eq!(replaced(&mut terminal, (7, 20), 0), Some((None, 2)));
    // A click past the end: at the end already.
    assert_eq!(click(&terminal, 15, 0), None);
    // Control-W writes spaces over the word it takes: `echo hello world`,
    // Control-W, then a Backspace, leave `echo hello`.
    let mut terminal = fed(
        40,
        &prompt(
            ";cl=line",
            "echo hello world\x08\x08\x08\x08\x08     \x08\x08\x08\x08\x08\x08 \x08",
        ),
    );
    assert_eq!(terminal.cursor().x, 12);
    assert_eq!(replaced(&mut terminal, (7, 30), 0), Some((None, 5)));
    // Spaces typed within the line stay its characters.
    let mut terminal = fed(40, &prompt(";cl=line", "a  b"));
    assert_eq!(replaced(&mut terminal, (3, 5), 0), Some((None, 3)));
}

/// fish 4's prompt, as it draws it: its start of the clicks' events, its
/// input's mark, a right prompt of no mark at the row's end after a
/// carriage return, then the cursor back in the input, which `typed` fills.
fn fish(typed: &str) -> Terminal {
    fed(
        60,
        &format!(
            "\x1b]133;A;click_events=1\x07fish> \x1b]133;B\x07\r\x1b[54CRIGHTP\r\x1b[6C{typed}"
        ),
    )
}

#[test]
fn a_right_prompt_of_no_mark_is_no_part_of_the_command_line() {
    let mut terminal = fish("echo hi");
    assert_eq!(terminal.cell(55, 0).semantic(), Semantic::Input);
    assert_eq!(terminal.cursor().x, 13);
    // A selection of the right prompt, or up to it, is no command line's.
    assert_eq!(replaced(&mut terminal, (54, 59), 0), None);
    assert_eq!(replaced(&mut terminal, (11, 59), 0), None);
    // One of `hi` to the blank cells before it is: its two characters.
    assert_eq!(replaced(&mut terminal, (11, 40), 0), Some((None, 2)));
    // A click on the right prompt is past the input's end.
    Parser::new().advance(&mut terminal, b"\x1b[5D");
    let at = terminal.cursor().x;
    assert_eq!(at, 8);
    assert_eq!(
        replaced(&mut terminal, (6, 9), 0),
        Some((Some(PromptClick::Right(2)), 4))
    );
    // Of no input yet, the right prompt is no command line either.
    let mut empty = fish("");
    assert_eq!(replaced(&mut empty, (54, 59), 0), None);
}

#[test]
fn a_right_prompt_of_no_input_s_mark_ends_as_the_text_goes_back_before_it() {
    let mut terminal = with_right_prompt(false);
    for x in 35..39 {
        assert_eq!(terminal.cell(x, 0).semantic(), Semantic::Prompt, "{x}");
    }
    // Clicks go on while it is drawn, of its end told by no mark.
    assert_eq!(click(&terminal, 4, 0), Some(PromptClick::Left(8)));
    // What the shell prints back on its line is input.
    Parser::new().advance(&mut terminal, b"x");
    assert_eq!(terminal.cell(12, 0).semantic(), Semantic::Input);
    assert_eq!(click(&terminal, 4, 0), Some(PromptClick::Left(9)));
}

#[test]
fn the_clicks_options_go_as_the_command_s_output_or_end_comes() {
    let terminal = fed(40, &format!("{}\x1b]133;C\x07", prompt(";cl=line", "cat")));
    assert_eq!(terminal.prompt_clicks(), None);
    // Output that announces them holds them until its own end.
    let mut terminal = fed(40, "\x1b]133;A;click_events=1\x07\x1b]133;B\x07");
    assert!(terminal.prompt_clicks().is_some());
    Parser::new().advance(&mut terminal, b"\x1b]133;D\x07");
    assert_eq!(terminal.prompt_clicks(), None);
}

#[test]
fn a_cursor_of_a_pending_wrap_is_past_its_cell() {
    // `$ abcdefgh` fills ten columns, the cursor's wrap pending at the
    // `h`: eight characters before it.
    let terminal = fed(10, &prompt(";cl=line", "abcdefgh"));
    assert!(terminal.cursor().pending_wrap);
    assert_eq!(click(&terminal, 2, 0), Some(PromptClick::Left(8)));
    let mut terminal = fed(10, &prompt(";cl=line", "abcdefgh"));
    assert_eq!(replaced(&mut terminal, (8, 9), 0), Some((None, 2)));
}

#[test]
fn a_selection_past_the_most_arrows_or_deletes_sends_nothing() {
    let typed = "a".repeat(4200);
    // The cursor 4200 characters in: the first two, 4198 arrows away.
    let mut terminal = fed(4300, &prompt(";cl=line", &typed));
    assert_eq!(replaced(&mut terminal, (2, 3), 0), None);
    // All but the first two: 4198 deletes.
    assert_eq!(replaced(&mut terminal, (4, 4201), 0), None);
    // Within the bound, as before.
    assert_eq!(
        replaced(&mut terminal, (4000, 4001), 0),
        Some((Some(PromptClick::Left(200)), 2))
    );
}

#[test]
fn no_arrows_cross_a_grapheme_of_several_code_points() {
    // `a` at 2, `é` of `e` and its combining accent at 3, `b` at 4, the
    // cursor at 5.
    let mut terminal = fed(40, &prompt(";cl=line", "ae\u{301}b"));
    assert!(matches!(
        terminal.cell(3, 0).content(),
        Content::Grapheme(_)
    ));
    assert_eq!(click(&terminal, 2, 0), None);
    assert_eq!(click(&terminal, 4, 0), Some(PromptClick::Left(1)));
    assert_eq!(replaced(&mut terminal, (3, 3), 0), None);
    assert_eq!(replaced(&mut terminal, (4, 4), 0), Some((None, 1)));
}
