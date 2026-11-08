//! The built-in theme: its colors in a light and a dark variant, which follow
//! the system's appearance: the terminal's, and the interface's, by their
//! tokens.

use std::collections::{HashMap, HashSet};

use lokyr_plugin_sdk::theme::{TerminalColors, Theme as Spec, ThemeColor, ThemeVariant};
use lokyr_render_core::Color;
use lokyr_vt_core::page::Rgb;
use lokyr_vt_core::terminal::Colors;

/// The colors of a selection.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SelectionColors {
    /// The background of the cells selected.
    pub background: Color,
    /// The text's color in them; `None` keeps the text's own.
    pub foreground: Option<Color>,
}

/// The colors of a search's matches over the terminal's cells, translucent,
/// over the cells' own: a match's, and the current one's.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MatchColors {
    /// Over a match's cells.
    pub matched: Color,
    /// Over the current match's cells.
    pub current: Color,
}

/// A variant of the theme: the terminal's colors, the selection's, and the
/// interface's.
#[derive(Clone, Debug, PartialEq)]
pub struct Variant {
    /// The palette, the default colors and the cursor's, which a terminal
    /// takes as its own.
    pub colors: Colors,
    /// The selection's colors.
    pub selection: SelectionColors,
    /// The interface's colors, by their tokens.
    pub tokens: Tokens,
}

/// The colors of the interface, by their tokens' names, as `tabs.active.bg`,
/// and the backgrounds among them that take the place of what is under
/// them, as a translucent window's chrome's do.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Tokens(HashMap<String, Color>, HashSet<String>);

/// The token of no color, whatever a theme gives, as a progress's track
/// that shows nothing.
pub const TRANSPARENT: &str = "transparent";

impl Variant {
    /// The colors of a search's matches, of the tokens `search.match` and
    /// `search.match.current`.
    pub fn match_colors(&self) -> MatchColors {
        let matched = self.tokens.get("search.match").unwrap_or(Color::CLEAR);
        MatchColors {
            matched,
            current: self.tokens.get("search.match.current").unwrap_or(matched),
        }
    }
}

impl Tokens {
    /// The color of `token`; of the token without its last part where it
    /// has none, `tabs.active` for `tabs.active.bg`, then `tabs`; `None` when
    /// none of them has one. [`TRANSPARENT`] is clear in every theme, as
    /// the tokens that fall back to it are.
    pub fn get(&self, token: &str) -> Option<Color> {
        // A color of its own, as a tab's that its program set.
        if let Some(hex) = token.strip_prefix('#') {
            return hex_color(hex);
        }
        let mut name = token;
        loop {
            if name == TRANSPARENT {
                return Some(Color::CLEAR);
            }
            if let Some(&color) = self.0.get(name) {
                return Some(color);
            }
            name = &name[..name.rfind('.')?];
        }
    }

    /// Gives `token` its color.
    pub fn set(&mut self, token: &str, color: Color) {
        self.0.insert(token.to_owned(), color);
    }

    /// Has the backgrounds of `token` take the place of what is under them,
    /// by the part of each pixel that they cover, rather than go over it,
    /// as a translucent window's chrome shows the window's opacity over the
    /// window's own background, which the frame draws.
    pub fn knock_out(&mut self, token: &str) {
        self.1.insert(token.to_owned());
    }

    /// Whether the backgrounds of `token` take the place of what is under
    /// them, as [`Tokens::knock_out`] has them.
    pub fn knocks_out(&self, token: &str) -> bool {
        self.1.contains(token)
    }
}

