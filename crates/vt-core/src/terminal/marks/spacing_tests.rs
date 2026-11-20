//! The blank row before each prompt that follows a command that ended, as
//! room between the commands' blocks.

use crate::page::Content;
use crate::parser::Parser;
use crate::terminal::{Options, Snapshot, Terminal};

fn feed(terminal: &mut Terminal, text: &str) {
    Parser::new().advance(terminal, text.as_bytes());
}

/// A prompt, `$ `, where the user types.
const PROMPT: &str = "\x1b]133;A\x07$ \x1b]133;B\x07";

/// The command `line` after its prompt, of its output `output`, ended.
fn command(line: &str, output: &str) -> String {
    format!("{PROMPT}{line}\r\n\x1b]133;C\x07{output}\x1b]133;D;0\x07")
}

/// The screen's rows' texts, without the spaces that end them, to the last
/// that holds text.
fn screen(terminal: &mut Terminal) -> Vec<String> {
    let mut snapshot = Snapshot::default();
    terminal.snapshot(&mut snapshot);
    let mut rows: Vec<String> = (0..snapshot.rows())
        .map(|y| {
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
        })
        .collect();
    while rows.last().is_some_and(String::is_empty) {
        rows.pop();
    }
    rows
}

/// The view's blocks, as `(first, last, room_above, room_below)`.
fn blocks(terminal: &mut Terminal) -> Vec<(i32, i32, bool, bool)> {
    let mut snapshot = Snapshot::default();
    terminal.snapshot(&mut snapshot);
    snapshot
        .blocks()
        .iter()
        .map(|block| (block.first, block.last, block.room_above, block.room_below))
        .collect()
}

fn spaced() -> Terminal {
    let mut terminal = Terminal::new(20, 10, Options::default());
    terminal.set_block_spacing(true);
    terminal
}

#[test]
fn a_blank_row_comes_before_each_prompt_after_a_command_that_ended() {
    let mut terminal = spaced();
    feed(&mut terminal, &command("ls", "a\r\nb\r\n"));
    feed(&mut terminal, &command("true", ""));
    feed(&mut terminal, PROMPT);
    // None before the first prompt; one before each other, of no block.
    assert_eq!(
        screen(&mut terminal),
        ["$ ls", "a", "b", "", "$ true", "", "$"]
    );
    assert_eq!(
        blocks(&mut terminal),
        [(0, 2, false, true), (4, 4, true, true), (6, 6, true, false)]
    );
    let prompts: Vec<i64> = terminal
        .commands()
        .iter()
        .map(|command| command.prompt.unwrap().1)
        .collect();
    assert_eq!(prompts, [0, 4, 6]);
    assert_eq!(terminal.command_at(3), None);
}

#[test]
fn no_blank_row_comes_of_an_empty_line_a_redrawn_prompt_or_a_cleared_screen() {
    let mut terminal = spaced();
    feed(&mut terminal, &command("ls", "a\r\n"));
    // A line of no command, which has no output's mark, and a prompt drawn
    // again at its place.
    feed(&mut terminal, PROMPT);
    feed(&mut terminal, &format!("\r\n{PROMPT}\r\x1b[K{PROMPT}"));
    assert_eq!(screen(&mut terminal), ["$ ls", "a", "", "$", "$"]);
    // A screen cleared, its cursor at the first cell.
    feed(
        &mut terminal,
        "clear\r\n\x1b]133;C\x07\x1b[H\x1b[2J\x1b]133;D;0\x07",
    );
    feed(&mut terminal, PROMPT);
    assert_eq!(screen(&mut terminal), ["$"]);
}

#[test]
fn an_output_of_no_line_end_ends_its_line_before_the_blank_row() {
    let mut terminal = spaced();
    feed(&mut terminal, &command("printf", "foo"));
    feed(&mut terminal, PROMPT);
    assert_eq!(screen(&mut terminal), ["$ printf", "foo", "", "$"]);
    assert_eq!(
        blocks(&mut terminal),
        [(0, 1, false, true), (3, 3, true, false)]
    );
}

#[test]
fn at_the_screen_s_bottom_the_blank_row_scrolls_the_screen() {
    let mut terminal = spaced();
    feed(
        &mut terminal,
        &command("seq", "1\r\n2\r\n3\r\n4\r\n5\r\n6\r\n7\r\n8\r\n"),
    );
    feed(&mut terminal, PROMPT);
    assert_eq!(
        screen(&mut terminal),
        // The command's line gone into the scrollback.
        ["1", "2", "3", "4", "5", "6", "7", "8", "", "$"]
    );
}

#[test]
fn off_or_on_the_alternate_screen_no_blank_row_comes_and_ris_keeps_the_ask() {
    let mut terminal = Terminal::new(20, 10, Options::default());
    feed(&mut terminal, &command("ls", "a\r\n"));
    feed(&mut terminal, PROMPT);
    assert_eq!(screen(&mut terminal), ["$ ls", "a", "$"]);
    let mut terminal = spaced();
    feed(&mut terminal, "\x1b[?1049h");
    feed(&mut terminal, &command("ls", "a\r\n"));
    feed(&mut terminal, PROMPT);
    assert_eq!(screen(&mut terminal), ["$ ls", "a", "$"]);
    feed(&mut terminal, "\x1b[?1049l\x1bc");
    feed(&mut terminal, &command("ls", "a\r\n"));
    feed(&mut terminal, PROMPT);
    assert_eq!(screen(&mut terminal), ["$ ls", "a", "", "$"]);
}
