//! Styles and colors.

/// A color in 8-bit RGB.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Rgb {
    /// Red.
    pub r: u8,
    /// Green.
    pub g: u8,
    /// Blue.
    pub b: u8,
}

impl Rgb {
    /// The color with these components.
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }
}

/// The id of a style in its page's style table.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct StyleId(u16);

impl StyleId {
    /// The default style, which every page has.
    pub const DEFAULT: Self = Self(0);

    pub(crate) const fn new(id: u16) -> Self {
        Self(id)
    }

    pub(crate) const fn get(self) -> u16 {
        self.0
    }
}
