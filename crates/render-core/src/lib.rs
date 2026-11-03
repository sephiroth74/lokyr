//! Rendering interface shared by the GPU backends.
//!
//! Defines the [`Renderer`] trait that each backend implements, and the
//! [`DisplayList`] that it draws: a window's frame as rectangles, shapes of
//! rounded corners, borders and shadows, and glyphs placed anywhere for the
//! chrome, and panes' grids of cells, each with its background, its glyphs
//! from the atlas and the lines that decorate it.
//! Positions and sizes are in the pixels of the window's content, from its
//! top left corner.

#![forbid(unsafe_code)]

use std::time::Duration;

/// A color of the sRGB space, by its encoded components, from 0 to 1, as a
/// theme gives them, and its opacity; opaque black by default.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Color {
    /// Red.
    pub red: f32,
    /// Green.
    pub green: f32,
    /// Blue.
    pub blue: f32,
    /// The opacity, from 0, clear, to 1, opaque: what the color covers
    /// shows through it.
    pub alpha: f32,
}

impl Default for Color {
    fn default() -> Self {
        Self::BLACK
    }
}

impl Color {
    /// The opaque color of 8-bit components, as `#rrggbb` writes them.
    pub const fn from_rgb8(red: u8, green: u8, blue: u8) -> Self {
        Self {
            red: red as f32 / 255.0,
            green: green as f32 / 255.0,
            blue: blue as f32 / 255.0,
            alpha: 1.0,
        }
    }

    /// Opaque black.
    pub const BLACK: Self = Self::from_rgb8(0, 0, 0);

    /// Nothing: black, of no opacity.
    pub const CLEAR: Self = Self::BLACK.with_alpha(0.0);

    /// The color, with the opacity `alpha`.
    #[must_use]
    pub const fn with_alpha(self, alpha: f32) -> Self {
        Self { alpha, ..self }
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
    /// Whether its coverage was drawn of the system's font smoothing, which
    /// takes the system's weight where its batch asks for it.
    pub smoothed: bool,
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

impl CellLine {
    /// The rows of pixels that the line's pattern takes, from the cell's
    /// top edge, in a cell of `height` rows: its first, and how many. A
    /// single, dotted or dashed line takes its thickness; a double one two
    /// strokes of it with a gap of it between, down from its top; a wave
    /// swings about the line's middle, by its thickness and at least a
    /// pixel and a half, so that a thin one still reads as a wave. Each
    /// keeps within the cell.
    pub fn band(&self, height: u32) -> (u32, u32) {
        let height = height.max(1);
        let thickness = self.thickness.clamp(1, height);
        let (top, rows) = match self.style {
            LineStyle::Single | LineStyle::Dotted | LineStyle::Dashed => (self.top, thickness),
            LineStyle::Double => (self.top, thickness * 3),
            LineStyle::Curly => {
                let rows = (thickness * 3).max(4);
                ((self.top + thickness / 2).saturating_sub(rows / 2), rows)
            }
        };
        let rows = rows.min(height);
        (top.min(height - rows), rows)
    }
}

/// The shape of a grid's cursor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CursorShape {
    /// A block over the cell, under its glyph.
    Block,
    /// A bar at the cell's left edge.
    Bar,
    /// A line at the cell's bottom edge.
    Underline,
    /// The edges of a block, as a window without the focus shows it.
    Hollow,
}

/// The cursor of a grid.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CellCursor {
    /// The first cell's column.
    pub column: u16,
    /// The row.
    pub row: u16,
    /// The cells that it covers: two over a wide character.
    pub columns: u16,
    /// Its shape.
    pub shape: CursorShape,
    /// The pixels that a bar, an underline or a hollow block's edges take.
    pub thickness: u32,
    /// Its color.
    pub color: Color,
}

impl CellCursor {
    /// The rectangles that draw it, in a grid whose top left corner is at
    /// `x` and `y`, of cells of `width` by `height`.
    pub fn rects(&self, x: i32, y: i32, width: u32, height: u32) -> Vec<Rect> {
        let left = x + i32::from(self.column) * width as i32;
        let top = y + i32::from(self.row) * height as i32;
        let width = u32::from(self.columns) * width;
        let thickness = self.thickness.clamp(1, width.min(height).max(1));
        let rect = |x: i32, y: i32, width: u32, height: u32| Rect {
            x,
            y,
            width,
            height,
        };
        match self.shape {
            CursorShape::Block => vec![rect(left, top, width, height)],
            CursorShape::Bar => vec![rect(left, top, thickness, height)],
            CursorShape::Underline => vec![rect(
                left,
                top + (height - thickness) as i32,
                width,
                thickness,
            )],
            CursorShape::Hollow => {
                let inner = height.saturating_sub(thickness * 2);
                vec![
                    rect(left, top, width, thickness),
                    rect(left, top + (height - thickness) as i32, width, thickness),
                    rect(left, top + thickness as i32, thickness, inner),
                    rect(
                        left + (width - thickness) as i32,
                        top + thickness as i32,
                        thickness,
                        inner,
                    ),
                ]
            }
        }
    }
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
    /// Each cell's background, row by row, `columns` a row, over what is
    /// under the grid: a clear one shows the list's background.
    pub backgrounds: Vec<Color>,
    /// The cells' glyphs.
    pub glyphs: Vec<CellGlyph>,
    /// The lines across cells.
    pub lines: Vec<CellLine>,
    /// The cursor, when it shows: a block goes over the backgrounds and
    /// under the glyphs, the other shapes over the glyphs.
    pub cursor: Option<CellCursor>,
    /// Whether its glyphs of coverage drawn of the font smoothing take the
    /// weight of the system's text: dark text on a light background
    /// thinner than its coverage blended as it is drawn.
    pub system_weight: bool,
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
    /// Whether its coverage takes the weight of the system's text: of a
    /// glyph drawn of the font smoothing, of text that takes the weight, as
    /// the interface's always does.
    pub weight: bool,
}

