//! The `[text]` section of the configuration: how the text's styles are
//! drawn.
//!
//! ```toml
//! [text]
//! bold = true     # whether bold text takes the bold face
//! italic = true   # whether italic text takes the italic face
//! ```

use serde::Deserialize;

/// The `[text]` section: which styles of text take their faces.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct TextConfig {
    /// Whether bold text takes the bold face: without, the regular one.
    pub bold: bool,
    /// Whether italic text takes the italic face: without, it is upright.
    pub italic: bool,
}

impl Default for TextConfig {
    fn default() -> Self {
        Self {
            bold: true,
            italic: true,
        }
    }
}
