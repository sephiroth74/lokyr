//! Golden snapshots of the screen, from the recorded sessions of the VT
//! core: the text, the attributes and the cursor, at each marker of a
//! recording and at its end, `exit`, as `bolt-replay --attributes` prints
//! them, and the terminal's replies to the program since the point before.
//!
//! `cargo insta review` shows the snapshots that a change of the terminal,
//! or a recording made again, changes.

use std::path::PathBuf;

use bolt_replay::recording::Recording;
use bolt_replay::screen;
use bolt_vt_core::parser::Parser;
use bolt_vt_core::terminal::{Options, Terminal};

fn recordings() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../crates/vt-core/tests/recordings")
}

/// The screen as a snapshot keeps it: which screen and whether the cursor
/// shows, then what `bolt-replay --attributes` prints, then `replies`.
fn snapshot(terminal: &Terminal, replies: &[u8]) -> String {
    let screen = if terminal.is_alternate_screen() {
        "alternate"
    } else {
        "primary"
    };
    let cursor = if terminal.cursor_visible() {
        "shown"
    } else {
        "hidden"
    };
    let replies = if replies.is_empty() {
        "none".to_owned()
    } else {
        format!("{:?}", String::from_utf8_lossy(replies))
    };
    format!(
        "the {screen} screen, the cursor {cursor}\n{}replies: {replies}\n",
        screen::with_attributes(terminal)
    )
}

/// Replays the recording `name`, and checks the screen at each of its
/// markers and at its end against their snapshots.
fn golden(name: &str) {
    let bytes = std::fs::read(recordings().join(format!("{name}.cast"))).unwrap();
    let recording = Recording::parse(&bytes).unwrap();
    let (cols, rows) = recording.size.expect("a recording has its size");
    let mut terminal = Terminal::new(cols, rows, Options::default());
    let mut parser = Parser::new();
    let mut fed = 0;
    let mut points: Vec<(usize, &str)> = recording
        .marks
        .iter()
        .map(|(chunks, label)| (*chunks, label.as_str()))
        .collect();
    points.push((recording.chunks.len(), "exit"));
    let mut settings = insta::Settings::clone_current();
    settings.set_snapshot_path(recordings().join("snapshots"));
    settings.set_prepend_module_to_snapshot(false);
    settings.set_omit_expression(true);
    settings.bind(|| {
        for (chunks, label) in points {
            for chunk in &recording.chunks[fed..chunks] {
                parser.advance(&mut terminal, chunk);
            }
            fed = chunks;
            let replies = terminal.take_replies();
            insta::assert_snapshot!(format!("{name}@{label}"), snapshot(&terminal, &replies));
        }
    });
}

#[test]
fn vim() {
    golden("vim");
}

#[test]
fn htop() {
    golden("htop");
}

#[test]
fn tmux() {
    golden("tmux");
}

#[test]
fn less() {
    golden("less");
}

#[test]
fn fzf() {
    golden("fzf");
}
