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

/// How far below a window's top its titlebar's middle lies, at most, which
/// keeps the window where it is while it lies on a screen.
const GRIP: f64 = 14.0;

/// A point on the screens, in points, from the top left corner of the menu
/// bar's screen, rightwards and downwards, as the screens lie on one plane.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ScreenPoint {
    /// From the left, rightwards.
    pub x: f64,
    /// From the top, downwards.
    pub y: f64,
}

/// A rectangle on the screens, in points: its origin is its top left
/// corner, from the top left corner of the menu bar's screen, rightwards
/// and downwards.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ScreenRect {
    /// From the left, rightwards.
    pub x: f64,
    /// From the top, downwards.
    pub y: f64,
    /// Its width.
    pub width: f64,
    /// Its height.
    pub height: f64,
}

impl ScreenRect {
    /// Whether it and `other` share some area, more than an edge.
    pub fn overlaps(&self, other: &ScreenRect) -> bool {
        self.x < other.x + other.width
            && other.x < self.x + self.width
            && self.y < other.y + other.height
            && other.y < self.y + self.height
    }

    /// Whether it holds `point`, its left and top edges in, its right and
    /// bottom ones out.
    pub fn contains(&self, point: ScreenPoint) -> bool {
        point.x >= self.x
            && point.y >= self.y
            && point.x < self.x + self.width
            && point.y < self.y + self.height
    }

    /// Its top left corner.
    pub fn origin(&self) -> ScreenPoint {
        ScreenPoint {
            x: self.x,
            y: self.y,
        }
    }

    /// Where a window of this frame goes on the screens whose areas for
    /// windows are `areas`: within the area that its titlebar's middle lies
    /// on, or, where it lies on none, as a screen that held it went, the
    /// area nearest to its middle; of its size within that area, as near
    /// to here as it fits. Here, without areas.
    pub fn onto(&self, areas: &[ScreenRect]) -> ScreenRect {
        // The middle of the titlebar, by which the window is dragged.
        let grip = ScreenPoint {
            x: self.x + self.width / 2.0,
            y: self.y + (self.height / 2.0).min(GRIP),
        };
        let center = ScreenPoint {
            x: self.x + self.width / 2.0,
            y: self.y + self.height / 2.0,
        };
        let Some(area) = areas.iter().find(|area| area.contains(grip)).or_else(|| {
            areas
                .iter()
                .min_by(|one, other| one.distance(center).total_cmp(&other.distance(center)))
        }) else {
            return *self;
        };
        let width = self.width.min(area.width);
        let height = self.height.min(area.height);
        ScreenRect {
            x: self.x.clamp(area.x, area.x + area.width - width),
            y: self.y.clamp(area.y, area.y + area.height - height),
            width,
            height,
        }
    }

    /// How far `point` is from it: 0 within it.
    fn distance(&self, point: ScreenPoint) -> f64 {
        let x = (self.x - point.x)
            .max(point.x - (self.x + self.width))
            .max(0.0);
        let y = (self.y - point.y)
            .max(point.y - (self.y + self.height))
            .max(0.0);
        x.hypot(y)
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
    /// The system's, of its height and its buttons, of no title, which the
    /// content draws in its place: the window keeps its title for its menu
    /// and the system's lists of windows.
    Untitled,
}

/// The pixels of a length in points, rounded; a negative length, or one
/// that is not a number, is none.
fn to_pixels(points: f64, scale: f64) -> u32 {
    // A cast from a float saturates, and takes NaN to 0.
    (points * scale).round() as u32
}
