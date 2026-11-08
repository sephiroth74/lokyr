//! The font that a window's text takes: its faces, the family of the text
//! past ASCII when the configuration names one, the faces of the system that
//! draw what they lack, its cell, and the atlas that holds the glyphs drawn
//! so far.

use std::hash::{Hash, Hasher};
use std::rc::Rc;
use std::time::{Duration, Instant};

use lokyr_plugin_sdk::view::{FontFamily, FontWeight, TextSize};
use lokyr_render_core::{AtlasRegion, Color, PlacedGlyph, Texture, TextureUpdate};
use lokyr_text_core::{
    Atlas, BitmapFormat, CellMetrics, Change, Face, FaceError, FaceId, Faces, FastMap, Features,
    FontBackend, FontConfig, GlyphId, Raster, Rendering, Resolved, ShapedGlyph, Synthesis,
    choose_faces, resolve_faces, sprite,
};
use lokyr_vt_core::unicode;

use crate::field::{self, Carets};
use crate::interface::{self, InterfaceConfig, InterfaceFace};
use crate::layout::Measure;
use crate::pane::{AtlasGlyph, CellText, Glyphs as GlyphSource, Run, RunGlyph};
use crate::text::TextConfig;
use crate::tree::Typeset;

/// The face of the glyphs that the terminal draws itself, of box drawing
/// and of the powerline symbols, whose glyph is their character's code.
const SPRITES: FaceId = FaceId(u32::MAX);

/// The frames after which a glyph that the atlas refused is drawn again, as
/// the atlas may have room for it then: a second or two.
const REFUSED_FRAMES: u64 = 120;

/// The runs that the newer generation of the cache of shaped runs takes
/// before it becomes the older, at least: past it, twice the runs of the
/// frame before, so that a frame's runs always fit.
const SHAPED_RUNS: usize = 4096;

/// The glyphs that the newer generation of the cache of shaped runs takes
/// before it becomes the older, at least, about 2.5 MiB of them: past it,
/// twice the glyphs of the frame before. So rows without spaces, a long
/// run each, do not grow it without end.
const SHAPED_GLYPHS: usize = 1 << 17;

/// The texts whose caret's places the interface keeps, as a window's
/// fields and their compositions ask for them each frame.
const CARET_TEXTS: usize = 8;

/// How many texts of the interface, shortened to their room, the font
/// keeps, as each frame that draws the chrome shortens the same again.
const SHORTENED_TEXTS: usize = 64;

/// An icon's largest side, in pixels, past which it draws nothing: 512
/// points at a scale of 2, a megabyte of the atlas.
const ICON_LARGEST: f32 = 1024.0;

/// The slots of the glyphs that a frame drew, a power of two.
const DRAWN_SLOTS: usize = 512;

/// The share of an icon's square, a line of the interface's text, that its
/// ink takes, where the symbols' font takes it an em wide: three quarters,
/// 12 points of a line of 16, as the system's symbols beside text of 13
/// points.
const ICON_INK: f32 = 0.75;

/// The share of the interface's size that its small text takes: eleven
/// points of thirteen, as macOS's small controls' text of its regular
/// ones'.
pub const SMALL: f32 = 11.0 / 13.0;

/// The share of the interface's size that its large text takes: eighteen
/// points of thirteen, as a palette's field.
pub const LARGE: f32 = 18.0 / 13.0;

/// The fewest points of a node's text, as the interface's font's are.
pub const MOST_SMALL: f32 = 4.0;

/// The most points of a node's text.
pub const MOST_LARGE: f32 = 200.0;

/// VARIATION SELECTOR-16, which asks for a character's emoji.
const EMOJI_PRESENTATION: char = '\u{FE0F}';

/// VARIATION SELECTOR-15, which asks for a character's text.
const TEXT_PRESENTATION: char = '\u{FE0E}';

/// How a character is drawn: as text, in the text's color, or as an emoji,
/// in its own colors.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Presentation {
    Text,
    Emoji,
}

impl Presentation {
    /// The presentation that the variation selector of `cluster` asks for,
    /// if it has one.
    fn asked(cluster: &str) -> Option<Self> {
        if cluster.contains(EMOJI_PRESENTATION) {
            Some(Self::Emoji)
        } else if cluster.contains(TEXT_PRESENTATION) {
            Some(Self::Text)
        } else if is_emoji_sequence(cluster) {
            Some(Self::Emoji)
        } else {
            None
        }
    }

    /// The presentation that Unicode gives `character` without a selector:
    /// an emoji for one of Emoji_Presentation, as ⚡ and ☕, and text for
    /// the others, as ✔ and ⚠, which have an emoji too.
    fn default_of(character: char) -> Self {
        if unicode::has_emoji_presentation(character) {
            Self::Emoji
        } else {
            Self::Text
        }
    }
}

/// The key of a glyph in the atlas: the same glyph of the same face at
/// another size, or drawn otherwise, is another glyph.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GlyphKey {
    /// The face.
    pub face: FaceId,
    /// The glyph.
    pub glyph: GlyphId,
    /// The size, in pixels, by its bits.
    pub size: u32,
    /// What is drawn heavier or slanted.
    pub synthesis: Synthesis,
    /// Whether it is drawn of thin strokes.
    pub thin: bool,
    /// Whether it spans two columns: a drawing of the terminal's in a wide
    /// cell, which takes both; no other glyph.
    pub wide: bool,
    /// How far its origin is past its pixel's left edge, in thirds of a
    /// pixel: 0 for the cells' glyphs, which take whole pixels.
    pub phase: u8,
}

/// A key hashes as two words rather than a word a field, as every glyph of
/// a frame looks one up.
impl Hash for GlyphKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        let flags = u64::from(self.phase) << 4
            | u64::from(self.wide) << 3
            | u64::from(self.synthesis.bold) << 2
            | u64::from(self.synthesis.italic) << 1
            | u64::from(self.thin);
        state.write_u64(u64::from(self.face.0) << 32 | u64::from(self.glyph.0) << 16 | flags);
        state.write_u32(self.size);
    }
}

/// The font of the text: the family and faces that the configuration names,
/// at a size in pixels, and the atlas of their glyphs.
#[derive(Debug)]
pub struct Font<B> {
    backend: B,
    /// Whether the backend draws its glyphs of coverage of the system's
    /// font smoothing.
    smooths: bool,
    config: FontConfig,
    resolved: Resolved,
    /// The faces of the text past ASCII, when the configuration names an
    /// installed family for it.
    non_ascii: Option<Faces>,
    /// The family that the configuration names for the text past ASCII,
    /// when it is not installed.
    missing_non_ascii: Option<String>,
    pixels: f32,
    /// The sizes in pixels at which the symbols' font fits a cell of one
    /// column and one of two, once a frame asked for them, as its icons are
    /// an em wide.
    fitted: [Option<f32>; 2],
    /// The display's scale.
    scale: f32,
    /// Which styles take their faces.
    text: TextConfig,
    cell: CellMetrics,
    atlas: Atlas<GlyphKey>,
    glyphs: Glyphs,
    shaped: ShapedRuns,
    /// Whether each face substitutes glyphs in context, as the backend
    /// tells it once.
    substitutes: FastMap<FaceId, bool>,
    /// Where the run's cells find their glyphs, reused from run to run.
    found: Vec<Option<Found>>,
    /// The frames, counted from the first.
    frame: u64,
    /// The glyphs that the frame drew, in a slot of their key's bits, so
    /// that a glyph drawn again skips the atlas's map: a glyph that the
    /// frame looked up keeps its place in the atlas until the frame ends.
    this_frame: Box<[DrawnGlyph; DRAWN_SLOTS]>,
    /// The glyphs that the atlas refused, as too large or with no room, by
    /// the frame that it refused each, so that a frame does not draw them
    /// again at once.
    refused: FastMap<GlyphKey, u64>,
    /// Whether a glyph that the atlas lacks waits for
    /// [`Font::draw_pending`], rather than being drawn at once.
    collecting: bool,
    /// How long the frame looks up the faces of characters, and how long
    /// it draws glyphs, at most: past them, those wait for the frames
    /// after.
    budget: Option<(Duration, Duration)>,
    /// The time that the frame spent looking up faces.
    looking_up: Duration,
    /// Whether the frame left characters whose faces it had no time to
    /// look up.
    lookups_left: bool,
    /// The glyphs that the atlas lacked while collecting, in the order that
    /// they were asked for, each once.
    pending: Vec<(GlyphKey, Raster)>,
    /// The keys of `pending`.
    pending_keys: FastMap<GlyphKey, ()>,
    /// The glyphs that [`Font::draw_pending`] left, as the frame's time for
    /// glyphs ran out, which the frame shows without.
    deferred: FastMap<GlyphKey, ()>,
    /// The configuration of the interface's face, which its first text
    /// finds.
    interface_config: InterfaceConfig,
    /// The face of the interface's text, once found: the first text of the
    /// interface finds it, so that a window without them, and the app's
    /// start, do not pay for it.
    interface: Option<InterfaceFace>,
    /// The terminal's regular face at its size, as an interface's text of
    /// its family takes it, once one asks for it.
    terminal_text: Option<InterfaceFace>,
    /// The interface's faces of other sizes and weights, of its family or
    /// the terminal's, by their families, their points' bits and their
    /// weights, once a text of theirs asks for one.
    interfaces: FastMap<(FontFamily, u32, FontWeight), InterfaceFace>,
    /// How the interface's texts are set, as the node that they are
    /// measured or drawn for has it.
    typeset: Typeset,
    /// The glyph of each icon's character in the symbols' font, once
    /// asked for.
    icons: FastMap<char, Option<GlyphId>>,
    /// The size in pixels at which the symbols' font's advance is an icon's
    /// ink, by its square's side's bits, once asked for.
    icon_pixels: FastMap<u32, f32>,
    /// The places of the caret in the last texts of the interface asked
    /// for, the newest first, of their face and size, so that a field's
    /// frames find them once.
    carets: Vec<KnownCarets>,
    /// The texts of the interface shortened lately, the newest first.
    shortened: Vec<KnownShortened>,
    /// The region of each glyph drawn, with its key, for the tests.
    #[cfg(test)]
    drawn: Vec<(AtlasRegion, GlyphKey)>,
}

/// A glyph that a frame drew, in [`Font`]'s slots.
#[derive(Clone, Copy, Debug)]
struct DrawnGlyph {
    /// The frame, none for a slot never filled.
    frame: Option<u64>,
    key: GlyphKey,
    glyph: AtlasGlyph,
}

impl DrawnGlyph {
    const EMPTY: Self = Self {
        frame: None,
        key: GlyphKey {
            face: FaceId(0),
            glyph: GlyphId(0),
            size: 0,
            synthesis: Synthesis {
                bold: false,
                italic: false,
            },
            thin: false,
            wide: false,
            phase: 0,
        },
        glyph: AtlasGlyph {
            region: AtlasRegion {
                texture: Texture::Coverage,
                x: 0,
                y: 0,
                width: 0,
                height: 0,
            },
            left: 0,
            top: 0,
            smoothed: false,
        },
    };

    /// The slot of `key`.
    fn slot(key: &GlyphKey) -> usize {
        // A glyph's phases, as `lll` draws them, in slots of their own.
        let phase = usize::from(key.phase).wrapping_mul(0x51);
        (usize::from(key.glyph.0) ^ (key.face.0 as usize).wrapping_mul(0x9E37) ^ phase)
            & (DRAWN_SLOTS - 1)
    }
}

/// The key of a shaped run: its face, the bits of its size, whether its
/// ligatures join, and its text.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct RunKey {
    face: u32,
    size: u32,
    ligatures: bool,
    text: String,
}

/// A key hashes its face, size and ligatures as one word.
impl Hash for RunKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        let ligatures = u64::from(self.ligatures) << 63;
        state.write_u64((u64::from(self.face) << 32 | u64::from(self.size)) ^ ligatures);
        self.text.hash(state);
    }
}

/// The runs shaped lately, so that a frame shapes only the runs that
/// changed: those of the frames before stay, in two generations. The newer
/// becomes the older as a frame begins, never within one, once it holds
/// more runs or more glyphs than the frame before looked up twice, and
/// than [`SHAPED_RUNS`] and [`SHAPED_GLYPHS`]; a run that the older has
/// comes back into the newer.
#[derive(Debug, Default)]
struct ShapedRuns {
    newer: FastMap<RunKey, Rc<[ShapedGlyph]>>,
    older: FastMap<RunKey, Rc<[ShapedGlyph]>>,
    /// The glyphs of the newer generation.
    glyphs: usize,
    /// The runs that the frame looked up so far.
    runs_looked_up: usize,
    /// Their glyphs.
    glyphs_looked_up: usize,
    /// The key being looked up, reused so that a lookup allocates nothing.
    key: RunKey,
    /// How many runs were shaped, for the tests.
    #[cfg(test)]
    shapings: usize,
}

impl ShapedRuns {
    /// Begins a frame, which may turn the newer generation into the older.
    fn begin_frame(&mut self) {
        let runs = SHAPED_RUNS.max(self.runs_looked_up.saturating_mul(2));
        let glyphs = SHAPED_GLYPHS.max(self.glyphs_looked_up.saturating_mul(2));
        if self.newer.len() > runs || self.glyphs > glyphs {
            self.older = std::mem::take(&mut self.newer);
            self.glyphs = 0;
        }
        self.runs_looked_up = 0;
        self.glyphs_looked_up = 0;
    }

    /// The glyphs of `text` shaped in `face` at `size` pixels, with
    /// `features`: from the cache, or shaped by `backend`.
    fn shape<B: FontBackend>(
        &mut self,
        backend: &mut B,
        face: FaceId,
        size: f32,
        features: Features,
        text: &str,
    ) -> Rc<[ShapedGlyph]> {
        self.runs_looked_up += 1;
        self.key.face = face.0;
        self.key.size = size.to_bits();
        self.key.ligatures = features.ligatures;
        self.key.text.clear();
        self.key.text.push_str(text);
        if let Some(known) = self.newer.get(&self.key) {
            self.glyphs_looked_up += known.len();
            return Rc::clone(known);
        }
        let known = match self.older.remove(&self.key) {
            Some(known) => known,
            None => {
                #[cfg(test)]
                {
                    self.shapings += 1;
                }
                let mut shaped = Vec::new();
                backend.shape(face, size, text, features, &mut shaped);
                Rc::from(shaped)
            }
        };
        self.glyphs += known.len();
        self.glyphs_looked_up += known.len();
        self.newer.insert(self.key.clone(), Rc::clone(&known));
        known
    }
}

/// Where a character's glyph is: the face that draws it, what its style
/// draws heavier or slanted, the glyph, and whether it is drawn as an emoji.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Found {
    face: FaceId,
    synthesis: Synthesis,
    glyph: GlyphId,
    emoji: bool,
    /// Whether the glyph is the symbols' font's, which draws at the size
    /// that fits its cell, in the cell's middle.
    fitted: bool,
}

/// Where each style finds each character that it was asked for, in its
/// face, the non-ASCII family's or a face of the system's cascade, so that
/// a frame finds them without the backend: ASCII's without a variation
/// selector in a table, the others' in a map. The styles whose faces are the same share theirs. The
/// backend's answers are kept too, so that it is asked once for each face
/// and character.
#[derive(Debug)]
struct Glyphs {
    /// The table of each style.
    tables: [usize; 4],
    /// By table, then by character: `None` until asked for.
    ascii: [[Option<Option<Found>>; 128]; 4],
    /// By table, character, and the presentation that a variation selector
    /// asks for.
    others: FastMap<(usize, char, Option<Presentation>), Option<Found>>,
    /// The backend's glyph of each face and character.
    ids: FastMap<(FaceId, char), Option<GlyphId>>,
    /// The backend's fallback of each face, character, and presentation.
    fallbacks: FastMap<(FaceId, char, Presentation), Option<FaceId>>,
    /// The face that the cascade gave the emoji of a character that is an
    /// emoji by default, which draws the others too.
    emoji: Option<FaceId>,
    /// The faces that the cascade gave characters of the private use areas,
    /// but the last resort, which draw the others of them that they have: a
    /// family of icons, as a Nerd Font, has them all.
    private: Vec<FaceId>,
}

impl Glyphs {
    fn new(faces: &Faces, non_ascii: Option<&Faces>) -> Self {
        let of = |style| {
            (
                face_of(faces, style),
                non_ascii.map(|faces| face_of(faces, style)),
            )
        };
        Self {
            tables: [0, 1, 2, 3].map(|style| {
                (0..style)
                    .find(|&other| of(other) == of(style))
                    .unwrap_or(style)
            }),
            ascii: [[None; 128]; 4],
            others: FastMap::default(),
            ids: FastMap::default(),
            fallbacks: FastMap::default(),
            emoji: None,
            private: Vec::new(),
        }
    }
}

impl<B: FontBackend> Font<B> {
    /// The font that `config` names, through `backend`, at a display's
    /// `scale`.
    pub fn new(mut backend: B, config: FontConfig, scale: f32) -> Result<Self, FaceError> {
        let resolved = resolve_faces(&mut backend, &config)?;
        // The family of the text past ASCII takes its faces by their
        // weights, as the styles that the configuration names are the
        // text's family's.
        let mut non_ascii = None;
        let mut missing_non_ascii = None;
        if let Some(name) = &config.non_ascii_family {
            match backend.family(name.as_str()) {
                Some(faces) => {
                    let weights = FontConfig {
                        style: None,
                        bold_style: None,
                        italic_style: None,
                        bold_italic_style: None,
                        ..config.clone()
                    };
                    non_ascii = Some(choose_faces(name.as_str(), &faces, &weights)?);
                }
                None => missing_non_ascii = Some(name.to_string()),
            }
        }
        let pixels = config.size.points() * scale;
        let cell = CellMetrics::new(&backend.metrics(resolved.faces.regular.id, pixels), &config);
        let smooths = backend.smooths();
        Ok(Self {
            smooths,
            backend,
            config,
            pixels,
            fitted: [None; 2],
            scale,
            text: TextConfig::default(),
            cell,
            atlas: Atlas::new(),
            glyphs: Glyphs::new(&resolved.faces, non_ascii.as_ref()),
            shaped: ShapedRuns::default(),
            substitutes: FastMap::default(),
            found: Vec::new(),
            frame: 0,
            this_frame: Box::new([DrawnGlyph::EMPTY; DRAWN_SLOTS]),
            refused: FastMap::default(),
            collecting: false,
            budget: None,
            looking_up: Duration::ZERO,
            lookups_left: false,
            pending: Vec::new(),
            pending_keys: FastMap::default(),
            deferred: FastMap::default(),
            interface_config: InterfaceConfig::default(),
            interface: None,
            terminal_text: None,
            interfaces: FastMap::default(),
            typeset: Typeset::default(),
            icons: FastMap::default(),
            icon_pixels: FastMap::default(),
            carets: Vec::new(),
            shortened: Vec::new(),
            #[cfg(test)]
            drawn: Vec::new(),
            resolved,
            non_ascii,
            missing_non_ascii,
        })
    }

