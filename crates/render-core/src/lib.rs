//! Rendering interface shared by the GPU backends.
//!
//! Defines the `Renderer` trait that each backend implements, the
//! display-list types it consumes, and the shader sources.

#![forbid(unsafe_code)]

/// An opaque color of the sRGB space, by its encoded components, from 0 to
/// 1, as a theme gives them.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Color {
    /// Red.
    pub red: f32,
    /// Green.
    pub green: f32,
    /// Blue.
    pub blue: f32,
}

impl Color {
    /// The color of 8-bit components, as `#rrggbb` writes them.
    pub const fn from_rgb8(red: u8, green: u8, blue: u8) -> Self {
        Self {
            red: red as f32 / 255.0,
            green: green as f32 / 255.0,
            blue: blue as f32 / 255.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_color_of_bytes_spans_zero_to_one() {
        assert_eq!(
            Color::from_rgb8(0, 255, 51),
            Color {
                red: 0.0,
                green: 1.0,
                blue: 0.2
            }
        );
    }
}
