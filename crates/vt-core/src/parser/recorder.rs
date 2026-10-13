//! A handler that records what it receives, for tests and fuzzing.

use super::{Handler, Params, StringEnd};

/// What the parser asked a handler to do, as a [`Recorder`] keeps it.
///
/// Consecutive prints merge into one, and a control string becomes a single
/// action with its whole payload, so the actions do not depend on how the
/// input was split into reads. For the same reason, a string that passed its
/// size limit keeps no payload: how much of it arrived depends on the reads,
/// and the handler discards it anyway.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    /// Text printed, by [`Handler::print`] or [`Handler::print_ascii`].
    Print(String),
    /// A C0 control character executed.
    Execute(u8),
    /// An escape sequence.
    Esc {
        /// Its intermediate bytes.
        intermediates: Vec<u8>,
        /// Its final byte.
        final_byte: u8,
    },
    /// A control sequence.
    Csi {
        /// Its parameters, each with its sub-parameters.
        params: Vec<Vec<u16>>,
        /// Its private marker and intermediate bytes.
        intermediates: Vec<u8>,
        /// Its final byte.
        final_byte: u8,
    },
    /// An OSC string.
    Osc {
        /// Its whole payload.
        payload: Vec<u8>,
        /// How it ended.
        end: StringEnd,
    },
    /// A DCS string.
    Dcs {
        /// The parameters that introduce it.
        params: Vec<Vec<u16>>,
        /// Its private marker and intermediate bytes.
        intermediates: Vec<u8>,
        /// The final byte of its introducer.
        final_byte: u8,
        /// Its whole payload.
        payload: Vec<u8>,
        /// How it ended.
        end: StringEnd,
    },
    /// An APC string.
    Apc {
        /// Its whole payload.
        payload: Vec<u8>,
        /// How it ended.
        end: StringEnd,
    },
}

/// A [`Handler`] that records every call as an [`Action`].
///
/// It panics when the parser breaks the protocol of control strings, for
/// example with a chunk outside a string, so tests and fuzzing catch it.
#[derive(Debug, Default)]
pub struct Recorder {
    actions: Vec<Action>,
    open: Option<OpenString>,
}

impl Recorder {
    /// A recorder with nothing recorded.
    pub fn new() -> Self {
        Self::default()
    }

    /// The actions recorded so far. A control string shows up once it has
    /// ended.
    pub fn actions(&self) -> &[Action] {
        &self.actions
    }

    /// The actions recorded so far, by value.
    pub fn into_actions(self) -> Vec<Action> {
        self.actions
    }

    fn open(&mut self, kind: Kind) {
        assert!(
            self.open.is_none(),
            "a control string started inside another one"
        );
        self.open = Some(OpenString {
            kind,
            payload: Vec::new(),
        });
    }

    fn put(&mut self, which: Which, data: &[u8]) {
        let open = self
            .open
            .as_mut()
            .expect("a chunk arrived outside a control string");
        assert_eq!(
            open.kind.which(),
            which,
            "a chunk for the wrong control string"
        );
        open.payload.extend_from_slice(data);
    }

    fn close(&mut self, which: Which, end: StringEnd) {
        let mut open = self
            .open
            .take()
            .expect("a control string ended without starting");
        assert_eq!(open.kind.which(), which, "the wrong control string ended");
        if end == StringEnd::TooLong {
            open.payload.clear();
        }
        self.actions.push(open.finish(end));
    }
}

impl Handler for Recorder {
    fn print(&mut self, c: char) {
        if let Some(Action::Print(text)) = self.actions.last_mut() {
            text.push(c);
        } else {
            self.actions.push(Action::Print(c.to_string()));
        }
    }

    fn execute(&mut self, byte: u8) {
        self.actions.push(Action::Execute(byte));
    }

    fn esc_dispatch(&mut self, intermediates: &[u8], final_byte: u8) {
        self.actions.push(Action::Esc {
            intermediates: intermediates.to_vec(),
            final_byte,
        });
    }

    fn csi_dispatch(&mut self, params: &Params, intermediates: &[u8], final_byte: u8) {
        self.actions.push(Action::Csi {
            params: groups(params),
            intermediates: intermediates.to_vec(),
            final_byte,
        });
    }

    fn osc_start(&mut self) {
        self.open(Kind::Osc);
    }

    fn osc_put(&mut self, data: &[u8]) {
        self.put(Which::Osc, data);
    }

    fn osc_end(&mut self, end: StringEnd) {
        self.close(Which::Osc, end);
    }

    fn dcs_start(&mut self, params: &Params, intermediates: &[u8], final_byte: u8) {
        self.open(Kind::Dcs {
            params: groups(params),
            intermediates: intermediates.to_vec(),
            final_byte,
        });
    }

    fn dcs_put(&mut self, data: &[u8]) {
        self.put(Which::Dcs, data);
    }

    fn dcs_end(&mut self, end: StringEnd) {
        self.close(Which::Dcs, end);
    }

