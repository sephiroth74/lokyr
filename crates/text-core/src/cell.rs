//! The cell of the grid: its size in pixels, and where the baseline and the
//! lines that decorate text fall in it, from a face's metrics and the
//! configuration's factors.

use crate::backend::FaceMetrics;
use crate::config::FontConfig;

/// A cell's size, and the rows of its baseline and of its lines, in pixels
/// from its top left corner.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CellMetrics {
    /// Its width.
    pub width: u32,
    /// Its height.
    pub height: u32,
    /// The row of the baseline, from the top.
    pub baseline: u32,
    /// The first row of the underline, from the top.
    pub underline_top: u32,
    /// The rows that the underline takes.
    pub underline_thickness: u32,
    /// The first row of the strikeout, from the top.
    pub strikeout_top: u32,
    /// The rows that the strikeout takes.
    pub strikeout_thickness: u32,
}

impl CellMetrics {
    /// The cell of a face that measures `face`, at the size that the face
    /// was measured at, with the factors of `config`.
    ///
    /// The width is the advance, and the height the line's, ascent, descent
    /// and leading, each times its factor and rounded up, so that no glyph
    /// is cut: a factor less than one, for a closer grid, still rounds up.
    /// The glyphs' box, ascent over descent, stands in the middle of the
    /// height, so that leading and a line height above the face's go half
    /// above and half below; the baseline is the nearest row to where the
    /// box puts it. The lines take their thickness rounded, of one row at
    /// least, centred where the face puts them, and kept within the cell.
    pub fn new(face: &FaceMetrics, config: &FontConfig) -> Self {
        let width = up(face.advance * config.cell_width.get());
        let height = up((face.ascent + face.descent + face.leading) * config.line_height.get());
        let padding = (height as f32 - (face.ascent + face.descent)) / 2.0;
        let baseline = clamp(padding + face.ascent, 1, height);
        let line = |middle: f32, thickness: f32| {
            let thickness = rows(thickness).min(height);
            let top = baseline as f32 - middle - thickness as f32 / 2.0;
            (clamp(top, 0, height - thickness), thickness)
        };
        let (underline_top, underline_thickness) =
            line(face.underline_position, face.underline_thickness);
        let (strikeout_top, strikeout_thickness) =
            line(face.strikeout_position, face.strikeout_thickness);
        Self {
            width,
            height,
            baseline,
            underline_top,
            underline_thickness,
            strikeout_top,
            strikeout_thickness,
        }
    }
}

/// `length`, rounded up to a whole pixel, of one pixel at least. It rounds
/// to a 64th first, so that a length that a float's error puts a hair past
/// a pixel stays that pixel.
fn up(length: f32) -> u32 {
    // A cast from a float saturates, and takes NaN to 0.
    ((length * 64.0).round() / 64.0).ceil().max(1.0) as u32
}

/// The rows of a line of `thickness`, rounded, one at least.
fn rows(thickness: f32) -> u32 {
    thickness.round().max(1.0) as u32
}

/// The nearest row to `position`, within `min` and `max`.
fn clamp(position: f32, min: u32, max: u32) -> u32 {
    (position.round().max(0.0) as u32).clamp(min, max)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Factor;

    /// Menlo's metrics at 13 pixels, as CoreText gives them.
    fn menlo() -> FaceMetrics {
        FaceMetrics {
            ascent: 12.066_895,
            descent: 3.065_918,
            leading: 0.0,
            advance: 7.826_66,
            underline_position: -1.111_08,
            underline_thickness: 0.571_289,
            strikeout_position: 3.554_688,
            strikeout_thickness: 0.571_289,
        }
    }

    fn factors(line_height: f64, cell_width: f64) -> FontConfig {
        FontConfig {
            line_height: Factor::try_from(line_height).unwrap(),
            cell_width: Factor::try_from(cell_width).unwrap(),
            ..FontConfig::default()
        }
    }

    #[test]
    fn a_cell_rounds_the_face_up() {
        let cell = CellMetrics::new(&menlo(), &FontConfig::default());
        assert_eq!((cell.width, cell.height), (8, 16));
        // The box of 15.13 pixels in 16: 0.43 above it, which puts the
        // baseline at 12.5, whose nearest row is 13.
        assert_eq!(cell.baseline, 13);
        assert_eq!((cell.underline_top, cell.underline_thickness), (14, 1));
        assert_eq!((cell.strikeout_top, cell.strikeout_thickness), (9, 1));
    }

    #[test]
    fn the_factors_widen_and_heighten_the_cell() {
        let cell = CellMetrics::new(&menlo(), &factors(1.5, 1.25));
        assert_eq!((cell.width, cell.height), (10, 23));
        // The 7.87 pixels more go half above and half below.
        assert_eq!(cell.baseline, 16);
        let cell = CellMetrics::new(&menlo(), &factors(0.5, 0.5));
        assert_eq!((cell.width, cell.height), (4, 8));
        assert!(cell.baseline >= 1 && cell.baseline <= 8);
        assert!(cell.underline_top + cell.underline_thickness <= cell.height);
    }

    #[test]
    fn a_float_s_error_does_not_take_a_pixel_more() {
        let face = FaceMetrics {
            advance: 7.000_001,
            ascent: 12.0,
            descent: 3.999_999,
            ..menlo()
        };
        let cell = CellMetrics::new(&face, &FontConfig::default());
        assert_eq!((cell.width, cell.height), (7, 16));
    }

    #[test]
    fn lines_stay_within_the_cell() {
        let face = FaceMetrics {
            underline_position: -40.0,
            underline_thickness: 3.2,
            strikeout_position: 40.0,
            strikeout_thickness: 0.1,
            ..menlo()
        };
        let cell = CellMetrics::new(&face, &FontConfig::default());
        assert_eq!((cell.underline_top, cell.underline_thickness), (13, 3));
        assert_eq!((cell.strikeout_top, cell.strikeout_thickness), (0, 1));
    }

    #[test]
    fn a_face_of_nothing_still_makes_a_pixel() {
        let cell = CellMetrics::new(&FaceMetrics::default(), &FontConfig::default());
        assert_eq!((cell.width, cell.height), (1, 1));
        assert_eq!(cell.baseline, 1);
        assert_eq!(cell.underline_top + cell.underline_thickness, 1);
    }
}
