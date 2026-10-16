//! Keys, and how xterm encodes them:
//! <https://invisible-island.net/xterm/ctlseqs/ctlseqs.html#h2-PC-Style-Function-Keys>,
//! or the kitty keyboard protocol, once a program asks for it.

use std::io::Write;

use super::kitty;
use crate::terminal::{InputModes, KeyboardFlags};

/// A key that was pressed, as the platform reports it, once it has
/// resolved dead keys and input methods: composition text never reaches
/// the encoder.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KeyEvent<'a> {
    /// The key.
    pub key: Key,
    /// Whether the key went down, repeats or went up.
    pub action: KeyAction,
    /// The modifiers held. For a modifier key, they are as the event leaves
    /// them: pressing Control holds Control, and releasing it does not,
    /// unless the other Control is still down.
    pub mods: Mods,
    /// What the key types with every modifier held, as the platform
    /// composed it: on macOS, Option's characters. Empty for keys that type
    /// nothing.
    pub text: &'a str,
    /// What the key types with Shift alone, without Control or Alt, as
    /// macOS's `charactersIgnoringModifiers` gives it: what an Alt that acts
    /// as Meta sends after its escape, and what Control maps.
    pub unmodified: &'a str,
    /// The key in the same place on the standard PC-101 layout, US
    /// English's, if the platform knows it: `c` for the key that types `с`
    /// in a Cyrillic layout.
    pub base_layout: Option<char>,
}

/// What happened to a key.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum KeyAction {
    /// It went down.
    #[default]
    Press,
    /// It is held, and the platform repeats it.
    Repeat,
    /// It went up.
    Release,
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
    /// A function key, from F1 to F35. xterm's sequences stop at F20.
    F(u8),
    /// A key of the numeric keypad.
    Keypad(Keypad),
    /// Caps Lock.
    CapsLock,
    /// Scroll Lock.
    ScrollLock,
    /// Num Lock, or Clear on Apple keyboards.
    NumLock,
    /// Print Screen.
    PrintScreen,
    /// Pause.
    Pause,
    /// The context menu key.
    Menu,
    /// A media key.
    Media(Media),
    /// A modifier key, pressed or released on its own.
    Modifier(ModifierKey),
}

impl Key {
    /// Whether the key is a modifier or a lock, which only the kitty
    /// protocol's report of every key sends.
    pub(super) fn is_modifier(self) -> bool {
        matches!(
            self,
            Self::Modifier(_) | Self::CapsLock | Self::ScrollLock | Self::NumLock
        )
    }
}

/// A key of the numeric keypad.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Keypad {
    /// A digit, from 0 to 9; those past 9 count as 9.
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
    /// The left arrow, which the 4 key is without Num Lock.
    Left,
    /// The right arrow, the 6 key without Num Lock.
    Right,
    /// The up arrow, the 8 key without Num Lock.
    Up,
    /// The down arrow, the 2 key without Num Lock.
    Down,
    /// Page Up, the 9 key without Num Lock.
    PageUp,
    /// Page Down, the 3 key without Num Lock.
    PageDown,
    /// Home, the 7 key without Num Lock.
    Home,
    /// End, the 1 key without Num Lock.
    End,
    /// Insert, the 0 key without Num Lock.
    Insert,
    /// Delete, the decimal point without Num Lock.
    Delete,
    /// The 5 key without Num Lock, which xterm calls Begin.
    Begin,
}

/// A media key, numbered as the kitty protocol numbers it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum Media {
    /// Play.
    Play = 57428,
    /// Pause.
    Pause = 57429,
    /// Play or pause.
    PlayPause = 57430,
    /// Reverse.
    Reverse = 57431,
    /// Stop.
    Stop = 57432,
    /// Fast forward.
    FastForward = 57433,
    /// Rewind.
    Rewind = 57434,
    /// The next track.
    TrackNext = 57435,
    /// The previous track.
    TrackPrevious = 57436,
    /// Record.
    Record = 57437,
    /// Lower the volume.
    LowerVolume = 57438,
    /// Raise the volume.
    RaiseVolume = 57439,
    /// Mute.
    MuteVolume = 57440,
}