    /// The family that the configuration names for the text past ASCII,
    /// when it is not installed and that text takes the text's family.
    pub fn missing_non_ascii_family(&self) -> Option<&str> {
        self.missing_non_ascii.as_deref()
    }

    /// The cell of the font's regular face.
    pub fn cell(&self) -> &CellMetrics {
        &self.cell
    }

    /// The family that the configuration names, when it is not installed and
    /// the text takes the default one.
    pub fn missing_family(&self) -> Option<&str> {
        self.resolved.missing_family.as_deref()
    }

    /// The family that the text takes.
    pub fn family(&self) -> &str {
        &self.resolved.family
    }

    /// The font at a display's `scale`: the cell measured again, and the
    /// glyphs of the other sizes left in the atlas, which they leave as it
    /// fills.
    pub fn set_scale(&mut self, scale: f32) {
        self.scale = scale;
        let pixels = self.config.size.points() * scale;
        if pixels == self.pixels {
            return;
        }
        self.pixels = pixels;
        self.fitted = [None; 2];
        let metrics = self.backend.metrics(self.resolved.faces.regular.id, pixels);
        self.cell = CellMetrics::new(&metrics, &self.config);
    }

    /// The size in pixels at which `face`, the symbols' font, fits a cell,
    /// of two columns when `wide`: its advance at the text's size brought
    /// down to the cell's width, or the text's size if it fits already.
    fn fitted_pixels(&mut self, face: FaceId, wide: bool) -> f32 {
        let fitted = &mut self.fitted[usize::from(wide)];
        if let Some(pixels) = *fitted {
            return pixels;
        }
        let advance = self.backend.metrics(face, self.pixels).advance;
        let width = (self.cell.width * if wide { 2 } else { 1 }) as f32;
        let pixels = if advance > width {
            self.pixels * width / advance
        } else {
            self.pixels
        };
        *fitted = Some(pixels);
        pixels
    }

    /// The glyph of `region` in the middle of its cell, of two columns when
    /// `wide`, across and down.
    fn in_the_middle(&self, glyph: AtlasGlyph, wide: bool) -> AtlasGlyph {
        let region = glyph.region;
        let half = |cell: u32, glyph: u32| (i64::from(cell) - i64::from(glyph)) / 2;
        let left = half(self.cell.width * if wide { 2 } else { 1 }, region.width);
        let below_top = half(self.cell.height, region.height);
        AtlasGlyph {
            region,
            left: i32::try_from(left).unwrap_or(0),
            // The top edge's rows above the baseline.
            top: i32::try_from(i64::from(self.cell.baseline) - below_top).unwrap_or(0),
            smoothed: glyph.smoothed,
        }
    }

    /// Takes `text`'s choice of which styles take their faces: bold text
    /// without its face takes the regular one, and italic text without its
    /// face is upright.
    pub fn set_text(&mut self, text: TextConfig) {
        self.text = text;
    }

    /// The style's index, bold its first bit and italic its second, as the
    /// text's configuration takes them.
    fn style_of(&self, style: &lokyr_vt_core::page::Style) -> usize {
        usize::from(style.bold && self.text.bold)
            | usize::from(style.italic && self.text.italic) << 1
    }

    /// Whether a glyph on a background darker than its text when `dark` is
    /// drawn of thin strokes, as the configuration says for the display's
    /// scale.
    fn thin(&self, dark: bool) -> bool {
        self.config.antialias && self.config.thin_strokes.thin(self.scale, dark)
    }

