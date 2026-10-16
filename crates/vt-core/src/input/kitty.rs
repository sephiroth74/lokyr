//! The kitty keyboard protocol, with which programs have keys reported
//! without the ambiguities of xterm's sequences:
//! <https://sw.kovidgoyal.net/kitty/keyboard-protocol/>.

use std::io::Write;

use super::keyboard::{Key, KeyAction, KeyEvent, KeyOptions, Keypad, encode_legacy, single_char};
use crate::terminal::{InputModes, KeyboardFlags};

// The bits of the modifiers, as the protocol numbers them.
const SHIFT: u8 = 1;
/// Alt, when it acts as Meta. An Alt that does not is no modifier: the
/// platform composed the key's text with it.
const ALT: u8 = 2;
const CONTROL: u8 = 4;
const SUPER: u8 = 8;
const HYPER: u8 = 16;
const META: u8 = 32;
const CAPS_LOCK: u8 = 64;
const NUM_LOCK: u8 = 128;
/// The locks, which do not count as modifiers held for Enter, Tab and
/// Backspace.
const LOCKS: u8 = CAPS_LOCK | NUM_LOCK;
/// The modifiers with which a key types no text.
const NO_TEXT: u8 = ALT | CONTROL | SUPER | HYPER | META;

/// Appends to `out` what `event` sends with the kitty protocol's `flags`,
/// of which one at least is set.
///
/// Keys that type text send it, unless every key is reported, and so do
/// Enter, Tab and Backspace without modifiers, as xterm sends them, so
/// that a shell stays usable after a program that left the flags set.
/// Other keys send `CSI code ; modifiers u`, or the forms that xterm has
/// for the functional keys, `CSI 1 ; modifiers final` and
/// `CSI code ; modifiers ~`. Without the flags that disambiguate or report
/// every key, presses that xterm has a sequence for send it, as without
/// flags, unless Super, Hyper, Meta or a lock is held, which xterm's
/// sequences cannot carry.
pub(super) fn encode_key(
    event: &KeyEvent<'_>,
    modes: &InputModes,
    flags: KeyboardFlags,
    options: KeyOptions,
    out: &mut Vec<u8>,
) {
    let action = if flags.contains(KeyboardFlags::REPORT_EVENTS) {
        event.action
    } else if event.action == KeyAction::Release {
        return;
    } else {
        // Without event types, a repeat is one more press.
        KeyAction::Press
    };
    let all_keys = flags.contains(KeyboardFlags::REPORT_ALL_KEYS);
    if event.key.is_modifier() && !all_keys {
        return;
    }
    let mods = modifiers(event, options);
    let text = typed_text(event, mods);
    if !all_keys {
        // A release types nothing, so it is reported as a key.
        if let Some(text) = text
            && action != KeyAction::Release
        {
            out.extend_from_slice(text.as_bytes());
            return;
        }
        if matches!(event.key, Key::Enter | Key::Tab | Key::Backspace) && mods & !LOCKS == 0 {
            if action != KeyAction::Release {
                encode_legacy(event, modes, options, out);
            }
            return;
        }
        // xterm's sequences carry Shift, Control and an Alt that acts as
        // Meta, but no other modifier and no lock: a press with those sends
        // its escape code, as its release does.
        if action == KeyAction::Press
            && !flags.contains(KeyboardFlags::DISAMBIGUATE)
            && mods & !(SHIFT | ALT | CONTROL) == 0
        {
            let start = out.len();
            encode_legacy(event, modes, options, out);
            // Keys that xterm sends nothing for are reported as keys.
            if out.len() > start {
                return;
            }
        }
    }
    let Some((code, final_byte)) = number(event.key) else {
        return;
    };
    // Only keys that type characters have alternates.
    let (shifted, base_layout) = match event.key {
        Key::Char(c) if flags.contains(KeyboardFlags::REPORT_ALTERNATES) => (
            single_char(event.unmodified).filter(|&shifted| mods & SHIFT != 0 && shifted != c),
            event.base_layout.filter(|&base| base != c),
        ),
        _ => (None, None),
    };
    let text =
        text.filter(|_| flags.contains(KeyboardFlags::REPORT_TEXT) && action != KeyAction::Release);
    Sequence {
        code,
        final_byte,
        shifted,
        base_layout,
        mods,
        action,
        text,
    }
    .write(out);
}

/// Appends `CSI 0 ; ; codepoints u` for `text`, which no key typed, with
/// its control characters left out: nothing if they are all it has.
pub(super) fn encode_text(text: &str, out: &mut Vec<u8>) {
    if text.chars().all(char::is_control) {
        return;
    }
    Sequence {
        code: 0,
        final_byte: b'u',
        shifted: None,
        base_layout: None,
        mods: 0,
        action: KeyAction::Press,
        text: Some(text),
    }
    .write(out);
}

/// The bits of the modifiers held with `event`, and of the locks on.
fn modifiers(event: &KeyEvent<'_>, options: KeyOptions) -> u8 {
    let mods = event.mods;
    [
        (mods.shift, SHIFT),
        (mods.alt_as_meta(options), ALT),
        (mods.control, CONTROL),
        (mods.super_key, SUPER),
        (mods.hyper, HYPER),
        (mods.meta, META),
        (mods.caps_lock, CAPS_LOCK),
        (mods.num_lock, NUM_LOCK),
    ]
    .into_iter()
    .filter(|&(held, _)| held)
    .fold(0, |bits, (_, bit)| bits | bit)
}

/// The text that `event`'s key types, if it is a key that types: none
/// with a modifier that keeps keys from typing, and none when the
/// platform's text has a control character, which is the key's sequence
/// rather than text.
fn typed_text<'a>(event: &KeyEvent<'a>, mods: u8) -> Option<&'a str> {
    let types = matches!(event.key, Key::Char(_) | Key::Keypad(_));
    let text = event.text;
    (types && mods & NO_TEXT == 0 && !text.is_empty() && !text.chars().any(char::is_control))
        .then_some(text)
}

