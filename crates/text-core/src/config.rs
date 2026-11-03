//! The `[font]` section of the configuration.
//!
//! ```toml
//! [font]
//! family = "CaskaydiaCove NFM"   # the default family when absent
//! size = 13                      # points, 4 to 200
//! style = "SemiBold"             # the regular face's style, by the font's name for it
//! bold-style = "Bold"            # the faces of bold, italic and bold italic text
//! italic-style = "Italic"
//! bold-italic-style = "Bold Italic"
//! non-ascii-family = "Hiragino Sans"   # the family of text that is not ASCII
//! line-height = 1.0              # the face's line height times this, 0.5 to 3
//! cell-width = 1.0               # the face's advance times this, 0.5 to 3
//! ligatures = true
//! builtin-box-drawing = true     # box drawing, blocks and braille drawn to the pixel
//! builtin-powerline = true       # the powerline symbols drawn to the pixel
//! antialias = true
//! thin-strokes = "never"         # never | always | retina | dark | retina-dark
//! system-weight = false          # true: dark text on light as thin as macOS's
//! ```
//!
//! A value out of its range is an error of the value, which the reader of
//! the configuration places in the file, as it does an unknown key.

use std::fmt;

use serde::{Deserialize, Serialize};

/// The `[font]` section: the faces that the terminal's text takes, and the
/// cell that they make.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct FontConfig {
    /// The family of the text; `None` for the default one.
    pub family: Option<Name>,
    /// The size, in points.
    pub size: Size,
    /// The style of the regular face, by the name that the font gives it,
    /// such as `SemiBold`; `None` for the family's regular face.
    pub style: Option<Name>,
    /// The style of bold text; `None` for the family's upright face, heavier
    /// than the regular one, nearest to bold's weight or one step past the
    /// regular one's, as Bold is Regular's and SemiBold's; or the regular
    /// face drawn heavier when the family has none.
    pub bold_style: Option<Name>,
    /// The style of italic text; `None` for the family's italic face of the
    /// regular one's weight, or a slanted one when it has none.
    pub italic_style: Option<Name>,
    /// The style of bold italic text; `None` for the family's italic face of
    /// the bold one's weight, or one made from the others.
    pub bold_italic_style: Option<Name>,
    /// The family of every character past ASCII, as iTerm2's non-ASCII
    /// font; `None` for the text's family.
    pub non_ascii_family: Option<Name>,
    /// The height of a cell, as a factor of the face's line height.
    pub line_height: Factor,
    /// The width of a cell, as a factor of the face's advance.
    pub cell_width: Factor,
    /// Whether the glyphs that a font joins into ligatures, such as `->` or
    /// `!=`, are joined.
    pub ligatures: bool,
    /// Whether the box drawing characters, the block elements and braille
    /// are drawn to the pixel at the cell's size, rather than taken from the
    /// font.
    pub builtin_box_drawing: bool,
    /// Whether the powerline symbols, U+E0B0 to U+E0BF, are drawn to the
    /// pixel at the cell's size, rather than taken from the font.
    pub builtin_powerline: bool,
    /// Whether the glyphs' edges are antialiased.
    pub antialias: bool,
    /// Where the glyphs are drawn of thinner strokes, without the darkening
    /// that the system's font smoothing adds.
    pub thin_strokes: ThinStrokes,
    /// Whether the text takes the weight of the system's text, as the
    /// interface's does: dark text on a light background thinner than its
    /// glyphs blended as they are drawn, light text on a dark one nearly
    /// alike.
    pub system_weight: bool,
}

/// Where the glyphs are drawn of thin strokes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ThinStrokes {
    /// Nowhere: as the system smooths fonts.
    #[default]
    Never,
    /// Everywhere.
    Always,
    /// On a display of a scale of 2 or more.
    Retina,
    /// On a cell whose background is darker than its text.
    Dark,
    /// On a cell whose background is darker than its text, on a display of
    /// a scale of 2 or more.
    RetinaDark,
}

impl ThinStrokes {
    /// Whether a glyph is thin on a display of `scale`, on a background
    /// darker than its text when `dark`.
    pub fn thin(self, scale: f32, dark: bool) -> bool {
        let retina = scale >= 2.0;
        match self {
            Self::Never => false,
            Self::Always => true,
            Self::Retina => retina,
            Self::Dark => dark,
            Self::RetinaDark => retina && dark,
        }
    }
}