    /// The keys of the glyphs drawn, with their regions in the atlas.
    #[cfg(test)]
    fn atlas_keys(&self) -> impl Iterator<Item = (GlyphKey, AtlasRegion)> + '_ {
        self.drawn.iter().map(|&(region, key)| (key, region))
    }

    /// The number of the atlas's frame in progress, which a renderer tells
    /// back as the GPU draws it.
    pub fn atlas_frame(&self) -> u64 {
        self.atlas.frame()
    }

    /// Tells the atlas the oldest frame that the GPU may still be drawing,
    /// as a renderer tells it, whose glyphs stay; `None` once it draws none.
    pub fn set_drawing(&mut self, oldest: Option<u64>) {
        self.atlas.set_drawing(oldest);
    }

    /// Starts a frame: the glyphs that it uses stay in the atlas, and those
    /// that the frame before left are drawn again.
    pub fn begin_frame(&mut self) {
        self.frame += 1;
        self.atlas.begin_frame();
        self.shaped.begin_frame();
        self.deferred.clear();
        self.collecting = false;
        self.budget = None;
        self.looking_up = Duration::ZERO;
        self.lookups_left = false;
    }

    /// Collects the glyphs that the atlas lacks, which wait for
    /// [`Font::draw_pending`] rather than being drawn as the layout asks
    /// for them, leaving their cells empty: so that a frame's layout finds
    /// the glyphs that it lacks, which the backend then draws together, on
    /// several threads, before the layout is done again. A glyph that the
    /// terminal draws itself is drawn at once. Till the frame ends, it
    /// starts looking up the faces of the characters that it has not met
    /// while it has spent less than `lookups` on them, so that one lookup
    /// can outlast the budget: the others wait for the frames after, as
    /// [`Font::has_deferred`] tells, their cells empty until then; and
    /// [`Font::draw_pending`] draws glyphs for `drawing` at most. Each
    /// frame looks up and draws some, however long its layout takes.
    pub fn collect_missing(&mut self, lookups: Duration, drawing: Duration) {
        self.collecting = true;
        self.budget = Some((lookups, drawing));
    }

    /// Whether the layout found glyphs that the atlas lacks while
    /// collecting, which [`Font::draw_pending`] draws.
    pub fn has_pending(&self) -> bool {
        !self.pending.is_empty()
    }

    /// Ends the collecting, and draws the glyphs that the atlas lacked, as
    /// many as there is time for in the `drawing` that the collecting was
    /// given, from this call, on the backend's threads, into the atlas. The
    /// others wait for the frames after: until the next frame begins, the
    /// layout leaves their cells empty, as [`Font::has_deferred`] tells.
    /// Whether it drew any, which a layout done again would show.
    pub fn draw_pending(&mut self) -> bool {
        self.collecting = false;
        if self.pending.is_empty() {
            return false;
        }
        let pending = std::mem::take(&mut self.pending);
        self.pending_keys.clear();
        let rasters: Vec<Raster> = pending.iter().map(|&(_, raster)| raster).collect();
        let drawing = self.budget.map_or(Duration::ZERO, |(_, drawing)| drawing);
        let bitmaps = self
            .backend
            .rasterize_many(&rasters, Instant::now() + drawing);
        let mut drew = false;
        for ((key, _), bitmap) in pending.into_iter().zip(bitmaps) {
            match bitmap {
                Some(bitmap) => {
                    drew = true;
                    if self.atlas.insert(key, &bitmap).is_none() {
                        self.refuse(key);
                    }
                }
                None => {
                    self.deferred.insert(key, ());
                }
            }
        }
        drew
    }

    /// Whether the frame leaves cells empty, of glyphs that it had no time
    /// to draw, or of characters whose faces it had no time to look up,
    /// which the next frame draws.
    pub fn has_deferred(&self) -> bool {
        !self.deferred.is_empty() || self.lookups_left
    }

    /// Whether the frame's time for looking up faces has run out, which
    /// leaves the character asked for to a later frame.
    fn out_of_time(&mut self) -> bool {
        // A budget of none still starts the frame's first lookup.
        let out = self
            .budget
            .is_some_and(|(lookups, _)| self.looking_up > lookups);
        self.lookups_left |= out;
        out
    }

    /// [`Font::look_up`], whose time the frame counts.
    fn look_up_timed(
        &mut self,
        style: usize,
        character: char,
        presentation: Presentation,
    ) -> Option<Found> {
        let start = Instant::now();
        let found = self.look_up(style, character, presentation);
        self.looking_up += start.elapsed();
        found
    }

    /// Keeps `key`, which the atlas refused, from being drawn again for a
    /// while.
    fn refuse(&mut self, key: GlyphKey) {
        // The refusals of long ago go, rather than pile up.
        let frame = self.frame;
        self.refused.retain(|_, at| frame < *at + REFUSED_FRAMES);
        self.refused.insert(key, frame);
    }

    /// The glyph of `shown`, in its presentation: as text, in the face of
    /// its style, of the non-ASCII family's style for a character past ASCII
    /// and else of the text's family, or else in the face that the system's
    /// cascade gives for its text; as an emoji, in the face that the cascade
    /// gives for its emoji, the system's emoji font, and else as text. An
    /// emoji sequence, of a ZWJ, a flag, a keycap or a skin tone, takes the
    /// one glyph that the emoji face shapes it into. Drawn into the atlas the
    /// first time. `None` when no face draws its character.
    pub fn glyph(&mut self, shown: &CellText<'_>) -> Option<AtlasGlyph> {
        let style = self.style_of(shown.style);
        let character = shown.text.chars().next()?;
        let found = self.find(style, character, Presentation::asked(shown.text))?;
        let found = self.sequence(found, shown.text);
        self.draw(found, self.thin(shown.dark), shown.wide)
    }

    /// The glyph of `found`, an emoji, of the whole `cluster`, when the
    /// cluster is a sequence that its face shapes into one glyph.
    fn sequence(&mut self, found: Found, cluster: &str) -> Found {
        if !found.emoji || cluster.chars().nth(1).is_none() {
            return found;
        }
        let features = Features {
            ligatures: self.config.ligatures,
        };
        let shaped = self.shaped.shape(
            &mut self.backend,
            found.face,
            self.pixels,
            features,
            cluster,
        );
        match &*shaped {
            [one] if one.face == found.face => Found {
                glyph: one.glyph,
                ..found
            },
            _ => found,
        }
    }

    /// The glyph `found` in the atlas, of thin strokes when `thin`, for a
    /// cell of two columns when `wide`: a glyph of the symbols' font drawn
    /// to fit the cell, in its middle.
    fn draw(&mut self, found: Found, thin: bool, wide: bool) -> Option<AtlasGlyph> {
        if !found.fitted {
            // The terminal's drawings take a wide cell's two columns.
            let spans = wide && found.face == SPRITES;
            return self.draw_at(found, self.pixels, thin, spans);
        }
        let size = self.fitted_pixels(found.face, wide);
        let glyph = self.draw_at(found, size, thin, false)?;
        Some(self.in_the_middle(glyph, wide))
    }

    /// The glyph `found` at `size` in the atlas, of thin strokes when
    /// `thin`, across two columns when `wide`, which the terminal's drawings
    /// alone take, drawn into it the first time, antialiased as the
    /// configuration says.
    fn draw_at(&mut self, found: Found, size: f32, thin: bool, wide: bool) -> Option<AtlasGlyph> {
        self.draw_at_phase(found, size, (thin, wide), 0)
    }

    /// The glyph of `found` at `size` pixels, as [`Font::draw_at`] gives
    /// it, its origin `phase` thirds of a pixel past its pixel's left edge.
    fn draw_at_phase(
        &mut self,
        found: Found,
        size: f32,
        (thin, wide): (bool, bool),
        phase: u8,
    ) -> Option<AtlasGlyph> {
        let key = GlyphKey {
            face: found.face,
            glyph: found.glyph,
            size: size.to_bits(),
            synthesis: found.synthesis,
            thin,
            wide,
            phase,
        };
        let slot = DrawnGlyph::slot(&key);
        let drawn = self.this_frame[slot];
        if drawn.frame == Some(self.frame) && drawn.key == key {
            return Some(drawn.glyph);
        }
        let placement = match self.atlas.get(&key) {
            Some(placement) => placement,
            None => {
                if self
                    .refused
                    .get(&key)
                    .is_some_and(|&at| self.frame < at + REFUSED_FRAMES)
                {
                    return None;
                }
                let antialias = self.config.antialias;
                let rendering = Rendering {
                    antialias,
                    thin,
                    phase,
                };
                if found.face != SPRITES {
                    if self.deferred.contains_key(&key) {
                        return None;
                    }
                    if self.collecting {
                        if self.pending_keys.insert(key, ()).is_none() {
                            let raster = Raster {
                                face: found.face,
                                glyph: found.glyph,
                                size,
                                synthesis: found.synthesis,
                                rendering,
                            };
                            self.pending.push((key, raster));
                        }
                        return None;
                    }
                }
                let bitmap = if found.face == SPRITES {
                    let character = char::from_u32(u32::from(found.glyph.0))?;
                    let cell = CellMetrics {
                        width: self.cell.width * if wide { 2 } else { 1 },
                        ..self.cell
                    };
                    sprite::draw(character, &cell, antialias)?
                } else {
                    self.backend.rasterize(
                        found.face,
                        found.glyph,
                        size,
                        found.synthesis,
                        rendering,
                    )
                };
                let Some(placement) = self.atlas.insert(key, &bitmap) else {
                    self.refuse(key);
                    return None;
                };
                placement
            }
        };
        let region = AtlasRegion {
            texture: texture(placement.format),
            x: placement.x,
            y: placement.y,
            width: placement.width,
            height: placement.height,
        };
        #[cfg(test)]
        if !self.drawn.contains(&(region, key)) {
            self.drawn.push((region, key));
        }
        // The glyphs of coverage that the backend drew of its font
        // smoothing: not the terminal's drawings, nor the thin ones.
        let smoothed = self.smooths
            && key.face != SPRITES
            && !key.thin
            && self.config.antialias
            && region.texture == Texture::Coverage;
        let glyph = AtlasGlyph {
            region,
            left: placement.left,
            top: placement.top,
            smoothed,
        };
        self.this_frame[slot] = DrawnGlyph {
            frame: Some(self.frame),
            key,
            glyph,
        };
        Some(glyph)
    }

    /// Appends the glyphs of `run` to `out`: its cells in one face, as
    /// text, shaped together when their ligatures or their clusters of
    /// several characters ask for it, each glyph at the cell of its cluster;
    /// an emoji alone. A cell alone, and text without ligatures of clusters
    /// of one character, takes each cell's glyph without shaping.
    fn lay_out(&mut self, run: &Run<'_>, out: &mut Vec<RunGlyph>) {
        let style = self.style_of(run.style);
        let thin = self.thin(run.dark);
        let mut cells = std::mem::take(&mut self.found);
        cells.clear();
        for index in 0..run.cells.len() {
            let cluster = run.cluster(index);
            let found = cluster.chars().next().and_then(|character| {
                // A cluster of one character asks for no presentation.
                let asked = if cluster.len() == character.len_utf8() {
                    None
                } else {
                    Presentation::asked(cluster)
                };
                self.find(style, character, asked)
            });
            cells.push(found);
        }
        let mut index = 0;
        while index < cells.len() {
            let Some(found) = cells[index] else {
                index += 1;
                continue;
            };
            // An emoji, and a glyph that the terminal draws, alone.
            if found.emoji || found.face == SPRITES {
                let found = self.sequence(found, run.cluster(index));
                if let Some(glyph) = self.draw(found, thin, run.cells[index].wide) {
                    out.push(RunGlyph {
                        cell: index,
                        glyph,
                        x: 0,
                        y: 0,
                    });
                }
                index += 1;
                continue;
            }
            // The cells after it in the same face, as text.
            let end = (index + 1..cells.len())
                .find(|&next| {
                    cells[next].is_none_or(|other| {
                        other.emoji
                            || (other.face, other.synthesis) != (found.face, found.synthesis)
                    })
                })
                .unwrap_or(cells.len());
            let start = run.cells[index].start as usize;
            let stop = run
                .cells
                .get(end)
                .map_or(run.text.len(), |cell| cell.start as usize);
            let text = &run.text[start..stop];
            if self.needs_shaping(text, end - index, found.face) {
                self.shape_cells(run, (index, end), found, text, thin, out);
            } else {
                for (cell, found) in cells.iter().enumerate().take(end).skip(index) {
                    let wide = run.cells[cell].wide;
                    if let Some(glyph) = found.and_then(|found| self.draw(found, thin, wide)) {
                        out.push(RunGlyph {
                            cell,
                            glyph,
                            x: 0,
                            y: 0,
                        });
                    }
                }
            }
            index = end;
        }
        self.found = cells;
    }

    /// Whether `text`, of `cells`, in `face`, can change once shaped: it has
    /// a cluster of several characters, or, with ligatures, several cells
    /// of a face that substitutes glyphs in context, as its contextual
    /// alternates can change letters too.
    fn needs_shaping(&mut self, text: &str, cells: usize, face: FaceId) -> bool {
        if self.config.ligatures && cells > 1 {
            let backend = &mut self.backend;
            let substitutes = *self
                .substitutes
                .entry(face)
                .or_insert_with(|| backend.substitutes(face));
            if substitutes {
                return true;
            }
        }
        // A cluster takes a byte at least: as many bytes as cells are as
        // many clusters of one character.
        text.len() != cells && text.chars().nth(cells).is_some()
    }

    /// Shapes `text`, the clusters of the cells of `run` from `first` to
    /// `end`, in `found`'s face, and appends its glyphs to `out`, of thin
    /// strokes when `thin`, each at the cell where its cluster starts, moved
    /// from the place of that cell's first glyph as the shaping places it:
    /// so that a glyph after a run that the shaping took to another face,
    /// with its own lead, stays in its cell. A glyph of another face is
    /// drawn in that face, upright and of its own weight.
    fn shape_cells(
        &mut self,
        run: &Run<'_>,
        (first, end): (usize, usize),
        found: Found,
        text: &str,
        thin: bool,
        out: &mut Vec<RunGlyph>,
    ) {
        let features = Features {
            ligatures: self.config.ligatures,
        };
        let shaped = self
            .shaped
            .shape(&mut self.backend, found.face, self.pixels, features, text);
        let base = run.cells[first].start;
        let starts = &run.cells[first..end];
        let (mut origin, mut at) = (0.0_f32, usize::MAX);
        for glyph in shaped.iter() {
            let cluster = base + glyph.cluster;
            let cell = first
                + starts
                    .partition_point(|cell| cell.start <= cluster)
                    .saturating_sub(1);
            if cell != at {
                at = cell;
                origin = glyph.x;
            }
            let drawn = if glyph.face == found.face {
                Found {
                    glyph: glyph.glyph,
                    ..found
                }
            } else {
                Found {
                    face: glyph.face,
                    synthesis: Synthesis::default(),
                    glyph: glyph.glyph,
                    emoji: false,
                    fitted: false,
                }
            };
            if let Some(atlas) = self.draw(drawn, thin, run.cells[cell].wide) {
                out.push(RunGlyph {
                    cell,
                    glyph: atlas,
                    x: (glyph.x - origin).round() as i32,
                    y: glyph.y.round() as i32,
                });
            }
        }
    }

    /// The sides of the atlas's two textures, coverage's and color's.
    pub fn texture_sizes(&self) -> (u32, u32) {
        (
            self.atlas.texture_size(BitmapFormat::Gray),
            self.atlas.texture_size(BitmapFormat::Rgba),
        )
    }

    /// What changed in the atlas's textures since the last take, for the
    /// renderer, in order.
    pub fn texture_updates(&mut self) -> Vec<TextureUpdate> {
        self.atlas
            .take_changes()
            .into_iter()
            .map(|change| match change {
                Change::Grown { format, size } => TextureUpdate::Grown {
                    texture: texture(format),
                    size,
                },
                Change::Upload {
                    format,
                    x,
                    y,
                    width,
                    height,
                    data,
                } => TextureUpdate::Upload {
                    region: AtlasRegion {
                        texture: texture(format),
                        x,
                        y,
                        width,
                        height,
                    },
                    data,
                },
            })
            .collect()
    }

    /// Takes `config` for the interface's face, which the interface's next
    /// text finds, as [`Font::interface`] tells.
    pub fn set_interface(&mut self, config: &InterfaceConfig) {
        self.interface_config = config.clone();
        self.interface = None;
        self.terminal_text = None;
        self.interfaces.clear();
    }

    /// Takes `typeset` for the interface's texts that it measures and draws
    /// next, until another one.
    pub fn set_typeset(&mut self, typeset: Typeset) {
        self.typeset = typeset;
    }

    /// Takes `size` for the interface's texts that it measures and draws
    /// next, of the regular weight, until another size.
    pub fn set_text_size(&mut self, size: TextSize) {
        self.typeset = Typeset {
            size,
            ..Typeset::default()
        };
    }

    /// The face of the interface's text, found the first time: the one that
    /// [`Font::set_interface`]'s configuration names; the default one where
    /// that fails, as the log tells; the terminal's face where the system
    /// has no interface family; of [`SMALL`] or [`LARGE`] of its size where
    /// [`Font::set_typeset`] took the small one or the large one, or of the
    /// points that it took, from 4 to 200, and of the family's face nearest
    /// to the weight that it took. A typeset of the terminal's family takes
    /// its regular face and its size, as `[font]` names them, in the place
    /// of the interface's.
    pub fn interface(&mut self) -> &InterfaceFace {
        let (backend, resolved, config, interfaces, typeset) = (
            &mut self.backend,
            &self.resolved,
            &self.interface_config,
            &mut self.interfaces,
            self.typeset,
        );
        let terminal_points = self.config.size.points();
        let regular = match typeset.family {
            FontFamily::Terminal => self.terminal_text.get_or_insert_with(|| {
                InterfaceFace::of(
                    backend,
                    resolved.family.clone(),
                    None,
                    resolved.faces.regular.id,
                    terminal_points,
                )
            }),
            FontFamily::Interface => self.interface.get_or_insert_with(|| {
                InterfaceFace::new(backend, config)
                    .or_else(|error| {
                        eprintln!("lokyr: the interface's font is the default one, {error}");
                        let default = InterfaceConfig {
                            font_family: None,
                            ..config.clone()
                        };
                        InterfaceFace::new(backend, &default)
                    })
                    .unwrap_or_else(|_| {
                        InterfaceFace::of(
                            backend,
                            resolved.family.clone(),
                            None,
                            resolved.faces.regular.id,
                            config.font_size.points(),
                        )
                    })
            }),
        };
        let points = typeset.points.map_or_else(
            || match typeset.size {
                TextSize::Regular => regular.size(),
                TextSize::Small => regular.size() * SMALL,
                TextSize::Large => regular.size() * LARGE,
            },
            |points| points.clamp(MOST_SMALL, MOST_LARGE),
        );
        if typeset.weight == FontWeight::Regular && points == regular.size() {
            return regular;
        }
        interfaces
            .entry((typeset.family, points.to_bits(), typeset.weight))
            .or_insert_with(|| {
                regular
                    .weighted(backend, typeset.weight)
                    .sized(backend, points)
            })
    }

    /// The glyphs of `text` on one line in the interface's face, at the
    /// display's scale, from the pen at `x` on the baseline at `baseline`,
    /// in pixels, in `color`, drawn into the atlas the first time, of thin
    /// strokes as the configuration has them for a background darker than
    /// the text when `dark`, each where the shaping puts it to a third of a
    /// pixel, as a glyph drawn past its pixel's edge by the thirds left; and
    /// where the last one's ink ends, rightwards.
    pub fn interface_text(
        &mut self,
        text: &str,
        (x, baseline): (f32, f32),
        color: Color,
        dark: bool,
    ) -> (Vec<PlacedGlyph>, f32) {
        let (size, face) = {
            let interface = self.interface();
            (interface.size(), interface.face())
        };
        let pixels = size * self.scale;
        let features = Features { ligatures: true };
        let shaped = self
            .shaped
            .shape(&mut self.backend, face, pixels, features, text);
        let thin = self.thin(dark);
        let mut placed = Vec::with_capacity(shaped.len());
        let mut end = x;
        for glyph in shaped.iter() {
            let found = Found {
                face: glyph.face,
                synthesis: Synthesis::default(),
                glyph: glyph.glyph,
                emoji: false,
                fitted: false,
            };
            // The pen at a third of a pixel: its whole pixel, and the
            // glyph drawn past it by the thirds left.
            let thirds = ((x + glyph.x) * f32::from(Rendering::PHASES)).round() as i32;
            let phases = i32::from(Rendering::PHASES);
            let (pixel, phase) = (thirds.div_euclid(phases), thirds.rem_euclid(phases) as u8);
            // A glyph without ink, as a space, draws nothing.
            let Some(drawn) = self
                .draw_at_phase(found, pixels, (thin, false), phase)
                .filter(|drawn| drawn.region.width > 0 && drawn.region.height > 0)
            else {
                continue;
            };
            let left = pixel + drawn.left;
            end = end.max((left + drawn.region.width as i32) as f32);
            // A glyph of its own colors, as an emoji, of an opaque color,
            // whose opacity a fade takes alone, and not the text's.
            let color = match drawn.region.texture {
                Texture::Color => Color::WHITE,
                Texture::Coverage => color,
            };
            placed.push(PlacedGlyph {
                region: drawn.region,
                x: left,
                y: (baseline - glyph.y).round() as i32 - drawn.top,
                color,
                weight: drawn.smoothed,
            });
        }
        (placed, end)
    }

    /// `text` of the interface's face cut at a grapheme cluster's end, of
    /// an ellipsis after it, within `room` points: the most clusters whose
    /// prefix fits with the ellipsis, measured of its own, as a text from
    /// right to left or of both directions lays out its prefix apart, the
    /// spaces before the ellipsis left out; the ellipsis alone where none
    /// fits. Found once for the last texts shortened, which each frame of
    /// the chrome shortens again.
    pub fn interface_shortened(&mut self, text: &str, room: f32) -> Rc<str> {
        const ELLIPSIS: &str = "…";
        let (size, face) = {
            let interface = self.interface();
            (interface.size(), interface.face())
        };
        let key = (face, size.to_bits(), room.to_bits());
        let known = self
            .shortened
            .iter()
            .position(|known| (known.face, known.size, known.room) == key && known.text == text);
        if let Some(at) = known {
            let known = self.shortened.remove(at);
            let shortened = Rc::clone(&known.shortened);
            self.shortened.insert(0, known);
            return shortened;
        }
        let features = Features { ligatures: true };
        let ellipsis = self.backend.width(face, size, ELLIPSIS, features);
        // The clusters' boundaries, from the text's start.
        let mut bounds: Vec<usize> = self
            .interface_carets(text)
            .iter()
            .map(|&(byte, _)| byte)
            .filter(|&byte| byte > 0 && byte < text.len())
            .collect();
        bounds.sort_unstable();
        bounds.dedup();
        // The most clusters whose prefix, of its own width, which a text
        // from right to left or of both directions lays out apart, fits
        // with the ellipsis.
        let (mut fit, mut over) = (0, bounds.len());
        while fit < over {
            let mid = (fit + over).div_ceil(2);
            let prefix = text[..bounds[mid - 1]].trim_end();
            if self.backend.width(face, size, prefix, features) + ellipsis <= room {
                fit = mid;
            } else {
                over = mid - 1;
            }
        }
        let cut = fit.checked_sub(1).map_or(0, |at| bounds[at]);
        let shortened: Rc<str> = format!("{}{ELLIPSIS}", text[..cut].trim_end()).into();
        self.shortened.truncate(SHORTENED_TEXTS - 1);
        self.shortened.insert(
            0,
            KnownShortened {
                face,
                size: size.to_bits(),
                room: room.to_bits(),
                text: text.to_owned(),
                shortened: Rc::clone(&shortened),
            },
        );
        shortened
    }

    /// The places between the grapheme clusters of `text` in the
    /// interface's face, in points from its start, as
    /// [`Font::interface_text`] draws it: each cluster's first byte and the
    /// caret's place before it, then the text's end and the caret's place
    /// after its last cluster, as [`carets_of`] finds them; found once for
    /// the last texts asked for.
    pub fn interface_carets(&mut self, text: &str) -> Rc<[(usize, f32)]> {
        let (size, face) = {
            let interface = self.interface();
            (interface.size(), interface.face())
        };
        let known = self.carets.iter().position(|known| {
            (known.face, known.size) == (face, size.to_bits()) && known.text == text
        });
        if let Some(at) = known {
            let known = self.carets.remove(at);
            let places = Rc::clone(&known.places);
            self.carets.insert(0, known);
            return places;
        }
        let features = Features { ligatures: true };
        let width = self.backend.width(face, size, text, features);
        let shaped = self
            .shaped
            .shape(&mut self.backend, face, size, features, text);
        let places: Rc<[(usize, f32)]> = carets_of(text, &shaped, width).into();
        self.carets.truncate(CARET_TEXTS - 1);
        self.carets.insert(
            0,
            KnownCarets {
                face,
                size: size.to_bits(),
                text: text.to_owned(),
                places: Rc::clone(&places),
            },
        );
        places
    }

    /// The glyph of the icon `name`, in the middle of the square of `side`
    /// pixels whose top left corner is `(x, y)`, in `color`: the symbols'
    /// font's, at the size whose advance, an em, is the icon's ink, most of
    /// the side. `None` for an icon that the host lacks, or that its font
    /// cannot draw.
    pub fn interface_icon(
        &mut self,
        name: &str,
        (x, y): (f32, f32),
        side: f32,
        color: Color,
        dark: bool,
    ) -> Option<PlacedGlyph> {
        // A square of less than a pixel draws nothing, where a font of no
        // size would draw at the system's default; and one past the largest,
        // whose glyph would take much of the atlas, or more memory than the
        // app has.
        if side.is_nan() || !(1.0..=ICON_LARGEST).contains(&side) {
            return None;
        }
        let character = interface::icon(name)?;
        let face = self.backend.symbols()?;
        let glyph = match self.icons.get(&character) {
            Some(&known) => known,
            None => {
                let glyph = self.backend.glyph(face, character);
                self.icons.insert(character, glyph);
                glyph
            }
        }?;
        let pixels = match self.icon_pixels.get(&side.to_bits()) {
            Some(&pixels) => pixels,
            None => {
                let ink = side * ICON_INK;
                let advance = self.backend.metrics(face, ink).advance;
                let pixels = if advance > 0.0 {
                    ink * ink / advance
                } else {
                    ink
                };
                self.icon_pixels.insert(side.to_bits(), pixels);
                pixels
            }
        };
        let found = Found {
            face,
            synthesis: Synthesis::default(),
            glyph,
            emoji: false,
            fitted: false,
        };
        let drawn = self.draw_at(found, pixels, self.thin(dark), false)?;
        let middle = |start: f32, size: u32| (start + (side - size as f32) / 2.0).round() as i32;
        Some(PlacedGlyph {
            region: drawn.region,
            x: middle(x, drawn.region.width),
            y: middle(y, drawn.region.height),
            color,
            weight: drawn.smoothed,
        })
    }

    /// Where `style` finds `character`, in the presentation that `asked`
    /// gives, or else in its own, found once.
    fn find(
        &mut self,
        style: usize,
        character: char,
        asked: Option<Presentation>,
    ) -> Option<Found> {
        let table = self.glyphs.tables[style];
        // ASCII without a selector, as nearly all text is, in the table; a
        // keycap's digit, with VS16, is another glyph.
        if asked.is_none()
            && let Some(slot) = self.glyphs.ascii[table].get(character as usize)
        {
            if let Some(known) = *slot {
                return known;
            }
            if self.out_of_time() {
                return None;
            }
            let found = self.look_up_timed(style, character, Presentation::Text);
            self.glyphs.ascii[table][character as usize] = Some(found);
            return found;
        }
        if let Some(known) = self.glyphs.others.get(&(table, character, asked)) {
            return *known;
        }
        if self.out_of_time() {
            return None;
        }
        let presentation = asked.unwrap_or_else(|| Presentation::default_of(character));
        let found = self
            .look_up_timed(style, character, presentation)
            .map(|found| Found {
                emoji: presentation == Presentation::Emoji,
                ..found
            });
        self.glyphs.others.insert((table, character, asked), found);
        found
    }

    /// Asks the backend where `style` finds `character` in `presentation`.
    /// As text: in the text's faces, the non-ASCII family's and then the
    /// text's family's; then, of the private use areas, in the symbols'
    /// font; and else in the face of the system's cascade that draws its
    /// text. As an emoji: in the cascade's face of its emoji, and
    /// else in the text's faces. A character that one presentation finds
    /// nowhere takes the other, if it has both, as a clock that no face
    /// draws as text.
    fn look_up(
        &mut self,
        style: usize,
        character: char,
        presentation: Presentation,
    ) -> Option<Found> {
        // The characters that the terminal draws itself, as the
        // configuration has it.
        let drawn = (self.config.builtin_box_drawing && sprite::is_box_drawing(character))
            || (self.config.builtin_powerline && sprite::is_powerline(character));
        if drawn && let Some(found) = in_sprites(character) {
            return Some(found);
        }
        let family = face_of(&self.resolved.faces, style);
        let own = match &self.non_ascii {
            Some(non_ascii) if !character.is_ascii() => face_of(non_ascii, style),
            _ => family,
        };
        let faces = [own, family];
        let (glyphs, backend) = (&mut self.glyphs, &mut self.backend);
        let (emoji, text) = (Presentation::Emoji, Presentation::Text);
        match presentation {
            Presentation::Emoji => cascade(glyphs, backend, own, character, emoji)
                .or_else(|| in_faces(&mut glyphs.ids, backend, &faces, character))
                .or_else(|| cascade(glyphs, backend, own, character, text)),
            Presentation::Text => in_faces(&mut glyphs.ids, backend, &faces, character)
                // A powerline symbol that the fonts lack, which the
                // symbols' font, fitted to the cell's width, would draw
                // short of its height.
                .or_else(|| {
                    sprite::is_powerline(character)
                        .then(|| in_sprites(character))
                        .flatten()
                })
                .or_else(|| in_symbols(&mut glyphs.ids, backend, character))
                .or_else(|| cascade(glyphs, backend, own, character, text))
                .or_else(|| {
                    unicode::properties(character)
                        .extended_pictographic
                        .then(|| cascade(glyphs, backend, own, character, emoji))
                        .flatten()
                }),
        }
    }
}