/// The number of `key` in the protocol, and the final byte of its escape
/// code. A key that types a character has the character's number; the
/// others have the numbers of the protocol's table of functional keys.
fn number(key: Key) -> Option<(u32, u8)> {
    Some(match key {
        Key::Char(c) => (u32::from(c), b'u'),
        Key::Escape => (27, b'u'),
        Key::Enter => (13, b'u'),
        Key::Tab => (9, b'u'),
        Key::Backspace => (127, b'u'),
        Key::Insert => (2, b'~'),
        Key::Delete => (3, b'~'),
        Key::Left => (1, b'D'),
        Key::Right => (1, b'C'),
        Key::Up => (1, b'A'),
        Key::Down => (1, b'B'),
        Key::PageUp => (5, b'~'),
        Key::PageDown => (6, b'~'),
        Key::Home => (1, b'H'),
        Key::End => (1, b'F'),
        Key::CapsLock => (57358, b'u'),
        Key::ScrollLock => (57359, b'u'),
        Key::NumLock => (57360, b'u'),
        Key::PrintScreen => (57361, b'u'),
        Key::Pause => (57362, b'u'),
        Key::Menu => (57363, b'u'),
        Key::F(1) => (1, b'P'),
        Key::F(2) => (1, b'Q'),
        // Not `CSI R`, which a cursor position report also is.
        Key::F(3) => (13, b'~'),
        Key::F(4) => (1, b'S'),
        Key::F(n @ 5..=12) => {
            const CODES: [u32; 8] = [15, 17, 18, 19, 20, 21, 23, 24];
            (CODES[usize::from(n - 5)], b'~')
        }
        Key::F(n @ 13..=35) => (57376 + u32::from(n - 13), b'u'),
        Key::F(_) => return None,
        Key::Keypad(key) => keypad_number(key),
        Key::Media(key) => (key as u32, b'u'),
        Key::Modifier(key) => (key as u32, b'u'),
    })
}

/// The number of a key of the keypad, which has its own even where it
/// stands for another key.
fn keypad_number(key: Keypad) -> (u32, u8) {
    let code = match key {
        Keypad::Digit(digit) => 57399 + u32::from(digit.min(9)),
        Keypad::Decimal => 57409,
        Keypad::Divide => 57410,
        Keypad::Multiply => 57411,
        Keypad::Subtract => 57412,
        Keypad::Add => 57413,
        Keypad::Enter => 57414,
        Keypad::Equal => 57415,
        Keypad::Separator => 57416,
        Keypad::Left => 57417,
        Keypad::Right => 57418,
        Keypad::Up => 57419,
        Keypad::Down => 57420,
        Keypad::PageUp => 57421,
        Keypad::PageDown => 57422,
        Keypad::Home => 57423,
        Keypad::End => 57424,
        Keypad::Insert => 57425,
        Keypad::Delete => 57426,
        Keypad::Begin => return (1, b'E'),
    };
    (code, b'u')
}

/// An escape code of the protocol:
/// `CSI code : shifted : base ; modifiers : action ; text final`.
struct Sequence<'a> {
    code: u32,
    final_byte: u8,
    shifted: Option<char>,
    base_layout: Option<char>,
    mods: u8,
    action: KeyAction,
    text: Option<&'a str>,
}

impl Sequence<'_> {
    /// Writes the escape code, without the fields that hold nothing, and
    /// without the code when it is 1 and nothing follows it.
    fn write(&self, out: &mut Vec<u8>) {
        let alternates = self.shifted.is_some() || self.base_layout.is_some();
        let modifiers = self.mods != 0 || self.action != KeyAction::Press;
        out.extend_from_slice(b"\x1b[");
        if self.code != 1 || alternates || modifiers || self.text.is_some() {
            write!(out, "{}", self.code).expect("writing to a Vec");
        }
        if alternates {
            out.push(b':');
            if let Some(shifted) = self.shifted {
                write!(out, "{}", u32::from(shifted)).expect("writing to a Vec");
            }
            if let Some(base) = self.base_layout {
                write!(out, ":{}", u32::from(base)).expect("writing to a Vec");
            }
        }
        if modifiers || self.text.is_some() {
            out.push(b';');
        }
        if modifiers {
            write!(out, "{}", u16::from(self.mods) + 1).expect("writing to a Vec");
            match self.action {
                KeyAction::Press => {}
                KeyAction::Repeat => out.extend_from_slice(b":2"),
                KeyAction::Release => out.extend_from_slice(b":3"),
            }
        }
        if let Some(text) = self.text {
            let mut separator = b';';
            for c in text.chars().filter(|c| !c.is_control()) {
                out.push(separator);
                write!(out, "{}", u32::from(c)).expect("writing to a Vec");
                separator = b':';
            }
        }
        out.push(self.final_byte);
    }
}

#[cfg(test)]
mod tests {
    use crate::input::{
        Key, KeyAction, KeyEvent, KeyOptions, Keypad, Media, ModifierKey, Mods, encode_key,
        encode_text,
    };
    use crate::terminal::{InputModes, KeyboardFlags};

    const DISAMBIGUATE: u16 = 1;
    const EVENTS: u16 = 2;
    const ALTERNATES: u16 = 4;
    const ALL_KEYS: u16 = 8;
    const TEXT: u16 = 16;

