//! The images of cells that the host renders: rows of text of their
//! styles, in the theme's colors and the terminal's font, in a frame as a
//! window's, which the host draws offscreen with the terminal's own
//! renderer and gives back as a PNG.

use serde::{Deserialize, Serialize};

use crate::command::{Code, Error};
use crate::host::{Bytes, StyleRun};

/// The most rows that an image draws: the rows past them are left out.
pub const MOST_ROWS: usize = 1000;

/// The most columns that an image draws: the cells of a row past them are
/// left out.
pub const MOST_COLUMNS: u16 = 1000;

/// The most characters of a row's text: a column's cell, its combining
/// marks too; the characters past them are left out.
pub const MOST_ROW_CHARS: usize = 4 * MOST_COLUMNS as usize;

/// The most characters of an image's title: a longer one is shortened, of
/// an ellipsis.
pub const MOST_TITLE_CHARS: usize = 256;

/// The most points of the room around an image's window.
pub const MOST_PADDING: f32 = 256.0;

/// The least pixels of a point of an image.
pub const LEAST_SCALE: f32 = 1.0;

/// The most pixels of a point of an image.
pub const MOST_SCALE: f32 = 4.0;

/// The least pixels of a point that an image is drawn at, of a scale
/// lowered until it is within [`MOST_SIDE`] and [`MOST_PIXELS`]: an image
/// past them even so is refused.
pub const LEAST_FITTED_SCALE: f32 = 0.5;

/// The most pixels of an image's side, as a GPU draws a texture.
pub const MOST_SIDE: u32 = 16_384;

/// The most pixels of an image, its width by its height: a screen of 5K
/// whole, whose PNG of text a Luau plugin can still save and copy.
pub const MOST_PIXELS: u64 = 16_000_000;

/// The most pixels of the images that the host keeps of a plugin's, all
/// together, past which it lets the oldest go.
pub const MOST_KEPT_PIXELS: u64 = MOST_PIXELS;

/// The pixels of the images that a plugin may have drawn at once, which
/// [`IMAGE_RATE`] gives back in time.
pub const IMAGE_BUDGET: u64 = 2 * MOST_PIXELS;

/// The pixels a second given back to a plugin's [`IMAGE_BUDGET`].
pub const IMAGE_RATE: u64 = 4_000_000;

/// The pixels that an image of cells costs at the least, a cell of 8 by 16
/// pixels, as the work of laying its cells out, though it is refused or
/// drawn smaller.
pub const NOMINAL_CELL: (u32, u32) = (8, 16);

/// The most images that one call renders, each of the main thread's time.
pub const MOST_IMAGES: usize = 4;

/// The most images that the host keeps of a plugin's, for its views' nodes
/// of them, past which it lets the oldest go.
pub const MOST_KEPT: usize = 4;

/// The message of the error of an image past [`MOST_IMAGES`] in a call,
/// which every host gives alike.
pub const IMAGES_BOUND: &str = "a call renders 4 images at most";

/// The message of the error of an image that a plugin asks for past its
/// [`IMAGE_BUDGET`], which every host gives alike.
pub const IMAGES_BUSY: &str = "a plugin's images take 4 million pixels a second at most";

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
    /// The rows, from the top, one at least, of which [`MOST_ROWS`] show.
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
    /// Whether the host keeps the image, which a view's node shows by the
    /// id that it answers, until the plugin lets it go, keeps
    /// [`MOST_KEPT`] others after it, or stops.
    #[serde(default)]
    pub keep: bool,
    /// The least columns of the window's cells, [`MOST_COLUMNS`] at most:
    /// a narrower text leaves the cells at its right empty.
    #[serde(default)]
    pub min_columns: Option<u16>,
    /// The least rows of the window's cells, [`MOST_ROWS`] at most: fewer
    /// rows leave those below them empty.
    #[serde(default)]
    pub min_rows: Option<u16>,
}

impl ImageSpec {
    /// Whether the window has a bar: of its buttons, or of its title.
    pub fn has_bar(&self) -> bool {
        self.buttons || self.title.is_some()
    }

