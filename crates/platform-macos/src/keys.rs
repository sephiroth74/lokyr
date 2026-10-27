//! Keys, as AppKit's events tell them: their place on the keyboard, by
//! macOS's virtual key codes, what they type, and the modifiers held, the
//! two of each side apart.

use lokyr_platform_core::{KeyCode, KeyEvent, KeyState, Modifiers};
use objc2_app_kit::{NSEvent, NSEventModifierFlags, NSEventType};

/// The bits of `modifierFlags` that tell the key of each side, as IOKit's
/// `NX_DEVICE…KEYMASK`s number them.
mod side {
    pub(super) const LEFT_CONTROL: usize = 0x0001;
    pub(super) const LEFT_SHIFT: usize = 0x0002;
    pub(super) const RIGHT_SHIFT: usize = 0x0004;
    pub(super) const LEFT_COMMAND: usize = 0x0008;
    pub(super) const RIGHT_COMMAND: usize = 0x0010;
    pub(super) const LEFT_OPTION: usize = 0x0020;
    pub(super) const RIGHT_OPTION: usize = 0x0040;
    pub(super) const RIGHT_CONTROL: usize = 0x2000;
}

/// The physical layout of an ISO keyboard, as Carbon's `kKeyboardISO`
/// names it: `'ISO '`.
const ISO: u32 = u32::from_be_bytes(*b"ISO ");

#[link(name = "Carbon", kind = "framework")]
unsafe extern "C" {
    /// The type of the keyboard that sent the last event.
    fn LMGetKbdType() -> u8;
    /// The physical layout of a type of keyboard: ANSI, ISO or JIS.
    fn KBGetLayoutType(keyboard: i16) -> u32;
}

/// The key that `event`, a key down, a key up or a change of the
/// modifiers, tells of; `None` for an event of another type.
pub(crate) fn key_event(event: &NSEvent) -> Option<KeyEvent> {
    let flags = event.modifierFlags();
    let mut code = key_code(event.keyCode());
    // The key left of 1 and the one right of the left Shift report each
    // other's codes on ISO keyboards.
    // SAFETY: Carbon's functions, which read the keyboard's type.
    if unsafe { KBGetLayoutType(i16::from(LMGetKbdType())) } == ISO {
        code = match code {
            KeyCode::IntlBackslash => KeyCode::Backquote,
            KeyCode::Backquote => KeyCode::IntlBackslash,
            other => other,
        };
    }
    let mods = modifiers(flags);
    let text = |text: Option<objc2::rc::Retained<objc2_foundation::NSString>>| {
        text.map(|text| text.to_string()).unwrap_or_default()
    };
    let (state, typed) = match event.r#type() {
        NSEventType::KeyDown | NSEventType::KeyUp => {
            let state = if event.r#type() == NSEventType::KeyUp {
                KeyState::Released
            } else if event.isARepeat() {
                KeyState::Repeated
            } else {
                KeyState::Pressed
            };
            let typed = (
                text(event.characters()),
                text(event.charactersIgnoringModifiers()),
                text(event.charactersByApplyingModifiers(NSEventModifierFlags::empty())),
            );
            (state, typed)
        }
        // A change of the modifiers has no characters, whose getters would
        // raise: the key went down if its bit is set now.
        NSEventType::FlagsChanged => {
            let down = modifier_down(code, flags)?;
            let state = if down {
                KeyState::Pressed
            } else {
                KeyState::Released
            };
            (state, Default::default())
        }
        _ => return None,
    };
    let (text, unmodified, plain) = typed;
    Some(KeyEvent {
        code,
        state,
        mods,
        text,
        unmodified,
        plain,
    })
}

/// The modifiers that `flags` hold, each Option apart; a flag without its
/// side's bit, as a synthetic event's, counts as the left key's.
pub(crate) fn modifiers(flags: NSEventModifierFlags) -> Modifiers {
    let bits = flags.0;
    let option = flags.contains(NSEventModifierFlags::Option);
    let left = bits & side::LEFT_OPTION != 0;
    let right = bits & side::RIGHT_OPTION != 0;
    Modifiers {
        shift: flags.contains(NSEventModifierFlags::Shift),
        control: flags.contains(NSEventModifierFlags::Control),
        left_alt: option && (left || !right),
        right_alt: option && right,
        super_key: flags.contains(NSEventModifierFlags::Command),
        caps_lock: flags.contains(NSEventModifierFlags::CapsLock),
    }
}