    /// The modifiers of `held`, such as `ctrl+shift`, with `caps` and
    /// `num` for the locks.
    fn mods(held: &str) -> Mods {
        let mut mods = Mods::NONE;
        for name in held.split('+').filter(|name| !name.is_empty()) {
            match name {
                "shift" => mods.shift = true,
                "ctrl" => mods.control = true,
                "alt" => mods.left_alt = true,
                "super" => mods.super_key = true,
                "hyper" => mods.hyper = true,
                "meta" => mods.meta = true,
                "caps" => mods.caps_lock = true,
                "num" => mods.num_lock = true,
                _ => panic!("no modifier {name}"),
            }
        }
        mods
    }

    /// A press of `key` with `held`, which types `text`, and `unmodified`
    /// with Shift alone.
    fn event<'a>(key: Key, held: &str, text: &'a str, unmodified: &'a str) -> KeyEvent<'a> {
        KeyEvent {
            key,
            action: KeyAction::Press,
            mods: mods(held),
            text,
            unmodified,
            base_layout: None,
        }
    }

    /// A press of `key`, a key that types nothing, with `held`.
    fn named(key: Key, held: &str) -> KeyEvent<'static> {
        event(key, held, "", "")
    }

    fn send_in(event: KeyEvent<'_>, flags: u16, modes: &InputModes) -> String {
        let mut out = Vec::new();
        let flags = KeyboardFlags::from_bits(flags);
        encode_key(&event, modes, flags, KeyOptions::default(), &mut out);
        String::from_utf8(out).unwrap()
    }

    /// What `event` sends with `flags`, both Alt keys acting as Meta.
    fn send(event: KeyEvent<'_>, flags: u16) -> String {
        send_in(event, flags, &InputModes::default())
    }

    fn with(action: KeyAction, event: KeyEvent<'_>) -> KeyEvent<'_> {
        KeyEvent { action, ..event }
    }

    fn text(text: &str, flags: u16) -> String {
        let mut out = Vec::new();
        encode_text(text, KeyboardFlags::from_bits(flags), &mut out);
        String::from_utf8(out).unwrap()
    }

    /// What the platform types, what the key types with Shift alone, and
    /// what is sent.
    type Typed = (&'static str, &'static str, &'static str);

    #[test]
    fn legacy_text_keys_match_the_specification_s_examples() {
        // Each key, with plain, shift, alt, ctrl, shift+alt and alt+ctrl.
        let table: [(char, [Typed; 6]); 3] = [
            (
                'i',
                [
                    ("i", "i", "i"),
                    ("I", "I", "I"),
                    ("ˆ", "i", "\x1bi"),
                    ("\t", "i", "\t"),
                    ("ˆ", "I", "\x1bI"),
                    ("\t", "i", "\x1b\t"),
                ],
            ),
            (
                '3',
                [
                    ("3", "3", "3"),
                    ("#", "#", "#"),
                    ("£", "3", "\x1b3"),
                    ("", "3", "\x1b"),
                    ("‹", "#", "\x1b#"),
                    ("", "3", "\x1b\x1b"),
                ],
            ),
            (
                ';',
                [
                    (";", ";", ";"),
                    (":", ":", ":"),
                    ("…", ";", "\x1b;"),
                    (";", ";", ";"),
                    ("Ú", ":", "\x1b:"),
                    (";", ";", "\x1b;"),
                ],
            ),
        ];
        let columns = ["", "shift", "alt", "ctrl", "shift+alt", "alt+ctrl"];
        for (key, row) in table {
            for (held, (typed, unmodified, expected)) in columns.into_iter().zip(row) {
                let sent = send(event(Key::Char(key), held, typed, unmodified), 0);
                assert_eq!(sent, expected, "{held}+{key}");
            }
        }
        // Control and Shift lose Shift in xterm's encoding; disambiguated,
        // they send the key, unshifted, with both modifiers.
        for (key, shifted, expected) in [
            ('i', "I", "\x1b[105;6u"),
            ('3', "#", "\x1b[51;6u"),
            (';', ":", "\x1b[59;6u"),
        ] {
            let sent = send(
                event(Key::Char(key), "ctrl+shift", "", shifted),
                DISAMBIGUATE,
            );
            assert_eq!(sent, expected, "ctrl+shift+{key}");
        }
    }

    #[test]
    fn legacy_c0_keys_match_the_specification_s_table() {
        let columns = [
            "",
            "ctrl",
            "alt",
            "shift",
            "ctrl+shift",
            "alt+shift",
            "ctrl+alt",
        ];
        let table: [(Key, [&str; 7]); 5] = [
            (
                Key::Enter,
                ["\r", "\r", "\x1b\r", "\r", "\r", "\x1b\r", "\x1b\r"],
            ),
            (
                Key::Escape,
                [
                    "\x1b", "\x1b", "\x1b\x1b", "\x1b", "\x1b", "\x1b\x1b", "\x1b\x1b",
                ],
            ),
            (
                Key::Backspace,
                [
                    "\x7f", "\x08", "\x1b\x7f", "\x7f", "\x08", "\x1b\x7f", "\x1b\x08",
                ],
            ),
            (
                Key::Tab,
                [
                    "\t",
                    "\t",
                    "\x1b\t",
                    "\x1b[Z",
                    "\x1b[Z",
                    "\x1b\x1b[Z",
                    "\x1b\t",
                ],
            ),
            (
                Key::Char(' '),
                [" ", "\0", "\x1b ", " ", "\0", "\x1b ", "\x1b\0"],
            ),
        ];
        for (key, row) in table {
            for (held, expected) in columns.into_iter().zip(row) {
                let typed = if key == Key::Char(' ') { " " } else { "" };
                assert_eq!(
                    send(event(key, held, typed, typed), 0),
                    expected,
                    "{held} {key:?}"
                );
            }
        }
    }

    #[test]
    fn legacy_functional_keys_match_the_specification_s_table() {
        let cursor_keys = InputModes {
            cursor_keys_application: true,
            ..InputModes::default()
        };
        let table = [
            (Key::Insert, "\x1b[2~", "\x1b[2~"),
            (Key::Delete, "\x1b[3~", "\x1b[3~"),
            (Key::PageUp, "\x1b[5~", "\x1b[5~"),
            (Key::PageDown, "\x1b[6~", "\x1b[6~"),
            (Key::Up, "\x1b[A", "\x1bOA"),
            (Key::Down, "\x1b[B", "\x1bOB"),
            (Key::Right, "\x1b[C", "\x1bOC"),
            (Key::Left, "\x1b[D", "\x1bOD"),
            (Key::Home, "\x1b[H", "\x1bOH"),
            (Key::End, "\x1b[F", "\x1bOF"),
            (Key::F(1), "\x1bOP", "\x1bOP"),
            (Key::F(2), "\x1bOQ", "\x1bOQ"),
            (Key::F(3), "\x1bOR", "\x1bOR"),
            (Key::F(4), "\x1bOS", "\x1bOS"),
            (Key::F(5), "\x1b[15~", "\x1b[15~"),
            (Key::F(6), "\x1b[17~", "\x1b[17~"),
            (Key::F(7), "\x1b[18~", "\x1b[18~"),
            (Key::F(8), "\x1b[19~", "\x1b[19~"),
            (Key::F(9), "\x1b[20~", "\x1b[20~"),
            (Key::F(10), "\x1b[21~", "\x1b[21~"),
            (Key::F(11), "\x1b[23~", "\x1b[23~"),
            (Key::F(12), "\x1b[24~", "\x1b[24~"),
            (Key::Menu, "\x1b[29~", "\x1b[29~"),
        ];
        for (key, normal, application) in table {
            assert_eq!(send(named(key, ""), 0), normal, "{key:?}");
            assert_eq!(
                send_in(named(key, ""), 0, &cursor_keys),
                application,
                "{key:?} in cursor key mode"
            );
        }
        // Modifiers turn the third form into the second.
        assert_eq!(send(named(Key::F(1), "shift"), 0), "\x1b[1;2P");
        assert_eq!(send(named(Key::Up, "ctrl"), 0), "\x1b[1;5A");
    }

    #[test]
    fn functional_keys_have_the_numbers_of_the_specification_s_table() {
        // As the table writes them: the number and the final byte.
        let mut table = vec![
            (Key::Escape, "27 u"),
            (Key::Enter, "13 u"),
            (Key::Tab, "9 u"),
            (Key::Backspace, "127 u"),
            (Key::Insert, "2 ~"),
            (Key::Delete, "3 ~"),
            (Key::Left, "1 D"),
            (Key::Right, "1 C"),
            (Key::Up, "1 A"),
            (Key::Down, "1 B"),
            (Key::PageUp, "5 ~"),
            (Key::PageDown, "6 ~"),
            (Key::Home, "1 H"),
            (Key::End, "1 F"),
            (Key::CapsLock, "57358 u"),
            (Key::ScrollLock, "57359 u"),
            (Key::NumLock, "57360 u"),
            (Key::PrintScreen, "57361 u"),
            (Key::Pause, "57362 u"),
            (Key::Menu, "57363 u"),
            (Key::F(1), "1 P"),
            (Key::F(2), "1 Q"),
            (Key::F(3), "13 ~"),
            (Key::F(4), "1 S"),
            (Key::F(5), "15 ~"),
            (Key::F(6), "17 ~"),
            (Key::F(7), "18 ~"),
            (Key::F(8), "19 ~"),
            (Key::F(9), "20 ~"),
            (Key::F(10), "21 ~"),
            (Key::F(11), "23 ~"),
            (Key::F(12), "24 ~"),
            (Key::Keypad(Keypad::Decimal), "57409 u"),
            (Key::Keypad(Keypad::Divide), "57410 u"),
            (Key::Keypad(Keypad::Multiply), "57411 u"),
            (Key::Keypad(Keypad::Subtract), "57412 u"),
            (Key::Keypad(Keypad::Add), "57413 u"),
            (Key::Keypad(Keypad::Enter), "57414 u"),
            (Key::Keypad(Keypad::Equal), "57415 u"),
            (Key::Keypad(Keypad::Separator), "57416 u"),
            (Key::Keypad(Keypad::Left), "57417 u"),
            (Key::Keypad(Keypad::Right), "57418 u"),
            (Key::Keypad(Keypad::Up), "57419 u"),
            (Key::Keypad(Keypad::Down), "57420 u"),
            (Key::Keypad(Keypad::PageUp), "57421 u"),
            (Key::Keypad(Keypad::PageDown), "57422 u"),
            (Key::Keypad(Keypad::Home), "57423 u"),
            (Key::Keypad(Keypad::End), "57424 u"),
            (Key::Keypad(Keypad::Insert), "57425 u"),
            (Key::Keypad(Keypad::Delete), "57426 u"),
            (Key::Keypad(Keypad::Begin), "1 E"),
            (Key::Media(Media::Play), "57428 u"),
            (Key::Media(Media::Pause), "57429 u"),
            (Key::Media(Media::PlayPause), "57430 u"),
            (Key::Media(Media::Reverse), "57431 u"),
            (Key::Media(Media::Stop), "57432 u"),
            (Key::Media(Media::FastForward), "57433 u"),
            (Key::Media(Media::Rewind), "57434 u"),
            (Key::Media(Media::TrackNext), "57435 u"),
            (Key::Media(Media::TrackPrevious), "57436 u"),
            (Key::Media(Media::Record), "57437 u"),
            (Key::Media(Media::LowerVolume), "57438 u"),
            (Key::Media(Media::RaiseVolume), "57439 u"),
            (Key::Media(Media::MuteVolume), "57440 u"),
            (Key::Modifier(ModifierKey::LeftShift), "57441 u"),
            (Key::Modifier(ModifierKey::LeftControl), "57442 u"),
            (Key::Modifier(ModifierKey::LeftAlt), "57443 u"),
            (Key::Modifier(ModifierKey::LeftSuper), "57444 u"),
            (Key::Modifier(ModifierKey::LeftHyper), "57445 u"),
            (Key::Modifier(ModifierKey::LeftMeta), "57446 u"),
            (Key::Modifier(ModifierKey::RightShift), "57447 u"),
            (Key::Modifier(ModifierKey::RightControl), "57448 u"),
            (Key::Modifier(ModifierKey::RightAlt), "57449 u"),
            (Key::Modifier(ModifierKey::RightSuper), "57450 u"),
            (Key::Modifier(ModifierKey::RightHyper), "57451 u"),
            (Key::Modifier(ModifierKey::RightMeta), "57452 u"),
            (Key::Modifier(ModifierKey::IsoLevel3Shift), "57453 u"),
            (Key::Modifier(ModifierKey::IsoLevel5Shift), "57454 u"),
        ];
        let numbers: Vec<String> = (13..=35)
            .map(|n| format!("{} u", 57376 + n - 13))
            .chain((0..=9).map(|n| format!("{} u", 57399 + n)))
            .collect();
        table.extend(
            (13..=35)
                .map(Key::F)
                .zip(numbers[..23].iter().map(String::as_str)),
        );
        table.extend(
            (0..=9)
                .map(|n| Key::Keypad(Keypad::Digit(n)))
                .zip(numbers[23..].iter().map(String::as_str)),
        );
        for (key, code) in table {
            let (number, final_byte) = code.split_once(' ').unwrap();
            // The number 1 goes when nothing follows it.
            let plain = if number == "1" {
                format!("\x1b[{final_byte}")
            } else {
                format!("\x1b[{number}{final_byte}")
            };
            assert_eq!(send(named(key, ""), ALL_KEYS), plain, "{key:?}");
            assert_eq!(
                send(named(key, "shift"), ALL_KEYS),
                format!("\x1b[{number};2{final_byte}"),
                "shift {key:?}"
            );
        }
        // The number 1 stays when alternates or text follow it.
        let begin = event(Key::Keypad(Keypad::Begin), "", "5", "5");
        assert_eq!(send(begin, ALL_KEYS | TEXT), "\x1b[1;;53E");
        let one = KeyEvent {
            base_layout: Some('a'),
            ..named(Key::Char('\u{1}'), "")
        };
        assert_eq!(send(one, DISAMBIGUATE | ALTERNATES), "\x1b[1::97u");
        // A digit past 9 counts as 9.
        let digit = named(Key::Keypad(Keypad::Digit(12)), "");
        assert_eq!(send(digit, ALL_KEYS), "\x1b[57408u");
        // Keys past the table send nothing.
        assert_eq!(send(named(Key::F(36), ""), ALL_KEYS), "");
        assert_eq!(send(named(Key::F(0), ""), DISAMBIGUATE), "");
    }

    #[test]
    fn disambiguation_sends_ambiguous_keys_as_escape_codes() {
        let a = |held, typed, unmodified| event(Key::Char('a'), held, typed, unmodified);
        assert_eq!(send(a("", "a", "a"), DISAMBIGUATE), "a");
        assert_eq!(send(a("shift", "A", "A"), DISAMBIGUATE), "A");
        assert_eq!(send(named(Key::Escape, ""), DISAMBIGUATE), "\x1b[27u");
        assert_eq!(send(named(Key::Escape, "alt"), DISAMBIGUATE), "\x1b[27;3u");
        for (held, expected) in [
            ("ctrl", "\x1b[97;5u"),
            ("alt", "\x1b[97;3u"),
            ("ctrl+shift", "\x1b[97;6u"),
            ("alt+shift", "\x1b[97;4u"),
            ("ctrl+alt", "\x1b[97;7u"),
            ("super", "\x1b[97;9u"),
            ("hyper", "\x1b[97;17u"),
            ("meta", "\x1b[97;33u"),
        ] {
            let typed = if held.contains("ctrl") { "\x01" } else { "å" };
            let unmodified = if held.contains("shift") { "A" } else { "a" };
            assert_eq!(
                send(a(held, typed, unmodified), DISAMBIGUATE),
                expected,
                "{held}"
            );
        }
        // Alt with `[` no longer starts a CSI, nor Control with `c` a SIGINT.
        let bracket = event(Key::Char('['), "alt", "“", "[");
        assert_eq!(send(bracket, DISAMBIGUATE), "\x1b[91;3u");
        let c = event(Key::Char('c'), "ctrl", "\x03", "c");
        assert_eq!(send(c, DISAMBIGUATE), "\x1b[99;5u");
        let space = event(Key::Char(' '), "ctrl", "\0", " ");
        assert_eq!(send(space, DISAMBIGUATE), "\x1b[32;5u");
        // An Option that does not act as Meta types its character.
        let options = KeyOptions {
            left_alt_is_meta: false,
            right_alt_is_meta: true,
        };
        let mut out = Vec::new();
        let flags = KeyboardFlags::from_bits(DISAMBIGUATE);
        encode_key(
            &a("alt", "å", "a"),
            &InputModes::default(),
            flags,
            options,
            &mut out,
        );
        assert_eq!(out, "å".as_bytes());
    }

    #[test]
    fn enter_tab_and_backspace_stay_as_they_are_without_modifiers() {
        assert_eq!(send(named(Key::Enter, ""), DISAMBIGUATE), "\r");
        assert_eq!(send(named(Key::Tab, ""), DISAMBIGUATE), "\t");
        assert_eq!(send(named(Key::Backspace, ""), DISAMBIGUATE), "\x7f");
        assert_eq!(send(named(Key::Enter, "shift"), DISAMBIGUATE), "\x1b[13;2u");
        assert_eq!(send(named(Key::Tab, "shift"), DISAMBIGUATE), "\x1b[9;2u");
        assert_eq!(
            send(named(Key::Backspace, "ctrl"), DISAMBIGUATE),
            "\x1b[127;5u"
        );
        assert_eq!(send(named(Key::Enter, "alt"), DISAMBIGUATE), "\x1b[13;3u");
        // The locks are no modifiers for them, and LNM still applies.
        assert_eq!(send(named(Key::Enter, "caps+num"), DISAMBIGUATE), "\r");
        let newline = InputModes {
            newline: true,
            ..InputModes::default()
        };
        assert_eq!(
            send_in(named(Key::Enter, ""), DISAMBIGUATE, &newline),
            "\r\n"
        );
    }

    #[test]
    fn disambiguated_functional_and_keypad_keys() {
        let cursor_keys = InputModes {
            cursor_keys_application: true,
            keypad_application: true,
            ..InputModes::default()
        };
        // Cursor key mode is legacy, and so is the keypad's.
        assert_eq!(
            send_in(named(Key::Up, ""), DISAMBIGUATE, &cursor_keys),
            "\x1b[A"
        );
        assert_eq!(send(named(Key::Up, "ctrl"), DISAMBIGUATE), "\x1b[1;5A");
        assert_eq!(send(named(Key::F(1), ""), DISAMBIGUATE), "\x1b[P");
        assert_eq!(send(named(Key::F(3), ""), DISAMBIGUATE), "\x1b[13~");
        assert_eq!(send(named(Key::F(13), ""), DISAMBIGUATE), "\x1b[57376u");
        assert_eq!(send(named(Key::PageUp, "alt"), DISAMBIGUATE), "\x1b[5;3~");
        let keypad = |key, held, typed| event(Key::Keypad(key), held, typed, typed);
        assert_eq!(
            send_in(
                keypad(Keypad::Digit(0), "", "0"),
                DISAMBIGUATE,
                &cursor_keys
            ),
            "0",
            "a keypad key that types"
        );
        assert_eq!(
            send(keypad(Keypad::Digit(0), "ctrl", ""), DISAMBIGUATE),
            "\x1b[57399;5u"
        );
        assert_eq!(
            send(keypad(Keypad::PageUp, "", ""), DISAMBIGUATE),
            "\x1b[57421u"
        );
        assert_eq!(
            send(keypad(Keypad::Enter, "", "\r"), DISAMBIGUATE),
            "\x1b[57414u"
        );
        assert_eq!(send(keypad(Keypad::Begin, "", ""), DISAMBIGUATE), "\x1b[E");
        assert_eq!(
            send(keypad(Keypad::Begin, "ctrl", ""), DISAMBIGUATE),
            "\x1b[1;5E"
        );
        // modifyOtherKeys is legacy too.
        let other_keys = InputModes {
            modify_other_keys: 2,
            ..InputModes::default()
        };
        let ctrl_a = event(Key::Char('a'), "ctrl", "\x01", "a");
        assert_eq!(send_in(ctrl_a, DISAMBIGUATE, &other_keys), "\x1b[97;5u");
    }

    #[test]
    fn locks_are_reported_for_keys_that_type_nothing() {
        let a = event(Key::Char('a'), "caps", "A", "a");
        assert_eq!(send(a, DISAMBIGUATE), "A");
        assert_eq!(send(a, ALL_KEYS), "\x1b[97;65u");
        assert_eq!(
            send(named(Key::Escape, "num"), DISAMBIGUATE),
            "\x1b[27;129u"
        );
        assert_eq!(send(named(Key::Up, "caps"), DISAMBIGUATE), "\x1b[1;65A");
        let every = mods("shift+ctrl+alt+super+hyper+meta+caps+num");
        let all = KeyEvent {
            mods: every,
            ..named(Key::Escape, "")
        };
        assert_eq!(send(all, DISAMBIGUATE), "\x1b[27;256u");
    }

    #[test]
    fn modifier_keys_are_reported_only_with_every_key() {
        let shift = named(Key::Modifier(ModifierKey::LeftShift), "shift");
        assert_eq!(send(shift, DISAMBIGUATE), "");
        assert_eq!(send(shift, 0), "");
        assert_eq!(
            send(named(Key::CapsLock, "caps"), DISAMBIGUATE | EVENTS),
            ""
        );
        assert_eq!(send(named(Key::NumLock, ""), DISAMBIGUATE), "");
        assert_eq!(send(named(Key::ScrollLock, ""), DISAMBIGUATE), "");
        assert_eq!(send(shift, ALL_KEYS), "\x1b[57441;2u");
        // The modifiers are as the event leaves them.
        let control = Key::Modifier(ModifierKey::LeftControl);
        assert_eq!(
            send(named(control, "ctrl"), ALL_KEYS | EVENTS),
            "\x1b[57442;5u"
        );
        let released = with(KeyAction::Release, named(control, ""));
        assert_eq!(send(released, ALL_KEYS | EVENTS), "\x1b[57442;1:3u");
        assert_eq!(
            send(named(Key::CapsLock, "caps"), ALL_KEYS),
            "\x1b[57358;65u"
        );
    }

    #[test]
    fn event_types_report_repeats_and_releases() {
        let flags = DISAMBIGUATE | EVENTS;
        let a = event(Key::Char('a'), "", "a", "a");
        assert_eq!(send(a, flags), "a");
        assert_eq!(
            send(with(KeyAction::Repeat, a), flags),
            "a",
            "text repeats as text"
        );
        assert_eq!(send(with(KeyAction::Release, a), flags), "\x1b[97;1:3u");
        let shifted = event(Key::Char('a'), "shift", "A", "A");
        assert_eq!(
            send(with(KeyAction::Release, shifted), flags),
            "\x1b[97;2:3u"
        );
        let ctrl_a = event(Key::Char('a'), "ctrl", "\x01", "a");
        assert_eq!(send(with(KeyAction::Repeat, ctrl_a), flags), "\x1b[97;5:2u");
        assert_eq!(
            send(with(KeyAction::Release, named(Key::Up, "")), flags),
            "\x1b[1;1:3A"
        );
        assert_eq!(
            send(with(KeyAction::Repeat, named(Key::Escape, "")), flags),
            "\x1b[27;1:2u"
        );
        // Enter, Tab and Backspace have no releases, with or without the
        // locks, unless every key is reported.
        for key in [Key::Enter, Key::Tab, Key::Backspace] {
            assert_eq!(
                send(with(KeyAction::Release, named(key, "")), flags),
                "",
                "{key:?}"
            );
            let locked = named(key, "caps+num");
            assert_eq!(send(with(KeyAction::Release, locked), flags), "", "{key:?}");
            assert!(!send(with(KeyAction::Repeat, named(key, "")), flags).is_empty());
        }
        assert_eq!(send(named(Key::Backspace, ""), flags), "\x7f");
        assert_eq!(
            send(
                with(KeyAction::Release, named(Key::Enter, "")),
                flags | ALL_KEYS
            ),
            "\x1b[13;1:3u"
        );
        assert_eq!(
            send(with(KeyAction::Release, named(Key::Enter, "shift")), flags),
            "\x1b[13;2:3u"
        );
    }

    #[test]
    fn without_event_types_a_repeat_is_a_press_and_a_release_sends_nothing() {
        let ctrl_a = event(Key::Char('a'), "ctrl", "\x01", "a");
        assert_eq!(
            send(with(KeyAction::Repeat, ctrl_a), DISAMBIGUATE),
            "\x1b[97;5u"
        );
        assert_eq!(send(with(KeyAction::Release, ctrl_a), DISAMBIGUATE), "");
        let a = event(Key::Char('a'), "", "a", "a");
        assert_eq!(send(with(KeyAction::Repeat, a), ALL_KEYS), "\x1b[97u");
        assert_eq!(send(with(KeyAction::Release, a), ALL_KEYS), "");
        assert_eq!(send(with(KeyAction::Repeat, a), 0), "a");
        assert_eq!(send(with(KeyAction::Release, a), 0), "");
    }

    #[test]
    fn event_types_alone_keep_xterm_s_presses() {
        let cursor_keys = InputModes {
            cursor_keys_application: true,
            ..InputModes::default()
        };
        assert_eq!(send(named(Key::Escape, ""), EVENTS), "\x1b");
        assert_eq!(
            send(with(KeyAction::Release, named(Key::Escape, "")), EVENTS),
            "\x1b[27;1:3u"
        );
        let ctrl_a = event(Key::Char('a'), "ctrl", "\x01", "a");
        assert_eq!(send(ctrl_a, EVENTS), "\x01");
        assert_eq!(
            send(with(KeyAction::Repeat, ctrl_a), EVENTS),
            "\x1b[97;5:2u"
        );
        assert_eq!(send_in(named(Key::Up, ""), EVENTS, &cursor_keys), "\x1bOA");
        assert_eq!(
            send(with(KeyAction::Repeat, named(Key::Up, "")), EVENTS),
            "\x1b[1;1:2A"
        );
        // Nor do presses with modifiers that xterm cannot carry, which
        // send what their releases send.
        let super_up = named(Key::Up, "super");
        assert_eq!(send(super_up, EVENTS), "\x1b[1;9A");
        assert_eq!(
            send(with(KeyAction::Release, super_up), EVENTS),
            "\x1b[1;9:3A"
        );
        assert_eq!(
            send(event(Key::Char('a'), "super", "a", "a"), EVENTS),
            "\x1b[97;9u"
        );
        assert_eq!(
            send(event(Key::Char('a'), "ctrl+super", "\x01", "a"), EVENTS),
            "\x1b[97;13u"
        );
        assert_eq!(send(named(Key::Up, "caps"), EVENTS), "\x1b[1;65A");
        assert_eq!(send(named(Key::Up, "hyper"), ALTERNATES), "\x1b[1;17A");
        assert_eq!(send(named(Key::Escape, "meta"), TEXT), "\x1b[27;33u");
        assert_eq!(
            send(event(Key::Char('a'), "caps", "A", "a"), EVENTS),
            "A",
            "it types"
        );
        // Keys that xterm sends nothing for are reported as keys.
        assert_eq!(send(named(Key::PrintScreen, ""), EVENTS), "\x1b[57361u");
        assert_eq!(send(named(Key::F(21), ""), EVENTS), "\x1b[57384u");
        assert_eq!(
            send(named(Key::Media(Media::Play), ""), TEXT),
            "\x1b[57428u"
        );
    }

    #[test]
    fn alternate_keys_come_with_escape_codes() {
        let flags = DISAMBIGUATE | ALTERNATES;
        assert_eq!(send(event(Key::Char('a'), "", "a", "a"), flags), "a");
        assert_eq!(send(event(Key::Char('a'), "shift", "A", "A"), flags), "A");
        let ctrl = |key, held, unmodified| event(Key::Char(key), held, "", unmodified);
        assert_eq!(send(ctrl('a', "ctrl+shift", "A"), flags), "\x1b[97:65;6u");
        // Control with plus is Control and Shift with equal on some
        // keyboards; the shifted key tells them apart.
        assert_eq!(send(ctrl('=', "ctrl+shift", "+"), flags), "\x1b[61:43;6u");
        // The shifted key comes only with Shift, and only when it differs.
        assert_eq!(send(ctrl('a', "ctrl", "a"), flags), "\x1b[97;5u");
        assert_eq!(send(ctrl(' ', "ctrl+shift", " "), flags), "\x1b[32;6u");
        // Caps Lock is not Shift, whatever the key types with it.
        assert_eq!(send(ctrl('a', "ctrl+caps", "A"), flags), "\x1b[97;69u");
        // A Cyrillic layout: the key of `с`, which is `c` in the base layout.
        let cyrillic = |held, unmodified| KeyEvent {
            base_layout: Some('c'),
            ..ctrl('с', held, unmodified)
        };
        assert_eq!(send(cyrillic("ctrl", "с"), flags), "\x1b[1089::99;5u");
        assert_eq!(
            send(cyrillic("ctrl+shift", "С"), flags),
            "\x1b[1089:1057:99;6u"
        );
        let latin = KeyEvent {
            base_layout: Some('c'),
            ..ctrl('c', "ctrl", "c")
        };
        assert_eq!(send(latin, flags), "\x1b[99;5u");
        // Functional keys have none.
        assert_eq!(send(named(Key::Up, "shift"), flags), "\x1b[1;2A");
        // Without other flags, presses are xterm's, which have none.
        assert_eq!(send(ctrl('a', "ctrl+shift", "A"), ALTERNATES), "\x01");
        assert_eq!(
            send(
                with(KeyAction::Release, ctrl('a', "ctrl+shift", "A")),
                ALTERNATES | EVENTS
            ),
            "\x1b[97:65;6:3u"
        );
    }

    #[test]
    fn every_key_is_reported_as_an_escape_code() {
        let a = event(Key::Char('a'), "", "a", "a");
        assert_eq!(send(a, ALL_KEYS), "\x1b[97u");
        assert_eq!(
            send(event(Key::Char('a'), "ctrl", "\x01", "a"), ALL_KEYS),
            "\x1b[97;5u"
        );
        assert_eq!(
            send(event(Key::Char('a'), "shift", "A", "A"), ALL_KEYS),
            "\x1b[97;2u"
        );
        assert_eq!(send(named(Key::Up, ""), ALL_KEYS), "\x1b[A");
        assert_eq!(send(named(Key::Enter, ""), ALL_KEYS), "\x1b[13u");
        assert_eq!(send(named(Key::Enter, "ctrl"), ALL_KEYS), "\x1b[13;5u");
        assert_eq!(send(named(Key::Tab, ""), ALL_KEYS), "\x1b[9u");
        assert_eq!(send(named(Key::Backspace, ""), ALL_KEYS), "\x1b[127u");
        assert_eq!(send(named(Key::Escape, ""), ALL_KEYS), "\x1b[27u");
        let keypad = event(Key::Keypad(Keypad::Digit(5)), "", "5", "5");
        assert_eq!(send(keypad, ALL_KEYS), "\x1b[57404u");
    }

    #[test]
    fn associated_text_comes_with_the_key() {
        let flags = ALL_KEYS | TEXT;
        let key = |held, typed| event(Key::Char('a'), held, typed, typed);
        assert_eq!(send(key("", "a"), flags), "\x1b[97;;97u");
        // The specification's example: shift+a is `CSI 97 ; 2 ; 65 u`.
        assert_eq!(send(key("shift", "A"), flags), "\x1b[97;2;65u");
        assert_eq!(send(key("shift", "AB"), flags), "\x1b[97;2;65:66u");
        assert_eq!(send(key("caps", "A"), flags), "\x1b[97;65;65u");
        // Keys with Control, Meta or Super type nothing.
        assert_eq!(send(key("ctrl", "\x01"), flags), "\x1b[97;5u");
        assert_eq!(send(key("alt", "å"), flags), "\x1b[97;3u");
        assert_eq!(send(key("super", "a"), flags), "\x1b[97;9u");
        // Nor does a release, nor a key that types a control code.
        assert_eq!(
            send(with(KeyAction::Release, key("", "a")), flags | EVENTS),
            "\x1b[97;1:3u"
        );
        assert_eq!(
            send(with(KeyAction::Repeat, key("", "a")), flags | EVENTS),
            "\x1b[97;1:2;97u"
        );
        assert_eq!(send(event(Key::Enter, "", "\r", "\r"), flags), "\x1b[13u");
        assert_eq!(
            send(event(Key::Up, "", "\u{F700}", "\u{F700}"), flags),
            "\x1b[A"
        );
        let keypad = event(Key::Keypad(Keypad::Digit(5)), "", "5", "5");
        assert_eq!(send(keypad, flags), "\x1b[57404;;53u");
        // Every flag at once.
        let every = DISAMBIGUATE | EVENTS | ALTERNATES | ALL_KEYS | TEXT;
        assert_eq!(send(key("shift", "A"), every), "\x1b[97:65;2;65u");
        // Without every key reported, text goes as text.
        assert_eq!(send(key("shift", "A"), DISAMBIGUATE | TEXT), "A");
    }

    #[test]
    fn text_that_no_key_typed() {
        // The specification's example: alt+a, when the platform gives only
        // the text that Alt composed, is `CSI 0 ; ; 229 u`.
        assert_eq!(text("å", ALL_KEYS | TEXT), "\x1b[0;;229u");
        assert_eq!(text("ab", ALL_KEYS | TEXT), "\x1b[0;;97:98u");
        assert_eq!(text("a\x1bb", ALL_KEYS | TEXT), "\x1b[0;;97:98u");
        assert_eq!(text("\x07", ALL_KEYS | TEXT), "");
        assert_eq!(text("å", ALL_KEYS), "å");
        assert_eq!(text("å", TEXT), "å");
        assert_eq!(text("å", 0), "å");
    }
}