    /// The image within its bounds, as a host draws it: the rows past
    /// [`MOST_ROWS`], a row's characters past [`MOST_ROW_CHARS`] and its
    /// runs from [`MOST_COLUMNS`] left out, a run that goes past them cut
    /// there, and a title of more than [`MOST_TITLE_CHARS`] shortened, of
    /// an ellipsis.
    #[must_use]
    pub fn bounded(mut self) -> Self {
        self.rows.truncate(MOST_ROWS);
        for row in &mut self.rows {
            if let Some((cut, _)) = row.text.char_indices().nth(MOST_ROW_CHARS) {
                row.text.truncate(cut);
            }
            row.runs.retain(|run| run.first < MOST_COLUMNS);
            row.runs.truncate(usize::from(MOST_COLUMNS));
            for run in &mut row.runs {
                run.last = run.last.min(MOST_COLUMNS - 1);
            }
        }
        if let Some(title) = &mut self.title
            && let Some((cut, _)) = title.char_indices().nth(MOST_TITLE_CHARS - 1)
            && title[cut..].chars().nth(1).is_some()
        {
            title.truncate(cut);
            title.push('…');
        }
        self
    }

    /// The image as a host takes it, of [`ImageSpec::bounded`]'s bounds,
    /// or the error that tells why it does not: of no row, of a control
    /// character, of a run backwards, of a least size, room or a scale out
    /// of their ranges.
    pub fn check(&self) -> Result<(), Error> {
        let refused = |message: String| Err(Error::new(Code::Failed, message));
        if self.rows.is_empty() {
            return refused("an image draws a row at least".to_owned());
        }
        for row in &self.rows {
            if row.text.chars().any(char::is_control) {
                return refused("a row of an image has no control character".to_owned());
            }
            if row.runs.iter().any(|run| run.first > run.last) {
                return refused(
                    "a run of an image's cells goes from a column to one after it".to_owned(),
                );
            }
        }
        if let Some(title) = &self.title
            && title.chars().any(char::is_control)
        {
            return refused("an image's title has no control character".to_owned());
        }
        if self
            .min_columns
            .is_some_and(|columns| columns > MOST_COLUMNS)
            || self
                .min_rows
                .is_some_and(|rows| usize::from(rows) > MOST_ROWS)
        {
            return refused(format!(
                "an image's least size is {MOST_COLUMNS} columns and {MOST_ROWS} rows at most"
            ));
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

    /// The columns and the rows that it lays out at the most: its widest
    /// row's columns, of its text's characters and its runs, and its rows,
    /// of its least size too, within its bounds.
    fn extent(&self) -> (usize, usize) {
        let columns = self
            .rows
            .iter()
            .map(|row| {
                let text = row.text.chars().count().min(usize::from(MOST_COLUMNS));
                let runs = row.runs.iter().map(|run| usize::from(run.last) + 1).max();
                text.max(runs.unwrap_or(0))
            })
            .max()
            .unwrap_or(0)
            .max(usize::from(self.min_columns.unwrap_or(0)))
            .min(usize::from(MOST_COLUMNS));
        let rows = self
            .rows
            .len()
            .max(usize::from(self.min_rows.unwrap_or(0)))
            .min(MOST_ROWS);
        (columns, rows)
    }

    /// The cells that it lays out at the most, of [`ImageSpec::extent`]:
    /// what an image costs at the least, of [`NOMINAL_CELL`] each, though
    /// it is refused.
    pub fn cells(&self) -> u64 {
        let (columns, rows) = self.extent();
        (columns * rows) as u64
    }

    /// The numbers that it carries of its own, its padding and its scale.
    pub(crate) fn numbers(&self) -> Vec<f64> {
        let mut numbers = vec![f64::from(self.padding)];
        numbers.extend(self.scale.map(f64::from));
        numbers
    }
}

/// What a plugin's images may still draw: of [`IMAGE_BUDGET`] pixels at
/// once, given back at [`IMAGE_RATE`] a second, each image's cost taken
/// after it is drawn, its pixels or its cells', which may leave it owing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Budget {
    /// The pixels left, below zero of a plugin that owes them.
    left: f64,
}

impl Default for Budget {
    fn default() -> Self {
        Self {
            left: IMAGE_BUDGET as f64,
        }
    }
}

impl Budget {
    /// Gives back the pixels of `elapsed`, up to the whole budget.
    pub fn refill(&mut self, elapsed: std::time::Duration) {
        self.left =
            (self.left + elapsed.as_secs_f64() * IMAGE_RATE as f64).min(IMAGE_BUDGET as f64);
    }