/// Where the system's cascade, from `face`, draws `character` in
/// `presentation`, which the backend gives once: as text, heavier or
/// slanted as `face` is drawn; as an emoji, upright and of its own weight.
fn cascade<B: FontBackend>(
    glyphs: &mut Glyphs,
    backend: &mut B,
    face: Face,
    character: char,
    presentation: Presentation,
) -> Option<Found> {
    let selector = match presentation {
        Presentation::Emoji => Some(EMOJI_PRESENTATION),
        // A character that has an emoji too asks for its text, which the
        // cascade otherwise gives as the emoji.
        Presentation::Text if unicode::properties(character).extended_pictographic => {
            Some(TEXT_PRESENTATION)
        }
        Presentation::Text => None,
    };
    let key = (face.id, character, presentation);
    let fallback = match glyphs.fallbacks.get(&key) {
        Some(&known) => known,
        None => {
            // An emoji takes the face of the emoji found before, when it has
            // the character: the cascade, which gives them all that face,
            // takes about a tenth of a millisecond for each.
            let emoji = glyphs
                .emoji
                .filter(|_| presentation == Presentation::Emoji)
                .filter(|&emoji| glyph_of(&mut glyphs.ids, backend, emoji, character).is_some());
            // A character of the private use areas takes a face that drew
            // another of them, when it has it: the cascade, which searches
            // every installed font for them, takes about 22 ms for each of
            // the icons of a Nerd Font's plane, and gives the last resort's
            // box for those of the first plane.
            let private = presentation == Presentation::Text && is_private(character);
            let mut known = None;
            if private {
                for &face in &glyphs.private {
                    if glyph_of(&mut glyphs.ids, backend, face, character).is_some() {
                        known = Some(face);
                        break;
                    }
                }
            }
            let found = emoji.or(known).or_else(|| {
                let cluster: String = std::iter::once(character).chain(selector).collect();
                backend.fallback(face.id, &cluster)
            });
            if private
                && let Some(drawing) = found
                && !glyphs.private.contains(&drawing)
                && !backend.last_resort(drawing)
                && glyph_of(&mut glyphs.ids, backend, drawing, character).is_some()
            {
                glyphs.private.push(drawing);
            }
            if presentation == Presentation::Emoji
                && glyphs.emoji.is_none()
                && unicode::has_emoji_presentation(character)
            {
                glyphs.emoji = found;
            }
            glyphs.fallbacks.insert(key, found);
            found
        }
    };
    let drawing = fallback?;
    let glyph = glyph_of(&mut glyphs.ids, backend, drawing, character)?;
    Some(Found {
        face: drawing,
        synthesis: match presentation {
            Presentation::Emoji => Synthesis::default(),
            Presentation::Text => face.synthesis,
        },
        glyph,
        emoji: false,
        fitted: false,
    })
}

/// Whether `character` is of the private use areas, whose characters each
/// font gives meanings of its own, as the icons of the Nerd Fonts.
fn is_private(character: char) -> bool {
    matches!(
        u32::from(character),
        0xE000..=0xF8FF | 0xF_0000..=0xF_FFFD | 0x10_0000..=0x10_FFFD
    )
}

/// The glyph of `character`, of the private use areas, in the symbols'
/// font, which draws the icons that the configured font lacks, as a Nerd
/// Font of one cell's icons would, upright and of its own weight, fitted
/// to its cell.
fn in_symbols<B: FontBackend>(
    ids: &mut FastMap<(FaceId, char), Option<GlyphId>>,
    backend: &mut B,
    character: char,
) -> Option<Found> {
    if !is_private(character) {
        return None;
    }
    let face = backend.symbols()?;
    glyph_of(ids, backend, face, character).map(|glyph| Found {
        face,
        synthesis: Synthesis::default(),
        glyph,
        emoji: false,
        fitted: true,
    })
}

/// The glyph of `character` that the terminal draws itself.
fn in_sprites(character: char) -> Option<Found> {
    u16::try_from(u32::from(character)).ok().map(|code| Found {
        face: SPRITES,
        synthesis: Synthesis::default(),
        glyph: GlyphId(code),
        emoji: false,
        fitted: false,
    })
}

/// The first of `faces` that has `character`, with its glyph.
fn in_faces<B: FontBackend>(
    ids: &mut FastMap<(FaceId, char), Option<GlyphId>>,
    backend: &mut B,
    faces: &[Face],
    character: char,
) -> Option<Found> {
    faces.iter().find_map(|face| {
        glyph_of(ids, backend, face.id, character).map(|glyph| Found {
            face: face.id,
            synthesis: face.synthesis,
            glyph,
            emoji: false,
            fitted: false,
        })
    })
}

impl<B: FontBackend> GlyphSource for Font<B> {
    fn glyph(&mut self, shown: &CellText<'_>) -> Option<AtlasGlyph> {
        Font::glyph(self, shown)
    }

    fn system_weight(&self) -> bool {
        self.config.system_weight
    }

    fn run(&mut self, run: &Run<'_>, out: &mut Vec<RunGlyph>) {
        self.lay_out(run, out);
    }
}

/// Whether `cluster` is an emoji's sequence without a selector: a pictograph
/// with a skin tone's modifier, or joined to another by a ZWJ, as `☝🏽` and
/// `❤‍🔥`, which take the emoji's presentation whatever their first
/// character's default.
fn is_emoji_sequence(cluster: &str) -> bool {
    let mut characters = cluster.chars();
    characters
        .next()
        .is_some_and(unicode::is_extended_pictographic)
        && characters.any(|character| matches!(character, '\u{1F3FB}'..='\u{1F3FF}' | '\u{200D}'))
}

/// The glyph of `character` in `face`, which the backend gives once.
fn glyph_of<B: FontBackend>(
    ids: &mut FastMap<(FaceId, char), Option<GlyphId>>,
    backend: &mut B,
    face: FaceId,
    character: char,
) -> Option<GlyphId> {
    *ids.entry((face, character))
        .or_insert_with(|| backend.glyph(face, character))
}

/// The face of a style: bold its first bit, italic its second.
fn face_of(faces: &Faces, style: usize) -> Face {
    match style {
        0 => faces.regular,
        1 => faces.bold,
        2 => faces.italic,
        _ => faces.bold_italic,
    }
}

fn texture(format: BitmapFormat) -> Texture {
    match format {
        BitmapFormat::Gray => Texture::Coverage,
        BitmapFormat::Rgba => Texture::Color,
    }
}

/// The interface's texts and icons measured in its face, in points, for a
/// view tree's layout: a text of its advances, as its run draws them at
/// the display's scale, and an icon a square of a line.
impl<B: FontBackend> Measure for Font<B> {
    fn typeset(&mut self, typeset: Typeset) {
        self.set_typeset(typeset);
    }

    fn text(&mut self, text: &str) -> (f32, f32) {
        let features = Features { ligatures: true };
        let interface = self.interface();
        let (face, size, line) = (interface.face(), interface.size(), interface.line());
        let width = self.backend.width(face, size, text, features);
        (width, line)
    }

    fn icon(&mut self, _name: &str) -> (f32, f32) {
        let line = self.interface().line();
        (line, line)
    }
}

impl<B: FontBackend> Carets for Font<B> {
    fn carets(&mut self, text: &str) -> Rc<[(usize, f32)]> {
        self.interface_carets(text)
    }

    fn typeset(&mut self, typeset: Typeset) {
        self.set_typeset(typeset);
    }
}

/// A text of the interface shortened to a room, which the next frames
/// shorten again: of its face, its size's bits and the room's.
#[derive(Debug)]
struct KnownShortened {
    face: FaceId,
    size: u32,
    room: u32,
    text: String,
    shortened: Rc<str>,
}

/// The places of the caret in a text of the interface, which a field's
/// frames ask for again.
#[derive(Debug)]
struct KnownCarets {
    face: FaceId,
    /// The size's bits.
    size: u32,
    text: String,
    places: Rc<[(usize, f32)]>,
}

