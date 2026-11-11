//! The images of cells that the host renders: rows of text of their
//! styles, in the theme's colors and the terminal's font, in a frame as a
//! window's, which the host draws offscreen with the terminal's own
//! renderer and gives back as a PNG.

use serde::{Deserialize, Serialize};

use crate::command::{Code, Error};
use crate::host::{Bytes, StyleRun};

/// The most rows that an image draws.
pub const MOST_ROWS: usize = 1000;

/// The most columns that an image draws: the cells of a row past them are
/// left out.
pub const MOST_COLUMNS: u16 = 1000;

/// The most characters of a row's text: a column's cell, its combining
/// marks too.
pub const MOST_ROW_CHARS: usize = 4 * MOST_COLUMNS as usize;

/// The most characters of an image's title.
pub const MOST_TITLE_CHARS: usize = 256;

/// The most points of the room around an image's window.
pub const MOST_PADDING: f32 = 256.0;

/// The least pixels of a point of an image.
pub const LEAST_SCALE: f32 = 1.0;

/// The most pixels of a point of an image.
pub const MOST_SCALE: f32 = 4.0;

/// The most pixels of an image's side, as a GPU draws a texture.
pub const MOST_SIDE: u32 = 16_384;

/// The most pixels of an image, its width by its height: a screen of 8K
/// whole, and more.
pub const MOST_PIXELS: u64 = 40_000_000;

/// The most images that one call renders, each of the main thread's time.
pub const MOST_IMAGES: usize = 4;

/// The message of the error of an image past [`MOST_IMAGES`] in a call,
/// which every host gives alike.
pub const IMAGES_BOUND: &str = "a call renders 4 images at most";

/// The message of the error of an image that a call that only looks asks
/// for, which every host gives alike.
pub const LOOKING: &str = "a call that only looks, as `applies`, renders no image";

/// A row of cells that an image draws: its text, as a read of rows gives
/// it, a cell without a character a space and a wide character once, and
/// the styles of its cells, as a read of a row's styles gives them, a
/// later run over an earlier one; a cell of no run takes the default
/// style.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CellRow {
    /// Its text.
    pub text: String,
    /// Its styles, as runs of its cells.
    #[serde(default)]
    pub runs: Vec<StyleRun>,
}

/// What surrounds an image's window.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ImageBackground {
    /// The theme's background, darker, which the window stands out of.
    #[default]
    Theme,
    /// Nothing: the image is clear around the window.
    Clear,
    /// A color, red, green and blue.
    Rgb(u8, u8, u8),
}

/// An image of cells that a plugin asks the host for: the cells in the
/// theme's colors and the terminal's font, as a window shows them, in a
/// frame as a window's, of a bar of the window's three buttons and a title,
/// rounded corners and a shadow, with room around it. The window is as wide
/// as its widest row, a row's cells past its text taken where they show
/// something, as a background, and its bar's buttons and title.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ImageSpec {
    /// The rows, from the top, [`MOST_ROWS`] at most, one at least.
    pub rows: Vec<CellRow>,
    /// The title in the window's bar, if it shows one.
    #[serde(default)]
    pub title: Option<String>,
    /// Whether the window's bar shows its three buttons.
    #[serde(default)]
    pub buttons: bool,
    /// Whether the window's corners are rounded.
    #[serde(default)]
    pub rounded: bool,
    /// Whether the window casts a shadow.
    #[serde(default)]
    pub shadow: bool,
    /// The room around the window, in points, from 0 to [`MOST_PADDING`].
    #[serde(default)]
    pub padding: f32,
    /// What surrounds the window.
    #[serde(default)]
    pub background: ImageBackground,
    /// The pixels of a point, from [`LEAST_SCALE`] to [`MOST_SCALE`]; the
    /// display's of the focused window without.
    #[serde(default)]
    pub scale: Option<f32>,
}

impl ImageSpec {
    /// Whether the window has a bar: of its buttons, or of its title.
    pub fn has_bar(&self) -> bool {
        self.buttons || self.title.is_some()
    }

    /// The image as a host takes it, or the error that tells why it does
    /// not: of no row or too many, of a text too long or of a control
    /// character, of a run past the columns or backwards, of room or a
    /// scale out of their ranges.
    pub fn check(&self) -> Result<(), Error> {
        let refused = |message: String| Err(Error::new(Code::Failed, message));
        if self.rows.is_empty() || self.rows.len() > MOST_ROWS {
            return refused(format!("an image draws 1 to {MOST_ROWS} rows"));
        }
        for row in &self.rows {
            if row.text.chars().count() > MOST_ROW_CHARS {
                return refused(format!(
                    "a row of an image has {MOST_ROW_CHARS} characters at most"
                ));
            }
            if row.text.chars().any(char::is_control) {
                return refused("a row of an image has no control character".to_owned());
            }
            if row.runs.len() > usize::from(MOST_COLUMNS) {
                return refused(format!("a row of an image has {MOST_COLUMNS} runs at most"));
            }
            if row
                .runs
                .iter()
                .any(|run| run.first > run.last || run.last >= MOST_COLUMNS)
            {
                return refused(format!(
                    "a run of an image's cells goes from a column to one after it, \
                     below {MOST_COLUMNS}"
                ));
            }
        }
        if let Some(title) = &self.title {
            if title.chars().count() > MOST_TITLE_CHARS {
                return refused(format!(
                    "an image's title has {MOST_TITLE_CHARS} characters at most"
                ));
            }
            if title.chars().any(char::is_control) {
                return refused("an image's title has no control character".to_owned());
            }
        }
        if !(0.0..=MOST_PADDING).contains(&self.padding) {
            return refused(format!(
                "an image's padding is from 0 to {MOST_PADDING} points"
            ));
        }
        if self
            .scale
            .is_some_and(|scale| !(LEAST_SCALE..=MOST_SCALE).contains(&scale))
        {
            return refused(format!(
                "an image's scale is from {LEAST_SCALE} to {MOST_SCALE} pixels a point"
            ));
        }
        Ok(())
    }

