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

/// Sequences that programs send, with malformed ones mixed in.
pub(super) const CORPUS: &[&[u8]] = &[
    // Text, controls and UTF-8.
    b"hello, world\r\n",
    "naïve café — ✓ 🎉 日本語\r\n".as_bytes(),
    b"tab\there\x08\x07\x00\x0e\x0f",
    // SGR.
    b"\x1b[0m\x1b[1;31mred\x1b[m",
    b"\x1b[38;5;196m\x1b[48;2;10;20;30mrgb\x1b[38;2;255;255;255;48;2;0;0;0m",
    b"\x1b[38:2::255:128:0m\x1b[4:3m\x1b[58:5:9m\x1b[:m\x1b[1::2m",
    // Cursor, erase, scroll region and tabs.
    b"\x1b[H\x1b[2J\x1b[10;20H\x1b[K\x1b[3A\x1b[B\x1b[;H\x1b[5;H",
    b"\x1b[5;20r\x1bM\x1bD\x1bE\x1bH\x1b[3g\x1b[2@\x1b[3P",
    // Modes.
    b"\x1b[?1049h\x1b[?25l\x1b[?2004h\x1b[?1000;1006h\x1b[?2026h\x1b[?2026l",
    b"\x1b[4h\x1b[20l\x1b=\x1b>",
    // Charsets, saving the cursor, alignment test and reset.
    b"\x1b(0lqk\x1b(B\x1b)0\x0e\x0f\x1b(%5",
    b"\x1b7\x1b8\x1b#8\x1bc",
    // Queries and reports.
    b"\x1b[c\x1b[>c\x1b[=c\x1b[5n\x1b[6n\x1b[>0q\x1b[?2026$p\x1b[18t",
    b"\x1b[?1;2c\x1b[?2026;2$y\x1b[8;24;80t",
    // Cursor style, kitty keyboard, mouse and bracketed paste.
    b"\x1b[2 q\x1b[>1u\x1b[?u\x1b[<u\x1b[=1;1u",
    b"\x1b[<0;10;20M\x1b[<0;10;20m\x1b[200~pasted\x1b[201~",
    // OSC.
    b"\x1b]0;title\x07\x1b]2;w\xc3\xa9b\x1b\\",
    b"\x1b]7;file://host/home/user\x1b\\",
    b"\x1b]8;id=1;https://example.com\x1b\\link\x1b]8;;\x1b\\",
    b"\x1b]52;c;aGVsbG8=\x07\x1b]52;c;?\x07",
    b"\x1b]133;A\x07$ \x1b]133;B\x07ls\r\n\x1b]133;C\x07\x1b]133;D;0\x07",
    b"\x1b]10;?\x07\x1b]11;rgb:0000/0000/0000\x1b\\\x1b]4;1;#ff0000\x07",
    b"\x1b]9;4;1;50\x07\x1b]\x07\x1b];\x07",
    // DCS: DECRQSS, XTGETTCAP, a report, sixel, and synchronized updates.
    b"\x1bP$qm\x1b\\\x1bP+q544e\x1b\\\x1bP1$r0m\x1b\\",
    b"\x1bPq#0;2;0;0;0#1;2;100;100;0#1~~@@vv@@~~$-\x1b\\",
    b"\x1bP=1s\x1b\\text\x1bP=2s\x1b\\",
    // APC: kitty graphics.
    b"\x1b_Gf=100,a=T;iVBORw0KGgo=\x1b\\after",
    // SOS and PM.
    b"\x1bXsos\x1b\\\x1b^pm\x1b\\after",
    // Interrupted and malformed sequences.
    b"\x1b[1;2\x18x\x1b[3\x1ay\x1b]0;abc\x18def\x1bP1$qab\x1ax",
    b"\x1b[1\x1b[2m\x1b[1\nA\x1b\x1b\x1b[m",
    b"\x1b[1?hx\x1b[ 1py\x1b[\x7f1\x7fm\x1b(%!Bz",
    b"\x1bP1<qdata\x1b\\x\x1b]0;a\x1b[1m",
];

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
