//! What a platform's font system does for the terminal: the [`FontBackend`]
//! trait, and its types.
//!
//! Sizes are in pixels: a size in points times the display's scale. A face
//! is the same at every size, and the backend makes it at the size asked.

use std::time::Instant;

/// A face that the backend found, as long as the backend lives.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FaceId(pub u32);

/// A glyph of a face, by its index in the font.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct GlyphId(pub u16);

/// A face of a family, with what tells it from the family's others.
#[derive(Clone, Debug, PartialEq)]
pub struct FaceInfo {
    /// The face.
    pub id: FaceId,
    /// Its style, by the name that the font gives it, such as `SemiBold` or
    /// `Bold Italic`.
    pub style: String,
    /// Its weight, as CSS numbers them: 400 regular, 700 bold.
    pub weight: f32,
    /// Whether it is italic or oblique.
    pub italic: bool,
    /// Whether it is oblique: its upright forms slanted, rather than
    /// italic forms of their own.
    pub oblique: bool,
    /// Its width, as CoreText measures it: 0 for the normal width, below
    /// for a condensed face, above for an expanded one.
    pub width: f32,
    /// Whether it is in the style group that the family was named by: a
    /// name of a few of the family's faces, of a weight or a width, as
    /// `CaskaydiaCove NFM SemiBold` is of the SemiBold faces of CaskaydiaCove
    /// Nerd Font Mono. No face is when the name is the whole family's.
    pub in_group: bool,
    /// Whether it is the face that the system gives for the family named
    /// alone, which faces that tie for a style give way to.
    pub preferred: bool,
}

/// What a face measures at a size, in pixels: the distances that go up from
/// the baseline are positive, those that go down negative.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FaceMetrics {
    /// The height above the baseline.
    pub ascent: f32,
    /// The depth below the baseline, as a positive distance.
    pub descent: f32,
    /// The space between a line's descent and the next line's ascent.
    pub leading: f32,
    /// The advance of a monospaced glyph: of the widest digit, for a family
    /// whose glyphs differ.
    pub advance: f32,
    /// Where the underline's middle is, from the baseline.
    pub underline_position: f32,
    /// The thickness of the underline.
    pub underline_thickness: f32,
    /// Where the strikeout's middle is, from the baseline.
    pub strikeout_position: f32,
    /// The thickness of the strikeout.
    pub strikeout_thickness: f32,
}

/// What a face does to the text that it shapes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Features {
    /// Whether ligatures join glyphs: off turns off the font's contextual
    /// alternates, standard and discretionary ligatures (`calt`, `liga`,
    /// `dlig`), as some fonts put their ligatures in the first.
    pub ligatures: bool,
}

/// A glyph of a shaped run, with where it goes, in pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShapedGlyph {
    /// The face that draws the glyph: the face shaped in, or another that
    /// the shaping took for a cluster that it lacks a character of.
    pub face: FaceId,
    /// The glyph.
    pub glyph: GlyphId,
    /// The byte, in the run's text, of the character that the glyph draws,
    /// or of the first of those that it draws together.
    pub cluster: u32,
    /// Where the glyph is from the run's start, rightwards, in pixels.
    pub x: f32,
    /// Where the glyph is from the baseline, upwards, in pixels.
    pub y: f32,
    /// Whether the glyph's run goes from right to left, as Hebrew's and
    /// Arabic's do, its clusters' bytes falling as `x` rises.
    pub right_to_left: bool,
}

/// What a face that a family lacks is drawn from: another of its faces,
/// heavier or slanted.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Synthesis {
    /// Drawn heavier, for a bold that the family lacks.
    pub bold: bool,
    /// Slanted, for an italic that the family lacks.
    pub italic: bool,
}

