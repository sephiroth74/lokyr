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
    /// The height, in points, of the titlebar over the content's top, which
    /// the content runs under; 0 without one.
    pub bar: f64,
    /// The room, in points, at the titlebar's left that the window's
    /// buttons take, with a margin after them as wide as the one before
    /// them; 0 without them, as in full screen.
    pub buttons: f64,
}

impl WindowSize {
    /// The size of `points` at `scale`, with its pixels rounded to the
    /// nearest, without a titlebar over it.
    pub fn new(points: Size, scale: f64) -> Self {
        Self {
            points,
            pixels: PixelSize {
                width: to_pixels(points.width, scale),
                height: to_pixels(points.height, scale),
            },
            scale,
            bar: 0.0,
            buttons: 0.0,
        }
    }

    /// The size, with a titlebar `bar` points high over its top.
    pub fn with_bar(self, bar: f64) -> Self {
        Self { bar, ..self }
    }

    /// The size, with the window's buttons `buttons` points across the
    /// titlebar's left.
    pub fn with_buttons(self, buttons: f64) -> Self {
        Self { buttons, ..self }
    }

    /// The titlebar's height in pixels, rounded to the nearest.
    pub fn bar_pixels(&self) -> u32 {
        to_pixels(self.bar, self.scale)
    }
}

/// A rectangle of a window's content, in points from its top left corner.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ContentRect {
    /// Its top left corner.
    pub origin: Point,
    /// Its size.
    pub size: Size,
}

impl ContentRect {
    /// Whether it holds `point`, its left and top edges in, its right and
    /// bottom ones out.
    pub fn contains(&self, point: Point) -> bool {
        point.x >= self.origin.x
            && point.y >= self.origin.y
            && point.x < self.origin.x + self.size.width
            && point.y < self.origin.y + self.size.height
    }
}

/// What a window's titlebar holds, over its content's top, which the
/// content runs under.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Titlebar {
    /// The system's, of the window's title.
    #[default]
    Titled,
    /// Taller, of no title, for a tab bar that the content draws in it: on
    /// the Mac, a compact toolbar's, empty, on which the window's buttons
    /// stand centred. In full screen, the system's titlebar slides over the
    /// content's top as the pointer reaches the screen's top, and leaves no
    /// room of its own.
    Tabs,
}

/// The pixels of a length in points, rounded; a negative length, or one
/// that is not a number, is none.
fn to_pixels(points: f64, scale: f64) -> u32 {
    // A cast from a float saturates, and takes NaN to 0.
    (points * scale).round() as u32
}
