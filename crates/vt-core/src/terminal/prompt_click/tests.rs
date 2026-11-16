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
