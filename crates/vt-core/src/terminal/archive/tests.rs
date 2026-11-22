//! A pane's archive read back: the same rows, styles, wraps, links and
//! commands at the same width, the same lines at another, the newest rows
//! within its bound, and the prompt where the user types left out.

use std::time::Instant;

use super::*;
use crate::parser::Parser;
use crate::terminal::{Options, RowCells};

/// A terminal of `cols` columns and `rows` rows fed `bytes`.
fn fed(cols: u16, rows: u16, bytes: &[u8]) -> Terminal {
    let mut terminal = Terminal::new(cols, rows, Options::default());
    Parser::new().advance(&mut terminal, bytes);
    terminal
}

/// `terminal`'s archive read back by a terminal of `cols` columns and as
/// many rows.
fn restored(terminal: &Terminal, cols: u16) -> Terminal {
    fed(cols, terminal.rows, &terminal.archive(usize::MAX))
}

/// The cells of `terminal`'s rows, from its scrollback's first, as many as
/// `count`.
fn cells(terminal: &Terminal, count: usize) -> Vec<RowCells> {
    terminal
        .row_cells(terminal.row_number(0), count)
        .unwrap_or_default()
}

/// The logical lines of `terminal`'s rows, from its scrollback's first,
/// their soft-wrapped rows joined, without the empty ones that end them.
fn lines(terminal: &Terminal) -> Vec<String> {
    let total = terminal.primary.rows.total_rows();
    let mut lines = vec![String::new()];
    for row in terminal.read_rows(terminal.row_number(0), total) {
        lines.last_mut().unwrap().push_str(&row.text);
        if !row.wrapped {
            lines.push(String::new());
        }
    }
    while lines.last().is_some_and(String::is_empty) {
        lines.pop();
    }
    lines
}

/// The commands that `terminal` keeps: each mark's column and the index of
/// its row in the list, its status, its line and its folder.
type Kept = (
    [Option<(u16, usize)>; 4],
    Option<i32>,
    Option<String>,
    Option<String>,
);

fn commands(terminal: &Terminal) -> Vec<Kept> {
    let index = |mark: Option<(u16, i64)>| {
        let (col, row) = mark?;
        Some((col, terminal.row_index(row)?))
    };
    terminal
        .commands()
        .into_iter()
        .map(|command| {
            (
                [
                    index(command.prompt),
                    index(command.input),
                    index(command.output),
                    index(command.end),
                ],
                command.status,
                terminal.command_line(command.id),
                command.folder.map(|folder| folder.path.clone()),
            )
        })
        .collect()
}

/// A session of two commands, a failed `ls` of styles, wide characters,
/// a cluster, a link, a line that wraps and a background that an erase
/// left, and an `echo` that succeeded, then the prompt where the user
/// types.
fn session() -> Vec<u8> {
    [
        "\x1b]7;file://host/home/me\x1b\\",
        "\x1b]133;A\x1b\\$ \x1b]133;B\x1b\\ls\x1b]133;C;cmdline_url=ls\x1b\\\r\n",
        "\x1b[1;31mred bold\x1b[0m and \x1b[4:3;38;2;10;20;30mcurly\x1b[0m\r\n",
        "wide 日本 👍🏽 end\r\n",
        "\x1b]8;;https://lokyr.dev\x1b\\link\x1b]8;;\x1b\\ after\r\n",
        "a long line that wraps over more than one row of twenty columns\r\n",
        "\x1b[44m  \x1b[K\x1b[0m\r\n",
        "\x1b]133;D;1\x1b\\",
        "\x1b]133;A\x1b\\$ \x1b]133;B\x1b\\echo hi\x1b]133;C;cmdline_url=echo%20hi\x1b\\\r\nhi\r\n",
        "\x1b]133;D;0\x1b\\",
        "\x1b]133;A\x1b\\$ \x1b]133;B\x1b\\",
    ]
    .concat()
    .into_bytes()
}

#[test]
fn the_archive_read_back_at_the_same_width_holds_the_same_rows() {
    let terminal = fed(20, 6, &session());
    let restored = restored(&terminal, 20);
    // The rows up to the prompt where the user types, which is left out.
    let prompt = terminal.live_prompt_index().unwrap();
    assert_eq!(cells(&restored, prompt), cells(&terminal, prompt));
    assert_eq!(lines(&restored).last().map(String::as_str), Some("hi"));
    assert_eq!(
        restored.link_at(0, restored.row_number(3)).as_deref(),
        Some("https://lokyr.dev")
    );
    assert_eq!(restored.link_at(5, restored.row_number(3)), None);
}

#[test]
fn an_archive_read_back_archives_the_same_bytes() {
    let terminal = fed(20, 6, &session());
    let archive = terminal.archive(usize::MAX);
    let restored = fed(20, 6, &archive);
    assert_eq!(
        String::from_utf8_lossy(&restored.archive(usize::MAX)),
        String::from_utf8_lossy(&archive)
    );
}

