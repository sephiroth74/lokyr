//! The keys that the checks type into the app, as a hand would: posted to
//! the app's process alone, so that none reaches another app, whichever
//! is in front.
//!
//! Posting them takes the Accessibility permission of the app that runs
//! the checks.

use std::thread;
use std::time::Duration;

use objc2_core_graphics::{CGEvent, CGEventFlags};

/// The pause after each key, as a hand's, for the app to take it.
const PAUSE: Duration = Duration::from_millis(30);

/// A key of the Mac's keyboard, by its virtual key code, as the system's
/// `kVK_` constants name them on an ANSI keyboard.
#[derive(Clone, Copy, Debug)]
pub enum Key {
    Return,
    Escape,
    Letter(char),
    LeftBracket,
}

impl Key {
    /// Its virtual key code.
    fn code(self) -> u16 {
        match self {
            Self::Return => 0x24,
            Self::Escape => 0x35,
            Self::LeftBracket => 0x21,
            Self::Letter(letter) => match letter.to_ascii_lowercase() {
                'a' => 0x00,
                's' => 0x01,
                'd' => 0x02,
                'f' => 0x03,
                'h' => 0x04,
                'g' => 0x05,
                'z' => 0x06,
                'x' => 0x07,
                'c' => 0x08,
                'v' => 0x09,
                'b' => 0x0b,
                'q' => 0x0c,
                'w' => 0x0d,
                'e' => 0x0e,
                'r' => 0x0f,
                'y' => 0x10,
                't' => 0x11,
                'o' => 0x1f,
                'u' => 0x20,
                'i' => 0x22,
                'p' => 0x23,
                'l' => 0x25,
                'j' => 0x26,
                'k' => 0x28,
                'n' => 0x2d,
                'm' => 0x2e,
                other => panic!("no key code for {other:?}"),
            },
        }
    }
}

/// The modifiers held with a key.
#[derive(Clone, Copy, Debug, Default)]
pub struct Mods {
    pub command: bool,
    pub shift: bool,
}

/// Command alone.
pub const CMD: Mods = Mods {
    command: true,
    shift: false,
};

/// Shift and Command.
pub const SHIFT_CMD: Mods = Mods {
    command: true,
    shift: true,
};

/// The virtual key codes of the left Command and Shift keys.
const COMMAND: u16 = 0x37;
const SHIFT: u16 = 0x38;

/// Posts the key `code`, down or up, of `flags`, to the process `pid`.
fn post(pid: libc::pid_t, code: u16, down: bool, flags: CGEventFlags) {
    if let Some(event) = CGEvent::new_keyboard_event(None, code, down) {
        CGEvent::set_flags(Some(&event), flags);
        CGEvent::post_to_pid(pid, Some(&event));
    }
    thread::sleep(PAUSE);
}

/// Presses and releases `key` with `mods` in the process `pid`, each
/// modifier pressed before it and released after, as a keyboard sends
/// them, so that the app's state of the modifiers follows.
pub fn press(pid: libc::pid_t, key: Key, mods: Mods) {
    let mut held = Vec::new();
    if mods.command {
        held.push((COMMAND, CGEventFlags::MaskCommand));
    }
    if mods.shift {
        held.push((SHIFT, CGEventFlags::MaskShift));
    }
    let mut flags = CGEventFlags::empty();
    for &(code, flag) in &held {
        flags |= flag;
        post(pid, code, true, flags);
    }
    post(pid, key.code(), true, flags);
    post(pid, key.code(), false, flags);
    for &(code, flag) in held.iter().rev() {
        flags &= !flag;
        post(pid, code, false, flags);
    }
}

/// Types `text` in the process `pid`, a character at a time, each as the
/// key that types it, of its text alone, whatever the keyboard's layout.
pub fn type_text(pid: libc::pid_t, text: &str) {
    for character in text.chars() {
        let mut units = [0u16; 2];
        let units = character.encode_utf16(&mut units);
        for down in [true, false] {
            if let Some(event) = CGEvent::new_keyboard_event(None, 0, down) {
                // Of no modifier, whatever a hand holds meanwhile, as the
                // event takes the keyboard's state otherwise.
                CGEvent::set_flags(Some(&event), CGEventFlags::empty());
                // SAFETY: the units live through the call, of the length
                // given.
                #[allow(unsafe_code)]
                unsafe {
                    CGEvent::keyboard_set_unicode_string(
                        Some(&event),
                        units.len() as _,
                        units.as_ptr(),
                    );
                }
                CGEvent::post_to_pid(pid, Some(&event));
            }
            thread::sleep(PAUSE);
        }
    }
}
