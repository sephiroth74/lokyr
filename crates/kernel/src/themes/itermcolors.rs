//! Themes of `.itermcolors` files, iTerm2's color presets: property lists
//! of a dictionary of each color, read as iTerm2 reads them. A color's
//! `Color Space` says how its components read: `sRGB` as they are, `P3`
//! of Display P3, and any other, `Calibrated` or `Device`, or none, as the
//! old files have, of calibrated Generic RGB; each converted to sRGB, cut
//! to its gamut, as AppKit's conversion cuts it. A file of keys of the
//! light and the dark appearance, ending in ` (Light)` and ` (Dark)`, is
//! two themes, `<name> (Light)` and `<name> (Dark)`.

use std::io::Cursor;
use std::path::Path;

use lokyr_plugin_sdk::theme::{TerminalColors, Theme, ThemeColor, ThemeVariant};
use plist::{Dictionary, Value};

use crate::config::Error;

/// The suffixes of a key of the light appearance's and of the dark one's,
/// and the themes' that they make.
const LIGHT: &str = " (Light)";
const DARK: &str = " (Dark)";

/// The themes of the `.itermcolors` file of `bytes`, named `name`, as
/// `path`'s errors tell: one, or two of a file of the light and the dark
/// appearance's keys, the light one first.
pub fn read(bytes: &[u8], name: &str, path: &Path) -> Result<Vec<Theme>, Error> {
    let error = |message: String| Error {
        path: path.to_owned(),
        position: None,
        message,
    };
    let value = Value::from_reader(Cursor::new(bytes))
        .map_err(|e| error(format!("it is no property list of colors: {e}")))?;
    let dictionary = value
        .as_dictionary()
        .ok_or_else(|| error("it is no dictionary of colors".to_owned()))?;
    let variants = dictionary
        .keys()
        .any(|key| key.ends_with(LIGHT) || key.ends_with(DARK));
    if !variants {
        return Ok(vec![theme(dictionary, None, name, &error)?]);
    }
    Ok(vec![
        theme(
            dictionary,
            Some(ThemeVariant::Light),
            &format!("{name}{LIGHT}"),
            &error,
        )?,
        theme(
            dictionary,
            Some(ThemeVariant::Dark),
            &format!("{name}{DARK}"),
            &error,
        )?,
    ])
}

/// The theme `name` of `dictionary`, of the keys of `variant`'s
/// appearance where it has them.
fn theme(
    dictionary: &Dictionary,
    variant: Option<ThemeVariant>,
    name: &str,
    error: &dyn Fn(String) -> Error,
) -> Result<Theme, Error> {
    let suffix = match variant {
        Some(ThemeVariant::Light) => LIGHT,
        Some(ThemeVariant::Dark) => DARK,
        None => "",
    };
    let color = |key: &str| -> Result<Option<ThemeColor>, Error> {
        let own = (!suffix.is_empty())
            .then(|| dictionary.get(&format!("{key}{suffix}")))
            .flatten();
        match own.or_else(|| dictionary.get(key)) {
            None => Ok(None),
            Some(value) => component_color(value)
                .map(Some)
                .ok_or_else(|| error(format!("`{key}` is no color"))),
        }
    };
    let needed = |key: &str| -> Result<ThemeColor, Error> {
        color(key)?.ok_or_else(|| error(format!("the file has no `{key}`")))
    };
    let mut palette = [ThemeColor::rgb(0, 0, 0); 16];
    for (index, entry) in palette.iter_mut().enumerate() {
        *entry = needed(&format!("Ansi {index} Color"))?;
    }
    let background = needed("Background Color")?;
    let foreground = needed("Foreground Color")?;
    let lighter =
        u32::from(background.red) + u32::from(background.green) + u32::from(background.blue)
            > u32::from(foreground.red) + u32::from(foreground.green) + u32::from(foreground.blue);
    let mut theme = Theme::of_terminal(
        name,
        variant.unwrap_or(if lighter {
            ThemeVariant::Light
        } else {
            ThemeVariant::Dark
        }),
        TerminalColors {
            background,
            foreground,
            palette,
            cursor: color("Cursor Color")?,
            cursor_text: color("Cursor Text Color")?,
            selection: color("Selection Color")?,
            selection_text: color("Selected Text Color")?,
            bold: color("Bold Color")?,
            link: color("Link Color")?,
            underline: color("Underline Color")?,
        },
    );
    if let Some(matched) = color("Match Background Color")? {
        theme.tokens.insert("search.match".to_owned(), matched);
    }
    Ok(theme)
}