/// A shape of the interface: a rectangle of rounded corners, filled to its
/// edges under its border, over its shadow, as a panel or a button; none of
/// no width or height.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shape {
    /// Its rectangle, the border's outer edges.
    pub rect: Rect,
    /// The radius of its corners, in pixels: 0 for square ones, and at most
    /// half its shorter side.
    pub radius: f32,
    /// What fills it, under its border too; [`Color::CLEAR`] for nothing.
    pub fill: Color,
    /// Its border's width, in pixels, inside its edges: 0 for none.
    pub border: f32,
    /// Its border's color.
    pub border_color: Color,
    /// Its shadow, if it casts one.
    pub shadow: Option<Shadow>,
}

impl Shape {
    /// A shape of `rect`, filled with `fill`, of square corners, without a
    /// border or a shadow.
    pub fn filled(rect: Rect, fill: Color) -> Self {
        Self {
            rect,
            radius: 0.0,
            fill,
            border: 0.0,
            border_color: Color::CLEAR,
            shadow: None,
        }
    }
}

/// The shadow of a shape: the shape's outline moved, blurred, and filled
/// with its color, under the shape.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shadow {
    /// How far right it is moved, in pixels.
    pub x: f32,
    /// How far down it is moved, in pixels.
    pub y: f32,
    /// How far its edge blurs, in pixels, twice the deviation of a Gaussian
    /// blur: 0 for a sharp edge.
    pub blur: f32,
    /// Its color, which its opacity makes lighter.
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
    /// Glyphs placed anywhere, as the interface's text.
    Glyphs(Vec<PlacedGlyph>),
    /// Shapes, each over those before it, its shadow under itself alone.
    Shapes(Vec<Shape>),
    /// The drawings that follow, up to the matching [`Item::PopClip`], keep
    /// within the rectangle, and within the clips that hold it.
    PushClip(Rect),
    /// The end of the latest clip.
    PopClip,
}

/// A window's frame: its background, and what is drawn over it, in order.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DisplayList {
    /// The color of what nothing covers, through which, when it is not
    /// opaque, what is behind the window shows.
    pub background: Color,
    /// The drawings, from the first to the last, each over those before.
    pub items: Vec<Item>,
    /// The number of the atlas's frame that it draws, which the renderer
    /// tells back until the GPU has drawn it, so that the atlas keeps its
    /// glyphs.
    pub frame: u64,
}

/// How long the GPU took for some frames: how many, all together, and the
/// longest one.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GpuTimes {
    /// The frames.
    pub frames: u32,
    /// Their times together.
    pub total: Duration,
    /// The longest one's.
    pub longest: Duration,
}

impl GpuTimes {
    /// Takes the time of one more frame.
    pub fn add(&mut self, time: Duration) {
        self.frames += 1;
        self.total += time;
        self.longest = self.longest.max(time);
    }
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

    /// The oldest frame, by its list's `frame`, that the GPU may still be
    /// drawing; `None` once it finished every one that it took.
    fn drawing_from(&mut self) -> Option<u64>;

    /// The GPU's times of the frames that it finished since the last call,
    /// which gives each once; none for a backend that does not time them.
    fn take_gpu_times(&mut self) -> GpuTimes {
        GpuTimes::default()
    }

    /// How long the last draw waited for the target's drawable, which the
    /// display gives back as it shows the frame after the one that it
    /// shows; none for a backend that does not time it.
    fn drawable_wait(&self) -> Duration {
        Duration::ZERO
    }

    /// Draws `list` into `target`, as [`Renderer::draw`] does, but for its
    /// GPU's time, which [`Renderer::take_gpu_times`] leaves out: a layer's
    /// frame, of a few cells, apart from the windows' frames.
    fn draw_untimed(
        &mut self,
        target: &Self::Target,
        list: &DisplayList,
    ) -> Result<bool, Self::Error> {
        self.draw(target, list)
    }

