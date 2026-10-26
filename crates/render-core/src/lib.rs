//! Rendering interface shared by the GPU backends.
//!
//! Defines the [`Renderer`] trait that each backend implements, and the
//! [`DisplayList`] that it draws: a window's frame as rectangles, glyphs
//! placed anywhere for the chrome, and panes' grids of cells, each with its
//! background, its glyphs from the atlas and the lines that decorate it.
//! Positions and sizes are in the pixels of the window's content, from its
//! top left corner.

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

/// A texture of the glyph atlas.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Texture {
    /// One byte a pixel, coverage: glyphs that their color fills.
    Coverage,
    /// Four bytes a pixel, premultiplied RGBA: glyphs of their own colors.
    Color,
}

/// A region of a texture of the atlas, in its pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AtlasRegion {
    /// The texture.
    pub texture: Texture,
    /// Its column.
    pub x: u32,
    /// Its row.
    pub y: u32,
    /// Its width.
    pub width: u32,
    /// Its height.
    pub height: u32,
}

/// A change of a texture of the atlas, which the renderer makes before it
/// draws, in the order that the atlas gives them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TextureUpdate {
    /// The texture's side grew to `size`, and its old pixels go in its top
    /// left corner.
    Grown {
        /// The texture.
        texture: Texture,
        /// Its new side.
        size: u32,
    },
    /// Pixels to write into a region of the texture: its rows, from the top,
    /// of the texture's bytes a pixel.
    Upload {
        /// The region.
        region: AtlasRegion,
        /// The pixels.
        data: Vec<u8>,
    },
}

/// A rectangle, in pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Rect {
    /// Its left edge.
    pub x: i32,
    /// Its top edge.
    pub y: i32,
    /// Its width.
    pub width: u32,
    /// Its height.
    pub height: u32,
}

/// A glyph of a cell of a grid.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CellGlyph {
    /// The cell's column.
    pub column: u16,
    /// The cell's row.
    pub row: u16,
    /// Its pixels in the atlas.
    pub region: AtlasRegion,
    /// Where its left edge goes, from the cell's left edge.
    pub x: i32,
    /// Where its top edge goes, from the cell's top edge.
    pub y: i32,
    /// The color that fills a glyph of coverage; a glyph of its own colors
    /// ignores it.
    pub color: Color,
}

/// How a line across cells is drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineStyle {
    /// One line.
    Single,
    /// Two lines.
    Double,
    /// A wave.
    Curly,
    /// Dots.
    Dotted,
    /// Dashes.
    Dashed,
}

/// A line across cells of a row: an underline, a strikeout or an overline.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CellLine {
    /// The first cell's column.
    pub column: u16,
    /// The row.
    pub row: u16,
    /// The cells that it crosses.
    pub columns: u16,
    /// How it is drawn.
    pub style: LineStyle,
    /// Its first row of pixels, from the cell's top edge.
    pub top: u32,
    /// The rows of pixels that it takes.
    pub thickness: u32,
    /// Its color.
    pub color: Color,
}

/// A pane's grid of cells.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CellBatch {
    /// Where the grid's top left corner is.
    pub x: i32,
    /// Where the grid's top edge is.
    pub y: i32,
    /// Its columns.
    pub columns: u16,
    /// Its rows.
    pub rows: u16,
    /// A cell's width, in pixels.
    pub cell_width: u32,
    /// A cell's height, in pixels.
    pub cell_height: u32,
    /// Each cell's background, row by row, `columns` a row.
    pub backgrounds: Vec<Color>,
    /// The cells' glyphs.
    pub glyphs: Vec<CellGlyph>,
    /// The lines across cells.
    pub lines: Vec<CellLine>,
}

/// A glyph placed anywhere, such as the chrome's text.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlacedGlyph {
    /// Its pixels in the atlas.
    pub region: AtlasRegion,
    /// Where its left edge goes.
    pub x: i32,
    /// Where its top edge goes.
    pub y: i32,
    /// The color that fills a glyph of coverage.
    pub color: Color,
}

/// One drawing of a frame, over those before it.
#[derive(Clone, Debug, PartialEq)]
pub enum Item {
    /// A rectangle of one color.
    Rect {
        /// Where.
        rect: Rect,
        /// Its color.
        color: Color,
    },
    /// A pane's grid of cells.
    Cells(CellBatch),
    /// Glyphs placed anywhere.
    Glyphs(Vec<PlacedGlyph>),
    /// The drawings that follow, up to the matching [`Item::PopClip`], keep
    /// within the rectangle, and within the clips that hold it.
    PushClip(Rect),
    /// The end of the latest clip.
    PopClip,
}

/// A window's frame: its background, and what is drawn over it, in order.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DisplayList {
    /// The color of what nothing covers.
    pub background: Color,
    /// The drawings, from the first to the last, each over those before.
    pub items: Vec<Item>,
}

/// A GPU's backend: it keeps the atlas's textures, and draws display lists
/// into windows.
pub trait Renderer {
    /// What a frame is drawn into: a window's surface.
    type Target;
    /// What can fail.
    type Error;

    /// Makes the changes of the atlas's textures, in order.
    fn update_textures(&mut self, updates: &[TextureUpdate]) -> Result<(), Self::Error>;

    /// Draws `list` into `target`, and presents the frame. Returns whether
    /// it drew: not when the target has no drawable to give, as while its
    /// window is minimized.
    fn draw(&mut self, target: &Self::Target, list: &DisplayList) -> Result<bool, Self::Error>;
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
