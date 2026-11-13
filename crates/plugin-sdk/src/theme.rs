//! Themes: the colors that a theme gives the terminal and the interface, as
//! a file of the native format writes them, or an `.itermcolors` file
//! read into them. A theme is one variant, of the light appearance or the
//! dark one; it gives colors alone, the chrome's look staying in the
//! config. What it leaves out the host derives from its terminal's colors.

use std::collections::BTreeMap;
use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// The interface's tokens of the host's, by their names, which a theme can
/// give, and which the host derives of its terminal's colors where it does
/// not: a plugin's own tokens, of a section of its name, come beside them.
pub const TOKENS: [&str; 60] = [
    "text",
    "text.secondary",
    "text.disabled",
    "text.success",
    "text.warning",
    "accent",
    "separator",
    "pane.divider",
    "pane.drop",
    "shadow",
    "panel.bg",
    "panel.border",
    "field.bg",
    "field.border",
    "field.focus",
    "field.selection",
    "focus.ring",
    "button.bg",
    "button.hover.bg",
    "button.pressed.bg",
    "toggle.on.bg",
    "toggle.on.fg",
    "tooltip.bg",
    "tooltip.fg",
    "tooltip.border",
    "list.selection.bg",
    "list.selection.fg",
    "list.selection.inactive.bg",
    "scrollbar",
    "scrollbar.hover",
    "scrollbar.track",
    "scrollbar.mark",
    "scrollbar.mark.current",
    "progress.track",
    "progress.normal",
    "progress.error",
    "progress.paused",
    "tabs.bar.bg",
    "tabs.track.bg",
    "tabs.active.bg",
    "tabs.active.fg",
    "tabs.active.border",
    "tabs.active.shadow",
    "tabs.inactive.bg",
    "tabs.inactive.fg",
    "tabs.hover.bg",
    "tabs.seamless.line",
    "tabs.seamless.veil",
    "segments.bg",
    "segments.chosen.bg",
    "segments.chosen.fg",
    "segments.box.bg",
    "segments.box.border",
    "search.match",
    "search.match.current",
    "gutter.running",
    "gutter.succeeded",
    "gutter.failed",
    "terminal.bg",
    "terminal.fg",
];

/// A color of a theme, in sRGB, of an opacity: `#rrggbb`, or `#rrggbbaa`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ThemeColor {
    /// Red.
    pub red: u8,
    /// Green.
    pub green: u8,
    /// Blue.
    pub blue: u8,
    /// The opacity, from 0, clear, to 255, opaque.
    pub alpha: u8,
}

impl ThemeColor {
    /// The opaque color of `red`, `green` and `blue`.
    pub const fn rgb(red: u8, green: u8, blue: u8) -> Self {
        Self {
            red,
            green,
            blue,
            alpha: 255,
        }
    }

    /// The opaque color of `rgb`, `0xrrggbb`.
    pub const fn hex(rgb: u32) -> Self {
        Self::rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
    }

    /// The color, of `alpha`.
    pub const fn with_alpha(self, alpha: u8) -> Self {
        Self { alpha, ..self }
    }
}

impl fmt::Display for ThemeColor {
    /// `#rrggbb`, and its opacity after where it is not opaque.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{:02x}{:02x}{:02x}", self.red, self.green, self.blue)?;
        if self.alpha != 255 {
            write!(f, "{:02x}", self.alpha)?;
        }
        Ok(())
    }
}

/// Why a text is no color of a theme.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ColorError(String);

impl fmt::Display for ColorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "`{}` is no color: one is `#rrggbb` or `#rrggbbaa`",
            self.0
        )
    }
}

impl std::error::Error for ColorError {}

