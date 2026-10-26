//! Keys, the input method and the mouse, as the platform reports them.

use std::ops::Range;

/// The modifiers held, and Caps Lock. The two Alt keys, Option on the
/// Mac, are told apart, as each can act as Meta or not.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Modifiers {
    /// Shift.
    pub shift: bool,
    /// Control.
    pub control: bool,
    /// The left Alt, or Option.
    pub left_alt: bool,
    /// The right Alt, or Option.
    pub right_alt: bool,
    /// Super: Command on the Mac.
    pub super_key: bool,
    /// Caps Lock is on.
    pub caps_lock: bool,
}

impl Modifiers {
    /// No modifier, and Caps Lock off.
    pub const NONE: Self = Self {
        shift: false,
        control: false,
        left_alt: false,
        right_alt: false,
        super_key: false,
        caps_lock: false,
    };

    /// Whether either Alt is held.
    pub fn alt(self) -> bool {
        self.left_alt || self.right_alt
    }
}

/// What happened to a key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyState {
    /// It went down.
    Pressed,
    /// It is held, and the system repeats it.
    Repeated,
    /// It went up.
    Released,
}

/// A key, by its place on the keyboard rather than by what it types, named
/// as the [UI Events `code` values](https://www.w3.org/TR/uievents-code/)
/// name it: by what the key in that place types on the US English layout,
/// PC-101's, so that `KeyQ` is the key that types `a` on a French layout.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
// Most variants name their keys; the others say what they are.
#[allow(missing_docs)]
pub enum KeyCode {
    KeyA,
    KeyB,
    KeyC,
    KeyD,
    KeyE,
    KeyF,
    KeyG,
    KeyH,
    KeyI,
    KeyJ,
    KeyK,
    KeyL,
    KeyM,
    KeyN,
    KeyO,
    KeyP,
    KeyQ,
    KeyR,
    KeyS,
    KeyT,
    KeyU,
    KeyV,
    KeyW,
    KeyX,
    KeyY,
    KeyZ,
    Digit0,
    Digit1,
    Digit2,
    Digit3,
    Digit4,
    Digit5,
    Digit6,
    Digit7,
    Digit8,
    Digit9,
    /// `` ` `` and `~`.
    Backquote,
    /// `-` and `_`.
    Minus,
    /// `=` and `+`.
    Equal,
    /// `[` and `{`.
    BracketLeft,
    /// `]` and `}`.
    BracketRight,
    /// `\` and `|`.
    Backslash,
    /// `;` and `:`.
    Semicolon,
    /// `'` and `"`.
    Quote,
    /// `,` and `<`.
    Comma,
    /// `.` and `>`.
    Period,
    /// `/` and `?`.
    Slash,
    /// The key between the left Shift and Z on ISO keyboards, which
    /// PC-101 lacks.
    IntlBackslash,
    /// The Yen key of Japanese keyboards.
    IntlYen,
    /// The Ro key of Japanese keyboards.
    IntlRo,
    Space,
    Tab,
    Enter,
    Backspace,
    /// Delete forwards: ⌦ on the Mac.
    Delete,
    Escape,
    /// Insert, or Help on the Mac's keyboards that have it.
    Insert,
    Home,
    End,
    PageUp,
    PageDown,
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    /// A function key: F1 to F24.
    F(u8),
    Numpad0,
    Numpad1,
    Numpad2,
    Numpad3,
    Numpad4,
    Numpad5,
    Numpad6,
    Numpad7,
    Numpad8,
    Numpad9,
    NumpadDecimal,
    NumpadAdd,
    NumpadSubtract,
    NumpadMultiply,
    NumpadDivide,
    NumpadEnter,
    NumpadEqual,
    /// The keypad's comma of Japanese keyboards.
    NumpadComma,
    /// Num Lock, or Clear in its place on the Mac's keypad.
    NumLock,
    ShiftLeft,
    ShiftRight,
    ControlLeft,
    ControlRight,
    /// The left Alt, or Option.
    AltLeft,
    /// The right Alt, or Option.
    AltRight,
    /// The left Super, or Command.
    SuperLeft,
    /// The right Super, or Command.
    SuperRight,
    CapsLock,
    /// The Mac's Fn key, or 🌐.
    Fn,
    ContextMenu,
    /// Kana, on Japanese keyboards; Han/Yeong on Korean ones.
    Lang1,
    /// Eisu, on Japanese keyboards; Hanja on Korean ones.
    Lang2,
    AudioVolumeUp,
    AudioVolumeDown,
    AudioVolumeMute,
    /// A key that has no name here, by the platform's own code for it.
    Unidentified(u32),
}