    /// The numbers that it carries of its own, its padding and its scale.
    pub(crate) fn numbers(&self) -> Vec<f64> {
        let mut numbers = vec![f64::from(self.padding)];
        numbers.extend(self.scale.map(f64::from));
        numbers
    }
}

/// An image that the host rendered: a PNG, of its size in pixels, and the
/// pixels of a point that it took, which its PNG tells as its resolution.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Image {
    /// The PNG's bytes, which cross to Luau as a string.
    pub png: Bytes,
    /// Its width, in pixels.
    pub width: u32,
    /// Its height, in pixels.
    pub height: u32,
    /// The pixels of a point.
    pub scale: f32,
}

impl Image {
    /// The image of a clear pixel, at `scale`, which the fake host gives,
    /// as it draws nothing.
    pub fn blank(scale: f32) -> Self {
        Self {
            png: Bytes(BLANK.to_vec()),
            width: 1,
            height: 1,
            scale,
        }
    }
}

/// A PNG of a clear pixel.
const BLANK: [u8; 68] = [
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f, 0x15, 0xc4,
    0x89, 0x00, 0x00, 0x00, 0x0b, 0x49, 0x44, 0x41, 0x54, 0x78, 0xda, 0x63, 0x60, 0x00, 0x02, 0x00,
    0x00, 0x05, 0x00, 0x01, 0xe9, 0xfa, 0xdc, 0xd8, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e, 0x44,
    0xae, 0x42, 0x60, 0x82,
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::TextStyle;

    fn spec(rows: &[&str]) -> ImageSpec {
        ImageSpec {
            rows: rows
                .iter()
                .map(|text| CellRow {
                    text: (*text).to_owned(),
                    runs: Vec::new(),
                })
                .collect(),
            ..ImageSpec::default()
        }
    }

    #[test]
    fn an_image_of_rows_within_their_bounds_is_taken() {
        assert_eq!(spec(&["ls -la", "", "日本語 é"]).check(), Ok(()));
        let full = ImageSpec {
            title: Some("~/lokyr — zsh".to_owned()),
            padding: MOST_PADDING,
            scale: Some(MOST_SCALE),
            ..spec(&[&"x".repeat(MOST_ROW_CHARS)])
        };
        assert_eq!(full.check(), Ok(()));
        assert_eq!(spec(&[""; MOST_ROWS]).check(), Ok(()));
    }

    #[test]
    fn an_image_out_of_its_bounds_is_refused() {
        let refused = |image: ImageSpec| image.check().unwrap_err();
        assert!(refused(spec(&[])).message.contains("1 to 1000 rows"));
        assert!(refused(spec(&[""; MOST_ROWS + 1])).message.contains("rows"));
        let long = "x".repeat(MOST_ROW_CHARS + 1);
        assert!(refused(spec(&[&long])).message.contains("characters"));
        for control in ["a\x1b[31mb", "a\nb", "a\u{9b}b", "a\u{7f}"] {
            let error = refused(spec(&[control]));
            assert_eq!(error.code, Code::Failed);
            assert!(error.message.contains("control"), "{control:?}");
        }
        let run = |first, last| StyleRun {
            first,
            last,
            style: TextStyle::default(),
        };
        for runs in [vec![run(3, 2)], vec![run(0, MOST_COLUMNS)]] {
            let image = ImageSpec {
                rows: vec![CellRow {
                    text: "abc".to_owned(),
                    runs,
                }],
                ..ImageSpec::default()
            };
            assert!(refused(image).message.contains("run"));
        }
        let titled = |title: String| ImageSpec {
            title: Some(title),
            ..spec(&["a"])
        };
        assert!(
            refused(titled("t".repeat(MOST_TITLE_CHARS + 1)))
                .message
                .contains("title")
        );
        assert!(
            refused(titled("a\tb".to_owned()))
                .message
                .contains("control")
        );
        for padding in [-1.0, MOST_PADDING + 1.0, f32::NAN] {
            let padded = ImageSpec {
                padding,
                ..spec(&["a"])
            };
            assert!(refused(padded).message.contains("padding"));
        }
        for scale in [0.5, MOST_SCALE + 0.5, f32::NAN] {
            let scaled = ImageSpec {
                scale: Some(scale),
                ..spec(&["a"])
            };
            assert!(refused(scaled).message.contains("scale"));
        }
    }

    #[test]
    fn a_png_crosses_as_bytes_and_reads_back_of_bytes_a_text_or_numbers() {
        let image = Image::blank(2.0);
        let json = serde_json::to_string(&image).unwrap();
        assert!(json.starts_with("{\"png\":[137,80,78,71,"), "{json}");
        assert_eq!(serde_json::from_str::<Image>(&json).unwrap(), image);
        let png: Bytes = serde_json::from_str("\"PNG\"").unwrap();
        assert_eq!(png.0, b"PNG");
        assert_eq!(format!("{png:?}"), "Bytes(3)");
    }

    #[test]
    fn the_blank_image_is_a_png_of_one_clear_pixel() {
        let png = Image::blank(1.0).png.0;
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
        // Its header: 1 by 1, 8 bits a component of RGBA.
        assert_eq!(&png[12..16], b"IHDR");
        assert_eq!(&png[16..26], [0, 0, 0, 1, 0, 0, 0, 1, 8, 6]);
        assert_eq!(&png[png.len() - 8..png.len() - 4], b"IEND");
    }
}
