//! The time that reflow takes, over scrollbacks of several sizes: it should
//! grow with the number of rows, and no faster.

use std::hint::black_box;

use criterion::{BatchSize, BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use saetta_vt_core::page::Scrollback;
use saetta_vt_core::parser::Parser;
use saetta_vt_core::terminal::{Options, Terminal};

/// A terminal of 120 columns whose scrollback holds `lines` lines of
/// colored text, each a little shorter than a row.
fn filled(lines: usize) -> Terminal {
    let options = Options {
        scrollback: Scrollback {
            limit: 1 << 30,
            budget: None,
        },
        ..Options::default()
    };
    let mut terminal = Terminal::new(120, 40, options);
    let line = "The quick \x1b[31mbrown fox\x1b[m jumps over the lazy dog, \u{4E2D}\u{6587} and e\u{301}, 0123456789 The quick brown fox jumps\r\n";
    Parser::new().advance(&mut terminal, line.repeat(lines).as_bytes());
    terminal
}

fn reflow(c: &mut Criterion) {
    let mut group = c.benchmark_group("reflow");
    group.sample_size(20);
    for lines in [1_000, 10_000, 50_000] {
        group.throughput(Throughput::Elements(lines as u64));
        group.bench_with_input(
            BenchmarkId::new("120 to 80 columns", lines),
            &lines,
            |b, &lines| {
                b.iter_batched_ref(
                    || filled(lines),
                    |terminal| terminal.resize(black_box(80), 40),
                    BatchSize::LargeInput,
                );
            },
        );
    }
    group.finish();
}

criterion_group!(benches, reflow);
criterion_main!(benches);