impl Default for FontConfig {
    fn default() -> Self {
        Self {
            family: None,
            size: Size::DEFAULT,
            style: None,
            bold_style: None,
            italic_style: None,
            bold_italic_style: None,
            non_ascii_family: None,
            line_height: Factor::ONE,
            cell_width: Factor::ONE,
            ligatures: true,
            builtin_box_drawing: true,
            builtin_powerline: true,
            antialias: true,
            thin_strokes: ThinStrokes::Never,
            system_weight: false,
        }
    }
}

/// A name of a family or of a style: not empty, and without spaces at
/// either end.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(try_from = "String", into = "String")]
pub struct Name(String);

impl From<Name> for String {
    fn from(name: Name) -> Self {
        name.0
    }
}

impl Name {
    /// The name.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for Name {
    type Error = String;

    fn try_from(name: String) -> Result<Self, Self::Error> {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            return Err("a name cannot be empty".to_owned());
        }
        Ok(Self(trimmed.to_owned()))
    }
}

impl fmt::Display for Name {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A size of the font, in points.
#[derive(Clone, Copy, Debug, PartialEq, Deserialize, Serialize)]
#[serde(try_from = "f64", into = "f64")]
pub struct Size(f32);

impl From<Size> for f64 {
    fn from(size: Size) -> Self {
        as_written(size.0)
    }
}

impl Size {
    /// The smallest size.
    pub const MIN: f32 = 4.0;
    /// The largest size.
    pub const MAX: f32 = 200.0;
    /// The size when the configuration sets none.
    pub const DEFAULT: Self = Self(13.0);

    /// The size in points.
    pub fn points(self) -> f32 {
        self.0
    }
}

impl Default for Size {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl TryFrom<f64> for Size {
    type Error = String;

    fn try_from(points: f64) -> Result<Self, Self::Error> {
        in_range(points, Self::MIN, Self::MAX, "the size, in points,").map(Self)
    }
}

/// A factor of a face's dimension, from half of it to three times it.
#[derive(Clone, Copy, Debug, PartialEq, Deserialize, Serialize)]
#[serde(try_from = "f64", into = "f64")]
pub struct Factor(f32);

impl From<Factor> for f64 {
    fn from(factor: Factor) -> Self {
        as_written(factor.0)
    }
}

impl Factor {
    /// The smallest factor.
    pub const MIN: f32 = 0.5;
    /// The largest factor.
    pub const MAX: f32 = 3.0;
    /// The face's dimension as it is.
    pub const ONE: Self = Self(1.0);

    /// The factor.
    pub fn get(self) -> f32 {
        self.0
    }
}

impl Default for Factor {
    fn default() -> Self {
        Self::ONE
    }
}

impl TryFrom<f64> for Factor {
    type Error = String;