/// The places of the caret in `text`, which `glyphs` draw, `width` wide, in
/// their unit from its start: each grapheme cluster's first byte and its
/// leading edge, the left of a cluster of a run from left to right and the
/// right of one from right to left, then the text's end and its last
/// cluster's trailing edge, so that typing at the end of Hebrew goes on at
/// its left. A cluster's edges are its glyphs' leftmost place and the next
/// cluster's leftmost place past it, or the text's end; the clusters that a
/// glyph draws together, as a ligature, share its width in equal parts, in
/// its run's direction. One pass over the clusters and the glyphs, and a
/// sort of the edges.
pub fn carets_of(text: &str, glyphs: &[ShapedGlyph], width: f32) -> Vec<(usize, f32)> {
    let starts = field::boundaries(text);
    let clusters = starts.len() - 1;
    // Each cluster's leftmost glyph, and its run's direction.
    let mut left = vec![f32::INFINITY; clusters];
    let mut backward = vec![false; clusters];
    for glyph in glyphs {
        let byte = glyph.cluster as usize;
        let Some(at) = starts
            .partition_point(|&start| start <= byte)
            .checked_sub(1)
        else {
            continue;
        };
        if at < clusters && glyph.x.is_finite() {
            left[at] = left[at].min(glyph.x);
            backward[at] |= glyph.right_to_left;
        }
    }
    let mut edges: Vec<f32> = left.iter().copied().filter(|x| x.is_finite()).collect();
    edges.sort_by(f32::total_cmp);
    let right = |x: f32| {
        let past = edges.partition_point(|&edge| edge <= x);
        edges.get(past).copied().unwrap_or(width).max(x)
    };
    // Each cluster's span, left and right, and whether its run goes from
    // right to left: of its own glyphs, or its share of the glyphs of the
    // cluster before, which drew it with theirs; before the first of
    // glyphs, at that one's leading edge.
    let mut spans = vec![(0.0, 0.0, false); clusters];
    let first = left.iter().position(|x| x.is_finite());
    let mut at = 0;
    while at < clusters {
        let mut end = at + 1;
        while end < clusters && !left[end].is_finite() {
            end += 1;
        }
        let Some(own) = (left[at].is_finite()).then_some(at).or(first) else {
            // No glyph at all: every place at the start.
            break;
        };
        let (from, to, back) = (left[own], right(left[own]), backward[own]);
        if own != at {
            // The clusters before the first of glyphs.
            let edge = if back { to } else { from };
            for span in &mut spans[at..end.min(own)] {
                *span = (edge, edge, back);
            }
            at = own;
            continue;
        }
        let share = (to - from) / (end - at) as f32;
        for (part, span) in spans[at..end].iter_mut().enumerate() {
            let near = part as f32 * share;
            *span = if back {
                (to - near - share, to - near, true)
            } else {
                (from + near, from + near + share, false)
            };
        }
        at = end;
    }
    let mut places: Vec<(usize, f32)> = starts[..clusters]
        .iter()
        .zip(&spans)
        .map(|(&start, &(from, to, back))| (start, if back { to } else { from }))
        .collect();
    let end = match spans.last() {
        Some(&(from, _, true)) => from,
        Some(&(_, to, false)) if first.is_some() => to,
        _ => width,
    };
    places.push((text.len(), end));
    places
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use lokyr_text_core::{Bitmap, FaceInfo, FaceMetrics, Features, ShapedGlyph, ThinStrokes};
    use lokyr_vt_core::page::Style;

    use super::*;

    /// The face of the cascade of `Squares` that draws CJK.
    const CJK: FaceId = FaceId(9);
    /// Its face of emoji, which lacks U+E000.
    const EMOJI: FaceId = FaceId(8);
    /// Its face of symbols as text, which lacks ⌚.
    const SYMBOLS: FaceId = FaceId(7);
    /// The face of its family Wide, which substitutes nothing in context.
    const WIDE: FaceId = FaceId(5);
    /// The face that its shaping takes for a cluster of a keycap's mark.
    const KEYCAPS: FaceId = FaceId(6);

    /// A face of icons of the private use area, U+E100 to U+E1FF and
    /// U+F000 to U+F0FF, which Mono lacks.
    const ICONS: FaceId = FaceId(10);

    /// The last resort, which has every character.
    const LAST_RESORT: FaceId = FaceId(11);

    /// The symbols' font, of U+E200 to U+E2FF, which Mono lacks, and of ✔.
    const SYMBOLS_FONT: FaceId = FaceId(12);

    fn is_symbol(character: char) -> bool {
        ('\u{E200}'..='\u{E2FF}').contains(&character)
    }

    fn is_icon(character: char) -> bool {
        matches!(character, '\u{E100}'..='\u{E1FF}' | '\u{F000}'..='\u{F0FF}')
    }

    /// A backend of two families: Mono, of a regular and a bold face, which
    /// lack `A`, ✔, ⌚ and CJK, and Wide, of one face, which lacks `A`,
    /// U+E0A0 and U+E0B1; and of a cascade of a face that draws CJK, one of emoji, for
    /// a cluster with VS16, and one of symbols, for a cluster with VS15.
    /// Their glyphs are their characters' codes, drawn as squares of the
    /// size.
    #[derive(Default)]
    struct Squares {
        drawn: HashMap<(FaceId, u16), usize>,
        synthesized: HashMap<(FaceId, u16), Synthesis>,
        looked_up: HashMap<(FaceId, char), usize>,
        fallbacks: usize,
        asked: Vec<String>,
        shapings: usize,
        rendered: Vec<(FaceId, u16, Rendering)>,
        /// The faces asked whether they substitute glyphs in context.
        substituting: Vec<FaceId>,
        /// Whether it draws its glyphs of the font smoothing.
        smoothing: bool,
    }

    fn is_cjk(character: char) -> bool {
        ('\u{3000}'..='\u{9FFF}').contains(&character)
    }

    impl FontBackend for Squares {
        fn default_family(&mut self) -> String {
            "Mono".to_owned()
        }

        fn family(&mut self, name: &str) -> Option<Vec<FaceInfo>> {
            if name == "Wide" {
                return Some(vec![FaceInfo {
                    id: WIDE,
                    style: "Regular".to_owned(),
                    weight: 400.0,
                    italic: false,
                    oblique: false,
                    width: 0.0,
                    in_group: false,
                    preferred: false,
                }]);
            }
            (name == "Mono").then(|| {
                vec![
                    FaceInfo {
                        id: FaceId(0),
                        style: "Regular".to_owned(),
                        weight: 400.0,
                        italic: false,
                        oblique: false,
                        width: 0.0,
                        in_group: false,
                        preferred: false,
                    },
                    FaceInfo {
                        id: FaceId(1),
                        style: "Bold".to_owned(),
                        weight: 700.0,
                        italic: false,
                        oblique: false,
                        width: 0.0,
                        in_group: false,
                        preferred: false,
                    },
                ]
            })
        }

        fn metrics(&mut self, face: FaceId, size: f32) -> FaceMetrics {
            // The symbols' font is of icons wider than a cell, as Symbols
            // Nerd Font's are at the text's size.
            let advance = if face == SYMBOLS_FONT {
                size
            } else {
                size * 0.6
            };
            FaceMetrics {
                ascent: size * 0.8,
                descent: size * 0.2,
                advance,
                ..FaceMetrics::default()
            }
        }

        fn glyph(&mut self, face: FaceId, character: char) -> Option<GlyphId> {
            *self.looked_up.entry((face, character)).or_default() += 1;
            let has = match face {
                ICONS => is_icon(character),
                LAST_RESORT => true,
                SYMBOLS_FONT => is_symbol(character) || character == '\u{2714}',
                CJK => is_cjk(character),
                EMOJI => character != '\u{E000}',
                SYMBOLS => !is_cjk(character) && character != '\u{231A}',
                WIDE => !matches!(character, 'A' | '\u{E0A0}' | '\u{E0B1}'),
                _ => {
                    !matches!(character, 'A' | '\u{2714}' | '\u{231A}')
                        && !is_cjk(character)
                        && !is_icon(character)
                        && !is_symbol(character)
                }
            };
            u16::try_from(u32::from(character))
                .ok()
                .filter(|_| has)
                .map(GlyphId)
        }

        fn fallback(&mut self, _: FaceId, cluster: &str) -> Option<FaceId> {
            self.fallbacks += 1;
            self.asked.push(cluster.to_owned());
            let first = cluster.chars().next();
            if cluster.contains('\u{FE0F}') {
                Some(EMOJI)
            } else if cluster.contains('\u{FE0E}') {
                Some(SYMBOLS)
            } else if first.is_some_and(|c| ('\u{E100}'..='\u{E1FF}').contains(&c)) {
                // As macOS's cascade gives the private use area's first
                // plane.
                Some(LAST_RESORT)
            } else if first.is_some_and(is_icon) {
                Some(ICONS)
            } else {
                first.filter(|&c| is_cjk(c)).map(|_| CJK)
            }
        }

        fn last_resort(&mut self, face: FaceId) -> bool {
            face == LAST_RESORT
        }

        fn symbols(&mut self) -> Option<FaceId> {
            Some(SYMBOLS_FONT)
        }

        fn substitutes(&mut self, face: FaceId) -> bool {
            self.substituting.push(face);
            face != WIDE
        }

        /// Shapes a glyph a character, of its code, a cell's advance each,
        /// but for its ligatures: `->` as two alternates of its cells, as a
        /// font's contextual alternates are, and `fi` as one glyph; an acute
        /// accent goes back over its base; the emoji face shapes a ZWJ
        /// sequence into one glyph, and a skin tone's sequence into another;
        /// and a base with a keycap's mark goes to
        /// the face of keycaps, after a lead of half a cell, as CoreText's
        /// runs of another font start, which the glyphs after it keep.
        fn shape(
            &mut self,
            face: FaceId,
            size: f32,
            text: &str,
            features: Features,
            glyphs: &mut Vec<ShapedGlyph>,
        ) {
            self.shapings += 1;
            let advance = size * 0.6;
            let glyph = |face: FaceId, glyph: u16, cluster: usize, x: f32| ShapedGlyph {
                face,
                glyph: GlyphId(glyph),
                cluster: cluster as u32,
                x,
                y: 0.0,
                right_to_left: false,
            };
            let code = |character: char| u16::try_from(u32::from(character)).unwrap_or(0);
            if face == EMOJI && text.contains('\u{200D}') {
                glyphs.push(glyph(face, 0xF46A, 0, 0.0));
                return;
            }
            if face == EMOJI && text.contains(|c| matches!(c, '\u{1F3FB}'..='\u{1F3FF}')) {
                glyphs.push(glyph(face, 0xF46B, 0, 0.0));
                return;
            }
            let mut characters = text.char_indices().peekable();
            let mut pen = 0.0;
            while let Some((at, character)) = characters.next() {
                let next = characters.peek().map(|&(_, next)| next);
                match (character, next) {
                    (_, Some('\u{20E3}')) if face != KEYCAPS => {
                        let lead = advance / 2.0;
                        glyphs.push(glyph(KEYCAPS, code(character), at, pen + lead));
                        let mark = at + character.len_utf8();
                        glyphs.push(glyph(KEYCAPS, 0x20E3, mark, pen + lead));
                        characters.next();
                        pen += lead + advance;
                    }
                    ('-', Some('>')) if features.ligatures => {
                        glyphs.push(glyph(face, 0xE100, at, pen));
                        glyphs.push(glyph(face, 0xE101, at + 1, pen + advance));
                        pen += 2.0 * advance;
                        characters.next();
                    }
                    ('f', Some('i')) if features.ligatures => {
                        glyphs.push(glyph(face, 0xFB01, at, pen));
                        pen += 2.0 * advance;
                        characters.next();
                    }
                    ('\u{301}', _) => glyphs.push(glyph(face, 0x301, at, pen - advance / 2.0)),
                    _ => {
                        glyphs.push(glyph(face, code(character), at, pen));
                        pen += advance;
                    }
                }
            }
        }

        fn smooths(&mut self) -> bool {
            self.smoothing
        }

        fn rasterize(
            &mut self,
            face: FaceId,
            glyph: GlyphId,
            size: f32,
            synthesis: Synthesis,
            rendering: Rendering,
        ) -> Bitmap {
            *self.drawn.entry((face, glyph.0)).or_default() += 1;
            self.synthesized.insert((face, glyph.0), synthesis);
            self.rendered.push((face, glyph.0, rendering));
            let side = size as u32 / 2;
            Bitmap {
                format: BitmapFormat::Gray,
                width: side,
                height: side,
                left: 1,
                top: side as i32,
                data: vec![255; (side * side) as usize],
            }
        }
    }

    fn shown<'a>(text: &'a str, style: &'a Style) -> CellText<'a> {
        CellText {
            text,
            style,
            wide: false,
            dark: false,
        }
    }

    #[test]
    fn a_glyph_s_phases_take_slots_of_their_own_in_a_frame() {
        let key = |phase| GlyphKey {
            phase,
            ..DrawnGlyph::EMPTY.key
        };
        let slots: Vec<usize> = (0..3).map(|phase| DrawnGlyph::slot(&key(phase))).collect();
        assert!(slots[0] != slots[1] && slots[1] != slots[2] && slots[0] != slots[2]);
    }

    #[test]
    fn the_system_s_weight_is_of_smoothed_glyphs_alone() {
        let font = |smoothing: bool, config: FontConfig| {
            let backend = Squares {
                smoothing,
                ..Squares::default()
            };
            Font::new(backend, config, 1.0).unwrap()
        };
        let color = Color::from_rgb8(10, 20, 30);
        let weights = |font: &mut Font<Squares>| {
            let (glyphs, _) = font.interface_text("ab", (0.0, 20.0), color, false);
            let cell = font.glyph(&shown("a", &Style::DEFAULT)).unwrap();
            let sprite = font.glyph(&shown("\u{2500}", &Style::DEFAULT)).unwrap();
            (
                glyphs.iter().map(|glyph| glyph.weight).collect::<Vec<_>>(),
                cell.smoothed,
                sprite.smoothed,
            )
        };
        // Smoothed: the text and the cell's glyph; never the terminal's own
        // drawing.
        let mut smoothed = font(true, FontConfig::default());
        assert_eq!(weights(&mut smoothed), (vec![true, true], true, false));
        // Without the backend's smoothing, of thin strokes, or without
        // antialiasing: none.
        let thin = FontConfig {
            thin_strokes: ThinStrokes::Always,
            ..FontConfig::default()
        };
        let aliased = FontConfig {
            antialias: false,
            ..FontConfig::default()
        };
        for (smoothing, config) in [
            (false, FontConfig::default()),
            (true, thin),
            (true, aliased),
        ] {
            assert_eq!(
                weights(&mut font(smoothing, config)),
                (vec![false, false], false, false)
            );
        }
    }

    #[test]
    fn the_interface_s_glyphs_fall_at_thirds_of_a_pixel() {
        // 13 pixels at a scale of 1: glyphs 7.8 pixels apart, drawn a pixel
        // past their pen.
        let mut font = Font::new(Squares::default(), FontConfig::default(), 1.0).unwrap();
        let color = Color::from_rgb8(10, 20, 30);
        let (glyphs, _) = font.interface_text("abcd", (0.5, 20.0), color, false);
        // Pens at 0.5, 8.3, 16.1 and 23.9: whole pixels and thirds past
        // them, 0 and 2, 8 and 1, 16 and 0, 24 and 0.
        let xs: Vec<i32> = glyphs.iter().map(|glyph| glyph.x).collect();
        assert_eq!(xs, [1, 9, 17, 25]);
        let phases: Vec<(u16, u8)> = font
            .backend
            .rendered
            .iter()
            .map(|&(_, glyph, rendering)| (glyph, rendering.phase))
            .collect();
        assert_eq!(phases, [(97, 2), (98, 1), (99, 0), (100, 0)]);
        // Each pen within a sixth of a pixel of where its glyph draws.
        for (index, glyph) in glyphs.iter().enumerate() {
            let pen = 0.5 + 7.8 * index as f32;
            let (_, _, rendering) = font.backend.rendered[index];
            let drawn = (glyph.x - 1) as f32 + f32::from(rendering.phase) / 3.0;
            assert!((drawn - pen).abs() <= 1.0 / 6.0 + 1e-4, "{drawn} {pen}");
        }
        // A glyph at another third is drawn again; at the same, kept.
        let drawn = |font: &Font<Squares>| font.backend.drawn[&(FaceId(0), 97)];
        font.interface_text("aaa", (0.0, 20.0), color, false);
        // Pens at 0, 7.8 and 15.6: thirds 0, 2 and 2, the first of `abcd`'s
        // phase 2 kept.
        assert_eq!(drawn(&font), 2);
        // The cells' glyphs keep whole pixels.
        font.glyph(&shown("a", &Style::DEFAULT)).unwrap();
        let (_, _, rendering) = *font.backend.rendered.last().unwrap();
        assert_eq!(rendering.phase, 0);
    }

    #[test]
    fn a_glyph_is_drawn_once_and_uploaded_once() {
        let mut font = Font::new(Squares::default(), FontConfig::default(), 2.0).unwrap();
        assert_eq!(font.family(), "Mono");
        assert_eq!(font.missing_family(), None);
        // 13 points at a scale of 2.
        assert_eq!((font.cell().width, font.cell().height), (16, 26));
        let style = Style::DEFAULT;
        let first = font.glyph(&shown("b", &style)).unwrap();
        assert_eq!(font.glyph(&shown("b", &style)), Some(first));
        assert_eq!(first.region.texture, Texture::Coverage);
        assert_eq!(
            (
                first.region.width,
                first.region.height,
                first.left,
                first.top
            ),
            (13, 13, 1, 13)
        );
        assert_eq!(font.backend.drawn[&(FaceId(0), 0x62)], 1);
        let updates = font.texture_updates();
        assert_eq!(updates.len(), 1);
        assert!(
            matches!(&updates[0], TextureUpdate::Upload { region, data } if region.width == 14 && data.len() == 14 * 14)
        );
        assert!(font.texture_updates().is_empty());
    }

    #[test]
    fn a_frame_collects_the_glyphs_that_it_lacks_and_draws_them_together() {
        let mut font = Font::new(Squares::default(), FontConfig::default(), 2.0).unwrap();
        let style = Style::DEFAULT;
        font.begin_frame();
        font.collect_missing(Duration::from_secs(60), Duration::from_secs(60));
        assert_eq!(font.glyph(&shown("b", &style)), None, "it waits");
        assert_eq!(font.glyph(&shown("c", &style)), None);
        assert_eq!(font.glyph(&shown("b", &style)), None);
        // What the terminal draws itself comes at once.
        assert!(font.glyph(&shown("\u{2500}", &style)).is_some());
        assert!(font.has_pending());
        assert!(font.backend.drawn.is_empty());
        assert!(font.draw_pending());
        assert!(!font.has_pending());
        assert!(!font.has_deferred());
        assert_eq!(font.backend.drawn[&(FaceId(0), 0x62)], 1, "once");
        assert_eq!(font.backend.drawn[&(FaceId(0), 0x63)], 1);
        assert!(font.glyph(&shown("b", &style)).is_some());
        assert_eq!(font.backend.drawn[&(FaceId(0), 0x62)], 1, "from the atlas");
        assert_eq!(font.texture_updates().len(), 3, "each glyph uploaded once");
    }

    #[test]
    fn the_glyphs_that_a_frame_has_no_time_for_wait_for_the_next() {
        let mut font = Font::new(Squares::default(), FontConfig::default(), 2.0).unwrap();
        let style = Style::DEFAULT;
        font.begin_frame();
        font.collect_missing(Duration::from_secs(60), Duration::ZERO);
        font.glyph(&shown("b", &style));
        font.glyph(&shown("c", &style));
        // The first one whatever the time, which leaves the other.
        assert!(font.draw_pending(), "one drawn");
        assert!(font.has_deferred());
        assert_eq!(
            font.glyph(&shown("c", &style)),
            None,
            "its cell stays empty"
        );
        assert_eq!(font.backend.drawn.get(&(FaceId(0), 0x63)), None);
        font.begin_frame();
        assert!(!font.has_deferred());
        assert!(
            font.glyph(&shown("c", &style)).is_some(),
            "the next frame draws it"
        );
    }

    #[test]
    fn the_faces_that_a_frame_has_no_time_to_look_up_wait_for_the_next() {
        let mut font = Font::new(Squares::default(), FontConfig::default(), 2.0).unwrap();
        let style = Style::DEFAULT;
        font.begin_frame();
        font.collect_missing(Duration::from_secs(60), Duration::from_secs(60));
        // The frame's time for lookups, spent, and past.
        font.looking_up = Duration::from_secs(61);
        assert_eq!(font.glyph(&shown("\u{4E2D}", &style)), None);
        assert!(!font.draw_pending());
        assert!(font.has_deferred());
        assert_eq!(font.backend.fallbacks, 0, "not looked up");
        assert_eq!(
            font.glyph(&shown("\u{4E2D}", &style)),
            None,
            "nor later in the frame"
        );
        font.begin_frame();
        assert!(!font.has_deferred());
        assert!(
            font.glyph(&shown("\u{4E2D}", &style)).is_some(),
            "the next frame finds it"
        );
    }

    #[test]
    fn an_emoji_takes_the_face_of_the_emoji_found_before() {
        let mut font = Font::new(Squares::default(), FontConfig::default(), 2.0).unwrap();
        let style = Style::DEFAULT;
        assert!(font.glyph(&shown("\u{231A}", &style)).is_some());
        assert_eq!(font.backend.drawn.get(&(EMOJI, 0x231A)), Some(&1));
        let asked = font.backend.fallbacks;
        assert!(font.glyph(&shown("\u{26A1}", &style)).is_some());
        assert_eq!(font.backend.drawn.get(&(EMOJI, 0x26A1)), Some(&1));
        assert_eq!(font.backend.fallbacks, asked, "without the cascade");
        // A character that the emoji's face lacks asks the cascade.
        font.glyph(&shown("\u{E000}\u{FE0F}", &style));
        assert!(font.backend.fallbacks > asked);
    }

    #[test]
    fn an_icon_takes_the_face_of_the_icons_found_before_but_the_last_resort() {
        let mut font = Font::new(Squares::default(), FontConfig::default(), 2.0).unwrap();
        let style = Style::DEFAULT;
        let face = |font: &mut Font<Squares>, text: &str| {
            font.find(0, text.chars().next().unwrap(), Presentation::asked(text))
                .map(|found| found.face)
        };
        // The cascade gives the last resort, which draws no other.
        assert_eq!(face(&mut font, "\u{E100}"), Some(LAST_RESORT));
        assert_eq!(font.backend.fallbacks, 1);
        assert_eq!(face(&mut font, "\u{F001}"), Some(ICONS));
        assert_eq!(font.backend.fallbacks, 2);
        // The icons' face draws the others, of either plane, without the
        // cascade.
        assert_eq!(face(&mut font, "\u{F002}"), Some(ICONS));
        assert_eq!(face(&mut font, "\u{E101}"), Some(ICONS));
        assert_eq!(font.backend.fallbacks, 2);
        assert!(font.glyph(&shown("\u{F003}", &style)).is_some());
        assert_eq!(font.backend.fallbacks, 2);
    }

    #[test]
    fn an_icon_that_the_font_lacks_takes_the_symbols_font_without_the_cascade() {
        let mut font = Font::new(Squares::default(), FontConfig::default(), 2.0).unwrap();
        let bold = Style {
            bold: true,
            ..Style::DEFAULT
        };
        let found = font.find(1, '\u{E201}', None).unwrap();
        assert_eq!(found.face, SYMBOLS_FONT);
        // Upright and of its own weight, in bold text too.
        assert_eq!(found.synthesis, Synthesis::default());
        assert!(font.glyph(&shown("\u{E202}", &bold)).is_some());
        assert_eq!(font.backend.fallbacks, 0);
        // A character that it lacks, past the private use areas, asks the
        // cascade.
        assert_eq!(font.find(0, 'A', None), None);
        assert_eq!(font.backend.fallbacks, 1);
    }

    #[test]
    fn the_symbols_font_s_icon_is_drawn_to_fit_its_cell_in_its_middle() {
        let mut font = Font::new(Squares::default(), FontConfig::default(), 2.0).unwrap();
        let cell = font.cell;
        let glyph = font.glyph(&shown("\u{E201}", &Style::DEFAULT)).unwrap();
        // Drawn at the size at which its advance is the cell's width.
        let &(_, key) = font.drawn.last().unwrap();
        // Its advance at the text's size is that size.
        let size = cell.width as f32;
        assert_eq!(key.size, size.to_bits());
        let side = size as u32 / 2;
        assert_eq!((glyph.region.width, glyph.region.height), (side, side));
        assert_eq!(glyph.left, ((cell.width - side) / 2) as i32);
        let below_top = (cell.height - side) / 2;
        assert_eq!(glyph.top, cell.baseline as i32 - below_top as i32);
        // A cell of two columns, as an ambiguous character's can be, holds
        // it at the text's size, in the middle of both.
        let two = CellText {
            wide: true,
            ..shown("\u{E201}", &Style::DEFAULT)
        };
        let glyph = font.glyph(&two).unwrap();
        let &(_, key) = font.drawn.last().unwrap();
        assert_eq!(key.size, font.pixels.to_bits());
        let side = font.pixels as u32 / 2;
        assert_eq!(glyph.left, ((2 * cell.width - side) / 2) as i32);
        let wide_left = glyph.left;
        // In a run, shaped, each cell's own columns.
        let cells = [(0, false), (1, false), (4, true)]
            .map(|(start, wide)| crate::pane::RunCell { start, wide });
        let run = Run {
            text: "a\u{E201}\u{E201}",
            cells: &cells,
            style: &Style::DEFAULT,
            dark: false,
        };
        let mut out = Vec::new();
        font.lay_out(&run, &mut out);
        let lefts: Vec<_> = out.iter().map(|placed| placed.glyph.left).collect();
        let narrow_left = ((cell.width - cell.width / 2) / 2) as i32;
        assert_eq!(lefts, [1, narrow_left, wide_left]);
        // The configured font's glyph keeps its size and its place.
        let text = font.glyph(&shown("a", &Style::DEFAULT)).unwrap();
        let &(_, key) = font.drawn.last().unwrap();
        assert_eq!(key.size, font.pixels.to_bits());
        assert_eq!((text.left, text.top), (1, (font.pixels as u32 / 2) as i32));
    }

    #[test]
    fn the_symbols_font_draws_only_the_private_use_areas() {
        let mut font = Font::new(Squares::default(), FontConfig::default(), 2.0).unwrap();
        // ✔, of text presentation, which the symbols' font has too, is the
        // cascade's.
        let found = font.find(0, '\u{2714}', None).map(|found| found.face);
        assert_ne!(found, Some(SYMBOLS_FONT));
        assert_eq!(
            font.backend.looked_up.get(&(SYMBOLS_FONT, '\u{2714}')),
            None
        );
    }

    #[test]
    fn the_configured_font_s_icon_is_its_own() {
        let config = FontConfig {
            family: Some(lokyr_text_core::Name::try_from("Wide".to_owned()).unwrap()),
            ..FontConfig::default()
        };
        let mut font = Font::new(Squares::default(), config, 2.0).unwrap();
        let found = font.find(0, '\u{E201}', None).unwrap();
        assert_eq!(found.face, WIDE);
        let glyph = font.glyph(&shown("\u{E201}", &Style::DEFAULT)).unwrap();
        let &(_, key) = font.drawn.last().unwrap();
        assert_eq!(key.size, font.pixels.to_bits());
        assert_eq!(glyph.left, 1);
    }

    #[test]
    fn a_budget_of_no_time_still_looks_one_face_up() {
        let mut font = Font::new(Squares::default(), FontConfig::default(), 2.0).unwrap();
        let style = Style::DEFAULT;
        font.begin_frame();
        font.collect_missing(Duration::ZERO, Duration::ZERO);
        // Two characters that the frame has not met: the first is looked up.
        font.glyph(&shown("\u{4E2D}", &style));
        font.glyph(&shown("\u{4E2E}", &style));
        assert_eq!(font.backend.fallbacks, 1);
        assert!(font.has_deferred());
        // And drawn, whatever the drawing's budget.
        assert!(font.draw_pending());
    }

    #[test]
    fn bold_takes_its_face() {
        let mut font = Font::new(Squares::default(), FontConfig::default(), 1.0).unwrap();
        let bold = Style {
            bold: true,
            ..Style::DEFAULT
        };
        let regular = font.glyph(&shown("x", &Style::DEFAULT)).unwrap();
        let heavy = font.glyph(&shown("x", &bold)).unwrap();
        assert_ne!(regular.region, heavy.region);
        assert_eq!(font.backend.drawn[&(FaceId(1), 0x78)], 1);
    }

    #[test]
    fn the_backend_gives_each_face_s_glyph_once() {
        let mut font = Font::new(Squares::default(), FontConfig::default(), 1.0).unwrap();
        let styles =
            [(false, false), (true, false), (false, true), (true, true)].map(|(bold, italic)| {
                Style {
                    bold,
                    italic,
                    ..Style::DEFAULT
                }
            });
        for _ in 0..2 {
            font.begin_frame();
            for text in ["a", "é", "A"] {
                for style in &styles {
                    font.glyph(&shown(text, style));
                }
            }
        }
        // Mono's two faces, as italic slants them, each asked once for each
        // character, the one that they lack too.
        let looked_up = &font.backend.looked_up;
        assert_eq!(looked_up.len(), 6, "{looked_up:?}");
        assert!(looked_up.values().all(|&count| count == 1), "{looked_up:?}");
    }

    #[test]
    fn a_character_that_the_face_lacks_takes_the_cascade_s_face() {
        let mut font = Font::new(Squares::default(), FontConfig::default(), 1.0).unwrap();
        let bold = Style {
            bold: true,
            ..Style::DEFAULT
        };
        for _ in 0..2 {
            font.glyph(&shown("日", &Style::DEFAULT)).unwrap();
            font.glyph(&shown("日", &bold)).unwrap();
        }
        // The cascade's face draws it, asked once from the regular face and
        // once from the bold one; the two find the same face here, and the
        // same glyph of the atlas, as Mono's bold is drawn as it is.
        assert_eq!(font.backend.drawn.get(&(CJK, 0x65E5)), Some(&1));
        assert_eq!(font.backend.fallbacks, 2);
        // A character that the cascade lacks too has no glyph.
        assert_eq!(font.glyph(&shown("A", &Style::DEFAULT)), None);
        assert_eq!(font.glyph(&shown("A", &Style::DEFAULT)), None);
        assert_eq!(font.backend.fallbacks, 3);
    }

    #[test]
    fn vs16_asks_for_the_emoji() {
        let mut font = Font::new(Squares::default(), FontConfig::default(), 1.0).unwrap();
        // `❤`, which Mono draws as text, and with VS16, as an emoji.
        font.glyph(&shown("\u{2764}", &Style::DEFAULT)).unwrap();
        font.glyph(&shown("\u{2764}\u{FE0F}", &Style::DEFAULT))
            .unwrap();
        assert_eq!(font.backend.drawn.get(&(FaceId(0), 0x2764)), Some(&1));
        assert_eq!(font.backend.drawn.get(&(EMOJI, 0x2764)), Some(&1));
    }

    /// The glyphs of a run of `clusters`, a cell each, wide ones with an
    /// emoji, in the regular face: each as its cell, its glyph's code, and
    /// how far it moves from the cell's pen.
    fn run(font: &mut Font<Squares>, clusters: &[&str]) -> Vec<(usize, u16, i32)> {
        let text = clusters.concat();
        let mut cells = Vec::new();
        let mut start = 0;
        for cluster in clusters {
            cells.push(crate::pane::RunCell {
                start,
                wide: cluster.chars().next().is_some_and(|c| c > '\u{1F000}'),
            });
            start += cluster.len() as u32;
        }
        let run = Run {
            text: &text,
            cells: &cells,
            style: &Style::DEFAULT,
            dark: false,
        };
        let mut out = Vec::new();
        font.lay_out(&run, &mut out);
        // The glyph's code from the atlas's key, as the backend drew it.
        out.iter()
            .map(|placed| {
                let code = font
                    .atlas_keys()
                    .find(|(_, region)| *region == placed.glyph.region)
                    .map_or(0, |(key, _)| key.glyph.0);
                (placed.cell, code, placed.x)
            })
            .collect()
    }

    #[test]
    fn a_ligature_shapes_across_its_cells() {
        let mut font = Font::new(Squares::default(), FontConfig::default(), 1.0).unwrap();
        // `->` takes its alternates, a glyph a cell.
        assert_eq!(
            run(&mut font, &["a", "-", ">", "b"]),
            [(0, 0x61, 0), (1, 0xE100, 0), (2, 0xE101, 0), (3, 0x62, 0)]
        );
        // `fi` takes one glyph, at its first cell, and none at its second.
        assert_eq!(
            run(&mut font, &["f", "i", "x"]),
            [(0, 0xFB01, 0), (2, 0x78, 0)]
        );
        // Without ligatures, each character its own glyph.
        let config = FontConfig {
            ligatures: false,
            ..FontConfig::default()
        };
        let mut font = Font::new(Squares::default(), config, 1.0).unwrap();
        assert_eq!(
            run(&mut font, &["a", "-", ">"]),
            [(0, 0x61, 0), (1, 0x2D, 0), (2, 0x3E, 0)]
        );
        assert_eq!(font.backend.shapings, 0, "no ligature, nothing to shape");
    }

    #[test]
    fn a_combining_mark_goes_over_its_base_in_its_cell() {
        let mut font = Font::new(Squares::default(), FontConfig::default(), 10.0 / 13.0).unwrap();
        // At 10 pixels, an advance of 6: the accent goes back half of it,
        // from the pen after its base, in the base's cell.
        assert_eq!(
            run(&mut font, &["e\u{301}", "x"]),
            [(0, 0x65, 0), (0, 0x301, 3), (1, 0x78, 0)]
        );
    }

    #[test]
    fn a_cell_alone_is_not_shaped_and_a_run_is_shaped_once() {
        let mut font = Font::new(Squares::default(), FontConfig::default(), 1.0).unwrap();
        run(&mut font, &["a"]);
        run(&mut font, &["\u{2714}"]);
        assert_eq!(font.backend.shapings, 0);
        for _ in 0..3 {
            font.begin_frame();
            run(&mut font, &["a", "-", ">"]);
        }
        assert_eq!(font.backend.shapings, 1);
        assert_eq!(font.shaped.shapings, 1);
    }

    #[test]
    fn an_emoji_sequence_takes_one_glyph() {
        let mut font = Font::new(Squares::default(), FontConfig::default(), 1.0).unwrap();
        // ❤️ and a ZWJ, as in ❤️‍🔥, of the codes that the fake backend
        // draws.
        let family = "\u{2764}\u{FE0F}\u{200D}\u{2B50}";
        font.glyph(&shown(family, &Style::DEFAULT)).unwrap();
        assert_eq!(font.backend.drawn.get(&(EMOJI, 0xF46A)), Some(&1));
        // In a run, alone, at its cell.
        assert_eq!(
            run(&mut font, &["a", family, "b"]),
            [(0, 0x61, 0), (1, 0xF46A, 0), (2, 0x62, 0)]
        );
    }

    #[test]
    fn a_cluster_that_the_shaping_takes_to_another_face_keeps_its_glyphs() {
        let mut font = Font::new(Squares::default(), FontConfig::default(), 1.0).unwrap();
        // `1⃣`, which the shaping draws in the face of keycaps after a lead,
        // in that face, and `x` after it in its cell, unmoved by the lead.
        assert_eq!(
            run(&mut font, &["a", "1\u{20E3}", "x"]),
            [(0, 0x61, 0), (1, 0x31, 0), (1, 0x20E3, 0), (2, 0x78, 0)]
        );
        assert_eq!(font.backend.drawn.get(&(KEYCAPS, 0x31)), Some(&1));
        assert_eq!(font.backend.drawn.get(&(KEYCAPS, 0x20E3)), Some(&1));
        assert_eq!(font.backend.drawn.get(&(FaceId(0), 0x31)), None);
        // Drawn upright and of its own weight in bold text.
        let bold = Style {
            bold: true,
            ..Style::DEFAULT
        };
        let text = "1\u{20E3}";
        let cells = [crate::pane::RunCell {
            start: 0,
            wide: false,
        }];
        let run = Run {
            text,
            cells: &cells,
            style: &bold,
            dark: false,
        };
        font.lay_out(&run, &mut Vec::new());
        assert_eq!(
            font.backend.synthesized[&(KEYCAPS, 0x31)],
            Synthesis::default()
        );
    }

    #[test]
    fn a_skin_tone_s_sequence_takes_its_emoji() {
        let mut font = Font::new(Squares::default(), FontConfig::default(), 1.0).unwrap();
        // ☝, of text presentation, which Mono has, takes the emoji with a
        // skin tone, which the emoji face shapes into one glyph.
        let raised = "\u{261D}\u{1F3FD}";
        assert_eq!(
            run(&mut font, &["a", raised, "b"]),
            [(0, 0x61, 0), (1, 0xF46B, 0), (2, 0x62, 0)]
        );
        assert_eq!(font.backend.drawn.get(&(EMOJI, 0xF46B)), Some(&1));
        assert_eq!(font.backend.drawn.get(&(FaceId(0), 0x261D)), None);
        // Alone, its cell's glyph is the same.
        font.glyph(&shown(raised, &Style::DEFAULT)).unwrap();
        assert_eq!(font.backend.drawn.get(&(EMOJI, 0xF46B)), Some(&1));
    }

    #[test]
    fn an_emoji_s_sequence_is_a_pictograph_with_a_skin_tone_or_a_zwj() {
        assert!(is_emoji_sequence("\u{261D}\u{1F3FD}"));
        assert!(is_emoji_sequence("\u{2764}\u{200D}\u{1F525}"));
        // A pictograph alone, a mark on a letter, and Devanagari's ZWJ.
        assert!(!is_emoji_sequence("\u{261D}"));
        assert!(!is_emoji_sequence("e\u{301}"));
        assert!(!is_emoji_sequence("\u{915}\u{94D}\u{200D}"));
        assert_eq!(
            Presentation::asked("\u{261D}\u{1F3FD}"),
            Some(Presentation::Emoji)
        );
        assert_eq!(Presentation::asked("\u{915}\u{94D}\u{200D}"), None);
    }

    #[test]
    fn a_face_that_substitutes_nothing_shapes_only_its_clusters() {
        let config = FontConfig {
            family: Some(lokyr_text_core::Name::try_from("Wide".to_owned()).unwrap()),
            ..FontConfig::default()
        };
        let mut font = Font::new(Squares::default(), config, 1.0).unwrap();
        for _ in 0..3 {
            run(&mut font, &["a", "-", ">"]);
        }
        assert_eq!(font.backend.shapings, 0);
        // A cluster of several characters is shaped still.
        run(&mut font, &["e\u{301}", "x"]);
        assert_eq!(font.backend.shapings, 1);
        // The face is asked once.
        assert_eq!(font.backend.substituting, [WIDE]);
    }

    /// Lays out a run of `text`, a cell a byte, in `font`'s regular face.
    fn lay_out_ascii(font: &mut Font<Squares>, text: &str) {
        let cells: Vec<_> = (0..text.len() as u32)
            .map(|start| crate::pane::RunCell { start, wide: false })
            .collect();
        let run = Run {
            text,
            cells: &cells,
            style: &Style::DEFAULT,
            dark: false,
        };
        font.lay_out(&run, &mut Vec::new());
    }

    #[test]
    fn a_frame_of_more_runs_than_the_cache_s_floor_lays_out_again_without_shaping() {
        let mut font = Font::new(Squares::default(), FontConfig::default(), 1.0).unwrap();
        let rows: Vec<String> = (0..SHAPED_RUNS + 1000)
            .map(|row| format!("{row:05}"))
            .collect();
        for _ in 0..3 {
            font.begin_frame();
            for row in &rows {
                lay_out_ascii(&mut font, row);
            }
        }
        assert_eq!(font.backend.shapings, rows.len());
        // The newer generation keeps them, as twice the frame's runs fit.
        font.begin_frame();
        assert!(font.shaped.older.is_empty());
    }

    #[test]
    fn rows_without_spaces_keep_the_cache_within_its_glyphs() {
        // 80 screens of 120 rows of 400 cells, of base64's characters but
        // `A`, which the fake backend lacks: a run of 400 glyphs a row.
        const ALPHABET: &[u8] = b"BCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let (rows, columns) = (120, 400);
        let mut font = Font::new(Squares::default(), FontConfig::default(), 1.0).unwrap();
        let mut state = 0x9E37_79B9_7F4A_7C15_u64;
        let mut screen = Vec::new();
        for _ in 0..80 {
            screen = (0..rows)
                .map(|_| {
                    (0..columns)
                        .map(|_| {
                            state ^= state << 13;
                            state ^= state >> 7;
                            state ^= state << 17;
                            char::from(ALPHABET[(state % ALPHABET.len() as u64) as usize])
                        })
                        .collect::<String>()
                })
                .collect::<Vec<_>>();
            font.begin_frame();
            for row in &screen {
                lay_out_ascii(&mut font, row);
            }
            let older: usize = font.shaped.older.values().map(|run| run.len()).sum();
            let held = font.shaped.glyphs + older;
            let frame = rows * columns;
            assert!(
                held <= 2 * (SHAPED_GLYPHS.max(2 * frame) + frame),
                "{held} glyphs held"
            );
        }
        assert_eq!(font.backend.shapings, 80 * rows);
        // The last screen, once more, is in the cache.
        font.begin_frame();
        for row in &screen {
            lay_out_ascii(&mut font, row);
        }
        assert_eq!(font.backend.shapings, 80 * rows);
    }

    #[test]
    fn box_drawing_and_powerline_are_drawn_to_the_cell() {
        let mut font = Font::new(Squares::default(), FontConfig::default(), 1.0).unwrap();
        let cell = *font.cell();
        for text in ["\u{2500}", "\u{2588}", "\u{28FF}", "\u{E0B0}"] {
            let glyph = font.glyph(&shown(text, &Style::DEFAULT)).unwrap();
            // The cell's whole size, on its top left corner.
            assert_eq!(
                (glyph.region.width, glyph.region.height),
                (cell.width, cell.height)
            );
            assert_eq!((glyph.left, glyph.top), (0, cell.baseline as i32));
        }
        // Not the font's, and not shaped, in a run of them.
        assert!(font.backend.drawn.is_empty(), "{:?}", font.backend.drawn);
        let line = ["\u{2500}"; 4];
        let laid = run(&mut font, &line);
        assert_eq!(
            laid.iter().map(|&(cell, ..)| cell).collect::<Vec<_>>(),
            [0, 1, 2, 3]
        );
        assert_eq!(font.backend.shapings, 0);
        // With the keys off, the font's.
        let config = FontConfig {
            builtin_box_drawing: false,
            builtin_powerline: false,
            ..FontConfig::default()
        };
        let mut font = Font::new(Squares::default(), config, 1.0).unwrap();
        font.glyph(&shown("\u{2500}", &Style::DEFAULT)).unwrap();
        font.glyph(&shown("\u{E0B0}", &Style::DEFAULT)).unwrap();
        assert_eq!(font.backend.drawn.get(&(FaceId(0), 0x2500)), Some(&1));
        assert_eq!(font.backend.drawn.get(&(FaceId(0), 0xE0B0)), Some(&1));
        // A powerline symbol that the font lacks, the terminal's, before
        // the symbols' font, which would draw it short of the cell.
        let config = FontConfig {
            family: Some(lokyr_text_core::Name::try_from("Wide".to_owned()).unwrap()),
            builtin_powerline: false,
            ..FontConfig::default()
        };
        let mut font = Font::new(Squares::default(), config, 1.0).unwrap();
        let found = font.find(0, '\u{E0B1}', None).map(|found| found.face);
        assert_eq!(found, Some(SPRITES));
        assert_eq!(
            font.find(0, '\u{E0B0}', None).map(|found| found.face),
            Some(WIDE)
        );
    }

    #[test]
    fn the_terminal_s_drawings_of_a_wide_cell_take_both_columns() {
        // As `ambiguous-width = 2` gives box drawing, blocks and the
        // powerline's symbols.
        let mut font = Font::new(Squares::default(), FontConfig::default(), 1.0).unwrap();
        let cell = font.cell;
        for character in ["\u{2500}", "\u{2588}", "\u{E0B0}"] {
            let narrow = font.glyph(&shown(character, &Style::DEFAULT)).unwrap();
            assert_eq!(narrow.region.width, cell.width, "{character}");
            let two = CellText {
                wide: true,
                ..shown(character, &Style::DEFAULT)
            };
            let wide = font.glyph(&two).unwrap();
            assert_eq!(wide.region.width, 2 * cell.width, "{character}");
            assert_eq!(wide.left, narrow.left, "{character}");
        }
        // In a run, each cell's own.
        let cells =
            [(0, false), (3, true)].map(|(start, wide)| crate::pane::RunCell { start, wide });
        let run = Run {
            text: "\u{2500}\u{2500}",
            cells: &cells,
            style: &Style::DEFAULT,
            dark: false,
        };
        let mut out = Vec::new();
        font.lay_out(&run, &mut out);
        let widths: Vec<_> = out.iter().map(|placed| placed.glyph.region.width).collect();
        assert_eq!(widths, [cell.width, 2 * cell.width]);
    }

    /// Whether the fake backend drew `glyph` of `face` thin, at its last
    /// drawing.
    fn drawn_thin(font: &Font<Squares>, face: FaceId, glyph: u16) -> Option<bool> {
        font.backend
            .rendered
            .iter()
            .rev()
            .find(|&&(drawn, code, _)| (drawn, code) == (face, glyph))
            .map(|&(.., rendering)| rendering.thin)
    }

    #[test]
    fn thin_strokes_follow_the_scale_and_the_background() {
        let config = |strokes| FontConfig {
            thin_strokes: strokes,
            ..FontConfig::default()
        };
        let dark = |text: &'static str| CellText {
            text,
            style: &Style::DEFAULT,
            wide: false,
            dark: true,
        };
        // On a Retina display only, whatever the background.
        let mut font = Font::new(Squares::default(), config(ThinStrokes::Retina), 1.0).unwrap();
        font.glyph(&dark("a")).unwrap();
        assert_eq!(drawn_thin(&font, FaceId(0), 0x61), Some(false));
        font.set_scale(2.0);
        font.glyph(&shown("a", &Style::DEFAULT)).unwrap();
        assert_eq!(drawn_thin(&font, FaceId(0), 0x61), Some(true));
        // On a dark background only: the atlas keeps both forms of a glyph.
        let mut font = Font::new(Squares::default(), config(ThinStrokes::Dark), 1.0).unwrap();
        let light = font.glyph(&shown("b", &Style::DEFAULT)).unwrap();
        let thin = font.glyph(&dark("b")).unwrap();
        assert_ne!(light.region, thin.region);
        assert_eq!(font.glyph(&dark("b")), Some(thin));
        assert_eq!(font.backend.drawn[&(FaceId(0), 0x62)], 2);
        // Never, by default.
        let mut font = Font::new(Squares::default(), FontConfig::default(), 2.0).unwrap();
        font.glyph(&dark("c")).unwrap();
        assert_eq!(drawn_thin(&font, FaceId(0), 0x63), Some(false));
    }

    #[test]
    fn without_antialiasing_the_glyphs_and_the_sprites_are_covered_or_not() {
        let config = FontConfig {
            antialias: false,
            thin_strokes: ThinStrokes::Always,
            ..FontConfig::default()
        };
        let mut font = Font::new(Squares::default(), config, 2.0).unwrap();
        font.glyph(&shown("a", &Style::DEFAULT)).unwrap();
        let (.., rendering) = font.backend.rendered[0];
        // Thin strokes need antialiasing.
        assert_eq!(
            rendering,
            Rendering {
                antialias: false,
                ..Rendering::default()
            }
        );
        // An arc, which the terminal draws with its edges antialiased.
        font.glyph(&shown("\u{256D}", &Style::DEFAULT)).unwrap();
        let uploads = font.texture_updates();
        let arc = uploads
            .iter()
            .filter_map(|update| match update {
                TextureUpdate::Upload { data, .. } => Some(data),
                TextureUpdate::Grown { .. } => None,
            })
            .next_back()
            .unwrap();
        assert!(arc.contains(&255));
        assert!(arc.iter().all(|&pixel| pixel == 0 || pixel == 255));
    }

    #[test]
    fn bold_and_italic_without_their_faces_take_the_regular_one() {
        let mut font = Font::new(Squares::default(), FontConfig::default(), 1.0).unwrap();
        font.set_text(TextConfig {
            bold: false,
            italic: false,
            ..TextConfig::default()
        });
        let styled = Style {
            bold: true,
            italic: true,
            ..Style::DEFAULT
        };
        font.glyph(&shown("x", &styled)).unwrap();
        // Mono's regular face, not its bold one, and not slanted.
        assert_eq!(font.backend.drawn.get(&(FaceId(1), 0x78)), None);
        assert!(!font.backend.synthesized[&(FaceId(0), 0x78)].italic);
        // With them, the bold face slanted, as Mono has no italic.
        font.set_text(TextConfig::default());
        font.glyph(&shown("x", &styled)).unwrap();
        assert!(font.backend.synthesized[&(FaceId(1), 0x78)].italic);
    }

    #[test]
    fn a_keycap_leaves_its_digit_s_text_as_it_is() {
        // The keycap first, then the digit, and the other way round.
        for order in [["1\u{FE0F}\u{20E3}", "1"], ["1", "1\u{FE0F}\u{20E3}"]] {
            let mut font = Font::new(Squares::default(), FontConfig::default(), 1.0).unwrap();
            for text in order {
                font.glyph(&shown(text, &Style::DEFAULT)).unwrap();
            }
            font.glyph(&shown("1", &Style::DEFAULT)).unwrap();
            assert_eq!(font.backend.drawn.get(&(FaceId(0), 0x31)), Some(&1));
            assert_eq!(font.backend.drawn.get(&(EMOJI, 0x31)), Some(&1));
        }
    }

    #[test]
    fn a_character_takes_the_presentation_that_unicode_gives_it() {
        let mut font = Font::new(Squares::default(), FontConfig::default(), 1.0).unwrap();
        // ⚡, of emoji presentation, takes the emoji though Mono has it, and
        // with VS15 Mono's text.
        font.glyph(&shown("\u{26A1}", &Style::DEFAULT)).unwrap();
        font.glyph(&shown("\u{26A1}\u{FE0E}", &Style::DEFAULT))
            .unwrap();
        assert_eq!(font.backend.drawn.get(&(EMOJI, 0x26A1)), Some(&1));
        assert_eq!(font.backend.drawn.get(&(FaceId(0), 0x26A1)), Some(&1));
        // ✔, of text presentation, which Mono lacks, asks the cascade for
        // its text, with VS15; 日, which has no emoji, as it is.
        font.glyph(&shown("\u{2714}", &Style::DEFAULT)).unwrap();
        font.glyph(&shown("日", &Style::DEFAULT)).unwrap();
        assert_eq!(font.backend.drawn.get(&(SYMBOLS, 0x2714)), Some(&1));
        assert!(font.backend.asked.contains(&"\u{2714}\u{FE0E}".to_owned()));
        assert!(font.backend.asked.contains(&"日".to_owned()));
        // An emoji is upright, of its own weight, in italic text that Mono
        // slants; text of the cascade is slanted as the style is.
        let italic = Style {
            italic: true,
            ..Style::DEFAULT
        };
        font.glyph(&shown("\u{2615}", &italic)).unwrap();
        font.glyph(&shown("本", &italic)).unwrap();
        assert_eq!(
            font.backend.synthesized[&(EMOJI, 0x2615)],
            Synthesis::default()
        );
        assert!(font.backend.synthesized[&(CJK, 0x672C)].italic);
        // A clock with VS15, which no face draws as text, takes its emoji.
        font.glyph(&shown("\u{231A}\u{FE0E}", &Style::DEFAULT))
            .unwrap();
        assert_eq!(font.backend.drawn.get(&(EMOJI, 0x231A)), Some(&1));
        // An emoji that its face lacks keeps the text's glyph.
        font.glyph(&shown("\u{E000}\u{FE0F}", &Style::DEFAULT))
            .unwrap();
        assert_eq!(font.backend.drawn.get(&(FaceId(0), 0xE000)), Some(&1));
    }

    #[test]
    fn the_non_ascii_family_takes_the_text_past_ascii() {
        let config = |family: &str| FontConfig {
            non_ascii_family: Some(lokyr_text_core::Name::try_from(family.to_owned()).unwrap()),
            ..FontConfig::default()
        };
        let mut font = Font::new(Squares::default(), config("Wide"), 1.0).unwrap();
        assert_eq!(font.missing_non_ascii_family(), None);
        for text in ["a", "é", "日"] {
            font.glyph(&shown(text, &Style::DEFAULT)).unwrap();
        }
        assert_eq!(font.backend.drawn.get(&(FaceId(0), 0x61)), Some(&1));
        assert_eq!(font.backend.drawn.get(&(WIDE, 0xE9)), Some(&1));
        assert_eq!(font.backend.drawn.get(&(WIDE, 0x65E5)), Some(&1));
        // A character that it lacks takes the text's family's glyph, before
        // the cascade.
        font.glyph(&shown("\u{E0A0}", &Style::DEFAULT)).unwrap();
        assert_eq!(font.backend.drawn.get(&(FaceId(0), 0xE0A0)), Some(&1));
        // A family that is not installed leaves that text to the text's
        // family.
        let mut font = Font::new(Squares::default(), config("Nope"), 1.0).unwrap();
        assert_eq!(font.missing_non_ascii_family(), Some("Nope"));
        font.glyph(&shown("é", &Style::DEFAULT)).unwrap();
        assert_eq!(font.backend.drawn.get(&(FaceId(0), 0xE9)), Some(&1));
    }

    #[test]
    fn a_glyph_that_the_atlas_refuses_is_not_drawn_at_each_frame() {
        // At 200 points and a scale of 50, the fake backend's squares are
        // 5000 pixels a side, past the atlas's largest texture.
        let config = FontConfig {
            size: lokyr_text_core::Size::try_from(200.0).unwrap(),
            ..FontConfig::default()
        };
        let mut font = Font::new(Squares::default(), config, 50.0).unwrap();
        for _ in 0..REFUSED_FRAMES {
            font.begin_frame();
            assert_eq!(font.glyph(&shown("x", &Style::DEFAULT)), None);
        }
        assert_eq!(font.backend.drawn.get(&(FaceId(0), 0x78)), Some(&1));
        // Later, it is drawn again, as the atlas may have room for it then.
        font.begin_frame();
        font.glyph(&shown("x", &Style::DEFAULT));
        assert_eq!(font.backend.drawn.get(&(FaceId(0), 0x78)), Some(&2));
    }

    #[test]
    fn a_glyph_that_the_atlas_evicted_is_drawn_again() {
        // At 200 points and a scale of 21, the fake backend's squares are
        // 2100 pixels a side: the atlas's largest texture holds one.
        let config = FontConfig {
            size: lokyr_text_core::Size::try_from(200.0).unwrap(),
            ..FontConfig::default()
        };
        let mut font = Font::new(Squares::default(), config, 21.0).unwrap();
        let style = Style::DEFAULT;
        font.begin_frame();
        let first = font.glyph(&shown("a", &style)).unwrap();
        assert_eq!(font.glyph(&shown("a", &style)), Some(first));
        // Two frames later, `b` takes its place, and later `a` takes it back.
        font.begin_frame();
        font.begin_frame();
        let second = font.glyph(&shown("b", &style)).unwrap();
        assert_eq!(second.region, first.region);
        font.begin_frame();
        font.begin_frame();
        font.glyph(&shown("a", &style)).unwrap();
        assert_eq!(font.backend.drawn[&(FaceId(0), 0x61)], 2);
    }

    #[test]
    fn a_character_that_the_face_lacks_has_no_glyph() {
        let mut font = Font::new(Squares::default(), FontConfig::default(), 1.0).unwrap();
        assert_eq!(font.glyph(&shown("A", &Style::DEFAULT)), None);
        assert!(font.texture_updates().is_empty());
    }

    #[test]
    fn another_scale_measures_the_cell_again() {
        let mut font = Font::new(Squares::default(), FontConfig::default(), 1.0).unwrap();
        assert_eq!(font.cell().width, 8);
        let small = font.glyph(&shown("m", &Style::DEFAULT)).unwrap();
        font.set_scale(2.0);
        assert_eq!(font.cell().width, 16);
        let large = font.glyph(&shown("m", &Style::DEFAULT)).unwrap();
        assert_ne!(small.region, large.region);
        assert_eq!(font.texture_sizes(), (1024, 1024));
    }

    /// The glyphs of `text` at `places`, each a glyph of the cluster at
    /// that byte, of a run from right to left where `backward`.
    fn glyphs_at(places: &[(usize, f32)], backward: bool) -> Vec<ShapedGlyph> {
        places
            .iter()
            .map(|&(cluster, x)| ShapedGlyph {
                face: FaceId(0),
                glyph: GlyphId(1),
                cluster: cluster as u32,
                x,
                y: 0.0,
                right_to_left: backward,
            })
            .collect()
    }

    #[test]
    fn the_caret_s_places_are_each_cluster_s_leading_edge() {
        // From left to right, the end at the last cluster's right.
        let glyphs = glyphs_at(&[(0, 0.0), (1, 10.0), (2, 20.0)], false);
        assert_eq!(
            carets_of("abc", &glyphs, 30.0),
            [(0, 0.0), (1, 10.0), (2, 20.0), (3, 30.0)]
        );
        // From right to left, as CoreText gives a run of Hebrew, in its
        // order on the line: each cluster's right, the end at the last
        // cluster's left.
        let glyphs = glyphs_at(&[(4, 0.0), (2, 10.0), (0, 20.0)], true);
        assert_eq!(
            carets_of("אבג", &glyphs, 30.0),
            [(0, 30.0), (2, 20.0), (4, 10.0), (6, 0.0)]
        );
        // Hebrew after Latin text: typing at the end goes on at the Hebrew's
        // left, after the space.
        let mut glyphs = glyphs_at(&[(0, 0.0), (1, 10.0), (2, 20.0)], false);
        glyphs.extend(glyphs_at(&[(5, 30.0), (3, 40.0)], true));
        assert_eq!(
            carets_of("ab אב", &glyphs, 50.0),
            [
                (0, 0.0),
                (1, 10.0),
                (2, 20.0),
                (3, 50.0),
                (5, 40.0),
                (7, 30.0)
            ]
        );
    }

    #[test]
    fn a_ligature_s_clusters_share_it_and_a_mark_is_its_letter_s() {
        // One glyph of two clusters, each half of it, in its direction.
        let glyphs = glyphs_at(&[(0, 0.0), (2, 20.0)], false);
        assert_eq!(
            carets_of("fix", &glyphs, 30.0),
            [(0, 0.0), (1, 10.0), (2, 20.0), (3, 30.0)]
        );
        let glyphs = glyphs_at(&[(0, 0.0)], true);
        assert_eq!(
            carets_of("לא", &glyphs, 20.0),
            [(0, 20.0), (2, 10.0), (4, 0.0)]
        );
        // A mark drawn over its letter, its glyph within the letter's.
        let glyphs = glyphs_at(&[(0, 0.0), (1, 2.0), (3, 10.0)], false);
        assert_eq!(
            carets_of("e\u{301}x", &glyphs, 20.0),
            [(0, 0.0), (3, 10.0), (4, 20.0)]
        );
        // No glyph, every place at the start and the end at the width.
        assert_eq!(carets_of("ab", &[], 5.0), [(0, 0.0), (1, 0.0), (2, 5.0)]);
        // A cluster before the first of glyphs, at that one's leading edge.
        let glyphs = glyphs_at(&[(1, 10.0), (2, 20.0)], false);
        assert_eq!(
            carets_of("abc", &glyphs, 30.0),
            [(0, 10.0), (1, 10.0), (2, 20.0), (3, 30.0)]
        );
        let glyphs = glyphs_at(&[(2, 0.0)], true);
        assert_eq!(
            carets_of("אב", &glyphs, 10.0),
            [(0, 10.0), (2, 10.0), (4, 0.0)]
        );
        // A glyph past the text's width, of no width, not one that ends
        // before it starts.
        let glyphs = glyphs_at(&[(0, 0.0), (1, 25.0)], false);
        assert_eq!(
            carets_of("ab", &glyphs, 20.0),
            [(0, 0.0), (1, 25.0), (2, 25.0)]
        );
        assert_eq!(carets_of("", &[], 0.0), [(0, 0.0)]);
    }

    #[test]
    fn the_caret_s_places_of_a_long_text_cost_about_its_length() {
        // A cost of the square of the length would take a hundred times
        // as long of ten times the text.
        let cost = |length: usize| {
            let text = "ab ".repeat(length / 3);
            let places: Vec<(usize, f32)> = (0..text.len()).map(|at| (at, at as f32)).collect();
            let glyphs = glyphs_at(&places, false);
            (0..5)
                .map(|_| {
                    let start = Instant::now();
                    let carets = carets_of(&text, &glyphs, text.len() as f32);
                    assert_eq!(carets.len(), text.len() + 1);
                    start.elapsed()
                })
                .min()
                .unwrap()
        };
        let (short, long) = (cost(10_000), cost(100_000));
        assert!(long < short * 40, "{short:?} {long:?}");
    }

    #[test]
    fn the_small_size_takes_its_share_of_the_interface_s_until_the_regular_one_again() {
        let mut font = Font::new(Squares::default(), FontConfig::default(), 1.0).unwrap();
        let regular = font.interface().clone();
        let wide = Measure::text(&mut font, "find").0;
        let places = font.interface_carets("find");
        font.set_text_size(TextSize::Small);
        let small = font.interface().clone();
        assert_eq!(small.size(), regular.size() * SMALL);
        assert_eq!(small.family(), regular.family());
        assert!(Measure::text(&mut font, "find").0 < wide);
        assert_ne!(*font.interface_carets("find"), *places);
        font.set_text_size(TextSize::Regular);
        assert_eq!(font.interface().size(), regular.size());
        // A new config takes a new small face too.
        let larger = InterfaceConfig {
            font_size: lokyr_text_core::Size::try_from(26.0).unwrap(),
            ..InterfaceConfig::default()
        };
        font.set_interface(&larger);
        font.set_text_size(TextSize::Small);
        assert_eq!(font.interface().size(), 22.0);
    }

    #[test]
    fn a_typeset_takes_its_points_its_weight_s_nearest_face_and_the_terminal_s_family() {
        let mut font = Font::new(Squares::default(), FontConfig::default(), 1.0).unwrap();
        let regular = font.interface().clone();
        // Points in place of the interface's size, within their bounds.
        font.set_typeset(Typeset {
            points: Some(30.0),
            ..Typeset::default()
        });
        assert_eq!(font.interface().size(), 30.0);
        font.set_typeset(Typeset {
            points: Some(1000.0),
            size: TextSize::Small,
            ..Typeset::default()
        });
        assert_eq!(font.interface().size(), MOST_LARGE);
        // The family's face nearest to a weight, upright.
        font.set_typeset(Typeset {
            weight: FontWeight::Semibold,
            ..Typeset::default()
        });
        let bold = font.interface().clone();
        assert_eq!(
            (bold.face(), bold.size(), bold.family()),
            (FaceId(1), regular.size(), regular.family())
        );
        // The terminal's family, of its regular face and its size; its
        // weights of its own faces.
        font.set_typeset(Typeset {
            family: FontFamily::Terminal,
            ..Typeset::default()
        });
        let terminal = font.interface().clone();
        assert_eq!(
            (terminal.face(), terminal.size(), terminal.family()),
            (FaceId(0), font.config.size.points(), "Mono")
        );
        font.set_typeset(Typeset {
            family: FontFamily::Terminal,
            weight: FontWeight::Bold,
            points: Some(9.0),
            ..Typeset::default()
        });
        assert_eq!(
            (font.interface().face(), font.interface().size()),
            (FaceId(1), 9.0)
        );
        // The regular typeset again.
        font.set_typeset(Typeset::default());
        assert_eq!(*font.interface(), regular);
    }

    #[test]
    fn a_text_s_places_are_found_once() {
        let mut font = Font::new(Squares::default(), FontConfig::default(), 1.0).unwrap();
        let first = font.interface_carets("find");
        let shapings = font.shaped.shapings;
        let again = font.interface_carets("find");
        assert!(Rc::ptr_eq(&first, &again));
        assert_eq!(font.shaped.shapings, shapings);
        // Of another size, found anew.
        let larger = InterfaceConfig {
            font_size: lokyr_text_core::Size::try_from(20.0).unwrap(),
            ..InterfaceConfig::default()
        };
        font.set_interface(&larger);
        // Its face is found as its first text needs it, not as it is set.
        assert!(font.interface.is_none());
        assert_ne!(*font.interface_carets("find"), *first);
        assert!(font.interface.is_some());
        font.set_interface(&InterfaceConfig::default());
        assert!(Rc::ptr_eq(&first, &font.interface_carets("find")));
        // The last texts asked for kept, a text asked for again the newest,
        // the oldest past them found anew.
        for text in 1..CARET_TEXTS {
            font.interface_carets(&text.to_string());
        }
        assert!(Rc::ptr_eq(&first, &font.interface_carets("find")));
        font.interface_carets("0");
        assert!(Rc::ptr_eq(&first, &font.interface_carets("find")));
        for text in 0..CARET_TEXTS {
            font.interface_carets(&format!("new {text}"));
        }
        assert!(!Rc::ptr_eq(&first, &font.interface_carets("find")));
    }
}