/// A token's color: `#rrggbb` and its opacity.
type TokenColor = (&'static str, u32, f32);

/// The interface's colors in the light variant: the system's grays, the
/// text the terminal's.
const LIGHT_TOKENS: &[TokenColor] = &[
    ("text", 0x1e1e20, 1.0),
    ("text.secondary", 0x6c6c72, 1.0),
    ("text.disabled", 0xa8a8ae, 1.0),
    ("accent", 0x0a64d8, 1.0),
    ("separator", 0x000000, 0.1),
    ("pane.divider", 0x000000, 0.16),
    ("pane.drop", 0x0a64d8, 0.25),
    ("shadow", 0x000000, 0.25),
    ("panel.bg", 0xf6f6f6, 1.0),
    ("panel.border", 0x000000, 0.14),
    ("field.bg", 0xffffff, 1.0),
    ("field.border", 0x000000, 0.18),
    ("field.focus", 0x0a64d8, 0.5),
    ("field.selection", 0xb3d7ff, 1.0),
    ("focus.ring", 0x0a64d8, 0.5),
    ("button.bg", 0x000000, 0.06),
    ("button.hover.bg", 0x000000, 0.1),
    ("button.pressed.bg", 0x000000, 0.16),
    ("toggle.on.bg", 0x0a64d8, 0.12),
    ("toggle.on.fg", 0x0a5cc6, 1.0),
    ("tooltip.bg", 0xfbfbfb, 1.0),
    ("tooltip.fg", 0x1e1e20, 1.0),
    ("tooltip.border", 0x000000, 0.15),
    ("list.selection.bg", 0x0a64d8, 1.0),
    ("list.selection.fg", 0xffffff, 1.0),
    ("list.selection.inactive.bg", 0x000000, 0.1),
    ("scrollbar", 0x000000, 0.3),
    ("scrollbar.hover", 0x000000, 0.5),
    ("scrollbar.track", 0x000000, 0.05),
    ("scrollbar.mark", 0xd9a300, 1.0),
    ("scrollbar.mark.current", 0xff7a00, 1.0),
    ("progress.track", 0x000000, 0.1),
    ("progress.normal", 0x0a64d8, 1.0),
    ("progress.error", 0xd63b3b, 1.0),
    ("progress.paused", 0xd99a00, 1.0),
    ("tabs.bar.bg", 0xf4f4f6, 1.0),
    ("tabs.track.bg", 0x000000, 0.07),
    ("tabs.active.bg", 0xffffff, 1.0),
    ("tabs.active.fg", 0x1e1e20, 1.0),
    ("tabs.active.border", 0x000000, 0.1),
    ("tabs.active.shadow", 0x000000, 0.12),
    ("tabs.inactive.bg", 0x000000, 0.0),
    ("tabs.inactive.fg", 0x5e5e64, 1.0),
    ("tabs.hover.bg", 0x000000, 0.05),
    ("tabs.seamless.line", 0x000000, 0.12),
    ("tabs.seamless.veil", 0x000000, 0.06),
    ("segments.bg", 0x000000, 0.06),
    ("segments.chosen.bg", 0x0a64d8, 1.0),
    ("segments.chosen.fg", 0xffffff, 1.0),
    ("segments.box.bg", 0x000000, 0.03),
    ("segments.box.border", 0x000000, 0.1),
    ("search.match", 0xffd60a, 0.5),
    ("search.match.current", 0xff7a00, 0.6),
    ("gutter.running", 0x0a64d8, 1.0),
    ("gutter.succeeded", 0x000000, 0.22),
    ("gutter.failed", 0xd63b3b, 1.0),
];

/// The interface's colors in the dark variant.
const DARK_TOKENS: &[TokenColor] = &[
    ("text", 0xe6e6e6, 1.0),
    ("text.secondary", 0x9a9aa0, 1.0),
    ("text.disabled", 0x5e5e64, 1.0),
    ("accent", 0x4c9bff, 1.0),
    ("separator", 0xffffff, 0.1),
    ("pane.divider", 0xffffff, 0.16),
    ("pane.drop", 0x4c9bff, 0.3),
    ("shadow", 0x000000, 0.5),
    ("panel.bg", 0x2a2a2d, 1.0),
    ("panel.border", 0xffffff, 0.12),
    ("field.bg", 0x1e1e20, 1.0),
    ("field.border", 0xffffff, 0.16),
    ("field.focus", 0x4c9bff, 0.6),
    ("field.selection", 0x3f638b, 1.0),
    ("focus.ring", 0x4c9bff, 0.6),
    ("button.bg", 0xffffff, 0.08),
    ("button.hover.bg", 0xffffff, 0.12),
    ("button.pressed.bg", 0xffffff, 0.18),
    ("toggle.on.bg", 0x4c9bff, 0.2),
    ("toggle.on.fg", 0x7ab6ff, 1.0),
    ("tooltip.bg", 0x323235, 1.0),
    ("tooltip.fg", 0xe6e6e6, 1.0),
    ("tooltip.border", 0xffffff, 0.15),
    ("list.selection.bg", 0x2462c4, 1.0),
    ("list.selection.fg", 0xffffff, 1.0),
    ("list.selection.inactive.bg", 0xffffff, 0.12),
    ("scrollbar", 0xffffff, 0.35),
    ("scrollbar.hover", 0xffffff, 0.55),
    ("scrollbar.track", 0xffffff, 0.07),
    ("scrollbar.mark", 0xffd60a, 1.0),
    ("scrollbar.mark.current", 0xff9f0a, 1.0),
    ("progress.track", 0xffffff, 0.14),
    ("progress.normal", 0x4c9bff, 1.0),
    ("progress.error", 0xff5f57, 1.0),
    ("progress.paused", 0xffbd2e, 1.0),
    ("tabs.bar.bg", 0x232326, 1.0),
    ("tabs.track.bg", 0x000000, 0.3),
    ("tabs.active.bg", 0x404044, 1.0),
    ("tabs.active.fg", 0xf4f4f6, 1.0),
    ("tabs.active.border", 0xffffff, 0.12),
    ("tabs.active.shadow", 0x000000, 0.4),
    ("tabs.inactive.bg", 0x000000, 0.0),
    ("tabs.inactive.fg", 0x9a9aa0, 1.0),
    ("tabs.hover.bg", 0xffffff, 0.06),
    ("tabs.seamless.line", 0xffffff, 0.12),
    ("tabs.seamless.veil", 0x000000, 0.27),
    ("segments.bg", 0xffffff, 0.08),
    ("segments.chosen.bg", 0x2462c4, 1.0),
    ("segments.chosen.fg", 0xffffff, 1.0),
    ("segments.box.bg", 0xffffff, 0.03),
    ("segments.box.border", 0xffffff, 0.1),
    ("search.match", 0xffd60a, 0.3),
    ("search.match.current", 0xff7a00, 0.5),
    ("gutter.running", 0x4c9bff, 1.0),
    ("gutter.succeeded", 0xffffff, 0.25),
    ("gutter.failed", 0xff5f57, 1.0),
];

/// The built-in theme, of a light and a dark variant, and of the man
/// pages'.
#[derive(Clone, Debug, PartialEq)]
pub struct Theme {
    /// The variant of the light appearance.
    pub light: Variant,
    /// The variant of the dark appearance.
    pub dark: Variant,
    /// The variant of a window that shows a man page, whatever the
    /// appearance.
    pub man_page: Variant,
}

impl Default for Theme {
    fn default() -> Self {
        Self::builtin()
    }
}

/// The sixteen colors of a variant, from black to bright white.
type Sixteen = [u32; 16];

/// The light variant's sixteen colors, darker than its background, and its
/// text, background and selection.
const LIGHT: (Sixteen, u32, u32, u32) = (
    [
        0x1e1e20, 0xc4262e, 0x1d7f3c, 0x9a6500, 0x1f5fcc, 0x9b3cb5, 0x137f8b, 0x7a7a82, //
        0x5a5a62, 0xe0444b, 0x2a9a4c, 0xb87d00, 0x3a7be6, 0xb557cf, 0x1a9cab, 0x3a3a40,
    ],
    0x1e1e20,
    0xfafafa,
    0xb8d4ff,
);

/// The dark variant's sixteen colors, lighter than its background, and its
/// text, background and selection.
const DARK: (Sixteen, u32, u32, u32) = (
    [
        0x45454b, 0xf25d62, 0x58c672, 0xe3bd4c, 0x5aa2f7, 0xc983de, 0x4cc4cf, 0xc4c4ca, //
        0x75757d, 0xff8488, 0x7edb94, 0xf5d47a, 0x86bcff, 0xdca3ec, 0x7adce4, 0xf4f4f6,
    ],
    0xe6e6e6,
    0x1e1e20,
    0x34445f,
);

/// A man page's text, background and selection, as the system's terminal
/// shows a page: black on light yellow.
const MAN_PAGE: (u32, u32, u32) = (0x000000, 0xfef49c, 0xbfb875);

/// The name of the built-in theme of the light appearance.
pub const LIGHT_NAME: &str = "Lokyr Light";

/// The name of the built-in theme of the dark appearance.
pub const DARK_NAME: &str = "Lokyr Dark";

impl Theme {
    /// The built-in theme: a palette of our own, whose colors read on their
    /// variant's background; a man page's takes the light one's.
    pub fn builtin() -> Self {
        Self::of(&light_spec(), &dark_spec())
    }

    /// The theme of the variants of `light` and `dark`, a man page's the
    /// built-in one.
    pub fn of(light: &Spec, dark: &Spec) -> Self {
        let (foreground, background, selection) = MAN_PAGE;
        let man_page = spec(
            "man page",
            ThemeVariant::Light,
            (LIGHT.0, foreground, background, selection),
            LIGHT_TOKENS,
        );
        Self {
            light: Variant::of(light),
            dark: Variant::of(dark),
            man_page: Variant::of(&man_page),
        }
    }

    /// The variant of the dark appearance or of the light one.
    pub fn variant(&self, dark: bool) -> &Variant {
        if dark { &self.dark } else { &self.light }
    }
}

/// The built-in theme of the light appearance, every token its own.
pub fn light_spec() -> Spec {
    spec(LIGHT_NAME, ThemeVariant::Light, LIGHT, LIGHT_TOKENS)
}

/// The built-in theme of the dark appearance, every token its own.
pub fn dark_spec() -> Spec {
    spec(DARK_NAME, ThemeVariant::Dark, DARK, DARK_TOKENS)
}

/// The theme of `name` of the colors of `sixteen`, `foreground`,
/// `background` and `selection`, and of every token of `tokens`.
fn spec(
    name: &str,
    variant: ThemeVariant,
    (sixteen, foreground, background, selection): (Sixteen, u32, u32, u32),
    tokens: &[TokenColor],
) -> Spec {
    let mut theme = Spec::of_terminal(
        name,
        variant,
        TerminalColors {
            background: ThemeColor::hex(background),
            foreground: ThemeColor::hex(foreground),
            palette: sixteen.map(ThemeColor::hex),
            cursor: None,
            cursor_text: None,
            selection: Some(ThemeColor::hex(selection)),
            selection_text: None,
            bold: None,
            link: None,
            underline: None,
        },
    );
    theme.tokens = tokens
        .iter()
        .map(|&(name, value, alpha)| {
            let alpha = (alpha * 255.0).round() as u8;
            (name.to_owned(), ThemeColor::hex(value).with_alpha(alpha))
        })
        .collect();
    theme
}

impl Variant {
    /// The variant of `theme`: its terminal's colors, the selection's, and
    /// the interface's tokens, those that it gives over those that its
    /// terminal's colors derive, so that a theme of the terminal's colors
    /// alone colors the whole interface.
    pub fn of(theme: &Spec) -> Self {
        let terminal = &theme.terminal;
        let mut colors = Colors::default();
        for (entry, &color) in colors.palette.iter_mut().zip(&terminal.palette) {
            *entry = rgb_of(color);
        }
        colors.foreground = rgb_of(terminal.foreground);
        colors.background = rgb_of(terminal.background);
        colors.cursor = rgb_of(terminal.cursor.unwrap_or(terminal.foreground));
        // xterm's special colors of bold and of underlined text.
        if let Some(bold) = terminal.bold {
            colors.special[0] = rgb_of(bold);
        }
        if let Some(underline) = terminal.underline {
            colors.special[1] = rgb_of(underline);
        }
        let mut tokens = derived(terminal);
        for (name, &value) in &theme.tokens {
            tokens.set(name, color_of(value));
        }
        let selection = SelectionColors {
            background: terminal.selection.map_or_else(
                || tokens.get("field.selection").unwrap_or(Color::CLEAR),
                color_of,
            ),
            foreground: terminal.selection_text.map(color_of),
        };
        Variant {
            colors,
            selection,
            tokens,
        }
    }
}

/// The interface's tokens that the terminal's colors of `terminal` derive,
/// as iTerm2 derives a tab bar of its colors: the panels, the fields and
/// the bars a little toward the text from the background, the text its own
/// and fainter, the accent the palette's blue, the search's highlights its
/// yellow and orange, each of the opacities of the built-in theme of its
/// appearance, dark as the background is darker than the text.
pub fn derived(terminal: &TerminalColors) -> Tokens {
    let (bg, fg) = (color_of(terminal.background), color_of(terminal.foreground));
    let dark = luminance(bg) < luminance(fg);
    let palette = terminal.palette.map(color_of);
    let pick = |light: f32, dark_alpha: f32| if dark { dark_alpha } else { light };
    // The palette's blue where it reads on the background, else its bright
    // one, else the text's color.
    let accent = [palette[4], palette[12]]
        .into_iter()
        .find(|&blue| contrast(blue, bg) >= 3.0)
        .unwrap_or(fg);
    // The text on the accent, white or black, whichever reads better.
    let on_accent = if contrast(Color::WHITE, accent) >= contrast(Color::BLACK, accent) {
        Color::WHITE
    } else {
        Color::BLACK
    };
    let shadow = Color::BLACK;
    let toward = |share: f32| mix(bg, fg, share);
    let veil = |alpha: f32| fg.with_alpha(alpha);
    let orange = mix(palette[3], palette[1], 0.5);
    let (panel, bar) = (toward(pick(0.04, 0.06)), toward(0.06));
    // A fainter text, mixed toward the background by `share`, but of three
    // quarters of the theme's own contrast at least, 4.5 at most, over
    // `under`, so that a theme of a low contrast keeps it readable.
    let floor = (contrast(fg, bg) * 0.75).min(4.5);
    let fainter = |share: f32, under: Color| with_contrast(mix(fg, bg, share), under, floor);
    let entries: Vec<(&str, Color)> = vec![
        ("text", fg),
        ("text.secondary", fainter(0.38, panel)),
        ("text.disabled", mix(fg, bg, 0.62)),
        ("accent", accent),
        ("separator", veil(0.1)),
        ("pane.divider", veil(0.16)),
        ("pane.drop", accent.with_alpha(pick(0.25, 0.3))),
        ("shadow", shadow.with_alpha(pick(0.25, 0.5))),
        ("panel.bg", panel),
        ("panel.border", veil(pick(0.14, 0.12))),
        ("field.bg", bg),
        ("field.border", veil(pick(0.18, 0.16))),
        ("field.focus", accent.with_alpha(pick(0.5, 0.6))),
        ("field.selection", mix(bg, accent, pick(0.3, 0.4))),
        ("focus.ring", accent.with_alpha(pick(0.5, 0.6))),
        ("button.bg", veil(pick(0.06, 0.08))),
        ("button.hover.bg", veil(pick(0.1, 0.12))),
        ("button.pressed.bg", veil(pick(0.16, 0.18))),
        ("toggle.on.bg", accent.with_alpha(pick(0.12, 0.2))),
        ("toggle.on.fg", accent),
        ("tooltip.bg", toward(pick(0.03, 0.1))),
        ("tooltip.fg", fg),
        ("tooltip.border", veil(0.15)),
        ("list.selection.bg", accent),
        ("list.selection.fg", on_accent),
        ("list.selection.inactive.bg", veil(pick(0.1, 0.12))),
        ("scrollbar", veil(pick(0.3, 0.35))),
        ("scrollbar.hover", veil(pick(0.5, 0.55))),
        ("scrollbar.track", veil(pick(0.05, 0.07))),
        ("scrollbar.mark", palette[3]),
        ("scrollbar.mark.current", orange),
        ("progress.track", veil(pick(0.1, 0.14))),
        ("progress.normal", accent),
        ("progress.error", palette[1]),
        ("progress.paused", palette[3]),
        ("tabs.bar.bg", bar),
        ("tabs.track.bg", veil(pick(0.07, 0.1))),
        ("tabs.active.bg", bg),
        ("tabs.active.fg", fg),
        ("tabs.active.border", veil(pick(0.1, 0.12))),
        ("tabs.active.shadow", shadow.with_alpha(pick(0.12, 0.4))),
        ("tabs.inactive.bg", Color::CLEAR),
        ("tabs.inactive.fg", fainter(0.4, bar)),
        ("tabs.hover.bg", veil(pick(0.05, 0.06))),
        ("tabs.seamless.line", veil(0.12)),
        ("tabs.seamless.veil", shadow.with_alpha(pick(0.06, 0.27))),
        ("segments.bg", veil(pick(0.06, 0.08))),
        ("segments.chosen.bg", accent),
        ("segments.chosen.fg", on_accent),
        ("segments.box.bg", veil(0.03)),
        ("segments.box.border", veil(0.1)),
        ("search.match", palette[3].with_alpha(pick(0.5, 0.3))),
        ("search.match.current", orange.with_alpha(pick(0.6, 0.5))),
        ("gutter.running", accent),
        ("gutter.succeeded", veil(pick(0.22, 0.25))),
        ("gutter.failed", palette[1]),
        // The terminal's own, as a bar of its colors takes them.
        ("terminal.bg", bg),
        ("terminal.fg", fg),
    ];
    Tokens(
        entries
            .into_iter()
            .map(|(name, color)| (name.to_owned(), color))
            .collect(),
        HashSet::new(),
    )
}

/// `from` mixed toward `to` by `share`, from 0, `from`, to 1, `to`, of
/// their encoded components, opaque.
fn mix(from: Color, to: Color, share: f32) -> Color {
    let lerp = |a: f32, b: f32| a + (b - a) * share;
    Color {
        red: lerp(from.red, to.red),
        green: lerp(from.green, to.green),
        blue: lerp(from.blue, to.blue),
        alpha: 1.0,
    }
}

/// The relative luminance of `color`, as WCAG 2 defines it, of its
/// encoded sRGB components.
pub fn luminance(color: Color) -> f32 {
    let linear = |value: f32| {
        if value <= 0.040_45 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * linear(color.red) + 0.7152 * linear(color.green) + 0.0722 * linear(color.blue)
}

/// The contrast ratio of `a` and `b`, as WCAG 2 defines it, from 1 to 21.
pub fn contrast(a: Color, b: Color) -> f32 {
    let (a, b) = (luminance(a), luminance(b));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

/// `text`, of a contrast with `background` of `ratio` at least: as it is
/// where it has it, or moved toward white or black, whichever reads
/// better on the background, as little as gives it, its opacity kept.
pub fn with_contrast(text: Color, background: Color, ratio: f32) -> Color {
    if ratio <= 1.0 || contrast(text, background) >= ratio {
        return text;
    }
    let extreme = if contrast(Color::WHITE, background) >= contrast(Color::BLACK, background) {
        Color::WHITE
    } else {
        Color::BLACK
    };
    // The least share toward the extreme that gives the ratio, or the
    // extreme itself where none does, of a binary search.
    let (mut low, mut high) = (0.0_f32, 1.0_f32);
    for _ in 0..12 {
        let middle = (low + high) / 2.0;
        if contrast(mix(text, extreme, middle), background) >= ratio {
            high = middle;
        } else {
            low = middle;
        }
    }
    mix(text, extreme, high).with_alpha(text.alpha)
}

/// The color of a theme's `color`.
fn color_of(color: ThemeColor) -> Color {
    Color::from_rgb8(color.red, color.green, color.blue).with_alpha(f32::from(color.alpha) / 255.0)
}

/// The terminal's color of a theme's `color`, of no opacity.
fn rgb_of(color: ThemeColor) -> Rgb {
    Rgb {
        r: color.red,
        g: color.green,
        b: color.blue,
    }
}

/// The color of `hex`, `rrggbb` or `rrggbbaa`, if it writes one.
fn hex_color(hex: &str) -> Option<Color> {
    if !matches!(hex.len(), 6 | 8) || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let byte = |at: usize| u8::from_str_radix(hex.get(at..at + 2)?, 16).ok();
    let color = Color::from_rgb8(byte(0)?, byte(2)?, byte(4)?);
    Some(match hex.len() {
        8 => color.with_alpha(f32::from(byte(6)?) / 255.0),
        _ => color,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The relative luminance of `rgb`, as WCAG 2 defines it.
    fn luminance(rgb: Rgb) -> f64 {
        let linear = |channel: u8| {
            let value = f64::from(channel) / 255.0;
            if value <= 0.040_45 {
                value / 12.92
            } else {
                ((value + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * linear(rgb.r) + 0.7152 * linear(rgb.g) + 0.0722 * linear(rgb.b)
    }

    /// The contrast ratio of two colors, as WCAG 2 defines it: from 1 to 21.
    fn contrast(a: Rgb, b: Rgb) -> f64 {
        let (a, b) = (luminance(a), luminance(b));
        (a.max(b) + 0.05) / (a.min(b) + 0.05)
    }

    #[test]
    fn a_token_of_hex_digits_is_its_color_and_the_terminal_s_are_tokens() {
        let tokens = &Theme::builtin().dark.tokens;
        assert_eq!(tokens.get("#ff8800"), Some(Color::from_rgb8(255, 136, 0)));
        let translucent = tokens.get("#ff880080").unwrap();
        assert!((translucent.alpha - 128.0 / 255.0).abs() < 1e-6);
        for wrong in ["#ff88", "#gg8800", "#ff88000"] {
            assert_eq!(tokens.get(wrong), None, "{wrong}");
        }
        let colors = &Theme::builtin().dark.colors;
        let background = colors.background;
        assert_eq!(
            tokens.get("terminal.bg"),
            Some(Color::from_rgb8(background.r, background.g, background.b))
        );
        assert!(tokens.get("terminal.fg").is_some());
    }

    #[test]
    fn the_text_reads_on_its_background() {
        let theme = Theme::builtin();
        for variant in [theme.light, theme.dark, theme.man_page] {
            let colors = &variant.colors;
            // WCAG's AAA, for the text of a terminal.
            assert!(contrast(colors.foreground, colors.background) > 12.0);
            assert_eq!(colors.cursor, colors.foreground);
        }
    }

    #[test]
    fn the_sixteen_colors_read_on_their_background() {
        let theme = Theme::builtin();
        for (name, variant) in [("light", &theme.light), ("dark", &theme.dark)] {
            let background = variant.colors.background;
            // Red, green, yellow, blue, magenta and cyan: WCAG's AA for
            // text, 4.5. Their bright ones too, but on the light background,
            // where they are lighter than the others to stay bright, and
            // take WCAG's AA for large text, 3.
            for index in (1..7).chain(9..15) {
                let ratio = contrast(variant.colors.palette[index], background);
                let floor = if name == "light" && index > 8 {
                    3.0
                } else {
                    4.5
                };
                assert!(ratio >= floor, "{name} {index}: {ratio:.2}");
            }
            // Black reads apart from the dark background, and white from
            // the light one, as the two variants' grays.
            assert!(
                contrast(variant.colors.palette[0], background) > 1.4,
                "{name} 0"
            );
            assert!(
                contrast(variant.colors.palette[8], background) > 2.5,
                "{name} 8"
            );
        }
    }

    /// The color of `top` over the opaque `under`, as the drawable blends
    /// them, of their encoded components.
    fn over(top: Color, under: Rgb) -> Rgb {
        let alpha = top.alpha;
        let mix = |top: f32, under: u8| {
            (top * 255.0 * alpha + f32::from(under) * (1.0 - alpha)).round() as u8
        };
        Rgb {
            r: mix(top.red, under.r),
            g: mix(top.green, under.g),
            b: mix(top.blue, under.b),
        }
    }

    fn opaque(color: Color) -> Rgb {
        over(color, Rgb { r: 0, g: 0, b: 0 })
    }

    #[test]
    fn the_interface_s_text_reads_on_its_backgrounds() {
        let theme = Theme::builtin();
        for (name, variant) in [("light", &theme.light), ("dark", &theme.dark)] {
            let token = |token: &str| variant.tokens.get(token).unwrap();
            for (text, background) in [
                ("text", "panel.bg"),
                ("text", "field.bg"),
                ("text.secondary", "panel.bg"),
                ("tabs.active.fg", "tabs.active.bg"),
                ("tabs.inactive.fg", "tabs.bar.bg"),
                ("tabs.inactive.fg", "terminal.bg"),
                ("accent", "panel.bg"),
                ("tooltip.fg", "tooltip.bg"),
                ("list.selection.fg", "list.selection.bg"),
                ("segments.chosen.fg", "segments.chosen.bg"),
            ] {
                let under = opaque(token(background));
                let ratio = contrast(opaque(token(text)), under);
                // WCAG's AA for text.
                assert!(ratio >= 4.5, "{name}: {text} on {background}: {ratio:.2}");
            }
            // A tab's text on the track, over the bar, and on the veil,
            // over the terminal's background.
            let track = over(token("tabs.track.bg"), opaque(token("tabs.bar.bg")));
            let veiled = over(token("tabs.seamless.veil"), opaque(token("terminal.bg")));
            for (under, shown) in [(track, "the track"), (veiled, "the veil")] {
                let ratio = contrast(opaque(token("tabs.inactive.fg")), under);
                assert!(
                    ratio >= 4.5,
                    "{name}: tab.inactive.fg on {shown}: {ratio:.2}"
                );
            }
            // A match shows on the terminal's background, of its hue as
            // much as of its lightness, the current one apart from the
            // others, and the text reads on both.
            let background = variant.colors.background;
            let matched = over(token("search.match"), background);
            let current = over(token("search.match.current"), background);
            let distance = |a: Rgb, b: Rgb| {
                let d = |a: u8, b: u8| f64::from(a) - f64::from(b);
                (d(a.r, b.r).powi(2) + d(a.g, b.g).powi(2) + d(a.b, b.b).powi(2)).sqrt()
            };
            assert!(distance(matched, background) > 60.0, "{name}");
            assert!(distance(current, matched) > 40.0, "{name}");
            for shown in [matched, current] {
                let ratio = contrast(variant.colors.foreground, shown);
                assert!(ratio > 4.5, "{name}: {ratio:.2}");
            }
            // A toggle that is on reads on its tint over a panel, and on
            // the panel when the pointer is over it, a button's hover.
            let panel = opaque(token("panel.bg"));
            for under in [token("toggle.on.bg"), token("button.hover.bg")] {
                let ratio = contrast(opaque(token("toggle.on.fg")), over(under, panel));
                assert!(ratio >= 4.5, "{name}: toggle.on.fg: {ratio:.2}");
            }
            // The active tab stands out of the track that it is raised
            // over, lighter.
            let pill = opaque(token("tabs.active.bg"));
            assert!(contrast(pill, track) > 1.2, "{name}: {pill:?} on {track:?}");
        }
    }

    /// Dracula's and Solarized Light's terminal colors, as their
    /// `.itermcolors` give them, and the built-in themes' of their tokens
    /// left out: themes of the terminal's colors alone.
    fn terminal_themes() -> Vec<Spec> {
        let of = |name: &str, variant, background, foreground, palette: [u32; 16]| {
            Spec::of_terminal(
                name,
                variant,
                TerminalColors {
                    background: ThemeColor::hex(background),
                    foreground: ThemeColor::hex(foreground),
                    palette: palette.map(ThemeColor::hex),
                    cursor: None,
                    cursor_text: None,
                    selection: None,
                    selection_text: None,
                    bold: None,
                    link: None,
                    underline: None,
                },
            )
        };
        let mut bare = vec![light_spec(), dark_spec()];
        for theme in &mut bare {
            theme.tokens.clear();
        }
        bare.push(of(
            "Dracula",
            ThemeVariant::Dark,
            0x282a36,
            0xf8f8f2,
            [
                0x21222c, 0xff5555, 0x50fa7b, 0xf1fa8c, 0xbd93f9, 0xff79c6, 0x8be9fd, 0xf8f8f2,
                0x6272a4, 0xff6e6e, 0x69ff94, 0xffffa5, 0xd6acff, 0xff92df, 0xa4ffff, 0xffffff,
            ],
        ));
        bare.push(of(
            "Solarized Light",
            ThemeVariant::Light,
            0xfdf6e3,
            0x657b83,
            [
                0x073642, 0xdc322f, 0x859900, 0xb58900, 0x268bd2, 0xd33682, 0x2aa198, 0xeee8d5,
                0x002b36, 0xcb4b16, 0x586e75, 0x657b83, 0x839496, 0x6c71c4, 0x93a1a1, 0xfdf6e3,
            ],
        ));
        bare
    }

    #[test]
    fn every_token_derives_of_a_theme_of_the_terminal_s_colors_alone() {
        let names: Vec<&str> = LIGHT_TOKENS.iter().map(|&(name, _, _)| name).collect();
        assert_eq!(
            names,
            DARK_TOKENS
                .iter()
                .map(|&(name, _, _)| name)
                .collect::<Vec<_>>()
        );
        for theme in terminal_themes() {
            let variant = Variant::of(&theme);
            for name in &names {
                assert!(
                    variant.tokens.0.contains_key(*name),
                    "{}: {name}",
                    theme.name
                );
            }
            // None but the built-in ones, and the terminal's own.
            let extra: Vec<&String> = variant
                .tokens
                .0
                .keys()
                .filter(|name| !names.contains(&name.as_str()) && !name.starts_with("terminal."))
                .collect();
            assert!(extra.is_empty(), "{}: {extra:?}", theme.name);
            let selection = variant.selection.background;
            assert_ne!(selection.alpha, 0.0, "{}", theme.name);
        }
    }

    #[test]
    fn the_derived_interface_reads_on_its_backgrounds() {
        for theme in terminal_themes() {
            let variant = Variant::of(&theme);
            let token = |name: &str| variant.tokens.get(name).unwrap();
            let terminal = variant.colors.background;
            let panel = opaque(token("panel.bg"));
            let name = &theme.name;
            // The panel keeps the theme's own contrast of its text, which a
            // theme as Solarized's makes lower than 4.5.
            let own = contrast(variant.colors.foreground, terminal);
            let ratio = contrast(opaque(token("text")), panel);
            assert!(
                ratio >= (own * 0.9).min(4.5),
                "{name}: text on its panel: {ratio:.2} of {own:.2}"
            );
            let ratio = contrast(opaque(token("text.secondary")), panel);
            assert!(
                ratio >= (own * 0.5).min(3.0),
                "{name}: secondary text on its panel: {ratio:.2}"
            );
            let bar = opaque(token("tabs.bar.bg"));
            let ratio = contrast(opaque(token("tabs.inactive.fg")), bar);
            assert!(
                ratio >= (own * 0.7).min(3.0),
                "{name}: an inactive tab on the bar: {ratio:.2}"
            );
            let ratio = contrast(opaque(token("list.selection.fg")), opaque(token("accent")));
            assert!(ratio >= 3.0, "{name}: a list's selection: {ratio:.2}");
            // A match shows on the terminal's background.
            let matched = over(token("search.match"), terminal);
            assert_ne!(matched, terminal, "{name}");
        }
    }

    #[test]
    fn the_default_theme_is_f2_s_palette_and_tokens() {
        let theme = Theme::builtin();
        for (variant, (sixteen, foreground, background, selection), tokens) in [
            (&theme.light, LIGHT, LIGHT_TOKENS),
            (&theme.dark, DARK, DARK_TOKENS),
        ] {
            let rgb = |hex: u32| rgb_of(ThemeColor::hex(hex));
            assert_eq!(variant.colors.palette[..16], sixteen.map(rgb));
            assert_eq!(variant.colors.foreground, rgb(foreground));
            assert_eq!(variant.colors.background, rgb(background));
            assert_eq!(variant.colors.cursor, rgb(foreground));
            assert_eq!(opaque(variant.selection.background), rgb(selection));
            for &(name, value, alpha) in tokens {
                let got = variant.tokens.get(name).unwrap();
                let channels = |c: Color| {
                    let byte = |v: f32| (v * 255.0).round() as u8;
                    Rgb {
                        r: byte(c.red),
                        g: byte(c.green),
                        b: byte(c.blue),
                    }
                };
                assert_eq!(channels(got), rgb(value), "{name}");
                assert!(
                    (got.alpha - alpha).abs() <= 0.5 / 255.0 + 1e-6,
                    "{name}: {}",
                    got.alpha
                );
            }
        }
        assert_eq!(
            theme.man_page.colors.background,
            rgb_of(ThemeColor::hex(MAN_PAGE.1))
        );
    }

    #[test]
    fn a_text_s_contrast_rises_to_the_ratio_asked_toward_what_reads_better() {
        let dark = Color::from_rgb8(0x1e, 0x1e, 0x20);
        let gray = Color::from_rgb8(0x45, 0x45, 0x4b).with_alpha(0.8);
        assert!(super::contrast(gray, dark) < 4.5);
        let raised = with_contrast(gray, dark, 4.5);
        assert!(super::contrast(raised, dark) >= 4.5);
        assert!(super::contrast(raised, dark) < 4.8, "as little as gives it");
        assert!(raised.red > gray.red, "toward white on a dark background");
        assert_eq!(raised.alpha, 0.8);
        // Of a light background, toward black; already enough, as it is.
        let light = Color::from_rgb8(0xfa, 0xfa, 0xfa);
        let pale = Color::from_rgb8(0xc4, 0xc4, 0xca);
        assert!(with_contrast(pale, light, 4.5).red < pale.red);
        let text = Color::from_rgb8(0xe6, 0xe6, 0xe6);
        assert_eq!(with_contrast(text, dark, 4.5), text);
        assert_eq!(with_contrast(gray, dark, 1.0), gray);
    }

    #[test]
    fn a_token_without_a_color_takes_its_parent_s() {
        let mut tokens = Tokens::default();
        tokens.set("tabs", Color::from_rgb8(1, 2, 3));
        tokens.set("tabs.active.bg", Color::from_rgb8(4, 5, 6));
        assert_eq!(
            tokens.get("tabs.active.bg"),
            Some(Color::from_rgb8(4, 5, 6))
        );
        assert_eq!(
            tokens.get("tabs.active.fg"),
            Some(Color::from_rgb8(1, 2, 3))
        );
        assert_eq!(tokens.get("tabs.hover"), Some(Color::from_rgb8(1, 2, 3)));
        assert_eq!(tokens.get("tab"), None);
        // Clear, whatever a theme says.
        tokens.set("transparent", Color::from_rgb8(1, 2, 3));
        assert_eq!(tokens.get("transparent"), Some(Color::CLEAR));
        assert_eq!(tokens.get("transparent.bg"), Some(Color::CLEAR));
        assert_eq!(tokens.get("panel.bg"), None);
        assert_eq!(tokens.get(""), None);
        assert_eq!(
            Theme::builtin().dark.tokens.get("search.match.other"),
            Theme::builtin().dark.tokens.get("search.match")
        );
    }

    #[test]
    fn the_rest_of_the_256_colors_are_xterm_s() {
        let xterm = Colors::default();
        for variant in [Theme::builtin().light, Theme::builtin().dark] {
            assert_eq!(variant.colors.palette[16..], xterm.palette[16..]);
        }
    }

    #[test]
    fn the_selection_shows_on_its_background() {
        for variant in [Theme::builtin().light, Theme::builtin().dark] {
            let to_rgb = |color: Color| Rgb {
                r: (color.red * 255.0).round() as u8,
                g: (color.green * 255.0).round() as u8,
                b: (color.blue * 255.0).round() as u8,
            };
            let selection = to_rgb(variant.selection.background);
            assert!(contrast(selection, variant.colors.background) > 1.3);
            assert!(contrast(variant.colors.foreground, selection) > 7.0);
        }
    }
}