    /// Calls `presented` as the next frame that the renderer draws, into
    /// any target, shows on the display, from any thread, with how long ago
    /// it showed, or `None` if the display skipped it; a backend that does
    /// not learn when its frames show never calls it.
    fn on_next_presented(&mut self, presented: impl FnOnce(Option<Duration>) + Send + 'static) {
        let _ = presented;
    }

    /// Forgets the call that [`Renderer::on_next_presented`] left, as a
    /// frame that drew nothing leaves it to no other target.
    fn forget_next_presented(&mut self) {}
}

/// What a renderer draws a window's frames into, or a layer's of it.
pub trait Surface {
    /// What can fail.
    type Error;

    /// Sizes it to the window's content, `width` by `height` pixels at
    /// `scale`.
    fn resize(&self, width: u32, height: u32, scale: f64);

    /// Has the frames to come show with the window's own changes of size, as
    /// the user resizes it, so that the content follows its edge, rather
    /// than as the GPU finishes them.
    fn set_presents_with_transaction(&self, with: bool);

    /// Lets the memory of its frames go, as for a window that does not
    /// show, but the frame that shows, until the next
    /// [`Surface::resize`].
    fn release(&self);

    /// Draws a frame of one color, and presents it. Returns whether it did.
    fn clear(&self, color: Color) -> Result<bool, Self::Error>;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cursor(shape: CursorShape, columns: u16) -> CellCursor {
        CellCursor {
            column: 2,
            row: 1,
            columns,
            shape,
            thickness: 2,
            color: Color::default(),
        }
    }

    fn line(style: LineStyle, top: u32, thickness: u32) -> CellLine {
        CellLine {
            column: 0,
            row: 0,
            columns: 4,
            style,
            top,
            thickness,
            color: Color::default(),
        }
    }

    #[test]
    fn a_line_s_pattern_takes_its_rows_within_the_cell() {
        // A cell of 16 rows, whose underline takes row 13.
        assert_eq!(line(LineStyle::Single, 13, 1).band(16), (13, 1));
        assert_eq!(line(LineStyle::Dotted, 13, 1).band(16), (13, 1));
        assert_eq!(line(LineStyle::Dashed, 13, 2).band(16), (13, 2));
        // Two strokes and their gap, moved up to keep within the cell.
        assert_eq!(line(LineStyle::Double, 12, 1).band(16), (12, 3));
        assert_eq!(line(LineStyle::Double, 14, 1).band(16), (13, 3));
        // A wave about the line's middle, of four rows at least.
        assert_eq!(line(LineStyle::Curly, 13, 1).band(16), (11, 4));
        assert_eq!(line(LineStyle::Curly, 26, 2).band(34), (24, 6));
        assert_eq!(line(LineStyle::Curly, 15, 1).band(16), (12, 4));
        // A cell smaller than the pattern takes it whole; a thickness of 0
        // takes a row.
        assert_eq!(line(LineStyle::Double, 0, 2).band(4), (0, 4));
        assert_eq!(line(LineStyle::Single, 3, 0).band(4), (3, 1));
        assert_eq!(line(LineStyle::Single, 9, 1).band(0), (0, 1));
    }

    fn rect(x: i32, y: i32, width: u32, height: u32) -> Rect {
        Rect {
            x,
            y,
            width,
            height,
        }
    }

    #[test]
    fn each_shape_of_cursor_takes_its_part_of_the_cell() {
        // Cells of 8 by 16, from (10, 20): the third of the second row is
        // at (26, 36).
        let rects = |shape, columns| cursor(shape, columns).rects(10, 20, 8, 16);
        assert_eq!(rects(CursorShape::Block, 1), [rect(26, 36, 8, 16)]);
        assert_eq!(rects(CursorShape::Block, 2), [rect(26, 36, 16, 16)]);
        assert_eq!(rects(CursorShape::Bar, 2), [rect(26, 36, 2, 16)]);
        assert_eq!(rects(CursorShape::Underline, 1), [rect(26, 50, 8, 2)]);
        assert_eq!(
            rects(CursorShape::Hollow, 1),
            [
                rect(26, 36, 8, 2),
                rect(26, 50, 8, 2),
                rect(26, 38, 2, 12),
                rect(32, 38, 2, 12),
            ]
        );
    }

    #[test]
    fn a_cursor_s_thickness_keeps_within_its_cell() {
        let mut thick = cursor(CursorShape::Bar, 1);
        thick.thickness = 40;
        assert_eq!(thick.rects(0, 0, 8, 16), [rect(16, 16, 8, 16)]);
        thick.thickness = 0;
        assert_eq!(thick.rects(0, 0, 8, 16), [rect(16, 16, 1, 16)]);
    }

    #[test]
    fn a_color_of_bytes_spans_zero_to_one() {
        assert_eq!(
            Color::from_rgb8(0, 255, 51),
            Color {
                red: 0.0,
                green: 1.0,
                blue: 0.2,
                alpha: 1.0
            }
        );
        assert_eq!(Color::from_rgb8(0, 0, 0).with_alpha(0.5).alpha, 0.5);
    }
}
