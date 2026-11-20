//! The prompt rows of the block whose output the view's top shows, pinned
//! over the view's first rows.

use crate::page::{Content, Scrollback};
use crate::parser::Parser;
use crate::terminal::{Options, Scroll, Snapshot, Terminal};

fn feed(terminal: &mut Terminal, text: &str) {
    Parser::new().advance(terminal, text.as_bytes());
}

/// A command of a prompt of `prompt_rows` rows, the last `$ `, its line
/// `line` and its output `lines` rows of numbers from 1, ended.
fn command(prompt_rows: usize, line: &str, lines: usize) -> String {
    let mut text = String::from("\x1b]133;A\x07");
    for row in 1..prompt_rows {
        text.push_str(&format!("user {row} ~/dir\r\n"));
    }
    text.push_str(&format!("$ \x1b]133;B\x07{line}\r\n\x1b]133;C\x07"));
    for n in 1..=lines {
        text.push_str(&format!("{line} {n}\r\n"));
    }
    text.push_str("\x1b]133;D;0\x07");
    text
}

/// The text of `snapshot`'s line `y`, without the spaces that end it.
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

/// The pinned rows' texts of `terminal`'s snapshot updated now.
fn pinned(terminal: &mut Terminal, snapshot: &mut Snapshot) -> Vec<String> {
    terminal.snapshot(snapshot);
    snapshot.sticky().map_or_else(Vec::new, |sticky| {
        (0..sticky.rows()).map(|y| text(sticky, y)).collect()
    })
}

/// A terminal of 40 columns and 10 rows of `ls`, of a prompt of two rows
/// and 50 rows of output, then the prompt where the user types; the
/// snapshots pin the prompt rows.
fn terminal() -> Terminal {
    let options = Options {
        scrollback: Scrollback {
            limit: 1 << 30,
            budget: None,
        },
        ..Options::default()
    };
    let mut terminal = Terminal::new(40, 10, options);
    feed(&mut terminal, &command(2, "ls", 50));
    feed(&mut terminal, "\x1b]133;A\x07$ \x1b]133;B\x07");
    terminal.set_sticky_prompts(true);
    terminal
}

#[test]
fn a_block_s_prompt_rows_pin_while_its_output_fills_the_view_s_top() {
    let mut terminal = terminal();
    let mut snapshot = Snapshot::default();
    let ls = terminal.commands()[0].clone();
    let prompt = ls.prompt.unwrap().1;
    let output = ls.output.unwrap().1;
    // At the screen, `ls`' output fills it: its prompt's two rows pinned.
    assert_eq!(
        pinned(&mut terminal, &mut snapshot),
        ["user 1 ~/dir", "$ ls"]
    );
    assert_eq!(snapshot.sticky_prompt(), Some((ls.id, prompt)));
    // Up, the output's rows at the top, the same rows.
    terminal.scroll_viewport(Scroll::Row(output + 5));
    assert_eq!(
        pinned(&mut terminal, &mut snapshot),
        ["user 1 ~/dir", "$ ls"]
    );
    assert_eq!(text(&snapshot, 0), "ls 6");
    terminal.scroll_viewport(Scroll::Row(output));
    assert_eq!(pinned(&mut terminal, &mut snapshot).len(), 2);
    // The prompt's first row gone, the command's line at the top: pinned
    // already, over it, as a sticky header.
    terminal.scroll_viewport(Scroll::Row(output - 1));
    assert_eq!(
        pinned(&mut terminal, &mut snapshot),
        ["user 1 ~/dir", "$ ls"]
    );
    // The prompt's first row at the top pins nothing.
    terminal.scroll_viewport(Scroll::Row(prompt));
    assert!(pinned(&mut terminal, &mut snapshot).is_empty());
    assert_eq!(snapshot.sticky_prompt(), None);
    // Not asked for, none; nor on the alternate screen.
    terminal.scroll_viewport(Scroll::Row(output + 5));
    terminal.set_sticky_prompts(false);
    assert!(pinned(&mut terminal, &mut snapshot).is_empty());
    terminal.set_sticky_prompts(true);
    assert_eq!(pinned(&mut terminal, &mut snapshot).len(), 2);
    feed(&mut terminal, "\x1b[?1049h");
    assert!(pinned(&mut terminal, &mut snapshot).is_empty());
    feed(&mut terminal, "\x1b[?1049l");
    // RIS keeps the embedder's ask.
    feed(&mut terminal, "\x1bc");
    feed(&mut terminal, &command(1, "seq", 30));
    feed(&mut terminal, "\x1b]133;A\x07$ \x1b]133;B\x07");
    assert_eq!(pinned(&mut terminal, &mut snapshot), ["$ seq"]);
}

