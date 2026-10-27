//! The platform's keys, as the terminal's encoder takes them.

use lokyr_kernel::config::OptionAsMeta;
use lokyr_platform_core::{KeyCode, KeyEvent, KeyState};
use lokyr_vt_core::input::{self, Key, KeyAction, KeyOptions, Keypad, Media, ModifierKey, Mods};

/// The key options of `option_as_meta`: which Option keys act as Meta.
pub(crate) fn options(option_as_meta: OptionAsMeta) -> KeyOptions {
    let (left, right) = option_as_meta.sides();
    KeyOptions {
        left_alt_is_meta: left,
        right_alt_is_meta: right,
    }
}

/// `event`, as the encoder takes it; `None` for a key that it has no name
/// for, such as Fn.
pub(crate) fn terminal_key(event: &KeyEvent) -> Option<input::KeyEvent<'_>> {
    Some(input::KeyEvent {
        key: key(event)?,
        action: match event.state {
            KeyState::Pressed => KeyAction::Press,
            KeyState::Repeated => KeyAction::Repeat,
            KeyState::Released => KeyAction::Release,
        },
        mods: Mods {
            shift: event.mods.shift,
            control: event.mods.control,
            left_alt: event.mods.left_alt,
            right_alt: event.mods.right_alt,
            super_key: event.mods.super_key,
            caps_lock: event.mods.caps_lock,
            ..Mods::NONE
        },
        text: &event.text,
        unmodified: &event.unmodified,
        base_layout: event.code.base_layout(),
    })
}

/// The encoder's key for `event`'s: a key that types a character takes the
/// one that it types without any modifier, or, when the layout gives none,
/// as a dead key's, the US layout's.
fn key(event: &KeyEvent) -> Option<Key> {
    use KeyCode::*;
    Some(match event.code {
        Enter => Key::Enter,
        Tab => Key::Tab,
        Backspace => Key::Backspace,
        Escape => Key::Escape,
        Delete => Key::Delete,
        Insert => Key::Insert,
        Home => Key::Home,
        End => Key::End,
        PageUp => Key::PageUp,
        PageDown => Key::PageDown,
        ArrowUp => Key::Up,
        ArrowDown => Key::Down,
        ArrowLeft => Key::Left,
        ArrowRight => Key::Right,
        F(n) => Key::F(n),
        Numpad0 | Numpad1 | Numpad2 | Numpad3 | Numpad4 | Numpad5 | Numpad6 | Numpad7 | Numpad8
        | Numpad9 => Key::Keypad(Keypad::Digit(numpad_digit(event.code))),
        NumpadDecimal => Key::Keypad(Keypad::Decimal),
        NumpadAdd => Key::Keypad(Keypad::Add),
        NumpadSubtract => Key::Keypad(Keypad::Subtract),
        NumpadMultiply => Key::Keypad(Keypad::Multiply),
        NumpadDivide => Key::Keypad(Keypad::Divide),
        NumpadEnter => Key::Keypad(Keypad::Enter),
        NumpadEqual => Key::Keypad(Keypad::Equal),
        NumpadComma => Key::Keypad(Keypad::Separator),
        NumLock => Key::NumLock,
        CapsLock => Key::CapsLock,
        ShiftLeft => Key::Modifier(ModifierKey::LeftShift),
        ShiftRight => Key::Modifier(ModifierKey::RightShift),
        ControlLeft => Key::Modifier(ModifierKey::LeftControl),
        ControlRight => Key::Modifier(ModifierKey::RightControl),
        AltLeft => Key::Modifier(ModifierKey::LeftAlt),
        AltRight => Key::Modifier(ModifierKey::RightAlt),
        SuperLeft => Key::Modifier(ModifierKey::LeftSuper),
        SuperRight => Key::Modifier(ModifierKey::RightSuper),
        ContextMenu => Key::Menu,
        AudioVolumeUp => Key::Media(Media::RaiseVolume),
        AudioVolumeDown => Key::Media(Media::LowerVolume),
        AudioVolumeMute => Key::Media(Media::MuteVolume),
        Fn | Lang1 | Lang2 => return None,
        _ => Key::Char(typed(event)?),
    })
}

/// The character that a key types without any modifier: the layout's, or
/// the US layout's when the layout gives none.
fn typed(event: &KeyEvent) -> Option<char> {
    let mut plain = event.plain.chars();
    match (plain.next(), plain.next()) {
        (Some(character), None) if !character.is_control() => Some(character),
        _ => event.code.base_layout(),
    }
}

