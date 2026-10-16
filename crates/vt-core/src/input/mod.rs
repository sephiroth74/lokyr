//! Encoding of keys, mouse events, focus changes and pastes into the bytes
//! that programs expect.

mod keyboard;
mod kitty;
mod mouse;
mod paste;

pub use keyboard::{
    Key, KeyAction, KeyEvent, KeyOptions, Keypad, Media, ModifierKey, Mods, encode_key, encode_text,
};
pub use mouse::{MouseAction, MouseButton, MouseEvent, Wheel, encode_focus, encode_mouse};
pub use paste::encode_paste;
