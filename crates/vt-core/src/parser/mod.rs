//! The escape-sequence parser.
//!
//! It turns the bytes of the PTY into calls on a [`Handler`]: text to print,
//! control characters to execute, escape and control sequences to dispatch,
//! and control strings (OSC, DCS and APC), whose payloads arrive in chunks.

mod handler;
mod limits;
mod params;
mod recorder;
mod scan;
mod table;
mod utf8;

#[cfg(test)]
mod properties;
#[cfg(test)]
mod reference;
#[cfg(test)]
mod seeds;
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
/// use lokyr_vt_core::parser::{Action, Parser, Recorder};
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
    /// Limits that [`Parser::set_limits`] gave, which apply from the next
    /// control string.
    next_limits: Option<Limits>,
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

    /// The size limits of control strings, as [`Parser::set_limits`] last
    /// set them.
    pub fn limits(&self) -> &Limits {
        self.next_limits.as_ref().unwrap_or(&self.limits)
    }

    /// Replaces the size limits of control strings, from the next string
    /// on. A string in progress keeps the limits it started with, the limit
    /// of an OSC number that comes after the change included.
    pub fn set_limits(&mut self, limits: Limits) {
        self.next_limits = Some(limits);
    }

    /// Whether the parser is in the ground state, between escape
    /// sequences and control strings, where bytes are text: an ignored
    /// string, or an ignored control sequence, ends with no call to the
    /// handler, which this tells.
    pub fn in_ground(&self) -> bool {
        self.state == State::Ground
    }

    /// The limits for a control string that starts now.
    fn string_limits(&mut self) -> &Limits {
        if let Some(limits) = self.next_limits.take() {
            self.limits = limits;
        }
        &self.limits
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
            if transition.action == table::Action::Print {
                // Printable ASCII, in the ground state: the whole run goes
                // at once.
                let len = 1 + scan::printable_len(&bytes[i + 1..]);
                handler.print_ascii(&bytes[i..i + len]);
                i += len;
            } else if transition.action == table::Action::Execute {
                // C0 controls, as CR and LF, which every line has.
                self.state = transition.next;
                handler.execute(byte);
                i += 1;
            } else if transition.action == table::Action::Utf8
                && let len @ 1.. = print_utf8_run(handler, &bytes[i..])
            {
                i += len;
            } else if let Some(payload) = payload_of(transition.action) {
                let len = self.run_of(transition, &bytes[i..]);
                self.put(handler, payload, &bytes[i..i + len]);
                i += len;
            } else if transition.action == table::Action::Ignore && transition.next == self.state {
                i += self.run_of(transition, &bytes[i..]);
            } else {
                self.state = transition.next;
                self.perform(handler, transition.action, byte);
                i += 1;
                if self.state == State::CsiEntry {
                    i += self.csi_params(handler, &bytes[i..]);
                }
            }
        }
    }

    /// Parses the parameters of a CSI sequence at the start of `bytes`, and
    /// its final byte, as the table would one byte at a time: digits, `;`
    /// and `:`. Returns how many bytes it took. At any other byte it stops,
    /// in the state that the table would be in, for the table to go on.
    fn csi_params<H: Handler>(&mut self, handler: &mut H, bytes: &[u8]) -> usize {
        // The entry state has no value yet. The one being parsed stays here,
        // out of the parameters, until a separator or the end takes it.
        let (mut value, mut written) = (0_u16, false);
        let mut i = 0;
        let taken = loop {
            let Some(&byte) = bytes.get(i) else {
                break bytes.len();
            };
            match byte {
                b'0'..=b'9' => {
                    value = value
                        .saturating_mul(10)
                        .saturating_add(u16::from(byte - b'0'));
                    written = true;
                }
                b';' | b':' => {
                    self.params.set_current(value, written);
                    if byte == b';' {
                        self.params.end_param();
                    } else {
                        self.params.end_sub_param();
                    }
                    (value, written) = (0, false);
                }
                0x40..=0x7E => {
                    self.state = State::Ground;
                    self.params.set_current(value, written);
                    self.params.end_param();
                    // Nothing is collected in the entry state, so the
                    // sequence has no intermediates, and is not ignored.
                    debug_assert!(!self.ignoring && self.intermediate_count == 0);
                    handler.csi_dispatch(&self.params, &[], byte);
                    return i + 1;
                }
                _ => break i,
            }
            i += 1;
        };
        self.params.set_current(value, written);
        if taken > 0 {
            self.state = State::CsiParam;
        }
        taken
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
            Print => handler.print_ascii(std::slice::from_ref(&byte)),
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
                    let limit = self.string_limits().dcs;
                    self.start_string(limit);
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
                let limit = self.string_limits().osc;
                self.start_string(limit);
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
                let limit = self.string_limits().apc;
                self.start_string(limit);
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
        // Each byte counts against the limit in force when it arrives, so that
        // the outcome does not depend on the reads: the bytes that make an
        // OSC number known count against the general limit, and the whole
        // chunk against the number's own.
        let mut fits = true;
        if payload == Payload::Osc
            && let Some((read, limit)) = self.osc_number.read(chunk, &self.limits)
        {
            fits = read <= self.payload_limit.saturating_sub(self.payload_len);
            self.payload_limit = limit;
        }
        // A limit can shrink once the OSC number is known, so the room left
        // can be none.
        let room = self.payload_limit.saturating_sub(self.payload_len);
        if !fits || chunk.len() > room {
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

/// How many characters [`print_utf8_run`] hands the handler at once.
const CHARS: usize = 64;

/// Prints the characters of the valid UTF-8 at the start of `bytes`, in the
/// ground state, up to its first byte of ASCII, or the first that is invalid
/// or starts a character that `bytes` does not hold whole, and returns how
/// many bytes they take. The decoder takes the bytes from there, one at a
/// time, as it takes a character split across reads. C1 controls are
/// dropped, as [`print_decoded`] drops them.
fn print_utf8_run<H: Handler>(handler: &mut H, bytes: &[u8]) -> usize {
    let mut chars = ['\0'; CHARS];
    let (mut len, mut taken) = (0, 0);
    while let Some((c, size)) = utf8::decode_whole(&bytes[taken..]) {
        taken += size;
        if matches!(c, '\u{80}'..='\u{9F}') {
            continue;
        }
        chars[len] = c;
        len += 1;
        if len == CHARS {
            handler.print_chars(&chars);
            len = 0;
        }
    }
    if len > 0 {
        handler.print_chars(&chars[..len]);
    }
    taken
}

/// Prints a character decoded from UTF-8, unless it is a C1 control, which
/// UTF-8 terminals do not recognize there.
fn print_decoded<H: Handler>(handler: &mut H, c: char) {
    if !matches!(c, '\u{80}'..='\u{9F}') {
        handler.print(c);
    }
}
