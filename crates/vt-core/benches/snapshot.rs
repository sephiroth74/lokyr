//! The time of a snapshot's update after a scroll of a row, of a view in a
//! scrollback of about 10 000 rows of commands, which a renderer takes at
//! each frame of a scroll: it costs the view's rows, whatever the
//! scrollback.

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use lokyr_vt_core::page::Scrollback;
use lokyr_vt_core::page::Underline;
use lokyr_vt_core::parser::Parser;
use lokyr_vt_core::terminal::{
    Look, Options, Scroll, SelectionShape, SelectionUnit, Snapshot, Terminal,
};

/// A terminal of 120 columns and 40 rows whose scrollback holds 100
/// commands of the shell's marks of 100 rows of output each, then the
/// prompt, its view scrolled half way up.
pub fn commands() -> Terminal {
    let options = Options {
        scrollback: Scrollback {
            limit: 1 << 30,
            budget: None,
        },
        ..Options::default()
    };
    let mut terminal = Terminal::new(120, 40, options);
    let mut text = String::new();
    for command in 0..100 {
        text.push_str(&format!(
            "\x1b]133;A\x07$ \x1b]133;B\x07run {command}\r\n\x1b]133;C\x07"
        ));
        for row in 0..100 {
            text.push_str(&format!(
                "output {row} of command {command}, a line of some width\r\n"
            ));
        }
        text.push_str("\x1b]133;D;0\x07");
    }
    text.push_str("\x1b]133;A\x07$ \x1b]133;B\x07");
    Parser::new().advance(&mut terminal, text.as_bytes());
    terminal.scroll_viewport(Scroll::Up(5_000));
    terminal
}

fn snapshot(c: &mut Criterion) {
    let mut terminal = commands();
    let mut shown = Snapshot::default();
    terminal.snapshot(&mut shown);
    let mut up = true;
    c.bench_function("snapshot after a scroll of a row, of no fold", |b| {
        b.iter(|| {
            terminal.scroll_viewport(if up { Scroll::Up(1) } else { Scroll::Down(1) });
            up = !up;
            black_box(terminal.snapshot(&mut shown))
        });
    });
}

/// `terminal`, of a selection across its view, a decoration on each of its
/// rows and a search's matches shown, ten a row, the view's and those
/// around it, as a find bar and hints show them.
fn marked(mut terminal: Terminal) -> Terminal {
    let top = terminal.origin().top;
    terminal.select(
        (3, top + 2),
        (40, top + 30),
        SelectionShape::Cells,
        SelectionUnit::Cell,
        "",
    );
    let look = Look {
        underline: Underline::Single,
        underline_color: None,
        background: None,
        pointing: true,
        label: None,
    };
    for (id, row) in (1..).zip(top - 20..top + 60) {
        terminal.decorate(id, (0, row), (5, row), look);
    }
    let mut found = Vec::new();
    for row in top - 20..top + 60 {
        for column in (0..50).step_by(5) {
            let (Some(start), Some(end)) = (
                terminal.pin_cell((column, row)),
                terminal.pin_cell((column + 2, row)),
            ) else {
                continue;
            };
            found.push((start, end));
        }
    }
    terminal.add_matches(1, 0, found);
    terminal.show_matches(1, true, None);
    terminal
}

fn snapshot_marked(c: &mut Criterion) {
    let mut terminal = marked(commands());
    let mut shown = Snapshot::default();
    terminal.snapshot(&mut shown);
    let mut up = true;
    c.bench_function(
        "snapshot after a scroll of a row, of a selection, decorations and matches",
        |b| {
            b.iter(|| {
                terminal.scroll_viewport(if up { Scroll::Up(1) } else { Scroll::Down(1) });
                up = !up;
                black_box(terminal.snapshot(&mut shown))
            });
        },
    );
}

criterion_group!(benches, snapshot, snapshot_marked);
criterion_main!(benches);
