//! The parser against `vte`, which implements the same state machine: on
//! every transition, and on a corpus of sequences that programs send.
//!
//! The two disagree on purpose in a few places, which the comparison
//! accounts for:
//! - `vte` drops APC strings, and dispatches the `\` of ST as an escape
//!   sequence;
//! - it reports only whether BEL ended an OSC string, and nothing about how
//!   a DCS string ended;
//! - it prints DEL;
//! - it ignores a sequence with more than 32 values, where the parser keeps
//!   the first 32. No input here has that many;
//! - it treats bytes from 0x80 to 0x9F as C1 controls in text and ends a
//!   DCS string at 0x9C, while it drops the bytes from 0x80 up in DCS
//!   payloads. Those transitions have tests of their own, in `tests.rs`.

use super::table::{STATES, State};
use super::{Action, Parser, Recorder, StringEnd};

/// What the parser does with `input`, in the form both parsers share.
pub(super) fn ours(input: &[u8]) -> Vec<Action> {
    let mut recorder = Recorder::new();
    Parser::new().advance(&mut recorder, input);
    shared(recorder.into_actions())
}

/// What `vte` does with `input`, in the form both parsers share.
pub(super) fn reference(input: &[u8]) -> Vec<Action> {
    let mut vte = Vte::default();
    vte::Parser::new().advance(&mut vte, input);
    shared(vte.actions)
}

/// Keeps what both parsers report, and merges the prints that end up next
/// to each other.
fn shared(actions: Vec<Action>) -> Vec<Action> {
    let mut kept: Vec<Action> = Vec::new();
    for action in actions {
        let action = match action {
            Action::Apc { .. } => continue,
            Action::Esc {
                intermediates,
                final_byte: b'\\',
            } if intermediates.is_empty() => continue,
            Action::Osc { payload, end } => Action::Osc {
                payload,
                end: if end == StringEnd::Bel {
                    StringEnd::Bel
                } else {
                    StringEnd::St
                },
            },
            Action::Dcs {
                params,
                intermediates,
                final_byte,
                payload,
                end: _,
            } => Action::Dcs {
                params,
                intermediates,
                final_byte,
                payload,
                end: StringEnd::St,
            },
            Action::Print(text) => {
                if let Some(Action::Print(last)) = kept.last_mut() {
                    last.push_str(&text);
                    continue;
                }
                Action::Print(text)
            }
            other => other,
        };
        kept.push(action);
    }
    kept
}

/// Records what `vte` reports, as [`Recorder`] does.
#[derive(Default)]
struct Vte {
    actions: Vec<Action>,
    /// The DCS string in progress, unless `vte` ignores it.
    dcs: Option<Action>,
}

impl vte::Perform for Vte {
    fn print(&mut self, c: char) {
        if c != '\x7f' {
            self.actions.push(Action::Print(c.to_string()));
        }
    }

    fn execute(&mut self, byte: u8) {
        self.actions.push(Action::Execute(byte));
    }

    fn hook(&mut self, params: &vte::Params, intermediates: &[u8], ignore: bool, action: char) {
        self.dcs = (!ignore).then(|| Action::Dcs {
            params: groups(params),
            intermediates: intermediates.to_vec(),
            final_byte: final_byte(action),
            payload: Vec::new(),
            end: StringEnd::St,
        });
    }

    fn put(&mut self, byte: u8) {
        if let Some(Action::Dcs { payload, .. }) = &mut self.dcs {
            payload.push(byte);
        }
    }

    fn unhook(&mut self) {
        self.actions.extend(self.dcs.take());
    }

    fn osc_dispatch(&mut self, params: &[&[u8]], bell_terminated: bool) {
        self.actions.push(Action::Osc {
            payload: params.join(&b';'),
            end: if bell_terminated {
                StringEnd::Bel
            } else {
                StringEnd::St
            },
        });
    }

    fn csi_dispatch(
        &mut self,
        params: &vte::Params,
        intermediates: &[u8],
        ignore: bool,
        action: char,
    ) {
        if !ignore {
            self.actions.push(Action::Csi {
                params: groups(params),
                intermediates: intermediates.to_vec(),
                final_byte: final_byte(action),
            });
        }
    }

    fn esc_dispatch(&mut self, intermediates: &[u8], ignore: bool, byte: u8) {
        if !ignore {
            self.actions.push(Action::Esc {
                intermediates: intermediates.to_vec(),
                final_byte: byte,
            });
        }
    }
}

