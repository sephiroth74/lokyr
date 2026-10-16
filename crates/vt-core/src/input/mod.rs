//! Encoding of keys, mouse events, focus changes and pastes into the bytes
//! that programs expect.

mod keyboard;

pub use keyboard::{Key, KeyEvent, KeyOptions, Keypad, Mods, encode_key};
