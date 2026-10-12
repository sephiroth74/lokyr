//! The transition table of the parser: Paul Williams' DEC/ANSI state machine
//! (<https://vt100.net/emu/dec_ansi_parser>), adapted to UTF-8.
//!
//! [`transition`] states the rules; [`TABLE`] holds its result for every
//! state and byte, computed at compile time. The changes to the original:
//! - C1 controls exist only in their 7-bit form, `ESC` followed by a byte.
//!   Bytes from 0x80 up are UTF-8 in the ground state, payload in control
//!   strings, and ignored elsewhere;
//! - DEL is ignored in the ground state too;
//! - colons separate sub-parameters, as in `38:2::255:0:0`, instead of
//!   making the sequence invalid;
//! - SOS and PM strings are dropped but APC strings reach the handler, so
//!   the original's state for the three is split in two;
//! - BEL ends an OSC string, as xterm allows;
//! - after the `ESC` that ends a control string, a state of its own consumes
//!   the `\` of ST, which would otherwise reach the handler as an escape
//!   sequence;
//! - OSC and APC strings get an ignore state, as DCS strings have one, for
//!   the rest of a string past its size limit. The parser enters them when
//!   it ends the string, since the limit is not in the table.

/// The states of the machine.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum State {
    #[default]
    Ground,
    Escape,
    EscapeIntermediate,
    CsiEntry,
    CsiParam,
    CsiIntermediate,
    CsiIgnore,
    DcsEntry,
    DcsParam,
    DcsIntermediate,
    DcsPassthrough,
    DcsIgnore,
    OscString,
    /// The rest of an OSC string past its size limit.
    OscIgnore,
    /// SOS and PM strings, which are consumed and dropped.
    SosPmString,
    ApcString,
    /// The rest of an APC string past its size limit.
    ApcIgnore,
    /// Right after the `ESC` that ended a control string: like
    /// [`State::Escape`], except that `\` completes the string terminator.
    StringEscape,
}

/// Every state, in the order of the table's rows.
pub(crate) const STATES: [State; 18] = [
    State::Ground,
    State::Escape,
    State::EscapeIntermediate,
    State::CsiEntry,
    State::CsiParam,
    State::CsiIntermediate,
    State::CsiIgnore,
    State::DcsEntry,
    State::DcsParam,
    State::DcsIntermediate,
    State::DcsPassthrough,
    State::DcsIgnore,
    State::OscString,
    State::OscIgnore,
    State::SosPmString,
    State::ApcString,
    State::ApcIgnore,
    State::StringEscape,
];

/// What a byte does, besides moving the machine to its next state.
///
/// Every action that leads into the escape states from elsewhere forgets the
/// parameters and intermediates, as the original's entry action for the
/// escape state does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Action {
    /// Nothing: the byte is dropped.
    Ignore,
    /// Print the byte, which is printable ASCII.
    Print,
    /// Feed the byte to the UTF-8 decoder.
    Utf8,
    /// Execute the byte, a C0 control.
    Execute,
    /// Forget the parameters and intermediates, before a new sequence.
    Clear,
    /// Keep the byte as an intermediate or a private marker.
    Collect,
    /// Add the byte, a digit, to the value being parsed.
    Digit,
    /// `;`: end the parameter.
    NextParam,
    /// `:`: end the value, and start a sub-parameter.
    NextSubParam,
    EscDispatch,
    CsiDispatch,
    /// Start a DCS string, with the byte as the final byte of its
    /// introducer.
    DcsStart,
    DcsPut,
    /// `ESC`: end the DCS string as terminated by ST, whose `\` should
    /// follow.
    DcsEnd,
    /// CAN or SUB: cancel the DCS string, then execute the byte.
    DcsCancel,
    OscStart,
    OscPut,
    /// `ESC`: end the OSC string as terminated by ST.
    OscEnd,
    /// BEL: end the OSC string.
    OscBel,
    /// CAN or SUB: cancel the OSC string, then execute the byte.
    OscCancel,
    ApcStart,
    ApcPut,
    /// `ESC`: end the APC string as terminated by ST.
    ApcEnd,
    /// CAN or SUB: cancel the APC string, then execute the byte.
    ApcCancel,
}

/// The action to take for a byte, and the state to move to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Transition {
    pub(crate) action: Action,
    pub(crate) next: State,
}

/// [`transition`] for every state and byte, indexed by `state as usize`
/// then by byte.
pub(crate) static TABLE: [[Transition; 256]; STATES.len()] = build();

const fn build() -> [[Transition; 256]; STATES.len()] {
    let mut table = [[Transition {
        action: Action::Ignore,
        next: State::Ground,
    }; 256]; STATES.len()];
    let mut row = 0;
    while row < STATES.len() {
        let mut byte = 0;
        while byte < 256 {
            // `byte` is below 256, so the cast keeps every bit.
            #[allow(clippy::cast_possible_truncation)]
            let value = byte as u8;
            table[row][byte] = transition(STATES[row], value);
            byte += 1;
        }
        row += 1;
    }
    table
}