fn groups(params: &vte::Params) -> Vec<Vec<u16>> {
    params.iter().map(<[u16]>::to_vec).collect()
}

fn final_byte(action: char) -> u8 {
    u8::try_from(action).expect("final bytes are ASCII")
}

/// Inputs that leave the parser in each state, several for most states so
/// that their transitions run with different parameters and intermediates.
pub(super) const PREFIXES: &[(&[u8], State)] = &[
    (b"", State::Ground),
    (b"\x1b[m", State::Ground),
    (b"\x1b", State::Escape),
    (b"\x1b(", State::EscapeIntermediate),
    (b"\x1b#%", State::EscapeIntermediate),
    (b"\x1b[", State::CsiEntry),
    (b"\x1b[1", State::CsiParam),
    (b"\x1b[?", State::CsiParam),
    (b"\x1b[12;", State::CsiParam),
    (b"\x1b[4:", State::CsiParam),
    (b"\x1b[;:", State::CsiParam),
    (b"\x1b[>1;2", State::CsiParam),
    (b"\x1b[ ", State::CsiIntermediate),
    (b"\x1b[1$", State::CsiIntermediate),
    (b"\x1b[?1$", State::CsiIntermediate),
    (b"\x1b[1?", State::CsiIgnore),
    (b"\x1b[ 1", State::CsiIgnore),
    (b"\x1bP", State::DcsEntry),
    (b"\x1bP1", State::DcsParam),
    (b"\x1bP?", State::DcsParam),
    (b"\x1bP1;2:3", State::DcsParam),
    (b"\x1bP$", State::DcsIntermediate),
    (b"\x1bP=1$", State::DcsIntermediate),
    (b"\x1bPq", State::DcsPassthrough),
    (b"\x1bP1$qab", State::DcsPassthrough),
    (b"\x1bP1<", State::DcsIgnore),
    (b"\x1bP$1", State::DcsIgnore),
    (b"\x1b]", State::OscString),
    (b"\x1b]0;ab", State::OscString),
    (b"\x1bX", State::SosPmString),
    (b"\x1b^ab", State::SosPmString),
    (b"\x1b_", State::ApcString),
    (b"\x1b_Ga=q", State::ApcString),
    (b"\x1b]0;a\x1b", State::StringEscape),
    (b"\x1bP1$qm\x1b", State::StringEscape),
    (b"\x1b_a\x1b", State::StringEscape),
    (b"\x1bXa\x1b", State::StringEscape),
    (b"\x1bP1<\x1b", State::StringEscape),
];

/// Bytes that end whatever came before them and print, so that the state
/// it left shows in the actions.
pub(super) const PROBE: &[u8] = b"x\x07\x18y";

/// Whether the parser and `vte` disagree on `byte` in `state` by design.
fn differs_by_design(state: State, byte: u8) -> bool {
    matches!(
        (state, byte),
        (State::Ground | State::DcsPassthrough, 0x80..=0xFF)
    )
}

#[test]
fn every_transition_matches_vte() {
    for &(prefix, state) in PREFIXES {
        let mut parser = Parser::new();
        parser.advance(&mut Recorder::new(), prefix);
        assert_eq!(parser.state, state, "after {}", prefix.escape_ascii());
        for byte in 0..=u8::MAX {
            if differs_by_design(state, byte) {
                continue;
            }
            let input = [prefix, &[byte], PROBE].concat();
            assert_eq!(ours(&input), reference(&input), "{}", input.escape_ascii());
        }
    }
}

#[test]
fn the_prefixes_reach_every_state() {
    // Only a string past its size limit leads to these, and `vte` has no
    // limits. They behave like SOS and PM strings, which the table's own
    // tests check.
    let past_a_limit = [State::OscIgnore, State::ApcIgnore];
    for state in STATES
        .into_iter()
        .filter(|state| !past_a_limit.contains(state))
    {
        assert!(
            PREFIXES.iter().any(|&(_, reached)| reached == state),
            "no prefix leads to {state:?}"
        );
    }
}

/// Sequences that programs send, with malformed ones mixed in, which the
/// differential tests of the terminal share.
mod corpus {
    include!("../../testdata/parser-corpus.rs");
}
pub(super) use corpus::CORPUS;

#[test]
fn the_corpus_matches_vte() {
    for &input in CORPUS {
        let actions = ours(input);
        assert!(!actions.is_empty(), "{}", input.escape_ascii());
        assert_eq!(actions, reference(input), "{}", input.escape_ascii());
    }
    let all = CORPUS.concat();
    assert_eq!(ours(&all), reference(&all));
}
