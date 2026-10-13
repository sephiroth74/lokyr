//! Feeds arbitrary bytes to the escape-sequence parser.
//!
//! The action recorder panics when the parser breaks the protocol of control
//! strings. The target also checks that splitting the input into reads
//! changes no action, with the default size limits and with small ones.

#![no_main]

use libfuzzer_sys::fuzz_target;
use saetta_vt_core::parser::{Action, Limits, Parser, Recorder};

/// The lengths of the reads that the input is split into, taken in turn.
const READS: [usize; 5] = [1, 7, 2, 13, 3];

/// Small enough for the strings in the input to pass them.
fn tight_limits() -> Limits {
    Limits {
        osc: 8,
        osc_commands: vec![(52, 16)],
        dcs: 8,
        apc: 8,
    }
}

fn record<'a>(limits: &Limits, reads: impl Iterator<Item = &'a [u8]>) -> Vec<Action> {
    let mut parser = Parser::with_limits(limits.clone());
    let mut recorder = Recorder::new();
    for read in reads {
        parser.advance(&mut recorder, read);
    }
    recorder.into_actions()
}

/// `data` in reads of the lengths in [`READS`].
fn split(data: &[u8]) -> impl Iterator<Item = &[u8]> {
    let mut rest = data;
    READS.iter().cycle().map_while(move |&len| {
        if rest.is_empty() {
            return None;
        }
        let (read, after) = rest.split_at(len.min(rest.len()));
        rest = after;
        Some(read)
    })
}

fuzz_target!(|data: &[u8]| {
    for limits in [Limits::default(), tight_limits()] {
        let whole = record(&limits, std::iter::once(data));
        assert_eq!(record(&limits, split(data)), whole);
        assert_eq!(record(&limits, data.chunks(1)), whole);
    }
});