/// The color of `value`, a dictionary of its components, in sRGB.
fn component_color(value: &Value) -> Option<ThemeColor> {
    let dictionary = value.as_dictionary()?;
    let component = |key: &str| -> Option<f64> {
        match dictionary.get(key)? {
            Value::Real(real) => Some(*real),
            Value::Integer(integer) => integer.as_signed().map(|n| n as f64),
            _ => None,
        }
    };
    let rgb = [
        component("Red Component")?,
        component("Green Component")?,
        component("Blue Component")?,
    ];
    let alpha = component("Alpha Component").unwrap_or(1.0);
    let space = dictionary
        .get("Color Space")
        .and_then(Value::as_string)
        .map(str::trim);
    let srgb = match space {
        Some("sRGB") => rgb,
        Some("P3") => p3_to_srgb(rgb),
        // Calibrated, Device, or none, as iTerm2 reads them.
        _ => generic_to_srgb(rgb),
    };
    let byte = |value: f64| (value.clamp(0.0, 1.0) * 255.0).round() as u8;
    Some(ThemeColor {
        red: byte(srgb[0]),
        green: byte(srgb[1]),
        blue: byte(srgb[2]),
        alpha: byte(alpha),
    })
}

/// sRGB's transfer function, from encoded to linear.
fn srgb_to_linear(value: f64) -> f64 {
    if value <= 0.040_45 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}

/// sRGB's transfer function, from linear to encoded, of a value cut to the
/// gamut first.
fn linear_to_srgb(value: f64) -> f64 {
    let value = value.clamp(0.0, 1.0);
    if value <= 0.003_130_8 {
        value * 12.92
    } else {
        1.055 * value.powf(1.0 / 2.4) - 0.055
    }
}

/// `matrix` times `vector`.
fn times(matrix: &[[f64; 3]; 3], vector: [f64; 3]) -> [f64; 3] {
    matrix.map(|row| row[0] * vector[0] + row[1] * vector[1] + row[2] * vector[2])
}

/// Display P3's linear components to sRGB's, of their common white, D65.
const P3_TO_SRGB: [[f64; 3]; 3] = [
    [1.224_940_2, -0.224_940_4, 0.0],
    [-0.042_056_9, 1.042_057_1, 0.0],
    [-0.019_637_6, -0.078_636_1, 1.098_273_5],
];

/// The encoded components of a Display P3 color in sRGB, which shares its
/// transfer function.
fn p3_to_srgb(rgb: [f64; 3]) -> [f64; 3] {
    times(&P3_TO_SRGB, rgb.map(srgb_to_linear)).map(linear_to_srgb)
}

/// Generic RGB's linear components to sRGB's, as macOS converts a
/// calibrated color, of AppKit's conversion of its primaries to extended
/// sRGB: nearly sRGB's own primaries, its gamma its own.
const GENERIC_TO_SRGB: [[f64; 3]; 3] = [
    [1.025_241_9, -0.026_545_2, 0.001_303_6],
    [0.019_400_8, 0.948_008_9, 0.032_590_4],
    [-0.001_762_1, -0.001_434_5, 1.003_196_7],
];

/// Generic RGB's gamma, of its profile's curve.
const GENERIC_GAMMA: f64 = 1.8;

/// The encoded components of a calibrated Generic RGB color in sRGB.
fn generic_to_srgb(rgb: [f64; 3]) -> [f64; 3] {
    let linear = rgb.map(|value| value.max(0.0).powf(GENERIC_GAMMA));
    times(&GENERIC_TO_SRGB, linear).map(linear_to_srgb)
}

#[cfg(test)]
mod tests;
