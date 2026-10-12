//! The escape-sequence parser.
//!
//! It turns the bytes of the PTY into calls on a [`Handler`]: text to print,
//! control characters to execute, escape and control sequences to dispatch,
//! and control strings (OSC, DCS and APC), whose payloads arrive in chunks.

mod handler;
mod params;
mod recorder;
mod table;
mod utf8;

#[cfg(test)]
mod reference;
#[cfg(test)]
mod tests;

pub use handler::{Handler, StringEnd};
pub use params::{MAX_PARAMS, Params};
pub use recorder::{Action, Recorder};

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
}

impl Parser {
    /// A parser in the ground state.
    pub fn new() -> Self {
        Self::default()
    }

    /// Parses `bytes`, calling `handler` for what they contain.
    pub fn advance<H: Handler>(&mut self, handler: &mut H, bytes: &[u8]) {
        for &byte in bytes {
            if self.utf8.is_partial() {
                match self.utf8.resume(byte) {
                    Step::Partial => continue,
                    Step::Done(c) => {
                        print_decoded(handler, c);
                        continue;
                    }
                    // The byte is parsed below, on its own.
                    Step::Invalid => handler.print(REPLACEMENT),
                }
            }
            let Transition { action, next } = TABLE[self.state as usize][usize::from(byte)];
            self.state = next;
            self.perform(handler, action, byte);
        }
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
                    handler.dcs_start(&self.params, self.intermediates(), byte);
                }
            }
            DcsPut => handler.dcs_put(&[byte]),
            DcsEnd => {
                handler.dcs_end(StringEnd::St);
                self.clear();
            }
            DcsCancel => {
                handler.dcs_end(StringEnd::Cancelled);
                handler.execute(byte);
            }
            OscStart => handler.osc_start(),
            OscPut => handler.osc_put(&[byte]),
            OscEnd => {
                handler.osc_end(StringEnd::St);
                self.clear();
            }
            OscBel => handler.osc_end(StringEnd::Bel),
            OscCancel => {
                handler.osc_end(StringEnd::Cancelled);
                handler.execute(byte);
            }
            ApcStart => handler.apc_start(),
            ApcPut => handler.apc_put(&[byte]),
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

/// Prints a character decoded from UTF-8, unless it is a C1 control, which
/// UTF-8 terminals do not recognize there.
fn print_decoded<H: Handler>(handler: &mut H, c: char) {
    if !matches!(c, '\u{80}'..='\u{9F}') {
        handler.print(c);
    }
}