/// A modifier key, numbered as the kitty protocol numbers it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum ModifierKey {
    /// The left Shift.
    LeftShift = 57441,
    /// The left Control.
    LeftControl = 57442,
    /// The left Alt, or Option.
    LeftAlt = 57443,
    /// The left Super, or Command.
    LeftSuper = 57444,
    /// The left Hyper.
    LeftHyper = 57445,
    /// The left Meta.
    LeftMeta = 57446,
    /// The right Shift.
    RightShift = 57447,
    /// The right Control.
    RightControl = 57448,
    /// The right Alt, or Option.
    RightAlt = 57449,
    /// The right Super, or Command.
    RightSuper = 57450,
    /// The right Hyper.
    RightHyper = 57451,
    /// The right Meta.
    RightMeta = 57452,
    /// AltGr, ISO's level 3 shift.
    IsoLevel3Shift = 57453,
    /// ISO's level 5 shift.
    IsoLevel5Shift = 57454,
}

/// The modifiers held with a key, and the locks on. Alt is Option on
/// macOS, and the two keys are told apart, as each can act as Meta or not.
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
    /// Super: Command on macOS, the logo key elsewhere.
    pub super_key: bool,
    /// Hyper, which some X11 and Wayland layouts have.
    pub hyper: bool,
    /// Meta, a modifier of its own on some X11 and Wayland layouts, apart
    /// from an Alt that acts as Meta.
    pub meta: bool,
    /// Caps Lock is on.
    pub caps_lock: bool,
    /// Num Lock is on.
    pub num_lock: bool,
}

impl Mods {
    /// No modifier, and no lock.
    pub const NONE: Self = Self {
        shift: false,
        control: false,
        left_alt: false,
        right_alt: false,
        super_key: false,
        hyper: false,
        meta: false,
        caps_lock: false,
        num_lock: false,
    };