/// The presentations as macOS's fonts give them: SF Mono, the default, and
/// the system's cascade.
#[cfg(all(test, target_os = "macos"))]
mod system {
    use lokyr_font_coretext::CoreText;
    use lokyr_vt_core::page::Style;

    use super::*;

    /// The texture of the glyph of `text` in `font`'s regular face.
    fn texture_of(font: &mut Font<CoreText>, text: &str) -> Option<Texture> {
        let shown = CellText {
            text,
            style: &Style::DEFAULT,
            wide: false,
            dark: false,
        };
        font.glyph(&shown).map(|glyph| glyph.region.texture)
    }

    #[test]
    fn the_caret_s_places_in_hebrew_go_from_its_right() {
        let mut font = Font::new(CoreText::new(), FontConfig::default(), 2.0).unwrap();
        // Each cluster left of the one before, the end at the line's left.
        let places = font.interface_carets("שלום");
        assert_eq!(places.len(), 5);
        assert!(
            places.windows(2).all(|pair| pair[1].1 < pair[0].1),
            "{places:?}"
        );
        assert!(places[4].1.abs() < 0.5, "{places:?}");
        // After Latin text, the Hebrew's start at the line's right, and its
        // end after the space, where typing goes on.
        let mixed = font.interface_carets("ab שלום");
        let (space, hebrew, end) = (mixed[2].1, &mixed[3..7], mixed[7].1);
        assert!(
            hebrew.windows(2).all(|pair| pair[1].1 < pair[0].1),
            "{mixed:?}"
        );
        assert!(space < end && end < hebrew[3].1, "{mixed:?}");
        assert!(mixed.iter().all(|&(_, x)| x <= hebrew[0].1), "{mixed:?}");
    }