/// How a glyph's coverage is drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Rendering {
    /// Whether its edges are antialiased: without, each pixel is covered or
    /// not.
    pub antialias: bool,
    /// Whether it is drawn without the darkening that the system's font
    /// smoothing adds, of thinner strokes.
    pub thin: bool,
    /// How far its origin is past its pixel's left edge, in thirds of a
    /// pixel, from 0 to 2: of text placed at fractions of a pixel, as the
    /// interface's, rather than of the cells' whole pixels.
    pub phase: u8,
}

impl Rendering {
    /// The phases of a pixel, its thirds.
    pub const PHASES: u8 = 3;
}

impl Default for Rendering {
    fn default() -> Self {
        Self {
            antialias: true,
            thin: false,
            phase: 0,
        }
    }
}

/// A glyph to draw, as [`FontBackend::rasterize`] takes it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Raster {
    /// The face that draws it.
    pub face: FaceId,
    /// The glyph, of that face.
    pub glyph: GlyphId,
    /// The size, in pixels.
    pub size: f32,
    /// What makes it heavier or slanted.
    pub synthesis: Synthesis,
    /// How its coverage is drawn.
    pub rendering: Rendering,
}

/// The pixels of a bitmap.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BitmapFormat {
    /// One byte a pixel, its coverage: a glyph that the text's color fills.
    Gray,
    /// Four bytes a pixel, red, green, blue and alpha, premultiplied: a
    /// glyph of its own colors, such as an emoji.
    Rgba,
}

/// A glyph drawn into pixels, and where they go from the pen on the
/// baseline.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Bitmap {
    /// Its pixels.
    pub format: BitmapFormat,
    /// Its width, in pixels.
    pub width: u32,
    /// Its height, in pixels.
    pub height: u32,
    /// How far its left edge is from the pen, rightwards.
    pub left: i32,
    /// How far its top edge is from the baseline, upwards.
    pub top: i32,
    /// Its rows, from the top, each of `width` pixels.
    pub data: Vec<u8>,
}

impl Bitmap {
    /// A bitmap of nothing, for a glyph without ink such as a space.
    pub fn empty(format: BitmapFormat) -> Self {
        Self {
            format,
            width: 0,
            height: 0,
            left: 0,
            top: 0,
            data: Vec::new(),
        }
    }
}

/// What the platform's font system does for the terminal: it finds faces,
/// measures them, finds the faces that draw what a face lacks, shapes text,
/// and draws glyphs. It runs on the main thread, and keeps what it found.
pub trait FontBackend {
    /// The name of the family that text takes when the configuration names
    /// none, or names one that is not installed: the system's monospaced
    /// family.
    fn default_family(&mut self) -> String;

    /// The faces of the family `name`, or `None` if it is not installed. A
    /// name of a style group gives the whole family that the group is in,
    /// the group's faces marked [`FaceInfo::in_group`].
    fn family(&mut self, name: &str) -> Option<Vec<FaceInfo>>;

    /// The names of the families that the system has, as
    /// [`FontBackend::family`] takes them, without those that it hides from
    /// the user, in any order: none by default.
    fn families(&mut self) -> Vec<String> {
        Vec::new()
    }

    /// Whether the glyphs of `face` all take one advance, as its font tells:
    /// `false` when it cannot tell.
    fn monospaced(&mut self, face: FaceId) -> bool {
        let _ = face;
        false
    }

    /// The face of [`SYMBOLS_FONT`](crate::SYMBOLS_FONT), the icons that
    /// the configured font may lack, which the backend loads the first time:
    /// `None` when it cannot.
    fn symbols(&mut self) -> Option<FaceId> {
        None
    }

    /// Whether the glyphs that it draws of coverage take the system's font
    /// smoothing, the darkening of their strokes that a thin glyph goes
    /// without: `false` by default.
    fn smooths(&mut self) -> bool {
        false
    }

    /// Whether `face` is the system's last resort, which draws a box of its
    /// block for any character that no other face has, as the cascade gives
    /// it: `false` when it cannot tell.
    fn last_resort(&mut self, face: FaceId) -> bool {
        let _ = face;
        false
    }