/// Whether the modifier key `code` is down, as `flags` hold it; `None` for
/// a key that is not a modifier.
fn modifier_down(code: KeyCode, flags: NSEventModifierFlags) -> Option<bool> {
    let bits = flags.0;
    let (bit, flag) = match code {
        KeyCode::ShiftLeft => (side::LEFT_SHIFT, NSEventModifierFlags::Shift),
        KeyCode::ShiftRight => (side::RIGHT_SHIFT, NSEventModifierFlags::Shift),
        KeyCode::ControlLeft => (side::LEFT_CONTROL, NSEventModifierFlags::Control),
        KeyCode::ControlRight => (side::RIGHT_CONTROL, NSEventModifierFlags::Control),
        KeyCode::AltLeft => (side::LEFT_OPTION, NSEventModifierFlags::Option),
        KeyCode::AltRight => (side::RIGHT_OPTION, NSEventModifierFlags::Option),
        KeyCode::SuperLeft => (side::LEFT_COMMAND, NSEventModifierFlags::Command),
        KeyCode::SuperRight => (side::RIGHT_COMMAND, NSEventModifierFlags::Command),
        KeyCode::CapsLock => return Some(flags.contains(NSEventModifierFlags::CapsLock)),
        KeyCode::Fn => return Some(flags.contains(NSEventModifierFlags::Function)),
        _ => return None,
    };
    // An event without the sides' bits, as a synthetic one, has the flag
    // alone.
    let sides = side::LEFT_SHIFT
        | side::RIGHT_SHIFT
        | side::LEFT_CONTROL
        | side::RIGHT_CONTROL
        | side::LEFT_OPTION
        | side::RIGHT_OPTION
        | side::LEFT_COMMAND
        | side::RIGHT_COMMAND;
    Some(if bits & sides == 0 {
        flags.contains(flag)
    } else {
        bits & bit != 0
    })
}