    fn apc_start(&mut self) {
        self.open(Kind::Apc);
    }

    fn apc_put(&mut self, data: &[u8]) {
        self.put(Which::Apc, data);
    }

    fn apc_end(&mut self, end: StringEnd) {
        self.close(Which::Apc, end);
    }
}

/// A control string that has started and not ended yet.
#[derive(Debug)]
struct OpenString {
    kind: Kind,
    payload: Vec<u8>,
}

impl OpenString {
    fn finish(self, end: StringEnd) -> Action {
        let payload = self.payload;
        match self.kind {
            Kind::Osc => Action::Osc { payload, end },
            Kind::Dcs {
                params,
                intermediates,
                final_byte,
            } => Action::Dcs {
                params,
                intermediates,
                final_byte,
                payload,
                end,
            },
            Kind::Apc => Action::Apc { payload, end },
        }
    }
}

/// Which control string is open, with the introducer of a DCS string.
#[derive(Debug)]
enum Kind {
    Osc,
    Dcs {
        params: Vec<Vec<u16>>,
        intermediates: Vec<u8>,
        final_byte: u8,
    },
    Apc,
}

impl Kind {
    fn which(&self) -> Which {
        match self {
            Self::Osc => Which::Osc,
            Self::Dcs { .. } => Which::Dcs,
            Self::Apc => Which::Apc,
        }
    }
}

/// The three kinds of control strings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Which {
    Osc,
    Dcs,
    Apc,
}

fn groups(params: &Params) -> Vec<Vec<u16>> {
    params.iter().map(<[u16]>::to_vec).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prints_merge_until_something_else_happens() {
        let mut recorder = Recorder::new();
        recorder.print('a');
        recorder.print_ascii(b"bc");
        recorder.print('é');
        recorder.execute(b'\n');
        recorder.print('d');
        assert_eq!(
            recorder.actions(),
            [
                Action::Print("abcé".to_owned()),
                Action::Execute(b'\n'),
                Action::Print("d".to_owned()),
            ]
        );
    }

    #[test]
    fn sequences_keep_their_parameters() {
        let mut recorder = Recorder::new();
        recorder.esc_dispatch(b"(", b'0');
        recorder.csi_dispatch(
            &Params::from_groups(&[&[38, 2, 0, 255, 0, 0], &[1]]),
            b"",
            b'm',
        );
        recorder.csi_dispatch(&Params::from_groups(&[&[25]]), b"?", b'h');
        assert_eq!(
            recorder.into_actions(),
            [
                Action::Esc {
                    intermediates: b"(".to_vec(),
                    final_byte: b'0',
                },
                Action::Csi {
                    params: vec![vec![38, 2, 0, 255, 0, 0], vec![1]],
                    intermediates: Vec::new(),
                    final_byte: b'm',
                },
                Action::Csi {
                    params: vec![vec![25]],
                    intermediates: b"?".to_vec(),
                    final_byte: b'h',
                },
            ]
        );
    }

    #[test]
    fn a_control_string_becomes_one_action() {
        let mut recorder = Recorder::new();
        recorder.osc_start();
        recorder.osc_put(b"0;ti");
        recorder.osc_put(b"tle");
        recorder.osc_end(StringEnd::Bel);
        recorder.dcs_start(&Params::default(), b"$", b'q');
        recorder.dcs_put(b"m");
        recorder.dcs_end(StringEnd::St);
        recorder.apc_start();
        recorder.apc_end(StringEnd::TooLong);
        assert_eq!(
            recorder.into_actions(),
            [
                Action::Osc {
                    payload: b"0;title".to_vec(),
                    end: StringEnd::Bel,
                },
                Action::Dcs {
                    params: Vec::new(),
                    intermediates: b"$".to_vec(),
                    final_byte: b'q',
                    payload: b"m".to_vec(),
                    end: StringEnd::St,
                },
                Action::Apc {
                    payload: Vec::new(),
                    end: StringEnd::TooLong,
                },
            ]
        );
    }

    #[test]
    fn a_string_past_its_limit_keeps_no_payload() {
        let mut recorder = Recorder::new();
        recorder.osc_start();
        recorder.osc_put(b"52;c;");
        recorder.osc_end(StringEnd::TooLong);
        assert_eq!(
            recorder.actions(),
            [Action::Osc {
                payload: Vec::new(),
                end: StringEnd::TooLong,
            }]
        );
    }

    #[test]
    #[should_panic(expected = "a chunk arrived outside a control string")]
    fn a_chunk_outside_a_string_is_a_parser_bug() {
        Recorder::new().osc_put(b"x");
    }

    #[test]
    #[should_panic(expected = "the wrong control string ended")]
    fn ending_the_wrong_string_is_a_parser_bug() {
        let mut recorder = Recorder::new();
        recorder.osc_start();
        recorder.dcs_end(StringEnd::St);
    }
}
