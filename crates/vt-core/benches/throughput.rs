//! The throughput of the parser alone, of the terminal that it drives, on
//! generated text, plain ASCII, colored output, Japanese text with emoji
//! and the cursor moved for every character, and of the grid's own
//! operations: scrolling at the scrollback's limit, and writing rows.

use std::hint::black_box;

use criterion::{BatchSize, Criterion, Throughput, criterion_group, criterion_main};
use lokyr_vt_core::page::{Capacity, Cell, PageList, Scrollback};
use lokyr_vt_core::parser::{Handler, Parser};
use lokyr_vt_core::terminal::{Options, Terminal};

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

/// A letter at every cell of a screen of 120 by 40, the cursor moved to
/// each, as editors redraw.
fn moves() -> Vec<u8> {
    let mut screen = Vec::new();
    for row in 1..=40 {
        for column in 1..=120 {
            screen.extend_from_slice(format!("\x1b[{row};{column}Hx").as_bytes());
        }
    }
    repeated(&screen)
}

fn inputs() -> [(&'static str, Vec<u8>); 4] {
    [
        ("plain", plain()),
        ("colored", colored()),
        ("unicode", unicode()),
        ("moves", moves()),
    ]
}

/// A handler that does nothing, so that the parser alone is timed.
struct Nothing;

impl Handler for Nothing {}

fn parser(c: &mut Criterion) {
    let mut group = c.benchmark_group("parser");
    for (name, input) in inputs() {
        group.throughput(Throughput::Bytes(input.len() as u64));
        group.bench_function(name, |b| {
            let mut parser = Parser::new();
            b.iter(|| parser.advance(&mut Nothing, black_box(&input)));
        });
    }
    group.finish();
}

fn grid(c: &mut Criterion) {
    let mut group = c.benchmark_group("grid");
    let rows = 100_000;
    group.throughput(Throughput::Elements(rows));
    // A list at its limit of four pages, which scrolling recycles.
    let limit = 4 * Capacity::standard(120).bytes();
    let full = || {
        let mut list = PageList::new(
            120,
            40,
            Scrollback {
                limit,
                budget: None,
            },
        );
        for _ in 0..5 * Capacity::standard(120).rows {
            list.scroll_up();
        }
        list
    };
    group.bench_function("scroll", |b| {
        b.iter_batched_ref(
            full,
            |list| {
                for _ in 0..rows {
                    list.scroll_up();
                }
            },
            BatchSize::LargeInput,
        );
    });
    // A row of text written into the bottom row, which then scrolls.
    let text = [b'x'; 120];
    group.bench_function("write", |b| {
        b.iter_batched_ref(
            full,
            |list| {
                for _ in 0..rows {
                    let (page, row) = list.screen_row_mut(39);
                    page.write_ascii(row, 0, black_box(&text), Cell::EMPTY);
                    list.scroll_up();
                }
            },
            BatchSize::LargeInput,
        );
    });
    group.finish();
}

fn throughput(c: &mut Criterion) {
    let mut group = c.benchmark_group("terminal");
    for (name, input) in inputs() {
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

criterion_group!(benches, parser, throughput, grid);
criterion_main!(benches);