#[test]
fn the_commands_come_back_of_their_marks_status_line_folder_and_times() {
    let terminal = fed(20, 6, &session());
    let restored = restored(&terminal, 20);
    let kept = commands(&terminal);
    // The prompt where the user types is left out, of no command; the
    // `echo`'s end, marked on its row, comes back.
    assert_eq!(commands(&restored), kept[..2]);
    assert_eq!(kept[1].1, Some(0));
    for (was, is) in terminal.commands().iter().zip(restored.commands()) {
        let millis = |time: Option<std::time::SystemTime>| {
            time.map(|time| {
                time.duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_millis()
            })
        };
        assert_eq!(millis(is.started), millis(was.started));
        assert_eq!(millis(is.ended), millis(was.ended));
    }
}

#[test]
fn the_archive_reflows_at_another_width() {
    let terminal = fed(20, 6, &session());
    let restored = restored(&terminal, 33);
    let mut kept = lines(&terminal);
    // The prompt where the user types is left out.
    kept.pop();
    assert_eq!(lines(&restored), kept);
    let statuses = |terminal: &Terminal| {
        commands(terminal)
            .into_iter()
            .map(|(_, status, line, _)| (status, line))
            .collect::<Vec<_>>()
    };
    assert_eq!(statuses(&restored), statuses(&terminal)[..2]);
}

#[test]
fn the_archive_keeps_the_newest_rows_within_its_bound() {
    let text: String = (1..=200).map(|n| format!("line {n}\r\n")).collect();
    let terminal = fed(20, 6, text.as_bytes());
    let archive = terminal.archive(500);
    // The pen's reset after the rows.
    assert!(archive.len() <= 500 + 4, "{}", archive.len());
    let restored = fed(20, 6, &archive);
    let lines = lines(&restored);
    assert_eq!(lines.last().map(String::as_str), Some("line 200"));
    assert!(lines.len() < 200 && lines.len() > 20, "{}", lines.len());
}

#[test]
fn a_screen_of_no_text_archives_nothing() {
    let terminal = Terminal::new(20, 6, Options::default());
    assert!(terminal.archive(usize::MAX).is_empty());
    // A prompt alone, where the user types, which a new shell draws again.
    let terminal = fed(20, 6, b"\x1b]133;A\x1b\\$ \x1b]133;B\x1b\\");
    assert!(terminal.archive(usize::MAX).is_empty());
}

#[test]
fn the_blank_rows_before_the_prompts_come_back_once() {
    let mut terminal = Terminal::new(20, 6, Options::default());
    terminal.set_block_spacing(true);
    Parser::new().advance(&mut terminal, &session());
    let mut restored = Terminal::new(20, 6, Options::default());
    restored.set_block_spacing(true);
    let archive = terminal.archive(usize::MAX);
    Parser::new().advance(&mut restored, &archive);
    let prompt = terminal.live_prompt_index().unwrap();
    assert_eq!(cells(&restored, prompt), cells(&terminal, prompt));
    // The prompt's blank row kept, as its mark tells it.
    assert!(String::from_utf8_lossy(&archive).contains("lokyr_spaced"));
    assert_eq!(
        String::from_utf8_lossy(&restored.archive(usize::MAX)),
        String::from_utf8_lossy(&archive)
    );
}

#[test]
fn a_wrapped_line_s_background_stays_within_its_text() {
    // Twenty-five cells of a blue background over twenty columns, the
    // second row's last fifteen empty, then a plain line.
    let bytes = format!("\x1b[44m{}\x1b[0m\r\nplain\r\n", "x".repeat(25));
    let terminal = fed(20, 6, bytes.as_bytes());
    let restored = restored(&terminal, 20);
    assert_eq!(cells(&restored, 3), cells(&terminal, 3));
}

#[test]
fn the_alternate_screen_is_not_archived() {
    let terminal = fed(20, 6, b"on the primary\r\n\x1b[?1049hon the alternate");
    let archive = String::from_utf8_lossy(&terminal.archive(usize::MAX)).into_owned();
    assert!(archive.contains("on the primary"));
    assert!(!archive.contains("alternate"));
}

/// The time of the archive of a full pane, of the default scrollback's
/// limit of 10 MiB of pages, of a styled text of 120 columns, and of its
/// reading back, as `cargo test --release -p lokyr-vt-core
/// archive_of_a_full_pane -- --ignored --nocapture` prints them.
#[test]
#[ignore = "a measure, of a release build"]
fn archive_of_a_full_pane() {
    let mut terminal = Terminal::new(120, 40, Options::default());
    let line = "\x1b[32mgreen\x1b[0m plain \x1b[1mbold\x1b[0m ".repeat(5);
    let mut parser = Parser::new();
    for n in 0..100_000 {
        parser.advance(&mut terminal, format!("{n:06} {line}\r\n").as_bytes());
    }
    let start = Instant::now();
    let archive = terminal.archive(usize::MAX);
    let took = start.elapsed();
    let start = Instant::now();
    let restored = fed(120, 40, &archive);
    let read = start.elapsed();
    println!(
        "archive of {} rows: {:.2} MiB in {took:?}; read back in {read:?}, {} rows",
        terminal.primary.rows.total_rows(),
        archive.len() as f64 / f64::from(1 << 20),
        restored.primary.rows.total_rows()
    );
}
