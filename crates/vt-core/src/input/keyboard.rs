//! Keys, and how xterm encodes them:
//! <https://invisible-island.net/xterm/ctlseqs/ctlseqs.html#h2-PC-Style-Function-Keys>.

use std::io::Write;

use crate::terminal::InputModes;

/// A key that was pressed, as the platform reports it, once it has
/// resolved dead keys and input methods: composition text never reaches
/// the encoder.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KeyEvent<'a> {
    /// The key.
    pub key: Key,
    /// The modifiers held.
    pub mods: Mods,
    /// What the key types with every modifier held, as the platform
    /// composed it: on macOS, Option's characters. Empty for keys that type
    /// nothing.
    pub text: &'a str,
    /// What the key types with Shift alone, without Control or Alt, as
    /// macOS's `charactersIgnoringModifiers` gives it: what an Alt that acts
    /// as Meta sends after its escape, and what Control maps.
    pub unmodified: &'a str,
}

/// A key of the keyboard.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    /// A key that types a character, as it types it without modifiers.
    Char(char),
    /// Return.
    Enter,
    /// Tab.
    Tab,
    /// Backspace, which macOS calls Delete.
    Backspace,
    /// Escape.
    Escape,
    /// The up arrow.
    Up,
    /// The down arrow.
    Down,
    /// The right arrow.
    Right,
    /// The left arrow.
    Left,
    /// Home.
    Home,
    /// End.
    End,
    /// Page Up.
    PageUp,
    /// Page Down.
    PageDown,
    /// Insert, or Help on Apple keyboards.
    Insert,
    /// Forward Delete.
    Delete,
    /// A function key, from F1 to F20.
    F(u8),
    /// A key of the numeric keypad.
    Keypad(Keypad),
}

/// A key of the numeric keypad.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Keypad {
    /// A digit, from 0 to 9.
    Digit(u8),
    /// The decimal point.
    Decimal,
    /// Enter.
    Enter,
    /// `+`.
    Add,
    /// `-`.
    Subtract,
    /// `*`.
    Multiply,
    /// `/`.
    Divide,
    /// The separator, `,`.
    Separator,
    /// `=`, which Apple keyboards have.
    Equal,
}

/// The modifiers held with a key. Alt is Option on macOS, and the two
/// keys are told apart, as each can act as Meta or not.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Mods {
    /// Shift.
    pub shift: bool,
    /// Control.
    pub control: bool,
    /// The left Alt, or Option.
    pub left_alt: bool,
    /// The right Alt, or Option.
    pub right_alt: bool,
}

/// How the keyboard is set up, from the configuration.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KeyOptions {
    /// The left Alt (Option) acts as Meta, sending an escape before the
    /// key, rather than typing the characters the platform composes with
    /// it.
    pub left_alt_is_meta: bool,
    /// The same, for the right Alt.
    pub right_alt_is_meta: bool,
}

impl Default for KeyOptions {
    /// Both Alt keys act as Meta, as terminals other than macOS's do.
    fn default() -> Self {
        Self {
            left_alt_is_meta: true,
            right_alt_is_meta: true,
        }
    }
}

const ESC: u8 = 0x1B;