/// The rules of the machine: what `byte` does in `state`.
pub(crate) const fn transition(state: State, byte: u8) -> Transition {
    use Action::*;
    use State::*;

    const fn to(action: Action, next: State) -> Transition {
        Transition { action, next }
    }

    // CAN and SUB cancel whatever is in progress, and ESC interrupts it.
    match (state, byte) {
        (OscString, 0x18 | 0x1A) => return to(OscCancel, Ground),
        (DcsPassthrough, 0x18 | 0x1A) => return to(DcsCancel, Ground),
        (ApcString, 0x18 | 0x1A) => return to(ApcCancel, Ground),
        (_, 0x18 | 0x1A) => return to(Execute, Ground),
        (OscString, 0x1B) => return to(OscEnd, StringEscape),
        (DcsPassthrough, 0x1B) => return to(DcsEnd, StringEscape),
        (ApcString, 0x1B) => return to(ApcEnd, StringEscape),
        (DcsIgnore | OscIgnore | SosPmString | ApcIgnore, 0x1B) => {
            return to(Clear, StringEscape);
        }
        (_, 0x1B) => return to(Clear, Escape),
        _ => {}
    }

    match state {
        Ground => match byte {
            0x20..=0x7E => to(Print, Ground),
            0x7F => to(Ignore, Ground),
            0x80..=0xFF => to(Utf8, Ground),
            _ => to(Execute, Ground),
        },
        Escape => match byte {
            0x00..=0x1F => to(Execute, Escape),
            0x20..=0x2F => to(Collect, EscapeIntermediate),
            b'P' => to(Clear, DcsEntry),
            b'X' | b'^' => to(Ignore, SosPmString),
            b'_' => to(ApcStart, ApcString),
            b'[' => to(Clear, CsiEntry),
            b']' => to(OscStart, OscString),
            0x30..=0x7E => to(EscDispatch, Ground),
            _ => to(Ignore, Escape),
        },
        StringEscape => match (byte, transition(Escape, byte)) {
            (b'\\', _) => to(Ignore, Ground),
            (
                _,
                Transition {
                    action,
                    next: Escape,
                },
            ) => to(action, StringEscape),
            (_, other) => other,
        },
        EscapeIntermediate => match byte {
            0x00..=0x1F => to(Execute, EscapeIntermediate),
            0x20..=0x2F => to(Collect, EscapeIntermediate),
            0x30..=0x7E => to(EscDispatch, Ground),
            _ => to(Ignore, EscapeIntermediate),
        },
        CsiEntry => match byte {
            0x00..=0x1F => to(Execute, CsiEntry),
            0x20..=0x2F => to(Collect, CsiIntermediate),
            0x30..=0x39 => to(Digit, CsiParam),
            b':' => to(NextSubParam, CsiParam),
            b';' => to(NextParam, CsiParam),
            0x3C..=0x3F => to(Collect, CsiParam),
            0x40..=0x7E => to(CsiDispatch, Ground),
            _ => to(Ignore, CsiEntry),
        },
        CsiParam => match byte {
            0x00..=0x1F => to(Execute, CsiParam),
            0x20..=0x2F => to(Collect, CsiIntermediate),
            0x30..=0x39 => to(Digit, CsiParam),
            b':' => to(NextSubParam, CsiParam),
            b';' => to(NextParam, CsiParam),
            0x3C..=0x3F => to(Ignore, CsiIgnore),
            0x40..=0x7E => to(CsiDispatch, Ground),
            _ => to(Ignore, CsiParam),
        },
        CsiIntermediate => match byte {
            0x00..=0x1F => to(Execute, CsiIntermediate),
            0x20..=0x2F => to(Collect, CsiIntermediate),
            0x30..=0x3F => to(Ignore, CsiIgnore),
            0x40..=0x7E => to(CsiDispatch, Ground),
            _ => to(Ignore, CsiIntermediate),
        },
        CsiIgnore => match byte {
            0x00..=0x1F => to(Execute, CsiIgnore),
            0x40..=0x7E => to(Ignore, Ground),
            _ => to(Ignore, CsiIgnore),
        },
        DcsEntry => match byte {
            0x20..=0x2F => to(Collect, DcsIntermediate),
            0x30..=0x39 => to(Digit, DcsParam),
            b':' => to(NextSubParam, DcsParam),
            b';' => to(NextParam, DcsParam),
            0x3C..=0x3F => to(Collect, DcsParam),
            0x40..=0x7E => to(DcsStart, DcsPassthrough),
            _ => to(Ignore, DcsEntry),
        },
        DcsParam => match byte {
            0x20..=0x2F => to(Collect, DcsIntermediate),
            0x30..=0x39 => to(Digit, DcsParam),
            b':' => to(NextSubParam, DcsParam),
            b';' => to(NextParam, DcsParam),
            0x3C..=0x3F => to(Ignore, DcsIgnore),
            0x40..=0x7E => to(DcsStart, DcsPassthrough),
            _ => to(Ignore, DcsParam),
        },
        DcsIntermediate => match byte {
            0x20..=0x2F => to(Collect, DcsIntermediate),
            0x30..=0x3F => to(Ignore, DcsIgnore),
            0x40..=0x7E => to(DcsStart, DcsPassthrough),
            _ => to(Ignore, DcsIntermediate),
        },
        DcsPassthrough => match byte {
            0x7F => to(Ignore, DcsPassthrough),
            _ => to(DcsPut, DcsPassthrough),
        },
        DcsIgnore => to(Ignore, DcsIgnore),
        OscString => match byte {
            0x07 => to(OscBel, Ground),
            0x00..=0x1F => to(Ignore, OscString),
            _ => to(OscPut, OscString),
        },
        OscIgnore => match byte {
            0x07 => to(Ignore, Ground),
            _ => to(Ignore, OscIgnore),
        },
        SosPmString => to(Ignore, SosPmString),
        // The payload of an APC string is made of the same bytes as an OSC
        // one, but only ST ends it.
        ApcString => match byte {
            0x00..=0x1F => to(Ignore, ApcString),
            _ => to(ApcPut, ApcString),
        },
        ApcIgnore => to(Ignore, ApcIgnore),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every state and byte, with what the byte does there.
    fn transitions() -> impl Iterator<Item = (State, u8, Transition)> {
        STATES.into_iter().flat_map(|state| {
            (0..=u8::MAX).map(move |byte| (state, byte, TABLE[state as usize][usize::from(byte)]))
        })
    }

    #[test]
    fn the_table_follows_the_rules() {
        for (row, &state) in STATES.iter().enumerate() {
            assert_eq!(row, state as usize, "{state:?} is out of place");
        }
        for (state, byte, found) in transitions() {
            assert_eq!(found, transition(state, byte));
        }
    }

    #[test]
    fn can_sub_and_esc_work_from_every_state() {
        for state in STATES {
            for byte in [0x18, 0x1A] {
                assert_eq!(transition(state, byte).next, State::Ground, "{state:?}");
            }
            let next = transition(state, 0x1B).next;
            assert!(
                matches!(next, State::Escape | State::StringEscape),
                "{state:?}"
            );
        }
    }

    #[test]
    fn the_rest_of_a_string_past_its_limit_is_consumed_like_sos_and_pm() {
        for byte in 0..=u8::MAX {
            let sos_pm = transition(State::SosPmString, byte);
            let same_state = |next| Transition {
                next: if sos_pm.next == State::SosPmString {
                    next
                } else {
                    sos_pm.next
                },
                ..sos_pm
            };
            assert_eq!(
                transition(State::ApcIgnore, byte),
                same_state(State::ApcIgnore),
                "{byte:#04x}"
            );
            let osc = transition(State::OscIgnore, byte);
            if byte == 0x07 {
                assert_eq!(
                    osc,
                    Transition {
                        action: Action::Ignore,
                        next: State::Ground
                    }
                );
            } else {
                assert_eq!(osc, same_state(State::OscIgnore), "{byte:#04x}");
            }
        }
    }

    #[test]
    fn a_new_sequence_starts_clear() {
        for (state, byte, Transition { action, next }) in transitions() {
            let starts = matches!(
                next,
                State::Escape | State::StringEscape | State::CsiEntry | State::DcsEntry
            );
            if starts && next != state {
                assert!(
                    matches!(
                        action,
                        Action::Clear | Action::DcsEnd | Action::OscEnd | Action::ApcEnd
                    ),
                    "{state:?} {byte:#04x} leads to {next:?} with {action:?}"
                );
            }
        }
    }

    #[test]
    fn strings_start_put_and_end_in_their_own_state() {
        for (state, byte, Transition { action, next }) in transitions() {
            let (string, entered) = match action {
                Action::OscStart => (State::OscString, true),
                Action::DcsStart => (State::DcsPassthrough, true),
                Action::ApcStart => (State::ApcString, true),
                Action::OscPut | Action::OscEnd | Action::OscBel | Action::OscCancel => {
                    (State::OscString, false)
                }
                Action::DcsPut | Action::DcsEnd | Action::DcsCancel => {
                    (State::DcsPassthrough, false)
                }
                Action::ApcPut | Action::ApcEnd | Action::ApcCancel => (State::ApcString, false),
                _ => {
                    // Leaving a string without ending it would break the
                    // handler's protocol.
                    let string = matches!(
                        state,
                        State::OscString | State::DcsPassthrough | State::ApcString
                    );
                    assert!(!string || next == state, "{state:?} {byte:#04x}");
                    continue;
                }
            };
            if entered {
                assert_eq!(next, string, "{state:?} {byte:#04x}");
            } else {
                assert_eq!(state, string, "{state:?} {byte:#04x}");
            }
        }
    }
}