impl FromStr for ThemeColor {
    type Err = ColorError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let error = || ColorError(text.to_owned());
        let hex = text.strip_prefix('#').ok_or_else(error)?;
        if !matches!(hex.len(), 6 | 8) || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(error());
        }
        let byte = |at: usize| u8::from_str_radix(&hex[at..at + 2], 16).map_err(|_| error());
        Ok(Self {
            red: byte(0)?,
            green: byte(2)?,
            blue: byte(4)?,
            alpha: if hex.len() == 8 { byte(6)? } else { 255 },
        })
    }
}

impl Serialize for ThemeColor {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for ThemeColor {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        text.parse().map_err(serde::de::Error::custom)
    }
}

/// The appearance that a theme is a variant of.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ThemeVariant {
    /// The light appearance's.
    Light,
    /// The dark appearance's.
    #[default]
    Dark,
}

/// The terminal's colors of a theme: its background, its text and its 16
/// colors, which every theme gives, and those that it can give.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct TerminalColors {
    /// The default background.
    pub background: ThemeColor,
    /// The default text's color.
    pub foreground: ThemeColor,
    /// The 16 colors, black to bright white.
    pub palette: [ThemeColor; 16],
    /// The cursor's color; the text's without.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<ThemeColor>,
    /// The color of the text under a block cursor; the background's without.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor_text: Option<ThemeColor>,
    /// The selection's background; derived without.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selection: Option<ThemeColor>,
    /// The selected text's color; the text's own without.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selection_text: Option<ThemeColor>,
    /// Bold text's color; its own without.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bold: Option<ThemeColor>,
    /// A link's color.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub link: Option<ThemeColor>,
    /// An underline's color; the text's without.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub underline: Option<ThemeColor>,
}

/// A theme: its name, its variant, its terminal's colors, the search's
/// highlights, and the interface's tokens that it gives, by their names,
/// as `panel.bg` or `tabs.active.bg`, of the host's or of a plugin's
/// section.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Theme {
    /// Its name, as the config and the picker name it.
    pub name: String,
    /// The appearance that it is a variant of.
    #[serde(default)]
    pub variant: ThemeVariant,
    /// Its terminal's colors.
    pub terminal: TerminalColors,
    /// The tokens that it gives, `search.match` and `search.match.current`
    /// among them; the host derives the others.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub tokens: BTreeMap<String, ThemeColor>,
}

impl Theme {
    /// A theme of `name` of the terminal's colors alone.
    pub fn of_terminal(name: &str, variant: ThemeVariant, terminal: TerminalColors) -> Self {
        Self {
            name: name.to_owned(),
            variant,
            terminal,
            tokens: BTreeMap::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_color_reads_and_writes_as_its_hex_of_its_opacity() {
        let color: ThemeColor = "#ffd60a4d".parse().unwrap();
        assert_eq!(color, ThemeColor::hex(0xffd60a).with_alpha(0x4d));
        assert_eq!(color.to_string(), "#ffd60a4d");
        assert_eq!(
            "#1E1E20".parse::<ThemeColor>().unwrap().to_string(),
            "#1e1e20"
        );
        for wrong in ["1e1e20", "#1e1e2", "#1e1e20f", "#gggggg", ""] {
            assert!(wrong.parse::<ThemeColor>().is_err(), "{wrong}");
        }
    }

    #[test]
    fn a_theme_reads_from_json_of_its_terminal_alone_and_back() {
        let palette: Vec<String> = (0..16).map(|n| format!("#0000{n:02x}")).collect();
        let json = serde_json::json!({
            "name": "Nightfall",
            "variant": "light",
            "terminal": {"background": "#fafafa", "foreground": "#1e1e20", "palette": palette},
            "tokens": {"tabs.active.bg": "#ffffff"},
        });
        let theme: Theme = serde_json::from_value(json.clone()).unwrap();
        assert_eq!(theme.variant, ThemeVariant::Light);
        assert_eq!(theme.terminal.palette[15], ThemeColor::hex(0x0f));
        assert_eq!(theme.terminal.cursor, None);
        assert_eq!(serde_json::to_value(&theme).unwrap(), json);
    }
}