/// Appends to `out` what xterm sends for `event`, with the terminal's
/// `modes` and the keyboard's `options`.
///
/// An Alt that acts as Meta sends ESC before the key, whatever mode 1036
/// says: xterm's eight-bit Meta does not suit UTF-8. With
/// `modifyOtherKeys` at 1, keys whose usual sequence would lose a modifier,
/// such as Control with a digit or Control and Shift with a letter, send
/// `CSI 27 ; modifiers ; code ~`; at 2, every modified key does.
pub fn encode_key(
    event: &KeyEvent<'_>,
    modes: &InputModes,
    options: KeyOptions,
    out: &mut Vec<u8>,
) {
    let mods = event.mods;
    let meta = (mods.left_alt && options.left_alt_is_meta)
        || (mods.right_alt && options.right_alt_is_meta);
    let modifiers = Modifiers {
        shift: mods.shift,
        meta,
        control: mods.control,
    };
    match event.key {
        Key::Char(_) => encode_char(event, modifiers, modes, out),
        Key::Enter => {
            let newline: &[u8] = if modes.newline { b"\r\n" } else { b"\r" };
            encode_c0(
                13,
                newline,
                modifiers,
                modes,
                modifiers.control || modifiers.shift,
                out,
            );
        }
        Key::Tab if modifiers.shift && !modifiers.control && !modifiers.meta => {
            out.extend_from_slice(b"\x1b[Z");
        }
        Key::Tab => encode_c0(9, b"\t", modifiers, modes, modifiers.control, out),
        Key::Backspace => {
            // Control sends BS, as xterm's Backarrow key does.
            let bytes: &[u8] = if modifiers.control { b"\x08" } else { b"\x7f" };
            encode_c0(127, bytes, modifiers, modes, false, out);
        }
        Key::Escape => encode_c0(27, b"\x1b", modifiers, modes, false, out),
        Key::Up => encode_cursor(b'A', modifiers, modes, out),
        Key::Down => encode_cursor(b'B', modifiers, modes, out),
        Key::Right => encode_cursor(b'C', modifiers, modes, out),
        Key::Left => encode_cursor(b'D', modifiers, modes, out),
        Key::Home => encode_cursor(b'H', modifiers, modes, out),
        Key::End => encode_cursor(b'F', modifiers, modes, out),
        Key::Insert => encode_tilde(2, modifiers, out),
        Key::Delete => encode_tilde(3, modifiers, out),
        Key::PageUp => encode_tilde(5, modifiers, out),
        Key::PageDown => encode_tilde(6, modifiers, out),
        Key::F(n @ 1..=4) => encode_ss3_or_csi(b"PQRS"[usize::from(n - 1)], modifiers, out),
        Key::F(n @ 5..=20) => {
            const CODES: [u16; 16] = [
                15, 17, 18, 19, 20, 21, 23, 24, 25, 26, 28, 29, 31, 32, 33, 34,
            ];
            encode_tilde(CODES[usize::from(n - 5)], modifiers, out);
        }
        Key::F(_) => {}
        Key::Keypad(key) => encode_keypad(key, event, modifiers, modes, out),
    }
}

/// The modifiers that the sequences carry: Alt only when it acts as Meta.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Modifiers {
    shift: bool,
    meta: bool,
    control: bool,
}

impl Modifiers {
    fn any(self) -> bool {
        self.shift || self.meta || self.control
    }

    /// The parameter that xterm's sequences carry: 1, plus 1 for Shift, 2
    /// for Meta and 4 for Control.
    fn parameter(self) -> u8 {
        1 + u8::from(self.shift) + 2 * u8::from(self.meta) + 4 * u8::from(self.control)
    }
}

/// A key that types a character.
fn encode_char(event: &KeyEvent<'_>, modifiers: Modifiers, modes: &InputModes, out: &mut Vec<u8>) {
    let base = single_char(event.unmodified).or(match event.key {
        Key::Char(c) => Some(c),
        _ => None,
    });
    let control = modifiers
        .control
        .then(|| base.and_then(control_byte))
        .flatten();
    if let Some(base) = base {
        // Control with a character that has no control code, or with Shift
        // and a letter, would lose a modifier.
        let lossy = modifiers.control
            && (control.is_none() || (modifiers.shift && base.is_ascii_alphabetic()));
        let modified = modifiers.control || modifiers.meta;
        if (modes.modify_other_keys == 2 && modified) || (modes.modify_other_keys == 1 && lossy) {
            modify_other_key(u32::from(base), modifiers, out);
            return;
        }
    }
    if modifiers.meta {
        out.push(ESC);
    }
    match control {
        Some(byte) => out.push(byte),
        None if modifiers.meta || modifiers.control => {
            out.extend_from_slice(event.unmodified.as_bytes());
        }
        None => out.extend_from_slice(event.text.as_bytes()),
    }
}

