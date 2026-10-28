//! The `[text]` section of the configuration: how the text's styles and
//! colors are drawn.
//!
//! ```toml
//! [text]
//! bold = true             # whether bold text takes the bold face
//! italic = true           # whether italic text takes the italic face
//! ansi-colors = true      # whether the colors that programs set show
//! bold-is-bright = false  # whether bold text of the first eight colors takes their bright ones
//! blink = true            # whether text that programs make blink blinks
//! ```

use serde::{Deserialize, Serialize};

/// The `[text]` section: which styles of text take their faces, and how the
/// programs' colors and blinking show.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct TextConfig {
    /// Whether bold text takes the bold face: without, the regular one.
    pub bold: bool,
    /// Whether italic text takes the italic face: without, it is upright.
    pub italic: bool,
    /// Whether the colors that programs set, of the palette or RGB, show:
    /// without, the text and its background take the default ones.
    pub ansi_colors: bool,
    /// Whether bold text of one of the palette's first eight colors takes
    /// its bright one, of the eight after.
    pub bold_is_bright: bool,
    /// Whether text that programs make blink, with SGR 5 or 6, blinks:
    /// without, it shows steadily.
    pub blink: bool,
}

impl Default for TextConfig {
    fn default() -> Self {
        Self {
            bold: true,
            italic: true,
            ansi_colors: true,
            bold_is_bright: false,
            blink: true,
        }
    }
}
