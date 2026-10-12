//! The escape-sequence parser.
//!
//! It turns the bytes of the PTY into calls on a [`Handler`]: text to print,
//! control characters to execute, escape and control sequences to dispatch,
//! and control strings (OSC, DCS and APC), whose payloads arrive in chunks.

mod handler;
mod limits;
mod params;
mod recorder;
mod table;
mod utf8;

#[cfg(test)]
mod reference;
#[cfg(test)]
mod tests;

pub use handler::{Handler, StringEnd};
pub use limits::Limits;
pub use params::{MAX_PARAMS, Params};
pub use recorder::{Action, Recorder};

use limits::OscNumber;
use table::{State, TABLE, Transition};
use utf8::{REPLACEMENT, Step, Utf8Decoder};

/// The most intermediate bytes that a sequence keeps, its private marker
/// included. No known sequence has more; one that does is ignored.
const MAX_INTERMEDIATES: usize = 2;

/// Parses the bytes that programs write to a terminal, and drives a
/// [`Handler`] with what it finds.
///
/// It implements the state machine that Paul Williams describes for DEC's
/// ANSI-compatible terminals (<https://vt100.net/emu/dec_ansi_parser>),
/// adapted to UTF-8: C1 controls are recognized only in their 7-bit form,
/// `ESC` followed by a byte. The parser keeps its state from one call to the
/// next, so the input can be split anywhere.
///
/// The parser buffers nothing: the payload of a control string reaches the
/// handler in chunks, one for each run of payload bytes in a read, and a
/// string that passes its [`Limits`] is dropped as it arrives.
///
/// ```
/// use saetta_vt_core::parser::{Action, Parser, Recorder};
///
/// let mut parser = Parser::new();
/// let mut recorder = Recorder::new();
/// parser.advance(&mut recorder, b"\x1b[1mbo");
/// parser.advance(&mut recorder, b"ld\r\n");
/// assert_eq!(
///     recorder.actions(),
///     [
///         Action::Csi {
///             params: vec![vec![1]],
///             intermediates: Vec::new(),
///             final_byte: b'm',
///         },
///         Action::Print("bold".to_owned()),
///         Action::Execute(b'\r'),
///         Action::Execute(b'\n'),
///     ]
/// );
/// ```
#[derive(Debug, Default)]
pub struct Parser {
    state: State,
    params: Params,
    intermediates: [u8; MAX_INTERMEDIATES],
    intermediate_count: usize,
    /// The sequence has more intermediates than it keeps, so it is consumed
    /// without being dispatched.
    ignoring: bool,
    utf8: Utf8Decoder,
    limits: Limits,
    /// How many bytes of its payload the current control string has
    /// delivered.
    payload_len: usize,
    /// The size limit of the current control string.
    payload_limit: usize,
    osc_number: OscNumber,
}

impl Parser {
    /// A parser in the ground state, with the default [`Limits`].
    pub fn new() -> Self {
        Self::default()
    }

    /// A parser in the ground state, with `limits`.
    pub fn with_limits(limits: Limits) -> Self {
        Self {
            limits,
            ..Self::default()
        }
    }

    /// The size limits of control strings.
    pub fn limits(&self) -> &Limits {
        &self.limits
    }

    /// Replaces the size limits of control strings. A string in progress
    /// keeps the limit it started with.
    pub fn set_limits(&mut self, limits: Limits) {
        self.limits = limits;
    }

    /// Parses `bytes`, calling `handler` for what they contain.
    pub fn advance<H: Handler>(&mut self, handler: &mut H, bytes: &[u8]) {
        let mut i = 0;
        while let Some(&byte) = bytes.get(i) {
            if self.utf8.is_partial() {
                match self.utf8.resume(byte) {
                    Step::Partial => {
                        i += 1;
                        continue;
                    }
                    Step::Done(c) => {
                        print_decoded(handler, c);
                        i += 1;
                        continue;
                    }
                    // The byte is parsed below, on its own.
                    Step::Invalid => handler.print(REPLACEMENT),
                }
            }
            let transition = TABLE[self.state as usize][usize::from(byte)];
            if let Some(payload) = payload_of(transition.action) {
                let len = self.run_of(transition, &bytes[i..]);
                self.put(handler, payload, &bytes[i..i + len]);
                i += len;
            } else if transition.action == table::Action::Ignore && transition.next == self.state {
                i += self.run_of(transition, &bytes[i..]);
            } else {
                self.state = transition.next;
                self.perform(handler, transition.action, byte);
                i += 1;
            }
        }
    }

    /// How many bytes at the start of `bytes` have `transition` in the
    /// current state.
    fn run_of(&self, transition: Transition, bytes: &[u8]) -> usize {
        let row = &TABLE[self.state as usize];
        bytes
            .iter()
            .position(|&byte| row[usize::from(byte)] != transition)
            .unwrap_or(bytes.len())
    }