#[test]
fn a_long_prompt_pins_its_first_three_rows_and_a_short_view_fewer() {
    let mut terminal = Terminal::new(40, 10, Options::default());
    feed(&mut terminal, &command(5, "make", 30));
    feed(&mut terminal, "\x1b]133;A\x07$ \x1b]133;B\x07");
    terminal.set_sticky_prompts(true);
    let mut snapshot = Snapshot::default();
    assert_eq!(
        pinned(&mut terminal, &mut snapshot),
        ["user 1 ~/dir", "user 2 ~/dir", "user 3 ~/dir"]
    );
    // A view of three rows pins two, so that one row shows.
    terminal.resize(40, 3);
    assert_eq!(pinned(&mut terminal, &mut snapshot).len(), 2);
}

#[test]
fn the_block_s_end_pushes_its_pinned_rows_up_off_the_next_block() {
    let mut terminal = terminal();
    feed(&mut terminal, "seq\r\n\x1b]133;C\x07");
    for n in 1..=20 {
        feed(&mut terminal, &format!("seq {n}\r\n"));
    }
    feed(
        &mut terminal,
        "\x1b]133;D;0\x07\x1b]133;A\x07$ \x1b]133;B\x07",
    );
    let mut snapshot = Snapshot::default();
    let ls = terminal.commands()[0].clone();
    let prompt = ls.prompt.unwrap().1;
    let end = ls.end.unwrap().1;
    // The block's last two rows at the top: both rows pinned.
    terminal.scroll_viewport(Scroll::Row(end - 2));
    assert_eq!(
        pinned(&mut terminal, &mut snapshot),
        ["user 1 ~/dir", "$ ls"]
    );
    // Its last row alone: the command's line alone, which the next
    // block's prompt shows under.
    terminal.scroll_viewport(Scroll::Row(end - 1));
    assert_eq!(pinned(&mut terminal, &mut snapshot), ["$ ls"]);
    assert_eq!(snapshot.sticky_prompt(), Some((ls.id, prompt)));
    assert_eq!(text(&snapshot, 0), "ls 50");
    assert_eq!(text(&snapshot, 1), "$ seq");
    // The next block's prompt at the top pins nothing.
    terminal.scroll_viewport(Scroll::Row(end));
    assert!(pinned(&mut terminal, &mut snapshot).is_empty());
}

#[test]
fn a_command_that_did_not_end_ends_its_pinned_rows_at_the_next_prompt() {
    let options = Options {
        scrollback: Scrollback {
            limit: 1 << 30,
            budget: None,
        },
        ..Options::default()
    };
    let mut terminal = Terminal::new(40, 10, options);
    // `make` of no end's mark, then `seq`, then the prompt.
    feed(
        &mut terminal,
        "\x1b]133;A\x07user ~/dir\r\n$ \x1b]133;B\x07make\r\n\x1b]133;C\x07",
    );
    for n in 1..=30 {
        feed(&mut terminal, &format!("make {n}\r\n"));
    }
    feed(&mut terminal, &command(1, "seq", 20));
    feed(&mut terminal, "\x1b]133;A\x07$ \x1b]133;B\x07");
    terminal.set_sticky_prompts(true);
    let mut snapshot = Snapshot::default();
    let next = terminal.commands()[1].prompt.unwrap().1;
    terminal.scroll_viewport(Scroll::Row(next - 2));
    assert_eq!(
        pinned(&mut terminal, &mut snapshot),
        ["user ~/dir", "$ make"]
    );
    terminal.scroll_viewport(Scroll::Row(next - 1));
    assert_eq!(pinned(&mut terminal, &mut snapshot), ["$ make"]);
    assert_eq!(text(&snapshot, 1), "$ seq");
}

#[test]
fn a_running_command_s_rows_pin_only_as_the_view_leaves_the_screen() {
    let options = Options {
        scrollback: Scrollback {
            limit: 1 << 30,
            budget: None,
        },
        ..Options::default()
    };
    let mut terminal = Terminal::new(40, 10, options);
    // `git log` that runs, its pager on the primary screen at the screen's
    // top, of no end's mark yet.
    feed(
        &mut terminal,
        "\x1b]133;A\x07$ \x1b]133;B\x07git log\r\n\x1b]133;C\x07",
    );
    for n in 1..=30 {
        feed(&mut terminal, &format!("commit {n}\r\n"));
    }
    feed(&mut terminal, "\x1b[H\x1b[2Jcommit 1\r\nAuthor\r\n");
    terminal.set_sticky_prompts(true);
    let mut snapshot = Snapshot::default();
    // At the screen, what the pager draws stays in sight.
    assert!(pinned(&mut terminal, &mut snapshot).is_empty());
    assert_eq!(text(&snapshot, 0), "commit 1");
    // Scrolled into the scrollback, its output: its rows pinned.
    let output = terminal.commands()[0].output.unwrap().1;
    terminal.scroll_viewport(Scroll::Row(output + 3));
    assert_eq!(pinned(&mut terminal, &mut snapshot), ["$ git log"]);
    // Ended, at the screen too, the view's top of its output.
    terminal.scroll_viewport(Scroll::Bottom);
    feed(&mut terminal, "\x1b]133;D;0\x07");
    assert_eq!(pinned(&mut terminal, &mut snapshot), ["$ git log"]);
}