/// The digit of a key of the keypad's digits.
fn numpad_digit(code: KeyCode) -> u8 {
    use KeyCode::*;
    match code {
        Numpad1 => 1,
        Numpad2 => 2,
        Numpad3 => 3,
        Numpad4 => 4,
        Numpad5 => 5,
        Numpad6 => 6,
        Numpad7 => 7,
        Numpad8 => 8,
        Numpad9 => 9,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use lokyr_platform_core::Modifiers;
    use lokyr_vt_core::input::encode_key;
    use lokyr_vt_core::terminal::{InputModes, KeyboardFlags};

    use super::*;

    fn event(
        code: KeyCode,
        mods: Modifiers,
        text: &str,
        unmodified: &str,
        plain: &str,
    ) -> KeyEvent {
        KeyEvent {
            code,
            state: KeyState::Pressed,
            mods,
            text: text.to_owned(),
            unmodified: unmodified.to_owned(),
            plain: plain.to_owned(),
        }
    }

    /// What `event` sends, with `flags` and Option as Meta as `meta` says.
    fn sends(event: &KeyEvent, flags: KeyboardFlags, meta: OptionAsMeta) -> String {
        let mut out = Vec::new();
        let key = terminal_key(event).unwrap();
        encode_key(&key, &InputModes::default(), flags, options(meta), &mut out);
        String::from_utf8(out).unwrap()
    }

    const LEFT_OPTION: Modifiers = Modifiers {
        left_alt: true,
        ..Modifiers::NONE
    };

    #[test]
    fn a_key_takes_what_it_types_without_modifiers() {
        // `ò` on the Italian layout, with Shift: `ç`.
        let shifted = Modifiers {
            shift: true,
            ..Modifiers::NONE
        };
        let key = event(KeyCode::Semicolon, shifted, "ç", "ç", "ò");
        let terminal = terminal_key(&key).unwrap();
        assert_eq!(terminal.key, Key::Char('ò'));
        assert_eq!(terminal.base_layout, Some(';'));
        // A dead key, which types nothing by itself, takes the US layout's.
        let dead = event(KeyCode::Quote, Modifiers::NONE, "", "", "");
        assert_eq!(terminal_key(&dead).unwrap().key, Key::Char('\''));
        assert!(terminal_key(&event(KeyCode::Fn, Modifiers::NONE, "", "", "")).is_none());
    }

    #[test]
    fn option_types_its_characters_unless_it_acts_as_meta() {
        // Option and `ò` type `@` on the Italian layout.
        let at = event(KeyCode::Semicolon, LEFT_OPTION, "@", "ò", "ò");
        let legacy = KeyboardFlags::NONE;
        assert_eq!(sends(&at, legacy, OptionAsMeta::None), "@");
        // As Meta, the escape and the key, as the US layout has it, for a
        // character past ASCII.
        assert_eq!(sends(&at, legacy, OptionAsMeta::Left), "\x1b;");
        assert_eq!(sends(&at, legacy, OptionAsMeta::Right), "@");
        let right = Modifiers {
            right_alt: true,
            ..Modifiers::NONE
        };
        let b = event(KeyCode::KeyB, right, "∫", "b", "b");
        assert_eq!(sends(&b, legacy, OptionAsMeta::Right), "\x1bb");
        assert_eq!(sends(&b, legacy, OptionAsMeta::Both), "\x1bb");
    }

    #[test]
    fn the_kitty_protocol_takes_the_key_without_shift() {
        let shifted = Modifiers {
            shift: true,
            ..Modifiers::NONE
        };
        let key = event(KeyCode::KeyA, shifted, "A", "A", "a");
        let flags = KeyboardFlags::from_bits(u16::from(
            KeyboardFlags::DISAMBIGUATE.bits() | KeyboardFlags::REPORT_ALL_KEYS.bits(),
        ));
        assert_eq!(sends(&key, flags, OptionAsMeta::None), "\x1b[97;2u");
        let control = Modifiers {
            control: true,
            ..Modifiers::NONE
        };
        let c = event(KeyCode::KeyC, control, "\u{3}", "c", "c");
        assert_eq!(sends(&c, KeyboardFlags::NONE, OptionAsMeta::None), "\u{3}");
        assert_eq!(
            sends(&c, KeyboardFlags::DISAMBIGUATE, OptionAsMeta::None),
            "\x1b[99;5u"
        );
    }

    #[test]
    fn the_keypad_and_the_modifiers_take_their_names() {
        let seven = event(KeyCode::Numpad7, Modifiers::NONE, "7", "7", "7");
        assert_eq!(
            terminal_key(&seven).unwrap().key,
            Key::Keypad(Keypad::Digit(7))
        );
        let option = event(KeyCode::AltRight, Modifiers::NONE, "", "", "");
        assert_eq!(
            terminal_key(&option).unwrap().key,
            Key::Modifier(ModifierKey::RightAlt)
        );
        let up = event(
            KeyCode::ArrowUp,
            Modifiers::NONE,
            "\u{f700}",
            "\u{f700}",
            "\u{f700}",
        );
        assert_eq!(
            sends(&up, KeyboardFlags::NONE, OptionAsMeta::None),
            "\x1b[A"
        );
    }
}
