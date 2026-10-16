//! Encoding of keys, mouse events, focus changes and pastes into the bytes
//! that programs expect.

mod keyboard;
mod kitty;

pub use keyboard::{
    Key, KeyAction, KeyEvent, KeyOptions, Keypad, Media, ModifierKey, Mods, encode_key, encode_text,
};