    fn try_from(factor: f64) -> Result<Self, Self::Error> {
        in_range(factor, Self::MIN, Self::MAX, "the factor").map(Self)
    }
}

/// `value`, a value that a file wrote as a decimal and the configuration
/// keeps as an `f32`, as the decimal that it was: `0.85` rather than the
/// `f64` nearest to its `f32`, `0.8500000238418579`.
pub fn as_written(value: f32) -> f64 {
    value.to_string().parse().unwrap_or(f64::from(value))
}

/// `value` as an `f32`, if it is within `min` and `max`; otherwise the
/// error that says what `what` takes.
fn in_range(value: f64, min: f32, max: f32, what: &str) -> Result<f32, String> {
    if (f64::from(min)..=f64::from(max)).contains(&value) {
        // Within the range, the value fits an `f32`.
        Ok(value as f32)
    } else {
        Err(format!("{what} is from {min} to {max}, not {value}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(toml: &str) -> Result<FontConfig, String> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct File {
            #[serde(default)]
            font: FontConfig,
        }
        toml::from_str::<File>(toml)
            .map(|file| file.font)
            .map_err(|error| error.message().to_owned())
    }

    #[test]
    fn a_section_that_is_absent_takes_the_defaults() {
        let font = read("").unwrap();
        assert_eq!(font, FontConfig::default());
        assert_eq!(font.family, None);
        assert_eq!(font.size.points(), 13.0);
        assert_eq!(font.line_height.get(), 1.0);
        assert_eq!(font.cell_width.get(), 1.0);
        assert!(font.ligatures);
        assert_eq!(read("[font]\n").unwrap(), FontConfig::default());
    }

    #[test]
    fn every_key_reads() {
        let font = read(
            r#"
            [font]
            family = "CaskaydiaCove NFM"
            size = 22
            style = "SemiBold"
            bold-style = "Bold"
            italic-style = "SemiBold Italic"
            bold-italic-style = "Bold Italic"
            non-ascii-family = "Hiragino Sans"
            line-height = 1.2
            cell-width = 0.95
            ligatures = false
            builtin-box-drawing = false
            builtin-powerline = false
            antialias = false
            thin-strokes = "retina-dark"
            system-weight = true
            "#,
        )
        .unwrap();
        let name = |name: &str| Some(Name::try_from(name.to_owned()).unwrap());
        assert_eq!(
            font,
            FontConfig {
                family: name("CaskaydiaCove NFM"),
                size: Size(22.0),
                style: name("SemiBold"),
                bold_style: name("Bold"),
                italic_style: name("SemiBold Italic"),
                bold_italic_style: name("Bold Italic"),
                non_ascii_family: name("Hiragino Sans"),
                line_height: Factor(1.2),
                cell_width: Factor(0.95),
                ligatures: false,
                builtin_box_drawing: false,
                builtin_powerline: false,
                antialias: false,
                thin_strokes: ThinStrokes::RetinaDark,
                system_weight: true,
            }
        );
    }

    #[test]
    fn thin_strokes_choose_by_the_scale_and_the_background() {
        let cases = [
            (ThinStrokes::Never, [false, false, false, false]),
            (ThinStrokes::Always, [true, true, true, true]),
            (ThinStrokes::Retina, [false, false, true, true]),
            (ThinStrokes::Dark, [false, true, false, true]),
            (ThinStrokes::RetinaDark, [false, false, false, true]),
        ];
        for (strokes, thin) in cases {
            let found = [(1.0, false), (1.0, true), (2.0, false), (2.0, true)]
                .map(|(scale, dark)| strokes.thin(scale, dark));
            assert_eq!(found, thin, "{strokes:?}");
        }
        assert_eq!(
            read("[font]\nthin-strokes = \"dark\"")
                .unwrap()
                .thin_strokes,
            ThinStrokes::Dark
        );
        assert!(read("[font]\nthin-strokes = \"thin\"").is_err());
    }

    #[test]
    fn a_size_keeps_within_its_limits() {
        assert_eq!(read("[font]\nsize = 4").unwrap().size.points(), 4.0);
        assert_eq!(read("[font]\nsize = 200.0").unwrap().size.points(), 200.0);
        assert_eq!(
            read("[font]\nsize = 3.5").unwrap_err(),
            "the size, in points, is from 4 to 200, not 3.5"
        );
        assert!(read("[font]\nsize = 250").is_err());
        assert!(read("[font]\nsize = nan").is_err());
        assert!(read("[font]\nsize = \"13\"").is_err());
    }

    #[test]
    fn a_factor_keeps_within_its_limits() {
        assert_eq!(
            read("[font]\nline-height = 0.5").unwrap().line_height.get(),
            0.5
        );
        assert_eq!(
            read("[font]\ncell-width = 3").unwrap().cell_width.get(),
            3.0
        );
        assert_eq!(
            read("[font]\nline-height = 0.4").unwrap_err(),
            "the factor is from 0.5 to 3, not 0.4"
        );
        assert!(read("[font]\ncell-width = 3.1").is_err());
        assert!(read("[font]\ncell-width = -1").is_err());
    }

    #[test]
    fn a_name_is_trimmed_and_cannot_be_empty() {
        let font = read("[font]\nfamily = \"  SF Mono \"").unwrap();
        assert_eq!(font.family.unwrap().as_str(), "SF Mono");
        assert_eq!(
            read("[font]\nstyle = \"   \"").unwrap_err(),
            "a name cannot be empty"
        );
    }

    #[test]
    fn an_unknown_key_is_an_error() {
        let error = read("[font]\nfamilly = \"Menlo\"").unwrap_err();
        assert!(error.starts_with("unknown field `familly`"), "{error}");
        // Keys are in kebab case.
        assert!(read("[font]\nbold_style = \"Bold\"").is_err());
    }
}
