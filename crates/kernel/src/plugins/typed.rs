//! A key that a plugin sends a pane, pressed and released, as the
//! keyboard's encoder sends a key typed with its modifiers.

use lokyr_plugin_sdk::keys::{Combo, Named, Press};
use lokyr_vt_core::input::{Key, KeyAction, KeyEvent, KeyOptions, Mods};

use crate::workspace::Pane;

/// Sends `pane` the key of `combo`, pressed and released, Option acting as
/// Meta whatever the config says, as a script means `alt+f` as Meta-f.
pub(super) fn send(pane: &Pane, combo: Combo) {
    let Some(typed) = typed(combo) else {
        return;
    };
    for action in [KeyAction::Press, KeyAction::Release] {
        let event = KeyEvent {
            key: typed.key,
            action,
            mods: typed.mods,
            text: &typed.text,
            unmodified: &typed.unmodified,
            base_layout: typed.base_layout,
        };
        pane.key(&event, KeyOptions::default());
    }
}

/// A key as the encoder takes it, of the texts that it owns.
#[derive(Debug, PartialEq, Eq)]
struct Typed {
    key: Key,
    mods: Mods,
    text: String,
    unmodified: String,
    base_layout: Option<char>,
}

/// The key that `combo` types, as a US keyboard types it: a character's,
/// its Shift's character with Shift, as `!` of `shift+1`, which Control
/// and Command type nothing of; none of a click or a wheel's step.
fn typed(combo: Combo) -> Option<Typed> {
    let mods = Mods {
        shift: combo.mods.shift,
        control: combo.mods.ctrl,
        left_alt: combo.mods.alt,
        super_key: combo.mods.cmd,
        ..Mods::NONE
    };
    let types = !combo.mods.ctrl && !combo.mods.cmd;
    let (key, unmodified) = match combo.press {
        Press::Char(c) => (Key::Char(c), Some(shifted(c, combo.mods.shift))),
        Press::Named(Named::Space) => (Key::Char(' '), Some(' ')),
        Press::Named(named) => (named_key(named), None),
        Press::Wheel(_) | Press::Click => return None,
    };
    let unmodified = unmodified.map(String::from).unwrap_or_default();
    Some(Typed {
        key,
        mods,
        text: if types {
            unmodified.clone()
        } else {
            String::new()
        },
        base_layout: match key {
            Key::Char(c) if c.is_ascii() => Some(c),
            _ => None,
        },
        unmodified,
    })
}

/// The encoder's key of a named key.
fn named_key(named: Named) -> Key {
    match named {
        Named::Enter => Key::Enter,
        Named::Tab => Key::Tab,
        Named::Backspace => Key::Backspace,
        Named::Escape => Key::Escape,
        Named::Space => Key::Char(' '),
        Named::Delete => Key::Delete,
        Named::Insert => Key::Insert,
        Named::Home => Key::Home,
        Named::End => Key::End,
        Named::PageUp => Key::PageUp,
        Named::PageDown => Key::PageDown,
        Named::Up => Key::Up,
        Named::Down => Key::Down,
        Named::Left => Key::Left,
        Named::Right => Key::Right,
        Named::F(number) => Key::F(number),
    }
}

/// The character that `c`'s key types with Shift on a US keyboard, if
/// `shift`: a letter's capital, a digit's or a sign's other character; `c`
/// otherwise, or of a key that it does not know.
fn shifted(c: char, shift: bool) -> char {
    const PAIRS: &str = "1!2@3#4$5%6^7&8*9(0)-_=+[{]}\\|;:'\",<.>/?`~";
    if !shift {
        return c;
    }
    if c.is_ascii_lowercase() {
        return c.to_ascii_uppercase();
    }
    let pairs: Vec<char> = PAIRS.chars().collect();
    pairs
        .chunks(2)
        .find(|pair| pair[0] == c)
        .map_or(c, |pair| pair[1])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn of(keys: &str) -> Typed {
        typed(lokyr_plugin_sdk::keys::typed_key(keys).unwrap()).unwrap()
    }

    /// What `keys` sends of no mode nor keyboard flag, as the shell's line
    /// editor takes it.
    fn sent(keys: &str) -> Vec<u8> {
        let typed = of(keys);
        let mut out = Vec::new();
        let event = KeyEvent {
            key: typed.key,
            action: KeyAction::Press,
            mods: typed.mods,
            text: &typed.text,
            unmodified: &typed.unmodified,
            base_layout: typed.base_layout,
        };
        lokyr_vt_core::input::encode_key(
            &event,
            &lokyr_vt_core::terminal::InputModes::default(),
            lokyr_vt_core::terminal::KeyboardFlags::NONE,
            KeyOptions::default(),
            &mut out,
        );
        out
    }

    #[test]
    fn a_combination_sends_what_the_keyboard_sends_of_its_key() {
        assert_eq!(sent("ctrl+c"), b"\x03");
        assert_eq!(sent("a"), b"a");
        assert_eq!(sent("shift+a"), b"A");
        assert_eq!(sent("shift+1"), b"!");
        assert_eq!(sent("alt+f"), b"\x1bf");
        assert_eq!(sent("enter"), b"\r");
        assert_eq!(sent("space"), b" ");
        assert_eq!(sent("up"), b"\x1b[A");
        assert_eq!(sent("shift+tab"), b"\x1b[Z");
        assert_eq!(sent("f5"), b"\x1b[15~");
        // Command sends nothing of no keyboard flag, as a program learns of
        // it through the keyboard protocol alone.
        assert_eq!(of("cmd+k").text, "");
    }
}