    /// What `face` measures at `size` pixels.
    fn metrics(&mut self, face: FaceId, size: f32) -> FaceMetrics;

    /// `face` of the design of `points`, whatever the size it is made at,
    /// as a family of optical sizes draws each size of its own: at 26
    /// pixels, the text of 13 points on a display of twice the pixels, its
    /// 13 points' forms and advances, twice as large. A face of its own,
    /// which the backend keeps; `face` itself by default.
    fn optical(&mut self, face: FaceId, points: f32) -> FaceId {
        let _ = points;
        face
    }

    /// The name of the family of the text of the app's own interface, as
    /// its tab bar and its panels: the system's, as its menus and windows
    /// show it; [`FontBackend::default_family`] by default.
    fn interface_family(&mut self) -> String {
        self.default_family()
    }

    /// How wide `text` is in `face` at `size` pixels, on one line, with
    /// `features`, its glyphs' advances: by default, its last glyph's place
    /// and the face's advance, which a face of one advance gives exactly.
    fn width(&mut self, face: FaceId, size: f32, text: &str, features: Features) -> f32 {
        let mut glyphs = Vec::new();
        self.shape(face, size, text, features, &mut glyphs);
        glyphs
            .iter()
            .map(|glyph| glyph.x)
            .fold(None, |last: Option<f32>, x| {
                Some(last.map_or(x, |last| last.max(x)))
            })
            .map_or(0.0, |last| last + self.metrics(face, size).advance)
    }

    /// The glyph of `character` in `face`, or `None` if the face lacks it.
    fn glyph(&mut self, face: FaceId, character: char) -> Option<GlyphId>;

    /// The face that draws `cluster`, a grapheme cluster, when `face`
    /// lacks it, as the system's cascade of fonts gives it; `None` when no
    /// face of the system draws it.
    fn fallback(&mut self, face: FaceId, cluster: &str) -> Option<FaceId>;

    /// Whether `face` substitutes glyphs for others in context, as its
    /// ligatures and contextual alternates do, which a shaping of text of
    /// one character a cluster can change: `true` when it cannot tell.
    fn substitutes(&mut self, face: FaceId) -> bool {
        let _ = face;
        true
    }

    /// Shapes `text` in `face` at `size` pixels, with `features`, and
    /// appends its glyphs, in the order that they are drawn, to `glyphs`:
    /// those of a cluster that the face lacks a character of in the face
    /// that the system draws it in.
    fn shape(
        &mut self,
        face: FaceId,
        size: f32,
        text: &str,
        features: Features,
        glyphs: &mut Vec<ShapedGlyph>,
    );

    /// Draws `glyph` of `face` at `size` pixels, made heavier or slanted as
    /// `synthesis` says, its coverage as `rendering` says; a glyph of its own
    /// colors keeps its antialiasing.
    fn rasterize(
        &mut self,
        face: FaceId,
        glyph: GlyphId,
        size: f32,
        synthesis: Synthesis,
        rendering: Rendering,
    ) -> Bitmap;

    /// Draws the glyphs of `rasters`, as [`FontBackend::rasterize`] draws
    /// each, as many as there is time for until `deadline`, in their order,
    /// and the first one whatever the time: their bitmaps, and `None` for
    /// those that it left, which no call started past the deadline. A
    /// backend may draw them on several threads at once; by default, they
    /// are drawn in turn.
    fn rasterize_many(&mut self, rasters: &[Raster], deadline: Instant) -> Vec<Option<Bitmap>> {
        rasters
            .iter()
            .enumerate()
            .map(|(index, raster)| {
                (index == 0 || Instant::now() < deadline).then(|| {
                    self.rasterize(
                        raster.face,
                        raster.glyph,
                        raster.size,
                        raster.synthesis,
                        raster.rendering,
                    )
                })
            })
            .collect()
    }
}