    #[test]
    fn the_interface_s_face_is_the_system_s_of_the_optical_size_of_its_points() {
        let mut font = Font::new(CoreText::new(), FontConfig::default(), 2.0).unwrap();
        // Found as the interface first asks for it.
        assert!(font.interface.is_none());
        let mut backend = CoreText::new();
        assert_eq!(font.interface().family(), backend.interface_family());
        assert_eq!(font.interface().missing_family(), None);
        assert_eq!(font.interface().size(), 13.0);
        // A line of 13 points is about 16 high.
        let line = font.interface().line();
        assert!((14.0..18.0).contains(&line), "{line}");
        // As AppKit lays out "Hello, world" at 13 points, about 71.5.
        let (width, height) = font.text("Hello, world");
        assert!((width - 71.5).abs() < 1.5, "{width}");
        assert_eq!(height, line);
        let (narrow, _) = font.text("iiii");
        let (wide, _) = font.text("mmmm");
        assert!(narrow * 2.0 < wide, "{narrow} {wide}");
        assert_eq!(font.icon("terminal"), (line, line));
        // Its run at 2 pixels a point: a glyph of each letter, of coverage,
        // the space none, its ink ending near twice the points.
        let color = Color::from_rgb8(10, 20, 30);
        let (glyphs, end) = font.interface_text("Hello, world", (4.0, 30.0), color, false);
        assert_eq!(glyphs.len(), 11);
        assert!((end - 4.0 - 2.0 * width).abs() < 4.0, "{end} {width}");
        assert!(glyphs.iter().all(|glyph| glyph.color == color));
        assert!(glyphs.windows(2).all(|pair| pair[0].x < pair[1].x));
        // An `H` on the baseline: its top about the cap height above it.
        assert!((10..=20).contains(&(30 - glyphs[0].y)), "{}", glyphs[0].y);
        // An icon in the middle of its square, of most of it.
        let icon = font
            .interface_icon("terminal", (0.0, 0.0), 32.0, color, false)
            .unwrap();
        let (width, height) = (icon.region.width, icon.region.height);
        assert!(
            (20..=32).contains(&width) && (16..=32).contains(&height),
            "{icon:?}"
        );
        assert!(icon.x.abs_diff((32 - width as i32) / 2) <= 1, "{icon:?}");
        assert!(
            font.interface_icon("no-such-icon", (0.0, 0.0), 32.0, color, false)
                .is_none()
        );
        // A square of no size, or of less than a pixel, draws nothing, where
        // CoreText's font of no size is of 12 points.
        for side in [0.0, 0.5, -3.0, f32::NAN] {
            assert!(
                font.interface_icon("close", (100.0, 100.0), side, color, false)
                    .is_none(),
                "{side}"
            );
        }
        // An emoji of an opaque white, whose opacity a fade takes alone,
        // a letter of the text's color.
        let (glyphs, _) = font.interface_text("a🦀", (0.0, 30.0), color.with_alpha(0.5), false);
        let of = |texture| {
            glyphs
                .iter()
                .find(|glyph| glyph.region.texture == texture)
                .unwrap()
                .color
        };
        assert_eq!(of(Texture::Color), Color::WHITE);
        assert_eq!(of(Texture::Coverage), color.with_alpha(0.5));
        // Nor one past the largest, of 10⁹ pixels, which aborted the app
        // out of memory; the largest draws.
        for side in [ICON_LARGEST + 1.0, 1e9, f32::INFINITY] {
            assert!(
                font.interface_icon("close", (0.0, 0.0), side, color, false)
                    .is_none(),
                "{side}"
            );
        }
        assert!(
            font.interface_icon("close", (0.0, 0.0), ICON_LARGEST, color, false)
                .is_some()
        );
        // A family that is not installed: the system's, which tells it.
        let named = |name: &str| InterfaceConfig {
            font_family: Some(lokyr_text_core::Name::try_from(name.to_owned()).unwrap()),
            ..InterfaceConfig::default()
        };
        font.set_interface(&named("No Such Family Here"));
        assert_eq!(
            font.interface().missing_family(),
            Some("No Such Family Here")
        );
        assert_eq!(font.interface().family(), backend.interface_family());
        font.set_interface(&named("Helvetica"));
        assert_eq!(font.interface().family(), "Helvetica");
        assert_eq!(font.interface().missing_family(), None);
    }

