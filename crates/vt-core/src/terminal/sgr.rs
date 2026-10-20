//! SGR: the attributes and colors of the text printed next.

use super::Terminal;
use crate::page::{Color, Rgb, Style, Underline};
use crate::parser::Params;

impl Terminal {
    /// SGR: changes the pen as `params` say, one attribute after the other.
    /// Attributes the terminal does not know, and colors it cannot read,
    /// are skipped.
    pub(super) fn select_graphic_rendition(&mut self, params: &Params) {
        let mut groups = params.iter();
        while let Some(group) = groups.next() {
            let pen = &mut self.pen;
            match group[0] {
                0 => *pen = Style::DEFAULT,
                1 => pen.bold = true,
                2 => pen.faint = true,
                3 => pen.italic = true,
                4 => {
                    if let Some(underline) = underline(group.get(1).copied().unwrap_or(1)) {
                        pen.underline = underline;
                    }
                }
                5 | 6 => pen.blink = true,
                7 => pen.inverse = true,
                8 => pen.invisible = true,
                9 => pen.strikethrough = true,
                21 => pen.underline = Underline::Double,
                22 => (pen.bold, pen.faint) = (false, false),
                23 => pen.italic = false,
                24 => pen.underline = Underline::None,
                25 => pen.blink = false,
                27 => pen.inverse = false,
                28 => pen.invisible = false,
                29 => pen.strikethrough = false,
                value @ 30..=37 => pen.fg = Color::Palette((value - 30) as u8),
                39 => pen.fg = Color::Default,
                value @ 40..=47 => pen.bg = Color::Palette((value - 40) as u8),
                49 => pen.bg = Color::Default,
                53 => pen.overline = true,
                55 => pen.overline = false,
                59 => pen.underline_color = Color::Default,
                value @ 90..=97 => pen.fg = Color::Palette((value - 90 + 8) as u8),
                value @ 100..=107 => pen.bg = Color::Palette((value - 100 + 8) as u8),
                which @ (38 | 48 | 58) => {
                    let color = if group.len() > 1 {
                        within(&group[1..])
                    } else {
                        extended_color(&mut groups.by_ref().map(|group| group[0]))
                    };
                    if let Some(color) = color {
                        match which {
                            38 => pen.fg = color,
                            48 => pen.bg = color,
                            _ => pen.underline_color = color,
                        }
                    }
                }
                _ => {}
            }
        }
        self.pen_changed();
    }
}

/// The underline that `4:style` asks for.
fn underline(style: u16) -> Option<Underline> {
    Some(match style {
        0 => Underline::None,
        1 => Underline::Single,
        2 => Underline::Double,
        3 => Underline::Curly,
        4 => Underline::Dotted,
        5 => Underline::Dashed,
        _ => return None,
    })
}

/// The color of 38, 48 or 58 written with colons, from the values after
/// it in its group: `5:n` for an entry of the palette, and `2:id:r:g:b`
/// for RGB, whose color-space id can be empty or, as many programs write
/// it, left out.
fn within(values: &[u16]) -> Option<Color> {
    let rgb = if values.len() > 4 {
        &values[2..]
    } else {
        &values[1..]
    };
    extended_color(&mut values[..1].iter().chain(rgb).copied())
}

/// The color that `values` give after 38, 48 or 58: `5` and an entry of
/// the palette, or `2` and the red, green and blue components. It takes
/// only the values it reads, so that those after a color it cannot read
/// count as attributes, as in `vte`.
fn extended_color(values: &mut impl Iterator<Item = u16>) -> Option<Color> {
    let component = |value: u16| u8::try_from(value).ok();
    match values.next()? {
        5 => Some(Color::Palette(component(values.next()?)?)),
        2 => {
            let r = component(values.next()?)?;
            let g = component(values.next()?)?;
            let b = component(values.next()?)?;
            Some(Color::Rgb(Rgb::new(r, g, b)))
        }
        _ => None,
    }
}