    /// Whether another image may be drawn, of pixels left; the error that
    /// tells it may not.
    pub fn check(&self) -> Result<(), Error> {
        if self.left > 0.0 {
            Ok(())
        } else {
            Err(Error::new(Code::Failed, IMAGES_BUSY))
        }
    }

    /// Takes the cost of `image`, drawn as `drawn` or refused: its pixels,
    /// or its cells' of [`NOMINAL_CELL`] where they are more.
    pub fn spend(&mut self, image: &ImageSpec, drawn: Option<&Image>) {
        let cells = image.cells() * u64::from(NOMINAL_CELL.0 * NOMINAL_CELL.1);
        let pixels = drawn.map_or(0, |drawn| u64::from(drawn.width) * u64::from(drawn.height));
        self.left -= pixels.max(cells) as f64;
    }
}

/// The image that a host of no drawing gives of `image`, as the fake host
/// does: a clear pixel, at the scale asked for, or `scale`, lowered as the
/// app lowers it for an image past [`MOST_SIDE`] or [`MOST_PIXELS`], of
/// cells of [`NOMINAL_CELL`] points and the window's frame; the error of
/// one past them at [`LEAST_FITTED_SCALE`].
pub fn nominal(image: &ImageSpec, scale: f32) -> Result<Image, Error> {
    let asked = image.scale.unwrap_or(scale);
    let (columns, rows) = image.extent();
    let (columns, rows) = (columns as f64, rows as f64);
    // The cells, the room around them and the bar, in points.
    let frame = 2.0 * f64::from(image.padding) + 2.0 * 8.0;
    let width = columns * f64::from(NOMINAL_CELL.0) + frame;
    let height =
        rows * f64::from(NOMINAL_CELL.1) + frame + if image.has_bar() { 28.0 } else { 0.0 };
    let fits = |scale: f64| {
        let (width, height) = ((width * scale).ceil(), (height * scale).ceil());
        width <= f64::from(MOST_SIDE)
            && height <= f64::from(MOST_SIDE)
            && width * height <= MOST_PIXELS as f64
    };
    let mut drawn = f64::from(asked);
    while !fits(drawn) {
        if drawn <= f64::from(LEAST_FITTED_SCALE) {
            return Err(Error::new(Code::Unavailable, past_the_most(width, height)));
        }
        drawn = (drawn * 0.9).max(f64::from(LEAST_FITTED_SCALE));
    }
    Ok(Image::blank(drawn as f32))
}

/// The error of an image of `width` by `height` points past the most at
/// [`LEAST_FITTED_SCALE`].
fn past_the_most(width: f64, height: f64) -> String {
    format!(
        "an image of {width:.0} by {height:.0} points is past the most, {MOST_SIDE} pixels a side \
         and {MOST_PIXELS} pixels, at a scale of {LEAST_FITTED_SCALE}"
    )
}

/// An image that the host rendered: a PNG, of its size in pixels, and the
/// pixels of a point that it took, which its PNG tells as its resolution:
/// the scale asked for, or lower, of an image that would be past
/// [`MOST_SIDE`] or [`MOST_PIXELS`] at it.
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
    /// The id that a view's node shows it by, of an image that the host
    /// keeps.
    #[serde(default)]
    pub id: Option<u64>,
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
            id: None,
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
    fn an_image_past_its_bounds_is_cut_to_them() {
        let run = |first, last| StyleRun {
            first,
            last,
            style: TextStyle::default(),
        };
        let long = "x".repeat(MOST_ROW_CHARS + 10);
        let mut rows = vec![
            CellRow {
                text: long,
                runs: vec![
                    run(0, MOST_COLUMNS + 5),
                    run(MOST_COLUMNS, MOST_COLUMNS + 1)
                ],
            };
            MOST_ROWS + 3
        ];
        rows[1].runs = vec![run(1, 2); usize::from(MOST_COLUMNS) + 1];
        let image = ImageSpec {
            rows,
            title: Some("t".repeat(MOST_TITLE_CHARS + 40)),
            ..ImageSpec::default()
        }
        .bounded();
        assert_eq!(image.check(), Ok(()));
        assert_eq!(image.rows.len(), MOST_ROWS);
        assert_eq!(image.rows[0].text.chars().count(), MOST_ROW_CHARS);
        assert_eq!(image.rows[0].runs, [run(0, MOST_COLUMNS - 1)]);
        assert_eq!(image.rows[1].runs.len(), usize::from(MOST_COLUMNS));
        let title = image.title.unwrap();
        assert_eq!(title.chars().count(), MOST_TITLE_CHARS);
        assert!(title.ends_with('…'));
        // A title of the most stays whole.
        let whole = "t".repeat(MOST_TITLE_CHARS);
        let titled = ImageSpec {
            title: Some(whole.clone()),
            ..spec(&["a"])
        }
        .bounded();
        assert_eq!(titled.title, Some(whole));
    }