    /// The frames, of a frame's budgets, that show a screen of 30 icons of a
    /// Nerd Font's plane, each other, which SF Mono lacks: each one's time.
    /// Run by hand, with the Nerd Fonts installed: `cargo test -p
    /// lokyr-ui-core icons_frames -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn icons_frames() {
        let mut font = Font::new(CoreText::new(), FontConfig::default(), 2.0).unwrap();
        let icons: Vec<String> = (0..30u32)
            .map(|n| char::from_u32(0xF_0001 + n * 37).unwrap().to_string())
            .collect();
        let mut frames = Vec::new();
        loop {
            let start = Instant::now();
            font.begin_frame();
            font.collect_missing(Duration::from_millis(2), Duration::from_millis(2));
            for icon in &icons {
                texture_of(&mut font, icon);
            }
            if font.draw_pending() {
                for icon in &icons {
                    texture_of(&mut font, icon);
                }
            }
            frames.push(start.elapsed());
            if !font.has_deferred() || frames.len() > 100 {
                break;
            }
        }
        let ms: Vec<String> = frames
            .iter()
            .map(|frame| format!("{:.2}", frame.as_secs_f64() * 1000.0))
            .collect();
        println!("{} frames, ms: {}", frames.len(), ms.join(" "));
    }

    #[test]
    fn the_icons_of_a_nerd_font_draw_from_the_symbols_font() {
        let mut font = Font::new(CoreText::new(), FontConfig::default(), 2.0).unwrap();
        let symbols = font.backend.symbols().expect("the symbols' font loads");
        // Font Awesome's house, a devicon, a Material Design icon, which SF
        // Mono lacks; of the first plane, the cascade's last resort's box.
        for icon in ['\u{F015}', '\u{E606}', '\u{F0001}'] {
            let found = font.find(0, icon, None).expect("a face");
            assert_eq!(found.face, symbols, "U+{:X}", u32::from(icon));
            // Within its cell, across, in its middle, but for the pixel that
            // the bitmap keeps past the outline on each side, and one of
            // rounding.
            let text = icon.to_string();
            let shown = CellText {
                text: &text,
                style: &Style::DEFAULT,
                wide: false,
                dark: false,
            };
            let glyph = font.glyph(&shown).expect("a glyph");
            let right = glyph.left + glyph.region.width as i32;
            let width = font.cell.width as i32;
            assert!(glyph.left >= -2, "U+{:X}: {glyph:?}", u32::from(icon));
            assert!(right <= width + 2, "U+{:X}: {glyph:?}", u32::from(icon));
            assert!(
                (glyph.left - (width - right)).abs() <= 1,
                "U+{:X}",
                u32::from(icon)
            );
        }
    }

    #[test]
    fn symbols_draw_as_text_and_emoji_as_emoji() {
        let mut font = Font::new(CoreText::new(), FontConfig::default(), 2.0).unwrap();
        // Symbols of text presentation, which command-line tools print, in
        // the text's color; those of emoji presentation in their own colors.
        for text in ["\u{2714}", "\u{26A0}", "\u{2139}", "\u{2764}", "\u{263A}"] {
            assert_eq!(
                texture_of(&mut font, text),
                Some(Texture::Coverage),
                "{text}"
            );
        }
        for text in ["\u{26A1}", "\u{2615}", "\u{2764}\u{FE0F}"] {
            assert_eq!(texture_of(&mut font, text), Some(Texture::Color), "{text}");
        }
        // A clock with VS15, which no face of the system draws as text, is
        // drawn still.
        assert!(texture_of(&mut font, "\u{231A}\u{FE0E}").is_some());
        // A keycap leaves the digit's text as it is.
        assert_eq!(
            texture_of(&mut font, "1\u{FE0F}\u{20E3}"),
            Some(Texture::Color)
        );
        assert_eq!(texture_of(&mut font, "1"), Some(Texture::Coverage));
    }

    /// A glyph laid out: its cell, its face, how far it moves from the
    /// cell's pen, and its texture.
    type LaidOut = (usize, FaceId, i32, Texture);

    /// The glyphs with ink that `font` lays out for a run of `clusters`, a
    /// cell each.
    fn laid_out(font: &mut Font<CoreText>, clusters: &[&str]) -> Vec<LaidOut> {
        let text = clusters.concat();
        let mut cells = Vec::new();
        let mut start = 0;
        for cluster in clusters {
            cells.push(crate::pane::RunCell { start, wide: false });
            start += cluster.len() as u32;
        }
        let run = Run {
            text: &text,
            cells: &cells,
            style: &Style::DEFAULT,
            dark: false,
        };
        let mut out = Vec::new();
        font.lay_out(&run, &mut out);
        out.iter()
            .filter(|placed| placed.glyph.region.width > 0)
            .map(|placed| {
                let region = placed.glyph.region;
                let face = font
                    .atlas_keys()
                    .find(|&(_, drawn)| drawn == region)
                    .map(|(key, _)| key.face)
                    .expect("a glyph of the atlas");
                (placed.cell, face, placed.x, region.texture)
            })
            .collect()
    }

    #[test]
    fn a_ligature_joins_across_its_cells() {
        // Hoefler Text, which macOS installs, joins `fi`.
        let name = lokyr_text_core::Name::try_from("Hoefler Text".to_owned()).unwrap();
        let config = FontConfig {
            family: Some(name),
            ..FontConfig::default()
        };
        let mut font = Font::new(CoreText::new(), config.clone(), 2.0).unwrap();
        let cells: Vec<usize> = laid_out(&mut font, &["f", "i", "x"])
            .into_iter()
            .map(|(cell, ..)| cell)
            .collect();
        assert_eq!(cells, [0, 2]);
        let apart = FontConfig {
            ligatures: false,
            ..config
        };
        let mut font = Font::new(CoreText::new(), apart, 2.0).unwrap();
        assert_eq!(laid_out(&mut font, &["f", "i", "x"]).len(), 3);
    }

    #[test]
    fn an_emoji_sequence_draws_as_one_glyph() {
        let mut font = Font::new(CoreText::new(), FontConfig::default(), 2.0).unwrap();
        let region = |font: &mut Font<CoreText>, text: &str| {
            let shown = CellText {
                text,
                style: &Style::DEFAULT,
                wide: true,
                dark: false,
            };
            font.glyph(&shown).map(|glyph| glyph.region)
        };
        // A ZWJ family, a flag, a keycap and a skin tone, each a glyph of
        // the emoji face, which is not its first character's.
        for (sequence, first) in [
            ("\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}", "\u{1F468}"),
            ("\u{1F1EE}\u{1F1F9}", "\u{1F1EE}"),
            ("1\u{FE0F}\u{20E3}", "1\u{FE0F}"),
            ("\u{1F44D}\u{1F3FD}", "\u{1F44D}"),
        ] {
            let whole = region(&mut font, sequence).expect("the sequence draws");
            assert_eq!(whole.texture, Texture::Color, "{sequence:?}");
            assert_ne!(Some(whole), region(&mut font, first), "{sequence:?}");
        }
    }

    #[test]
    fn a_cluster_of_a_mark_that_sf_mono_lacks_keeps_its_glyphs() {
        let mut font = Font::new(CoreText::new(), FontConfig::default(), 2.0).unwrap();
        let regular = font.resolved.faces.regular.id;
        // A long stroke and a keycap's mark, which CoreText draws with their
        // base in another font: the cluster's glyphs are drawn, and the `x`
        // after it is in its cell, unmoved.
        for (cluster, mark) in [("s\u{336}", '\u{336}'), ("1\u{20E3}", '\u{20E3}')] {
            let glyphs = laid_out(&mut font, &["a", cluster, "x"]);
            let marked: Vec<_> = glyphs.iter().filter(|glyph| glyph.0 == 1).collect();
            assert!(!marked.is_empty(), "{cluster:?}: {glyphs:?}");
            if font.backend.glyph(regular, mark).is_none() {
                assert!(
                    marked.iter().all(|glyph| glyph.1 != regular),
                    "{cluster:?}: {glyphs:?}"
                );
            }
            assert_eq!(glyphs[0], (0, regular, 0, Texture::Coverage));
            assert_eq!(
                glyphs.last(),
                Some(&(2, regular, 0, Texture::Coverage)),
                "{cluster:?}"
            );
        }
    }

    #[test]
    fn a_skin_tone_draws_a_pictograph_of_text_as_an_emoji() {
        let mut font = Font::new(CoreText::new(), FontConfig::default(), 2.0).unwrap();
        // ☝ is text alone, and an emoji with a skin tone, in a run too.
        assert_eq!(texture_of(&mut font, "\u{261D}"), Some(Texture::Coverage));
        let raised = "\u{261D}\u{1F3FD}";
        assert_eq!(texture_of(&mut font, raised), Some(Texture::Color));
        let glyphs = laid_out(&mut font, &["a", raised, "x"]);
        let textures: Vec<_> = glyphs.iter().map(|glyph| (glyph.0, glyph.3)).collect();
        assert_eq!(
            textures,
            [
                (0, Texture::Coverage),
                (1, Texture::Color),
                (2, Texture::Coverage)
            ]
        );
    }

    #[test]
    fn sf_mono_s_runs_skip_the_shaping() {
        let mut font = Font::new(CoreText::new(), FontConfig::default(), 2.0).unwrap();
        laid_out(&mut font, &["-", ">", "x"]);
        laid_out(&mut font, &["f", "i"]);
        assert_eq!(font.shaped.shapings, 0);
        // A cluster of several characters is shaped still.
        laid_out(&mut font, &["e\u{301}", "x"]);
        assert_eq!(font.shaped.shapings, 1);
    }

    /// With a Nerd Font and `non-ascii-family`, the icons stay the font's;
    /// nothing is checked where the font is not installed, as on CI.
    #[test]
    fn a_nerd_font_s_icons_stay_with_a_non_ascii_family() {
        let name = |family: &str| lokyr_text_core::Name::try_from(family.to_owned()).unwrap();
        let config = FontConfig {
            family: Some(name("CaskaydiaCove NFM")),
            non_ascii_family: Some(name("Hiragino Sans")),
            ..FontConfig::default()
        };
        let mut font = Font::new(CoreText::new(), config, 2.0).unwrap();
        if font.missing_family().is_some() || font.missing_non_ascii_family().is_some() {
            eprintln!(
                "CaskaydiaCove NFM or Hiragino Sans is not installed: the test checks nothing"
            );
            return;
        }
        let regular = font.resolved.faces.regular.id;
        for icon in ['\u{F015}', '\u{E0A0}'] {
            let found = font.find(0, icon, None);
            assert_eq!(found.map(|found| found.face), Some(regular), "{icon:?}");
        }
        // A powerline symbol is Lokyr's own.
        let arrow = font.find(0, '\u{E0B0}', None).map(|found| found.face);
        assert_eq!(arrow, Some(SPRITES));
        // Japanese takes the non-ASCII family.
        let japanese = font.find(0, '日', None).map(|found| found.face);
        assert_ne!(japanese, Some(regular));
    }
}