/// A key that sends a control code: `bytes`, after an escape for Meta, or
/// `CSI 27 ; modifiers ; code ~` when `modifyOtherKeys` asks, at 2 for any
/// modifier and at 1 when the usual sequence is `lossy`.
fn encode_c0(
    code: u32,
    bytes: &[u8],
    modifiers: Modifiers,
    modes: &InputModes,
    lossy: bool,
    out: &mut Vec<u8>,
) {
    if (modes.modify_other_keys == 2 && modifiers.any()) || (modes.modify_other_keys == 1 && lossy)
    {
        modify_other_key(code, modifiers, out);
        return;
    }
    if modifiers.meta {
        out.push(ESC);
    }
    out.extend_from_slice(bytes);
}

/// `CSI 27 ; modifiers ; code ~`, xterm's `modifyOtherKeys` form.
fn modify_other_key(code: u32, modifiers: Modifiers, out: &mut Vec<u8>) {
    write!(out, "\x1b[27;{};{code}~", modifiers.parameter()).expect("writing to a Vec");
}

/// A cursor key, or Home or End: `CSI final`, or `SS3 final` in application
/// mode (DECCKM), or `CSI 1 ; modifiers final` with modifiers.
fn encode_cursor(final_byte: u8, modifiers: Modifiers, modes: &InputModes, out: &mut Vec<u8>) {
    if modifiers.any() {
        write!(out, "\x1b[1;{}", modifiers.parameter()).expect("writing to a Vec");
        out.push(final_byte);
    } else if modes.cursor_keys_application {
        out.extend_from_slice(&[ESC, b'O', final_byte]);
    } else {
        out.extend_from_slice(&[ESC, b'[', final_byte]);
    }
}

/// F1 to F4: `SS3 final`, or `CSI 1 ; modifiers final` with modifiers.
fn encode_ss3_or_csi(final_byte: u8, modifiers: Modifiers, out: &mut Vec<u8>) {
    if modifiers.any() {
        write!(out, "\x1b[1;{}", modifiers.parameter()).expect("writing to a Vec");
        out.push(final_byte);
    } else {
        out.extend_from_slice(&[ESC, b'O', final_byte]);
    }
}

/// `CSI code ~`, or `CSI code ; modifiers ~` with modifiers.
fn encode_tilde(code: u16, modifiers: Modifiers, out: &mut Vec<u8>) {
    if modifiers.any() {
        write!(out, "\x1b[{code};{}~", modifiers.parameter())
    } else {
        write!(out, "\x1b[{code}~")
    }
    .expect("writing to a Vec");
}

/// A key of the keypad: `SS3 code` in application mode (DECKPAM), what it
/// types otherwise, as the platform gives it.
fn encode_keypad(
    key: Keypad,
    event: &KeyEvent<'_>,
    modifiers: Modifiers,
    modes: &InputModes,
    out: &mut Vec<u8>,
) {
    if modes.keypad_application {
        let code = match key {
            Keypad::Digit(digit) => b'p' + digit.min(9),
            Keypad::Decimal => b'n',
            Keypad::Enter => b'M',
            Keypad::Add => b'k',
            Keypad::Subtract => b'm',
            Keypad::Multiply => b'j',
            Keypad::Divide => b'o',
            Keypad::Separator => b'l',
            Keypad::Equal => b'X',
        };
        out.extend_from_slice(&[ESC, b'O', code]);
        return;
    }
    if key == Keypad::Enter {
        let newline: &[u8] = if modes.newline { b"\r\n" } else { b"\r" };
        encode_c0(13, newline, modifiers, modes, false, out);
        return;
    }
    encode_char(event, modifiers, modes, out);
}

