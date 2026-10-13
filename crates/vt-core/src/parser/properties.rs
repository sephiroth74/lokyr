//! Properties of the parser on generated input: how the input is split into
//! reads changes nothing, and the parser agrees with `vte`.

use proptest::prelude::*;

use super::reference::{ours, reference};
use super::{Action, Limits, Parser, Recorder};

/// The cases that each property tries, in every run.
const CASES: u32 = 1024;

/// Bytes that start, continue and end sequences, so that the input is made
/// of sequences more than of noise.
const PIECES: &[&[u8]] = &[
    b"\x1b",
    b"\x1b[",
    b"\x1b]",
    b"\x1b]52;",
    b"\x1bP",
    b"\x1b_",
    b"\x1bX",
    b"\x1b^",
    b"\x1b\\",
    b"\x07",
    b"\x18",
    b"\x1a",
    b"\x7f",
    b";",
    b":",
    b"?",
    b">",
    b"<",
    b"=",
    b" ",
    b"!",
    b"\"",
    b"#",
    b"$",
    b"%",
    b"(",
    b"m",
    b"H",
    b"q",
    b"u",
    b"p",
    b"\r\n",
];

/// A piece of input: part of a sequence, a number, text, a C0 control, or a
/// character that takes several bytes in UTF-8.
fn piece() -> impl Strategy<Value = Vec<u8>> {
    prop_oneof![
        4 => prop::sample::select(PIECES).prop_map(<[u8]>::to_vec),
        2 => (0_u32..100_000).prop_map(|number| number.to_string().into_bytes()),
        2 => prop::collection::vec(0x20_u8..=0x7E, 1..8),
        1 => (0_u8..=0x1F).prop_map(|byte| vec![byte]),
        1 => wide_char().prop_map(|c| c.to_string().into_bytes()),
    ]
}

/// A piece of input, or any byte at all.
fn piece_or_byte() -> impl Strategy<Value = Vec<u8>> {
    prop_oneof![
        9 => piece(),
        1 => any::<u8>().prop_map(|byte| vec![byte]),
    ]
}

/// A character from U+00A0 up, which `vte` and the parser decode alike. The
/// C1 controls below it are left out, and so are the characters with a 0x9C
/// byte, where `vte` ends a DCS string.
fn wide_char() -> impl Strategy<Value = char> {
    prop_oneof![
        prop::char::range('\u{A0}', '\u{7FF}'),
        prop::char::range('\u{800}', '\u{D7FF}'),
        prop::char::range('\u{E000}', '\u{FFFF}'),
        prop::char::range('\u{10000}', char::MAX),
    ]
    .prop_filter("vte ends a DCS string at 0x9C", |c| {
        !c.to_string().as_bytes().contains(&0x9C)
    })
}

fn input(piece: impl Strategy<Value = Vec<u8>>) -> impl Strategy<Value = Vec<u8>> {
    prop::collection::vec(piece, 0..48).prop_map(|pieces| pieces.concat())
}

/// Small enough for generated strings to pass them.
fn tight_limits() -> Limits {
    Limits {
        osc: 8,
        osc_commands: vec![(52, 16)],
        dcs: 8,
        apc: 8,
    }
}

/// `input` in reads of the lengths in `lens`, taken in turn.
fn split<'a>(input: &'a [u8], lens: &[usize]) -> Vec<&'a [u8]> {
    let mut reads = Vec::new();
    let mut rest = input;
    for &len in lens.iter().cycle() {
        if rest.is_empty() {
            break;
        }
        let (read, after) = rest.split_at(len.min(rest.len()));
        reads.push(read);
        rest = after;
    }
    reads
}

fn record(limits: &Limits, reads: &[&[u8]]) -> Vec<Action> {
    let mut parser = Parser::with_limits(limits.clone());
    let mut recorder = Recorder::new();
    for read in reads {
        parser.advance(&mut recorder, read);
    }
    recorder.into_actions()
}

/// Keeps `input` within what `vte` keeps, 32 values per sequence and 16
/// parameters per OSC string, by turning the separators past the 15th into
/// digits.
fn within_vte_limits(mut input: Vec<u8>) -> Vec<u8> {
    let mut separators = 0;
    for byte in &mut input {
        if matches!(*byte, b';' | b':') {
            separators += 1;
            if separators > 15 {
                *byte = b'0';
            }
        }
    }
    input
}

/// Drops the bytes from 0x80 up from DCS payloads, as `vte` does.
fn ascii_dcs(actions: Vec<Action>) -> Vec<Action> {
    actions
        .into_iter()
        .map(|action| match action {
            Action::Dcs {
                params,
                intermediates,
                final_byte,
                mut payload,
                end,
            } => {
                payload.retain(u8::is_ascii);
                Action::Dcs {
                    params,
                    intermediates,
                    final_byte,
                    payload,
                    end,
                }
            }
            other => other,
        })
        .collect()
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(CASES))]

    #[test]
    fn splitting_the_input_into_reads_changes_nothing(
        input in input(piece_or_byte()),
        lens in prop::collection::vec(1_usize..16, 1..8),
        tight in any::<bool>(),
    ) {
        let limits = if tight { tight_limits() } else { Limits::default() };
        let whole = record(&limits, &[&input]);
        prop_assert_eq!(&record(&limits, &split(&input, &lens)), &whole);
        prop_assert_eq!(&record(&limits, &split(&input, &[1])), &whole);
    }

    #[test]
    fn the_parser_agrees_with_vte(input in input(piece())) {
        let input = within_vte_limits(input);
        prop_assert_eq!(ascii_dcs(ours(&input)), ascii_dcs(reference(&input)));
    }
}