    /// Whether an Alt that acts as Meta, as `options` says, is held.
    pub(super) fn alt_as_meta(self, options: KeyOptions) -> bool {
        (self.left_alt && options.left_alt_is_meta) || (self.right_alt && options.right_alt_is_meta)
    }
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

/// Appends to `out` what `event` sends, with the terminal's `modes`, the
/// kitty protocol's `flags` in force and the keyboard's `options`.
///
/// With no flag, keys go as xterm sends them, and releases send nothing.
/// An Alt that acts as Meta sends ESC before the key, whatever mode 1036
/// says: xterm's eight-bit Meta does not suit UTF-8. With
/// `modifyOtherKeys` at 1, keys whose usual sequence would lose a modifier,
/// such as Control with a digit or Control and Shift with a letter, send
/// `CSI 27 ; modifiers ; code ~`; at 2, every modified key does.
///
/// With flags, keys go as the kitty keyboard protocol has them:
/// <https://sw.kovidgoyal.net/kitty/keyboard-protocol/>.
pub fn encode_key(
    event: &KeyEvent<'_>,
    modes: &InputModes,
    flags: KeyboardFlags,
    options: KeyOptions,
    out: &mut Vec<u8>,
) {
    if flags != KeyboardFlags::NONE {
        kitty::encode_key(event, modes, flags, options, out);
    } else if event.action != KeyAction::Release {
        encode_legacy(event, modes, options, out);
    }
}

/// Appends to `out` what `text` sends when no key typed it, as when an
/// input method commits it: the text itself, or, when the kitty protocol
/// reports every key as an escape code along with its text,
/// `CSI 0 ; ; codepoints u`.
pub fn encode_text(text: &str, flags: KeyboardFlags, out: &mut Vec<u8>) {
    if flags.contains(KeyboardFlags::REPORT_ALL_KEYS) && flags.contains(KeyboardFlags::REPORT_TEXT)
    {
        kitty::encode_text(text, out);
    } else {
        out.extend_from_slice(text.as_bytes());
    }
}

/// Appends to `out` what xterm sends for `event`.
pub(super) fn encode_legacy(
    event: &KeyEvent<'_>,
    modes: &InputModes,
    options: KeyOptions,
    out: &mut Vec<u8>,
) {
    let mods = event.mods;
    let modifiers = Modifiers {
        shift: mods.shift,
        meta: mods.alt_as_meta(options),
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
        // Shift makes Tab a back tab, `CSI Z`, which Control leaves as it
        // is and Meta sends after an escape, as with xterm's ISO_Left_Tab,
        // unless `modifyOtherKeys` reports the key, as it does with a
        // modifier other than Shift.
        Key::Tab if modifiers.shift => {
            if modes.modify_other_keys > 0 && (modifiers.control || modifiers.meta) {
                modify_other_key(9, modifiers, out);
            } else {
                if modifiers.meta {
                    out.push(ESC);
                }
                out.extend_from_slice(b"\x1b[Z");
            }
        }
        Key::Tab => encode_c0(9, b"\t", modifiers, modes, modifiers.control, out),
        // Control sends BS, as xterm's Backarrow key does, which
        // `modifyOtherKeys` at 2 reports, as BS, only with another modifier.
        Key::Backspace if modifiers.control => {
            if modes.modify_other_keys == 2 && (modifiers.shift || modifiers.meta) {
                modify_other_key(8, modifiers, out);
            } else {
                if modifiers.meta {
                    out.push(ESC);
                }
                out.push(0x08);
            }
        }
        Key::Backspace => encode_c0(127, b"\x7f", modifiers, modes, false, out),
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
        // xterm sends F16's sequence for Menu.
        Key::Menu => encode_tilde(29, modifiers, out),
        Key::CapsLock
        | Key::ScrollLock
        | Key::NumLock
        | Key::PrintScreen
        | Key::Pause
        | Key::Media(_)
        | Key::Modifier(_) => {}
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
    let mut unmodified = event.unmodified;
    let mut base = single_char(unmodified).or(match event.key {
        Key::Char(c) => Some(c),
        _ => None,
    });
    // Control and Meta take a key whose character is not ASCII, as in a
    // Cyrillic layout, as the key of the base layout, so that Control with
    // `с` interrupts as Control with `c` does.
    let mut latin = [0; 4];
    if let (Some(c), Some(key)) = (base, event.base_layout)
        && (modifiers.control || modifiers.meta)
        && !modifiers.shift
        && !c.is_ascii()
        && key.is_ascii()
    {
        base = Some(key);
        unmodified = key.encode_utf8(&mut latin);
    }
    let control = modifiers
        .control
        .then(|| base.and_then(control_byte))
        .flatten();
    if let Some(base) = base {
        // Control with a character that has no control code, or with Shift
        // and a letter, would lose a modifier.
        let lossy = modifiers.control
            && (control.is_none() || (modifiers.shift && base.is_ascii_alphabetic()));
        // At 2, as in xterm, Shift alone reports space and the characters
        // that Control makes control codes of, from `@` to DEL.
        let shifted = modifiers.shift && (base == ' ' || ('@'..='\x7f').contains(&base));
        let reported = match modes.modify_other_keys {
            0 => false,
            1 => lossy,
            _ => modifiers.control || modifiers.meta || shifted,
        };
        if reported {
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
            out.extend_from_slice(unmodified.as_bytes());
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
/// types otherwise, as the platform gives it. Without Num Lock, the keys
/// send what the keys they stand for send, in either mode.
fn encode_keypad(
    key: Keypad,
    event: &KeyEvent<'_>,
    modifiers: Modifiers,
    modes: &InputModes,
    out: &mut Vec<u8>,
) {
    let code = match key {
        Keypad::Left => return encode_cursor(b'D', modifiers, modes, out),
        Keypad::Right => return encode_cursor(b'C', modifiers, modes, out),
        Keypad::Up => return encode_cursor(b'A', modifiers, modes, out),
        Keypad::Down => return encode_cursor(b'B', modifiers, modes, out),
        Keypad::Home => return encode_cursor(b'H', modifiers, modes, out),
        Keypad::End => return encode_cursor(b'F', modifiers, modes, out),
        Keypad::Begin => return encode_cursor(b'E', modifiers, modes, out),
        Keypad::Insert => return encode_tilde(2, modifiers, out),
        Keypad::Delete => return encode_tilde(3, modifiers, out),
        Keypad::PageUp => return encode_tilde(5, modifiers, out),
        Keypad::PageDown => return encode_tilde(6, modifiers, out),
        // Otherwise the keys type what they type, as in xterm, which maps
        // none with Control nor reports any for `modifyOtherKeys`; Meta
        // sends an escape first.
        _ if !modes.keypad_application => {
            if modifiers.meta {
                out.push(ESC);
            }
            let typed: &[u8] = match key {
                Keypad::Enter if modes.newline => b"\r\n",
                Keypad::Enter => b"\r",
                _ if modifiers.meta => event.unmodified.as_bytes(),
                _ => event.text.as_bytes(),
            };
            return out.extend_from_slice(typed);
        }
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
pub(super) fn single_char(text: &str) -> Option<char> {
    let mut chars = text.chars();
    let c = chars.next()?;
    chars.next().is_none().then_some(c)
}

#[cfg(test)]
mod tests {
    use super::*;

    const NONE: Mods = Mods::NONE;
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
            action: KeyAction::Press,
            mods,
            text,
            unmodified,
            base_layout: None,
        };
        let mut out = Vec::new();
        encode_key(&event, modes, KeyboardFlags::NONE, options, &mut out);
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
        assert_eq!(keypad(Keypad::Digit(12), "", &application), "\x1bOy");
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
        // At 2, every key with Control or Meta, and some with Shift.
        let two = level(2);
        assert_eq!(
            send(Key::Char('a'), CONTROL, "", "a", &two),
            "\x1b[27;5;97~"
        );
        assert_eq!(send(Key::Char('a'), ALT, "", "a", &two), "\x1b[27;3;97~");
        assert_eq!(send(Key::Escape, SHIFT, "", "", &two), "\x1b[27;2;27~");
        assert_eq!(send(Key::Char('a'), NONE, "a", "a", &two), "a");
        assert_eq!(send(Key::Up, CONTROL, "", "", &two), "\x1b[1;5A");
    }

    #[test]
    fn modify_other_keys_at_2_follows_xterm() {
        let two = InputModes {
            modify_other_keys: 2,
            ..InputModes::default()
        };
        let both = Mods {
            control: true,
            shift: true,
            ..NONE
        };
        let alt_shift = Mods { shift: true, ..ALT };
        let char_key = |c, mods, typed| send(Key::Char(c), mods, typed, typed, &two);
        // Shift alone reports space and the characters from `@` to DEL,
        // which Control makes control codes of, and types the others.
        assert_eq!(char_key(' ', SHIFT, " "), "\x1b[27;2;32~");
        assert_eq!(char_key('a', SHIFT, "A"), "\x1b[27;2;65~");
        assert_eq!(char_key('[', SHIFT, "{"), "\x1b[27;2;123~");
        assert_eq!(char_key('`', SHIFT, "~"), "\x1b[27;2;126~");
        assert_eq!(char_key('1', SHIFT, "!"), "!");
        assert_eq!(char_key(';', SHIFT, ":"), ":");
        assert_eq!(char_key('2', SHIFT, "@"), "\x1b[27;2;64~");
        assert_eq!(char_key('é', SHIFT, "É"), "É");
        // Backspace with Control is BS, reported only with another
        // modifier, as BS too; without Control, with any.
        assert_eq!(send(Key::Backspace, CONTROL, "", "", &two), "\x08");
        assert_eq!(send(Key::Backspace, both, "", "", &two), "\x1b[27;6;8~");
        let control_alt = Mods {
            control: true,
            ..ALT
        };
        assert_eq!(
            send(Key::Backspace, control_alt, "", "", &two),
            "\x1b[27;7;8~"
        );
        assert_eq!(send(Key::Backspace, SHIFT, "", "", &two), "\x1b[27;2;127~");
        assert_eq!(send(Key::Backspace, ALT, "", "", &two), "\x1b[27;3;127~");
        assert_eq!(named(Key::Backspace, control_alt), "\x1b\x08");
        // The keypad is not reported, nor mapped with Control.
        let keypad = |key, mods, typed| send(Key::Keypad(key), mods, typed, typed, &two);
        assert_eq!(keypad(Keypad::Digit(5), CONTROL, "5"), "5");
        assert_eq!(keypad(Keypad::Digit(5), ALT, "5"), "\x1b5");
        assert_eq!(keypad(Keypad::Enter, CONTROL, ""), "\r");
        assert_eq!(keypad(Keypad::Enter, ALT, ""), "\x1b\r");
        // Meta sends what the key types without it.
        assert_eq!(
            send(Key::Keypad(Keypad::Digit(5)), ALT, "§", "5", &two),
            "\x1b5"
        );
        let newline = InputModes {
            newline: true,
            ..two
        };
        assert_eq!(
            send(Key::Keypad(Keypad::Enter), CONTROL, "", "", &newline),
            "\r\n"
        );
        assert_eq!(send(Key::Tab, alt_shift, "", "", &two), "\x1b[27;4;9~");
    }

    #[test]
    fn control_and_meta_take_the_key_of_the_base_layout() {
        let cyrillic = |mods, modes: &InputModes| {
            let event = KeyEvent {
                key: Key::Char('с'),
                action: KeyAction::Press,
                mods,
                text: "",
                unmodified: "с",
                base_layout: Some('c'),
            };
            let mut out = Vec::new();
            encode_key(
                &event,
                modes,
                KeyboardFlags::NONE,
                KeyOptions::default(),
                &mut out,
            );
            String::from_utf8(out).unwrap()
        };
        let plain = InputModes::default();
        let control_alt = Mods {
            control: true,
            ..ALT
        };
        assert_eq!(
            cyrillic(CONTROL, &plain),
            "\x03",
            "Control with с interrupts"
        );
        assert_eq!(cyrillic(ALT, &plain), "\x1bc");
        assert_eq!(cyrillic(control_alt, &plain), "\x1b\x03");
        let two = InputModes {
            modify_other_keys: 2,
            ..InputModes::default()
        };
        assert_eq!(cyrillic(CONTROL, &two), "\x1b[27;5;99~");
        // Not with Shift, nor for a key whose character is ASCII.
        let both = Mods {
            control: true,
            shift: true,
            ..NONE
        };
        assert_eq!(cyrillic(both, &plain), "с");
        let azerty = KeyEvent {
            key: Key::Char('a'),
            action: KeyAction::Press,
            mods: CONTROL,
            text: "",
            unmodified: "a",
            base_layout: Some('q'),
        };
        let mut out = Vec::new();
        encode_key(
            &azerty,
            &plain,
            KeyboardFlags::NONE,
            KeyOptions::default(),
            &mut out,
        );
        assert_eq!(out, b"\x01");
        // Nor for a base layout that is not ASCII, nor without a base
        // layout, nor without Control or Meta.
        let not_ascii = KeyEvent {
            key: Key::Char('с'),
            action: KeyAction::Press,
            mods: CONTROL,
            text: "",
            unmodified: "с",
            base_layout: Some('ж'),
        };
        let mut out = Vec::new();
        encode_key(
            &not_ascii,
            &plain,
            KeyboardFlags::NONE,
            KeyOptions::default(),
            &mut out,
        );
        assert_eq!(out, "с".as_bytes());
        assert_eq!(char_key('с', CONTROL, "", "с"), "с");
        assert_eq!(char_key('с', NONE, "с", "с"), "с");
    }

    #[test]
    fn shift_makes_tab_a_back_tab_whatever_else_is_held() {
        let both = Mods {
            control: true,
            shift: true,
            ..NONE
        };
        let alt_shift = Mods { shift: true, ..ALT };
        assert_eq!(named(Key::Tab, both), "\x1b[Z");
        assert_eq!(named(Key::Tab, alt_shift), "\x1b\x1b[Z");
        let level = |n| InputModes {
            modify_other_keys: n,
            ..InputModes::default()
        };
        assert_eq!(send(Key::Tab, both, "", "", &level(1)), "\x1b[27;6;9~");
        assert_eq!(send(Key::Tab, alt_shift, "", "", &level(1)), "\x1b[27;4;9~");
        assert_eq!(send(Key::Tab, SHIFT, "", "", &level(2)), "\x1b[Z");
        assert_eq!(send(Key::Tab, alt_shift, "", "", &level(2)), "\x1b[27;4;9~");
    }

    #[test]
    fn the_keypad_without_num_lock_sends_the_keys_it_stands_for() {
        let application = InputModes {
            cursor_keys_application: true,
            keypad_application: true,
            ..InputModes::default()
        };
        let numeric = InputModes::default();
        let cases = [
            (Keypad::Left, NONE, "\x1b[D", "\x1bOD"),
            (Keypad::Right, NONE, "\x1b[C", "\x1bOC"),
            (Keypad::Up, NONE, "\x1b[A", "\x1bOA"),
            (Keypad::Down, NONE, "\x1b[B", "\x1bOB"),
            (Keypad::Home, NONE, "\x1b[H", "\x1bOH"),
            (Keypad::End, NONE, "\x1b[F", "\x1bOF"),
            (Keypad::Begin, NONE, "\x1b[E", "\x1bOE"),
            (Keypad::Begin, CONTROL, "\x1b[1;5E", "\x1b[1;5E"),
            (Keypad::Insert, NONE, "\x1b[2~", "\x1b[2~"),
            (Keypad::Delete, NONE, "\x1b[3~", "\x1b[3~"),
            (Keypad::PageUp, NONE, "\x1b[5~", "\x1b[5~"),
            (Keypad::PageDown, SHIFT, "\x1b[6;2~", "\x1b[6;2~"),
        ];
        for (key, mods, normal, in_application) in cases {
            let keypad = |modes| send(Key::Keypad(key), mods, "", "", modes);
            assert_eq!(keypad(&numeric), normal, "{key:?}");
            assert_eq!(keypad(&application), in_application, "{key:?}");
        }
    }

    #[test]
    fn menu_is_f16_and_keys_without_a_sequence_send_nothing() {
        assert_eq!(named(Key::Menu, NONE), "\x1b[29~");
        assert_eq!(named(Key::Menu, CONTROL), "\x1b[29;5~");
        for key in [
            Key::CapsLock,
            Key::ScrollLock,
            Key::NumLock,
            Key::PrintScreen,
            Key::Pause,
            Key::Media(Media::Play),
            Key::Modifier(ModifierKey::LeftShift),
        ] {
            assert_eq!(named(key, NONE), "", "{key:?}");
        }
    }
}
