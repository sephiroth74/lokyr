//! Feeds arbitrary bytes to the VT core and to `alacritty_terminal`, a byte
//! at a time, and compares their screens after each: the first point where
//! they part must have a known cause. The input is the one that
//! `lokyr_differential::fuzz_input` makes of the bytes, control sequences
//! and text; the `terminal` target feeds the VT core the bytes as they are.

#![no_main]

use libfuzzer_sys::fuzz_target;

/// A small screen, whose margins, wraps and scrolls an input reaches soon.
const COLS: u16 = 20;
const ROWS: u16 = 6;

fuzz_target!(|data: &[u8]| {
    let bytes = lokyr_differential::fuzz_input(data);
    if let Err(divergence) = lokyr_differential::explore(COLS, ROWS, &bytes) {
        panic!("{divergence}");
    }
});