    fn perform<H: Handler>(&mut self, handler: &mut H, action: table::Action, byte: u8) {
        use table::Action::*;

        match action {
            Ignore => {}
            Print => handler.print(char::from(byte)),
            Utf8 => {
                if let Some(c) = self.utf8.start(byte) {
                    handler.print(c);
                }
            }
            Execute => handler.execute(byte),
            Clear => self.clear(),
            Collect => self.collect(byte),
            Digit => self.params.push_digit(byte - b'0'),
            NextParam => self.params.end_param(),
            NextSubParam => self.params.end_sub_param(),
            EscDispatch => {
                if !self.ignoring {
                    handler.esc_dispatch(self.intermediates(), byte);
                }
            }
            CsiDispatch => {
                self.params.end_param();
                if !self.ignoring {
                    handler.csi_dispatch(&self.params, self.intermediates(), byte);
                }
            }
            DcsStart => {
                self.params.end_param();
                if self.ignoring {
                    self.state = State::DcsIgnore;
                } else {
                    self.start_string(self.limits.dcs);
                    handler.dcs_start(&self.params, self.intermediates(), byte);
                }
            }
            DcsPut => self.put(handler, Payload::Dcs, &[byte]),
            DcsEnd => {
                handler.dcs_end(StringEnd::St);
                self.clear();
            }
            DcsCancel => {
                handler.dcs_end(StringEnd::Cancelled);
                handler.execute(byte);
            }
            OscStart => {
                self.start_string(self.limits.osc);
                self.osc_number = OscNumber::start();
                handler.osc_start();
            }
            OscPut => self.put(handler, Payload::Osc, &[byte]),
            OscEnd => {
                handler.osc_end(StringEnd::St);
                self.clear();
            }
            OscBel => handler.osc_end(StringEnd::Bel),
            OscCancel => {
                handler.osc_end(StringEnd::Cancelled);
                handler.execute(byte);
            }
            ApcStart => {
                self.start_string(self.limits.apc);
                handler.apc_start();
            }
            ApcPut => self.put(handler, Payload::Apc, &[byte]),
            ApcEnd => {
                handler.apc_end(StringEnd::St);
                self.clear();
            }
            ApcCancel => {
                handler.apc_end(StringEnd::Cancelled);
                handler.execute(byte);
            }
        }
    }

    fn start_string(&mut self, limit: usize) {
        self.payload_len = 0;
        self.payload_limit = limit;
    }

    /// Delivers `chunk`, a run of payload bytes, unless it takes the string
    /// past its limit. Then the string ends, and the rest of it is consumed.
    fn put<H: Handler>(&mut self, handler: &mut H, payload: Payload, chunk: &[u8]) {
        if payload == Payload::Osc
            && let Some(limit) = self.osc_number.read(chunk, &self.limits)
        {
            self.payload_limit = limit;
        }
        // A limit can shrink once the OSC number is known, so the room left
        // can be none.
        let room = self.payload_limit.saturating_sub(self.payload_len);
        if chunk.len() > room {
            self.state = match payload {
                Payload::Osc => {
                    handler.osc_end(StringEnd::TooLong);
                    State::OscIgnore
                }
                Payload::Dcs => {
                    handler.dcs_end(StringEnd::TooLong);
                    State::DcsIgnore
                }
                Payload::Apc => {
                    handler.apc_end(StringEnd::TooLong);
                    State::ApcIgnore
                }
            };
            return;
        }
        self.payload_len += chunk.len();
        match payload {
            Payload::Osc => handler.osc_put(chunk),
            Payload::Dcs => handler.dcs_put(chunk),
            Payload::Apc => handler.apc_put(chunk),
        }
    }

    fn clear(&mut self) {
        self.params.clear();
        self.intermediate_count = 0;
        self.ignoring = false;
    }

    fn collect(&mut self, byte: u8) {
        if self.intermediate_count == MAX_INTERMEDIATES {
            self.ignoring = true;
        } else {
            self.intermediates[self.intermediate_count] = byte;
            self.intermediate_count += 1;
        }
    }

    fn intermediates(&self) -> &[u8] {
        &self.intermediates[..self.intermediate_count]
    }
}

/// The control strings whose payload reaches the handler.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Payload {
    Osc,
    Dcs,
    Apc,
}

/// The control string whose payload `action` delivers a byte of.
fn payload_of(action: table::Action) -> Option<Payload> {
    match action {
        table::Action::OscPut => Some(Payload::Osc),
        table::Action::DcsPut => Some(Payload::Dcs),
        table::Action::ApcPut => Some(Payload::Apc),
        _ => None,
    }
}

/// Prints a character decoded from UTF-8, unless it is a C1 control, which
/// UTF-8 terminals do not recognize there.
fn print_decoded<H: Handler>(handler: &mut H, c: char) {
    if !matches!(c, '\u{80}'..='\u{9F}') {
        handler.print(c);
    }
}