/// The control code that Control sends with `c`, as xterm maps it: the
/// letters and `@[\]^_` to C0, space and `2` to NUL, `3` to `7` to ESC
/// and the next ones, `/` to US, and `8` and `?` to DEL.
fn control_byte(c: char) -> Option<u8> {
    let byte = u8::try_from(c).ok()?;
    Some(match byte {
        b'@' | b' ' | b'2' => 0,
        b'a'..=b'z' => byte - b'a' + 1,
        b'A'..=b'Z' => byte - b'A' + 1,
        b'[' | b'3' => 0x1B,
        b'\\' | b'4' => 0x1C,
        b']' | b'5' => 0x1D,
        b'^' | b'6' | b'~' => 0x1E,
        b'_' | b'7' | b'/' => 0x1F,
        b'8' | b'?' => 0x7F,
        _ => return None,
    })
}

/// The character of `text`, if it has one and only one.
fn single_char(text: &str) -> Option<char> {
    let mut chars = text.chars();
    let c = chars.next()?;
    chars.next().is_none().then_some(c)
}

#[cfg(test)]
mod tests {
    use super::*;

    const NONE: Mods = Mods {
        shift: false,
        control: false,
        left_alt: false,
        right_alt: false,
    };
    const SHIFT: Mods = Mods {
        shift: true,
        ..NONE
    };
    const CONTROL: Mods = Mods {
        control: true,
        ..NONE
    };
    const ALT: Mods = Mods {
        left_alt: true,
        ..NONE
    };
    const RIGHT_ALT: Mods = Mods {
        right_alt: true,
        ..NONE
    };

    /// What the key sends with `mods`, in `modes`, with both Alt keys as
    /// Meta.
    fn send(key: Key, mods: Mods, text: &str, unmodified: &str, modes: &InputModes) -> String {
        send_with(key, mods, text, unmodified, modes, KeyOptions::default())
    }

    fn send_with(
        key: Key,
        mods: Mods,
        text: &str,
        unmodified: &str,
        modes: &InputModes,
        options: KeyOptions,
    ) -> String {
        let event = KeyEvent {
            key,
            mods,
            text,
            unmodified,
        };
        let mut out = Vec::new();
        encode_key(&event, modes, options, &mut out);
        String::from_utf8(out).unwrap()
    }

    /// What a character key sends, typing `text` with `mods`.
    fn char_key(c: char, mods: Mods, text: &str, unmodified: &str) -> String {
        send(Key::Char(c), mods, text, unmodified, &InputModes::default())
    }

    fn named(key: Key, mods: Mods) -> String {
        send(key, mods, "", "", &InputModes::default())
    }

    #[test]
    fn characters_and_control_codes() {
        let both = Mods {
            control: true,
            shift: true,
            ..NONE
        };
        let cases = [
            (char_key('a', NONE, "a", "a"), "a"),
            (char_key('a', SHIFT, "A", "A"), "A"),
            (char_key('a', CONTROL, "\x01", "a"), "\x01"),
            (char_key('a', both, "\x01", "A"), "\x01"),
            (char_key(' ', CONTROL, "\0", " "), "\0"),
            (char_key('[', CONTROL, "\x1b", "["), "\x1b"),
            (char_key('\\', CONTROL, "", "\\"), "\x1c"),
            (char_key(']', CONTROL, "", "]"), "\x1d"),
            (char_key('2', CONTROL, "", "2"), "\0"),
            (char_key('3', CONTROL, "", "3"), "\x1b"),
            (char_key('7', CONTROL, "", "7"), "\x1f"),
            (char_key('8', CONTROL, "", "8"), "\x7f"),
            (char_key('/', CONTROL, "", "/"), "\x1f"),
            (char_key('1', CONTROL, "", "1"), "1"),
            (char_key('é', NONE, "é", "é"), "é"),
        ];
        for (sent, expected) in cases {
            assert_eq!(sent, expected);
        }
    }

