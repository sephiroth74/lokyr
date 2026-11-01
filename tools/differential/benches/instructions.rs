//! The instructions that the VT core takes for vtebench's scenarios, for the
//! test of kitty's benchmark of CSI sequences between few characters, and
//! for text as `cat` prints it, as callgrind counts them through gungraun:
//! one sample of each, fed in reads of 64 KiB, after the reset and the
//! setup, which the count leaves out. Valgrind runs on Linux only.
//!
//! The scenarios read vtebench's data files from the checkout that
//! `LOKYR_VTEBENCH` names, or else from `target/vtebench`, which `just
//! vtebench` fetches.

#![expect(
    missing_docs,
    reason = "gungraun's macros make public items without docs"
)]

use std::hint::black_box;
use std::path::PathBuf;

use gungraun::prelude::*;
use lokyr_differential::kitty;
use lokyr_differential::vtebench::{self, COLS, READ, ROWS};
use lokyr_vt_core::parser::Parser;
use lokyr_vt_core::terminal::{Options, Terminal};

/// A terminal as a sample finds it, with its parser, and the sample.
struct Prepared {
    parser: Parser,
    terminal: Terminal,
    sample: Vec<u8>,
}

/// The scenario called `name`, the terminal reset and the setup fed, and
/// a sample of it to feed; `cat` is text as `cat` prints it, and
/// `kitty_csi` a repetition of kitty's test of CSI sequences.
fn prepared(name: &str) -> Prepared {
    let (setup, sample) = if name == "cat" {
        (Vec::new(), cat())
    } else if name == "kitty_csi" {
        let test = kitty::tests()
            .into_iter()
            .find(|test| test.name == "CSI codes with few chars")
            .expect("kitty's test of CSI sequences");
        (test.setup, test.payload)
    } else {
        let dir = std::env::var_os("LOKYR_VTEBENCH").map_or_else(
            || PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/vtebench"),
            PathBuf::from,
        );
        let scenario = vtebench::scenarios(&dir)
            .expect("vtebench's checkout, which `just vtebench` fetches")
            .into_iter()
            .find(|scenario| scenario.name == name)
            .expect("a scenario of vtebench");
        (scenario.setup, vtebench::samples(&scenario.payload))
    };
    let mut prepared = Prepared {
        parser: Parser::new(),
        terminal: Terminal::new(COLS as u16, ROWS as u16, Options::default()),
        sample,
    };
    feed(&mut prepared, b"\x1bc");
    feed(&mut prepared, &setup);
    prepared
}

/// Text as `cat` prints it, 1 MiB of it, the same every time: lines of
/// words of one to twelve letters, of a length that varies from line to
/// line, each line ended by CR LF.
fn cat() -> Vec<u8> {
    let mut text = Vec::with_capacity(vtebench::MIN_BYTES + 128);
    // A linear congruential generator, so that the text does not change.
    let mut state: u32 = 1;
    let mut next = |below: u32| {
        state = state.wrapping_mul(1_103_515_245).wrapping_add(12_345);
        (state >> 16) % below
    };
    while text.len() < vtebench::MIN_BYTES {
        let words = next(16);
        for word in 0..words {
            if word > 0 {
                text.push(b' ');
            }
            for _ in 0..=next(12) {
                text.push(b'a' + next(26) as u8);
            }
        }
        text.extend_from_slice(b"\r\n");
    }
    text
}

/// Feeds `bytes` to the prepared terminal in reads of 64 KiB.
fn feed(prepared: &mut Prepared, bytes: &[u8]) {
    for read in bytes.chunks(READ) {
        prepared.parser.advance(&mut prepared.terminal, read);
    }
}

#[library_benchmark]
#[bench::cursor_motion(args = ("cursor_motion"), setup = prepared)]
#[bench::dense_cells(args = ("dense_cells"), setup = prepared)]
#[bench::light_cells(args = ("light_cells"), setup = prepared)]
#[bench::medium_cells(args = ("medium_cells"), setup = prepared)]
#[bench::scrolling(args = ("scrolling"), setup = prepared)]
#[bench::scrolling_bottom_region(args = ("scrolling_bottom_region"), setup = prepared)]
#[bench::scrolling_bottom_small_region(args = ("scrolling_bottom_small_region"), setup = prepared)]
#[bench::scrolling_fullscreen(args = ("scrolling_fullscreen"), setup = prepared)]
#[bench::scrolling_top_region(args = ("scrolling_top_region"), setup = prepared)]
#[bench::scrolling_top_small_region(args = ("scrolling_top_small_region"), setup = prepared)]
#[bench::sync_medium_cells(args = ("sync_medium_cells"), setup = prepared)]
#[bench::unicode(args = ("unicode"), setup = prepared)]
#[bench::kitty_csi(args = ("kitty_csi"), setup = prepared)]
#[bench::cat(args = ("cat"), setup = prepared)]
fn sample(mut prepared: Prepared) -> Prepared {
    let sample = std::mem::take(&mut prepared.sample);
    feed(&mut prepared, black_box(&sample));
    prepared.sample = sample;
    prepared
}

library_benchmark_group!(name = scenarios, benchmarks = [sample]);
main!(library_benchmark_groups = scenarios);
