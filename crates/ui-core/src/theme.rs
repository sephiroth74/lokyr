//! The built-in theme: its colors in a light and a dark variant, which follow
//! the system's appearance.

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

/// A variant of the theme: the terminal's colors, and the selection's.
#[derive(Clone, Debug, PartialEq)]
pub struct Variant {
    /// The palette, the default colors and the cursor's, which a terminal
    /// takes as its own.
    pub colors: Colors,
    /// The selection's colors.
    pub selection: SelectionColors,
}

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
            light: variant(LIGHT),
            dark: variant(DARK),
            man_page: variant((LIGHT.0, foreground, background, selection)),
        }
    }

    /// The variant of the dark appearance or of the light one.
    pub fn variant(&self, dark: bool) -> &Variant {
        if dark { &self.dark } else { &self.light }
    }
}

fn variant((sixteen, foreground, background, selection): (Sixteen, u32, u32, u32)) -> Variant {
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