/// The key of macOS's virtual key code `code`, as Carbon's `kVK_` constants
/// number them, by the place that it has on an ANSI keyboard.
pub(crate) fn key_code(code: u16) -> KeyCode {
    use KeyCode::*;
    match code {
        0x00 => KeyA,
        0x01 => KeyS,
        0x02 => KeyD,
        0x03 => KeyF,
        0x04 => KeyH,
        0x05 => KeyG,
        0x06 => KeyZ,
        0x07 => KeyX,
        0x08 => KeyC,
        0x09 => KeyV,
        0x0A => IntlBackslash,
        0x0B => KeyB,
        0x0C => KeyQ,
        0x0D => KeyW,
        0x0E => KeyE,
        0x0F => KeyR,
        0x10 => KeyY,
        0x11 => KeyT,
        0x12 => Digit1,
        0x13 => Digit2,
        0x14 => Digit3,
        0x15 => Digit4,
        0x16 => Digit6,
        0x17 => Digit5,
        0x18 => Equal,
        0x19 => Digit9,
        0x1A => Digit7,
        0x1B => Minus,
        0x1C => Digit8,
        0x1D => Digit0,
        0x1E => BracketRight,
        0x1F => KeyO,
        0x20 => KeyU,
        0x21 => BracketLeft,
        0x22 => KeyI,
        0x23 => KeyP,
        0x24 => Enter,
        0x25 => KeyL,
        0x26 => KeyJ,
        0x27 => Quote,
        0x28 => KeyK,
        0x29 => Semicolon,
        0x2A => Backslash,
        0x2B => Comma,
        0x2C => Slash,
        0x2D => KeyN,
        0x2E => KeyM,
        0x2F => Period,
        0x30 => Tab,
        0x31 => Space,
        0x32 => Backquote,
        0x33 => Backspace,
        // The Enter key of old PowerBooks' keyboards.
        0x34 => NumpadEnter,
        0x35 => Escape,
        0x36 => SuperRight,
        0x37 => SuperLeft,
        0x38 => ShiftLeft,
        0x39 => CapsLock,
        0x3A => AltLeft,
        0x3B => ControlLeft,
        0x3C => ShiftRight,
        0x3D => AltRight,
        0x3E => ControlRight,
        0x3F => Fn,
        0x40 => F(17),
        0x41 => NumpadDecimal,
        0x43 => NumpadMultiply,
        0x45 => NumpadAdd,
        0x47 => NumLock,
        0x48 => AudioVolumeUp,
        0x49 => AudioVolumeDown,
        0x4A => AudioVolumeMute,
        0x4B => NumpadDivide,
        0x4C => NumpadEnter,
        0x4E => NumpadSubtract,
        0x4F => F(18),
        0x50 => F(19),
        0x51 => NumpadEqual,
        0x52 => Numpad0,
        0x53 => Numpad1,
        0x54 => Numpad2,
        0x55 => Numpad3,
        0x56 => Numpad4,
        0x57 => Numpad5,
        0x58 => Numpad6,
        0x59 => Numpad7,
        0x5A => F(20),
        0x5B => Numpad8,
        0x5C => Numpad9,
        0x5D => IntlYen,
        0x5E => IntlRo,
        0x5F => NumpadComma,
        0x60 => F(5),
        0x61 => F(6),
        0x62 => F(7),
        0x63 => F(3),
        0x64 => F(8),
        0x65 => F(9),
        0x66 => Lang2,
        0x67 => F(11),
        0x68 => Lang1,
        0x69 => F(13),
        0x6A => F(16),
        0x6B => F(14),
        0x6D => F(10),
        0x6E => ContextMenu,
        0x6F => F(12),
        0x71 => F(15),
        0x72 => Insert,
        0x73 => Home,
        0x74 => PageUp,
        0x75 => Delete,
        0x76 => F(4),
        0x77 => End,
        0x78 => F(2),
        0x79 => PageDown,
        0x7A => F(1),
        0x7B => ArrowLeft,
        0x7C => ArrowRight,
        0x7D => ArrowDown,
        0x7E => ArrowUp,
        other => Unidentified(u32::from(other)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_keys_of_an_ansi_keyboard_take_their_places() {
        let codes: Vec<(u16, KeyCode)> = [
            (0x00, KeyCode::KeyA),
            (0x0C, KeyCode::KeyQ),
            (0x29, KeyCode::Semicolon),
            (0x32, KeyCode::Backquote),
            (0x24, KeyCode::Enter),
            (0x33, KeyCode::Backspace),
            (0x75, KeyCode::Delete),
            (0x7A, KeyCode::F(1)),
            (0x5A, KeyCode::F(20)),
            (0x52, KeyCode::Numpad0),
            (0x4C, KeyCode::NumpadEnter),
            (0x3D, KeyCode::AltRight),
            (0x36, KeyCode::SuperRight),
            (0x7E, KeyCode::ArrowUp),
        ]
        .into_iter()
        .collect();
        for (code, key) in codes {
            assert_eq!(key_code(code), key, "{code:#x}");
        }
        assert_eq!(key_code(0x42), KeyCode::Unidentified(0x42));
    }

    #[test]
    fn the_two_options_are_told_apart() {
        let flags = |bits: usize| NSEventModifierFlags(bits);
        let option = NSEventModifierFlags::Option.0;
        let left = modifiers(flags(option | side::LEFT_OPTION));
        assert!(left.left_alt && !left.right_alt);
        let right = modifiers(flags(option | side::RIGHT_OPTION));
        assert!(!right.left_alt && right.right_alt);
        let both = modifiers(flags(option | side::LEFT_OPTION | side::RIGHT_OPTION));
        assert!(both.left_alt && both.right_alt);
        // Without the sides' bits, the left one.
        let bare = modifiers(flags(option));
        assert!(bare.left_alt && !bare.right_alt);
        let none = modifiers(flags(side::LEFT_OPTION));
        assert!(!none.alt());
    }

    #[test]
    fn a_modifier_goes_down_with_its_side_s_bit() {
        let shift = NSEventModifierFlags::Shift.0;
        let flags = NSEventModifierFlags(shift | side::RIGHT_SHIFT);
        assert_eq!(modifier_down(KeyCode::ShiftRight, flags), Some(true));
        // The left one went up while the right one stays down.
        assert_eq!(modifier_down(KeyCode::ShiftLeft, flags), Some(false));
        assert_eq!(
            modifier_down(KeyCode::ShiftLeft, NSEventModifierFlags::Shift),
            Some(true)
        );
        assert_eq!(
            modifier_down(KeyCode::CapsLock, NSEventModifierFlags::empty()),
            Some(false)
        );
        assert_eq!(
            modifier_down(KeyCode::KeyA, NSEventModifierFlags::Shift),
            None
        );
    }
}