    #[test]
    fn an_image_costs_its_pixels_or_its_cells_and_the_budget_comes_back_in_time() {
        let mut budget = Budget::default();
        assert_eq!(budget.check(), Ok(()));
        let big = ImageSpec {
            min_columns: Some(MOST_COLUMNS),
            min_rows: Some(1000),
            ..spec(&["a"])
        };
        assert_eq!(big.cells(), 1_000_000);
        // Refused, its cells cost all the same: 128 million pixels.
        budget.spend(&big, None);
        let error = budget.check().unwrap_err();
        assert_eq!(error.message, IMAGES_BUSY);
        // 128 million owed less 32 million at once: 24 seconds and more.
        budget.refill(std::time::Duration::from_secs(24));
        assert!(budget.check().is_err());
        budget.refill(std::time::Duration::from_secs(1));
        assert_eq!(budget.check(), Ok(()));
        // A small image costs its pixels, drawn bigger than its cells.
        let mut budget = Budget::default();
        let drawn = Image {
            width: 4000,
            height: 4000,
            ..Image::blank(2.0)
        };
        budget.spend(&spec(&["a"]), Some(&drawn));
        budget.spend(&spec(&["a"]), Some(&drawn));
        assert!(budget.check().is_err(), "32 million spent");
        budget.refill(std::time::Duration::from_secs(3600));
        assert_eq!(budget, Budget::default(), "never more than the whole");
    }

    #[test]
    fn a_host_of_no_drawing_lowers_the_scale_of_an_image_past_the_most() {
        let tall = spec(&["x"; MOST_ROWS]);
        // 1000 rows of 16 points at 2: 32 000 pixels, lowered.
        let drawn = nominal(&tall, 2.0).unwrap();
        assert!(
            drawn.scale < 1.03 && drawn.scale >= LEAST_FITTED_SCALE,
            "{}",
            drawn.scale
        );
        assert_eq!(nominal(&spec(&["ls"]), 2.0).unwrap().scale, 2.0);
        let huge = ImageSpec {
            min_columns: Some(MOST_COLUMNS),
            ..tall
        };
        assert!(
            nominal(&huge, 2.0)
                .unwrap_err()
                .message
                .contains("past the most")
        );
    }

    #[test]
    fn an_image_out_of_its_bounds_is_refused() {
        let refused = |image: ImageSpec| image.check().unwrap_err();
        assert!(refused(spec(&[])).message.contains("a row at least"));
        for control in ["a\x1b[31mb", "a\nb", "a\u{9b}b", "a\u{7f}"] {
            let error = refused(spec(&[control]));
            assert_eq!(error.code, Code::Failed);
            assert!(error.message.contains("control"), "{control:?}");
        }
        let backwards = ImageSpec {
            rows: vec![CellRow {
                text: "abc".to_owned(),
                runs: vec![StyleRun {
                    first: 3,
                    last: 2,
                    style: TextStyle::default(),
                }],
            }],
            ..ImageSpec::default()
        };
        assert!(refused(backwards).message.contains("run"));
        let titled = |title: String| ImageSpec {
            title: Some(title),
            ..spec(&["a"])
        };
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
        let wide = ImageSpec {
            min_columns: Some(MOST_COLUMNS + 1),
            ..spec(&["a"])
        };
        assert!(refused(wide).message.contains("least size"));
        let tall = ImageSpec {
            min_rows: Some(1001),
            ..spec(&["a"])
        };
        assert!(refused(tall).message.contains("least size"));
        let least = ImageSpec {
            min_columns: Some(MOST_COLUMNS),
            min_rows: Some(1000),
            ..spec(&["a"])
        };
        assert_eq!(least.check(), Ok(()));
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