    #[test]
    fn an_alt_that_acts_as_meta_sends_an_escape_first() {
        assert_eq!(char_key('a', ALT, "å", "a"), "\x1ba");
        assert_eq!(
            char_key('a', Mods { shift: true, ..ALT }, "Å", "A"),
            "\x1bA"
        );
        assert_eq!(
            char_key(
                'a',
                Mods {
                    control: true,
                    ..ALT
                },
                "",
                "a"
            ),
            "\x1b\x01"
        );
        // macOS: the left Option types its characters, the right one is Meta.
        let options = KeyOptions {
            left_alt_is_meta: false,
            right_alt_is_meta: true,
        };
        let modes = InputModes::default();
        let d = |mods| send_with(Key::Char('d'), mods, "\u{2202}", "d", &modes, options);
        assert_eq!(d(ALT), "\u{2202}");
        assert_eq!(d(RIGHT_ALT), "\x1bd");
    }

    #[test]
    fn enter_tab_backspace_and_escape() {
        let newline = InputModes {
            newline: true,
            ..InputModes::default()
        };
        assert_eq!(named(Key::Enter, NONE), "\r");
        assert_eq!(send(Key::Enter, NONE, "", "", &newline), "\r\n");
        assert_eq!(named(Key::Enter, ALT), "\x1b\r");
        assert_eq!(named(Key::Tab, NONE), "\t");
        assert_eq!(named(Key::Tab, SHIFT), "\x1b[Z");
        assert_eq!(named(Key::Tab, ALT), "\x1b\t");
        assert_eq!(named(Key::Backspace, NONE), "\x7f");
        assert_eq!(named(Key::Backspace, CONTROL), "\x08");
        assert_eq!(named(Key::Backspace, ALT), "\x1b\x7f");
        assert_eq!(named(Key::Escape, NONE), "\x1b");
        assert_eq!(named(Key::Escape, ALT), "\x1b\x1b");
    }

    #[test]
    fn cursor_keys_follow_decckm_and_carry_modifiers() {
        let application = InputModes {
            cursor_keys_application: true,
            ..InputModes::default()
        };
        assert_eq!(named(Key::Up, NONE), "\x1b[A");
        assert_eq!(send(Key::Up, NONE, "", "", &application), "\x1bOA");
        assert_eq!(named(Key::Up, SHIFT), "\x1b[1;2A");
        assert_eq!(send(Key::Up, SHIFT, "", "", &application), "\x1b[1;2A");
        assert_eq!(named(Key::Left, CONTROL), "\x1b[1;5D");
        assert_eq!(named(Key::Right, ALT), "\x1b[1;3C");
        let all = Mods {
            shift: true,
            control: true,
            ..ALT
        };
        assert_eq!(named(Key::Down, all), "\x1b[1;8B");
        assert_eq!(named(Key::Home, NONE), "\x1b[H");
        assert_eq!(send(Key::End, NONE, "", "", &application), "\x1bOF");
        assert_eq!(named(Key::End, CONTROL), "\x1b[1;5F");
    }

    #[test]
    fn editing_and_function_keys() {
        let cases = [
            (Key::Insert, NONE, "\x1b[2~"),
            (Key::Delete, NONE, "\x1b[3~"),
            (Key::Delete, SHIFT, "\x1b[3;2~"),
            (Key::PageUp, NONE, "\x1b[5~"),
            (Key::PageDown, CONTROL, "\x1b[6;5~"),
            (Key::F(1), NONE, "\x1bOP"),
            (Key::F(4), NONE, "\x1bOS"),
            (Key::F(1), CONTROL, "\x1b[1;5P"),
            (Key::F(5), NONE, "\x1b[15~"),
            (Key::F(6), NONE, "\x1b[17~"),
            (Key::F(12), NONE, "\x1b[24~"),
            (Key::F(5), SHIFT, "\x1b[15;2~"),
            (Key::F(13), NONE, "\x1b[25~"),
            (Key::F(20), NONE, "\x1b[34~"),
            (Key::F(21), NONE, ""),
        ];
        for (key, mods, expected) in cases {
            assert_eq!(named(key, mods), expected, "{key:?} {mods:?}");
        }
    }

