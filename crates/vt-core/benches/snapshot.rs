//! The time of a snapshot's update after a scroll of a row, of a view in a
//! scrollback of about 10 000 rows of commands, which a renderer takes at
//! each frame of a scroll: it costs the view's rows, whatever the
//! scrollback.

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use lokyr_vt_core::page::Scrollback;
use lokyr_vt_core::parser::Parser;
use lokyr_vt_core::terminal::{Options, Scroll, Snapshot, Terminal};

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

criterion_group!(benches, snapshot);
criterion_main!(benches);
