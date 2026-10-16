//! The throughput of the terminal, driven by the parser, on generated text:
//! plain ASCII, colored output, and Japanese text with emoji.

use std::hint::black_box;

use bolt_vt_core::parser::Parser;
use bolt_vt_core::terminal::{Options, Terminal};
use criterion::{BatchSize, Criterion, Throughput, criterion_group, criterion_main};

/// About how many bytes each input has.
const SIZE: usize = 1024 * 1024;

/// `line`, repeated up to about [`SIZE`] bytes.
fn repeated(line: &[u8]) -> Vec<u8> {
    line.repeat(SIZE / line.len())
}

/// Lines of plain text, as `cat` prints them.
fn plain() -> Vec<u8> {
    repeated(b"The quick brown fox jumps over the lazy dog 0123456789 The quick brown fox jumps over the lazy dog 0123456789 \r\n")
}

/// File names in colors, as `ls --color` prints them: an SGR sequence
/// around every word.
fn colored() -> Vec<u8> {
    let words = [
        "src",
        "main.rs",
        "Cargo.toml",
        "README.md",
        "target",
        "docs",
    ];
    let mut line = Vec::new();
    for (color, word) in (1..).zip(words.iter().cycle().take(12)) {
        line.extend_from_slice(format!("\x1b[1;3{}m{word}\x1b[0m  ", color % 7 + 1).as_bytes());
    }
    line.extend_from_slice(b"\r\n");
    repeated(&line)
}

/// Japanese text with accents and emoji.
fn unicode() -> Vec<u8> {
    repeated("日本語のテキスト、そして é é 😀 日本語のテキスト、そして é é 😀\r\n".as_bytes())
}

fn throughput(c: &mut Criterion) {
    let mut group = c.benchmark_group("terminal");
    for (name, input) in [
        ("plain", plain()),
        ("colored", colored()),
        ("unicode", unicode()),
    ] {
        group.throughput(Throughput::Bytes(input.len() as u64));
        group.bench_function(name, |b| {
            b.iter_batched_ref(
                || (Parser::new(), Terminal::new(120, 40, Options::default())),
                |(parser, terminal)| parser.advance(terminal, black_box(&input)),
                BatchSize::LargeInput,
            );
        });
    }
    group.finish();
}

criterion_group!(benches, throughput);
criterion_main!(benches);