impl KeyCode {
    /// What the key types on the US English layout, PC-101's, without a
    /// modifier, for the keys that type a character; `None` for the others,
    /// and for the keys that PC-101 lacks.
    pub fn base_layout(self) -> Option<char> {
        use KeyCode::*;
        Some(match self {
            KeyA => 'a',
            KeyB => 'b',
            KeyC => 'c',
            KeyD => 'd',
            KeyE => 'e',
            KeyF => 'f',
            KeyG => 'g',
            KeyH => 'h',
            KeyI => 'i',
            KeyJ => 'j',
            KeyK => 'k',
            KeyL => 'l',
            KeyM => 'm',
            KeyN => 'n',
            KeyO => 'o',
            KeyP => 'p',
            KeyQ => 'q',
            KeyR => 'r',
            KeyS => 's',
            KeyT => 't',
            KeyU => 'u',
            KeyV => 'v',
            KeyW => 'w',
            KeyX => 'x',
            KeyY => 'y',
            KeyZ => 'z',
            Digit0 => '0',
            Digit1 => '1',
            Digit2 => '2',
            Digit3 => '3',
            Digit4 => '4',
            Digit5 => '5',
            Digit6 => '6',
            Digit7 => '7',
            Digit8 => '8',
            Digit9 => '9',
            Backquote => '`',
            Minus => '-',
            Equal => '=',
            BracketLeft => '[',
            BracketRight => ']',
            Backslash => '\\',
            Semicolon => ';',
            Quote => '\'',
            Comma => ',',
            Period => '.',
            Slash => '/',
            Space => ' ',
            _ => return None,
        })
    }
}

/// A key that went down, repeats or went up, once the platform has
/// resolved dead keys: what an input method composes comes as [`Ime`]
/// rather than as keys.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyEvent {
    /// The key.
    pub code: KeyCode,
    /// What happened to it.
    pub state: KeyState,
    /// The modifiers held. For a modifier key, as the event leaves them:
    /// pressing Shift holds Shift.
    pub mods: Modifiers,
    /// What the key types with every modifier held, as the layout composes
    /// it: on the Mac, Option's characters. Empty for a key that types
    /// nothing.
    pub text: String,
    /// What the key types with Shift alone, without Control or Alt: on the
    /// Mac, `charactersIgnoringModifiers`.
    pub unmodified: String,
    /// What the key types without any modifier, Shift neither, as the layout
    /// has it: on the Mac, `charactersByApplyingModifiers:` with none. The
    /// kitty protocol takes it as the key's code, which, with Shift held,
    /// the texts above cannot give.
    pub plain: String,
}

/// What an input method does in a window.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Ime {
    /// The text that it composes, which is not typed yet: shown at the
    /// cursor, and never sent. Empty when the composition ends with nothing
    /// typed.
    Preedit {
        /// The text.
        text: String,
        /// The part of `text` that the input method selects, in bytes, or
        /// where its cursor is, when the range is empty.
        selection: Range<usize>,
    },
    /// Text that it typed, which ends the composition.
    Commit(String),
}

/// A button of the mouse.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MouseButton {
    /// The left button, or the primary one.
    Left,
    /// The right button, or the secondary one.
    Right,
    /// The middle button, or the wheel's.
    Middle,
    /// The button that goes back.
    Back,
    /// The button that goes forward.
    Forward,
    /// Another button, by the platform's number for it.
    Other(u16),
}

/// How far a scroll goes. A positive `y` goes towards the top of the
/// content, which shows the lines above, and a positive `x` towards its
/// left edge; the system's choice of natural scrolling is already applied.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ScrollDelta {
    /// By lines, from a wheel that turns in steps.
    Lines {
        /// Across.
        x: f64,
        /// Up and down.
        y: f64,
    },
    /// By points, from a trackpad or a wheel that turns smoothly.
    Points {
        /// Across.
        x: f64,
        /// Up and down.
        y: f64,
    },
}

/// Where a scroll of a trackpad is in its gesture.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScrollPhase {
    /// The fingers touched down, or the momentum began.
    Started,
    /// The gesture goes on.
    Moved,
    /// The fingers lifted, the gesture was cancelled, or the momentum
    /// stopped.
    Ended,
}
