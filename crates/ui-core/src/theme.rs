//! The built-in theme: its colors in a light and a dark variant, which follow
//! the system's appearance: the terminal's, and the interface's, by their
//! tokens.

use std::collections::HashMap;

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

/// The colors of the interface, by their tokens' names, as `tab.active.bg`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Tokens(HashMap<String, Color>);

impl Tokens {
    /// The color of `token`; of the token without its last part where it
    /// has none, `tab.active` for `tab.active.bg`, then `tab`; `None` when
    /// none of them has one.
    pub fn get(&self, token: &str) -> Option<Color> {
        let mut name = token;
        loop {
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
    ("tab.bar.bg", 0xe6e6e8, 1.0),
    ("tab.active.bg", 0xfafafa, 1.0),
    ("tab.active.fg", 0x1e1e20, 1.0),
    ("tab.inactive.bg", 0xe6e6e8, 1.0),
    ("tab.inactive.fg", 0x5e5e64, 1.0),
    ("tab.hover.bg", 0x000000, 0.05),
    ("search.match", 0xffd60a, 0.5),
    ("search.match.current", 0xff7a00, 0.6),
];

/// The interface's colors in the dark variant.
const DARK_TOKENS: &[TokenColor] = &[
    ("text", 0xe6e6e6, 1.0),
    ("text.secondary", 0x9a9aa0, 1.0),
    ("text.disabled", 0x5e5e64, 1.0),
    ("accent", 0x4c9bff, 1.0),
    ("separator", 0xffffff, 0.1),
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
    ("tab.bar.bg", 0x161618, 1.0),
    ("tab.active.bg", 0x1e1e20, 1.0),
    ("tab.active.fg", 0xe6e6e6, 1.0),
    ("tab.inactive.bg", 0x161618, 1.0),
    ("tab.inactive.fg", 0x9a9aa0, 1.0),
    ("tab.hover.bg", 0xffffff, 0.06),
    ("search.match", 0xffd60a, 0.3),
    ("search.match.current", 0xff7a00, 0.5),
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

impl Theme {
    /// The built-in theme: a palette of our own, whose colors read on their
    /// variant's background; a man page's takes the light one's.
    pub fn builtin() -> Self {
        let (foreground, background, selection) = MAN_PAGE;
        Self {
            light: variant(LIGHT, LIGHT_TOKENS),
            dark: variant(DARK, DARK_TOKENS),
            man_page: variant((LIGHT.0, foreground, background, selection), LIGHT_TOKENS),
        }
    }

    /// The variant of the dark appearance or of the light one.
    pub fn variant(&self, dark: bool) -> &Variant {
        if dark { &self.dark } else { &self.light }
    }
}

fn variant(
    (sixteen, foreground, background, selection): (Sixteen, u32, u32, u32),
    tokens: &[TokenColor],
) -> Variant {
    let mut colors = Colors::default();
    for (entry, &color) in colors.palette.iter_mut().zip(&sixteen) {
        *entry = rgb(color);
    }
    colors.foreground = rgb(foreground);
    colors.background = rgb(background);
    colors.cursor = rgb(foreground);
    Variant {
        colors,
        selection: SelectionColors {
            background: color(rgb(selection)),
            foreground: None,
        },
        tokens: Tokens(
            tokens
                .iter()
                .map(|&(name, value, alpha)| (name.to_owned(), color(rgb(value)).with_alpha(alpha)))
                .collect(),
        ),
    }
}

fn rgb(color: u32) -> Rgb {
    let [_, r, g, b] = color.to_be_bytes();
    Rgb { r, g, b }
}

fn color(rgb: Rgb) -> Color {
    Color::from_rgb8(rgb.r, rgb.g, rgb.b)
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
                ("tab.active.fg", "tab.active.bg"),
                ("tab.inactive.fg", "tab.inactive.bg"),
                ("accent", "panel.bg"),
                ("tooltip.fg", "tooltip.bg"),
                ("list.selection.fg", "list.selection.bg"),
            ] {
                let under = opaque(token(background));
                let ratio = contrast(opaque(token(text)), under);
                // WCAG's AA for text.
                assert!(ratio >= 4.5, "{name}: {text} on {background}: {ratio:.2}");
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
            // The active tab is the terminal's background.
            assert_eq!(opaque(token("tab.active.bg")), background, "{name}");
        }
    }

    #[test]
    fn a_token_without_a_color_takes_its_parent_s() {
        let mut tokens = Tokens::default();
        tokens.set("tab", Color::from_rgb8(1, 2, 3));
        tokens.set("tab.active.bg", Color::from_rgb8(4, 5, 6));
        assert_eq!(tokens.get("tab.active.bg"), Some(Color::from_rgb8(4, 5, 6)));
        assert_eq!(tokens.get("tab.active.fg"), Some(Color::from_rgb8(1, 2, 3)));
        assert_eq!(tokens.get("tab.hover"), Some(Color::from_rgb8(1, 2, 3)));
        assert_eq!(tokens.get("tabs"), None);
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
