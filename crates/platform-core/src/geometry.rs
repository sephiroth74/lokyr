//! Sizes and positions, in points and in pixels.

/// A size in points, the window system's unit: a point is a pixel at a
/// scale of 1, and two pixels across at a scale of 2.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Size {
    /// The width.
    pub width: f64,
    /// The height.
    pub height: f64,
}

/// A size in the display's pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PixelSize {
    /// The width.
    pub width: u32,
    /// The height.
    pub height: u32,
}

/// A position in points, from the top left corner of a window's content.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Point {
    /// From the left edge, rightwards.
    pub x: f64,
    /// From the top edge, downwards.
    pub y: f64,
}

/// The size of a window's content, in points and in pixels, and the scale
/// between the two.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WindowSize {
    /// The size in points.
    pub points: Size,
    /// The size in pixels, which a drawable of the window takes.
    pub pixels: PixelSize,
    /// The pixels in a point, along each axis: 1 or 2 on the Mac.
    pub scale: f64,
}

impl WindowSize {
    /// The size of `points` at `scale`, with its pixels rounded to the
    /// nearest.
    pub fn new(points: Size, scale: f64) -> Self {
        Self {
            points,
            pixels: PixelSize {
                width: to_pixels(points.width, scale),
                height: to_pixels(points.height, scale),
            },
            scale,
        }
    }
}

/// The pixels of a length in points, rounded; a negative length, or one
/// that is not a number, is none.
fn to_pixels(points: f64, scale: f64) -> u32 {
    // A cast from a float saturates, and takes NaN to 0.
    (points * scale).round() as u32
}
