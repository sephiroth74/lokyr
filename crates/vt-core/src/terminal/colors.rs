//! The colors that programs can change: the palette (OSC 4 and 104), and
//! the default foreground, background and cursor colors (OSC 10 to 12, and
//! 110 to 112).

use super::{Event, Terminal};
use crate::page::Rgb;
use crate::parser::StringEnd;

/// The colors that the renderer draws with, and programs can change.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Colors {
    /// The 256 colors of the palette.
    pub palette: [Rgb; 256],
    /// The default foreground color.
    pub foreground: Rgb,
    /// The default background color.
    pub background: Rgb,
    /// The cursor's color.
    pub cursor: Rgb,
}

impl Default for Colors {
    /// xterm's colors, until the embedder gives the theme's.
    fn default() -> Self {
        const BASE: [(u8, u8, u8); 16] = [
            (0, 0, 0),
            (205, 0, 0),
            (0, 205, 0),
            (205, 205, 0),
            (0, 0, 238),
            (205, 0, 205),
            (0, 205, 205),
            (229, 229, 229),
            (127, 127, 127),
            (255, 0, 0),
            (0, 255, 0),
            (255, 255, 0),
            (92, 92, 255),
            (255, 0, 255),
            (0, 255, 255),
            (255, 255, 255),
        ];
        const LEVELS: [u8; 6] = [0, 95, 135, 175, 215, 255];
        let palette = std::array::from_fn(|index| match index {
            0..16 => {
                let (r, g, b) = BASE[index];
                Rgb::new(r, g, b)
            }
            // The 6×6×6 color cube.
            16..232 => {
                let cube = index - 16;
                Rgb::new(LEVELS[cube / 36], LEVELS[cube / 6 % 6], LEVELS[cube % 6])
            }
            // The 24 grays.
            _ => {
                let gray = (8 + 10 * (index - 232)) as u8;
                Rgb::new(gray, gray, gray)
            }
        });
        let foreground = palette[7];
        Self {
            palette,
            foreground,
            background: palette[0],
            cursor: foreground,
        }
    }
}

impl Terminal {
    /// OSC 4: pairs of an index of the palette and a color, or `?` to ask
    /// for the color, which gets a reply.
    pub(super) fn set_palette(&mut self, fields: &[u8], end: StringEnd) {
        let mut fields = fields.split(|&byte| byte == b';');
        let mut changed = false;
        while let (Some(index), Some(spec)) = (fields.next(), fields.next()) {
            let Some(index) = super::osc::decimal(index).filter(|&index| index < 256) else {
                continue;
            };
            let index = index as usize;
            if spec == b"?" {
                let color = color_spec(self.colors.palette[index]);
                self.reply_osc(&format!("4;{index};{color}"), end);
            } else if let Some(color) = parse_color(spec) {
                self.colors.palette[index] = color;
                changed = true;
            }
        }
        if changed {
            self.events.push(Event::ColorsChanged);
        }
    }

    /// OSC 104: brings back the theme's colors of the palette at the
    /// indexes, or of the whole palette without any.
    pub(super) fn reset_palette(&mut self, fields: &[u8]) {
        if fields.is_empty() {
            self.colors.palette = self.default_colors.palette;
        } else {
            for index in fields.split(|&byte| byte == b';') {
                if let Some(index) = super::osc::decimal(index).filter(|&index| index < 256) {
                    let index = index as usize;
                    self.colors.palette[index] = self.default_colors.palette[index];
                }
            }
        }
        self.events.push(Event::ColorsChanged);
    }

    /// OSC 10, 11 and 12: the foreground, background and cursor colors, or
    /// `?` to ask for one. As in xterm, the colors after the first go to the
    /// next ones: `OSC 10 ; fg ; bg` sets both.
    pub(super) fn set_dynamic_colors(&mut self, first: u16, fields: &[u8], end: StringEnd) {
        let mut changed = false;
        for (which, spec) in (first..=12).zip(fields.split(|&byte| byte == b';')) {
            if spec == b"?" {
                let color = color_spec(*self.dynamic_color(which));
                self.reply_osc(&format!("{which};{color}"), end);
            } else if let Some(color) = parse_color(spec) {
                *self.dynamic_color(which) = color;
                changed = true;
            }
        }
        if changed {
            self.events.push(Event::ColorsChanged);
        }
    }

    /// OSC 110, 111 and 112: bring back the theme's foreground, background
    /// or cursor color.
    pub(super) fn reset_dynamic_color(&mut self, which: u16) {
        let theme = match which {
            10 => self.default_colors.foreground,
            11 => self.default_colors.background,
            _ => self.default_colors.cursor,
        };
        *self.dynamic_color(which) = theme;
        self.events.push(Event::ColorsChanged);
    }

    /// The foreground (10), background (11) or cursor (12) color.
    fn dynamic_color(&mut self, which: u16) -> &mut Rgb {
        match which {
            10 => &mut self.colors.foreground,
            11 => &mut self.colors.background,
            _ => &mut self.colors.cursor,
        }
    }

    /// Queues `OSC` `text`, ended as the query that asked for it was.
    pub(super) fn reply_osc(&mut self, text: &str, end: StringEnd) {
        let terminator = if end == StringEnd::Bel {
            "\x07"
        } else {
            "\x1b\\"
        };
        self.reply(format!("\x1b]{text}{terminator}").as_bytes());
    }
}

/// `color` as xterm reports it: `rgb:` and 16 bits of each component, in
/// hexadecimal.
fn color_spec(color: Rgb) -> String {
    let wide = |component: u8| u16::from(component) * 257;
    format!(
        "rgb:{:04x}/{:04x}/{:04x}",
        wide(color.r),
        wide(color.g),
        wide(color.b)
    )
}

/// The color that `spec` names, as X11 writes them: `rgb:r/g/b` with one
/// to four hexadecimal digits for each component, scaled to its range, or
/// `#rgb` with one to four digits each, which are the high ones. Names of
/// colors are not known.
pub(super) fn parse_color(spec: &[u8]) -> Option<Rgb> {
    let spec = std::str::from_utf8(spec).ok()?;
    if let Some(components) = spec.strip_prefix("rgb:") {
        let mut components = components.split('/');
        let (r, g, b) = (components.next()?, components.next()?, components.next()?);
        if components.next().is_some() {
            return None;
        }
        return Some(Rgb::new(scaled(r)?, scaled(g)?, scaled(b)?));
    }
    let digits = spec.strip_prefix('#')?;
    if digits.is_empty() || digits.len() > 12 || !digits.len().is_multiple_of(3) {
        return None;
    }
    let width = digits.len() / 3;
    let component = |index: usize| -> Option<u8> {
        let value = hexadecimal(&digits[index * width..(index + 1) * width])?;
        Some(((value << (4 * (4 - width))) >> 8) as u8)
    };
    Some(Rgb::new(component(0)?, component(1)?, component(2)?))
}

/// The component that one to four hexadecimal `digits` give, scaled from
/// their range to 8 bits.
fn scaled(digits: &str) -> Option<u8> {
    if digits.is_empty() || digits.len() > 4 {
        return None;
    }
    let value = hexadecimal(digits)?;
    let max = (1 << (4 * digits.len())) - 1;
    Some(((value * 255 + max / 2) / max) as u8)
}

/// The number that `digits`, all hexadecimal, write.
fn hexadecimal(digits: &str) -> Option<u32> {
    if !digits.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    u32::from_str_radix(digits, 16).ok()
}