    #[test]
    fn the_keypad_follows_deckpam() {
        let application = InputModes {
            keypad_application: true,
            ..InputModes::default()
        };
        let numeric = InputModes::default();
        let keypad =
            |key, text, modes: &InputModes| send(Key::Keypad(key), NONE, text, text, modes);
        assert_eq!(keypad(Keypad::Digit(5), "5", &numeric), "5");
        assert_eq!(keypad(Keypad::Enter, "", &numeric), "\r");
        assert_eq!(keypad(Keypad::Digit(0), "0", &application), "\x1bOp");
        assert_eq!(keypad(Keypad::Digit(9), "9", &application), "\x1bOy");
        assert_eq!(keypad(Keypad::Enter, "", &application), "\x1bOM");
        assert_eq!(keypad(Keypad::Decimal, ".", &application), "\x1bOn");
        assert_eq!(keypad(Keypad::Add, "+", &application), "\x1bOk");
        assert_eq!(keypad(Keypad::Subtract, "-", &application), "\x1bOm");
        assert_eq!(keypad(Keypad::Multiply, "*", &application), "\x1bOj");
        assert_eq!(keypad(Keypad::Divide, "/", &application), "\x1bOo");
        assert_eq!(keypad(Keypad::Separator, ",", &application), "\x1bOl");
        assert_eq!(keypad(Keypad::Equal, "=", &application), "\x1bOX");
    }

    #[test]
    fn modify_other_keys_reports_what_the_usual_sequences_lose() {
        let level = |n| InputModes {
            modify_other_keys: n,
            ..InputModes::default()
        };
        let both = Mods {
            control: true,
            shift: true,
            ..NONE
        };
        // At 1, only keys whose usual sequence loses a modifier.
        let one = level(1);
        assert_eq!(
            send(Key::Char('1'), CONTROL, "", "1", &one),
            "\x1b[27;5;49~"
        );
        assert_eq!(send(Key::Char('a'), both, "", "A", &one), "\x1b[27;6;65~");
        assert_eq!(send(Key::Char('a'), CONTROL, "", "a", &one), "\x01");
        assert_eq!(send(Key::Char('a'), ALT, "", "a", &one), "\x1ba");
        assert_eq!(send(Key::Enter, CONTROL, "", "", &one), "\x1b[27;5;13~");
        assert_eq!(send(Key::Enter, SHIFT, "", "", &one), "\x1b[27;2;13~");
        assert_eq!(send(Key::Enter, ALT, "", "", &one), "\x1b\r");
        assert_eq!(send(Key::Tab, CONTROL, "", "", &one), "\x1b[27;5;9~");
        assert_eq!(send(Key::Backspace, CONTROL, "", "", &one), "\x08");
        // At 2, every modified key.
        let two = level(2);
        assert_eq!(
            send(Key::Char('a'), CONTROL, "", "a", &two),
            "\x1b[27;5;97~"
        );
        assert_eq!(send(Key::Char('a'), ALT, "", "a", &two), "\x1b[27;3;97~");
        assert_eq!(
            send(Key::Backspace, CONTROL, "", "", &two),
            "\x1b[27;5;127~"
        );
        assert_eq!(send(Key::Escape, SHIFT, "", "", &two), "\x1b[27;2;27~");
        assert_eq!(send(Key::Char('a'), NONE, "a", "a", &two), "a");
        assert_eq!(
            send(Key::Char('A'), SHIFT, "A", "A", &two),
            "A",
            "Shift alone types"
        );
        assert_eq!(send(Key::Up, CONTROL, "", "", &two), "\x1b[1;5A");
    }
}
