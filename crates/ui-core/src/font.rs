//! The font that a window's text takes: its faces, the family of the text
//! past ASCII when the configuration names one, the faces of the system that
//! draw what they lack, its cell, and the atlas that holds the glyphs drawn
//! so far.

use lokyr_render_core::{AtlasRegion, Texture, TextureUpdate};
use lokyr_text_core::{
    Atlas, BitmapFormat, CellMetrics, Change, Face, FaceError, FaceId, Faces, FastMap, Features,
    FontBackend, FontConfig, GlyphId, Rendering, Resolved, ShapedGlyph, Synthesis, choose_faces,
    resolve_faces, sprite,
};
use lokyr_vt_core::unicode;

use crate::pane::{AtlasGlyph, CellText, Glyphs as GlyphSource, Run, RunGlyph};
use crate::text::TextConfig;

/// The face of the glyphs that the terminal draws itself, of box drawing
/// and of the powerline symbols, whose glyph is their character's code.
const SPRITES: FaceId = FaceId(u32::MAX);

/// The frames after which a glyph that the atlas refused is drawn again, as
/// the atlas may have room for it then: a second or two.
const REFUSED_FRAMES: u64 = 120;

/// The runs that the cache of shaped runs keeps in each of its two
/// generations: a screen's runs, and more.
const SHAPED_RUNS: usize = 4096;

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
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
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
}

/// The font of the text: the family and faces that the configuration names,
/// at a size in pixels, and the atlas of their glyphs.
#[derive(Debug)]
pub struct Font<B> {
    backend: B,
    config: FontConfig,
    resolved: Resolved,
    /// The faces of the text past ASCII, when the configuration names an
    /// installed family for it.
    non_ascii: Option<Faces>,
    /// The family that the configuration names for the text past ASCII,
    /// when it is not installed.
    missing_non_ascii: Option<String>,
    pixels: f32,
    /// The display's scale.
    scale: f32,
    /// Which styles take their faces.
    text: TextConfig,
    cell: CellMetrics,
    atlas: Atlas<GlyphKey>,
    glyphs: Glyphs,
    shaped: ShapedRuns,
    /// The glyphs of the run being laid out, reused from run to run.
    scratch: Vec<ShapedGlyph>,
    /// Where the run's cells find their glyphs, reused from run to run.
    found: Vec<Option<Found>>,
    /// The frames, counted from the first.
    frame: u64,
    /// The glyphs that the atlas refused, as too large or with no room, by
    /// the frame that it refused each, so that a frame does not draw them
    /// again at once.
    refused: FastMap<GlyphKey, u64>,
    /// The region of each glyph drawn, with its key, for the tests.
    #[cfg(test)]
    drawn: Vec<(AtlasRegion, GlyphKey)>,
}

/// The key of a shaped run: its face, the bits of its size, whether its
/// ligatures join, and its text.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
struct RunKey {
    face: u32,
    size: u32,
    ligatures: bool,
    text: String,
}

/// The runs shaped lately, so that a frame shapes only the runs that
/// changed: those of the frame before stay, in two generations, the older
/// dropped once the newer is full.
#[derive(Debug, Default)]
struct ShapedRuns {
    newer: FastMap<RunKey, Vec<ShapedGlyph>>,
    older: FastMap<RunKey, Vec<ShapedGlyph>>,
    /// The key being looked up, reused so that a lookup allocates nothing.
    key: RunKey,
    /// How many runs were shaped, for the tests.
    #[cfg(test)]
    shapings: usize,
}

impl ShapedRuns {
    /// Puts the glyphs of `text` shaped in `face` at `size` pixels, with
    /// `features`, into `glyphs`: from the cache, or shaped by `backend`.
    fn shape<B: FontBackend>(
        &mut self,
        backend: &mut B,
        face: FaceId,
        size: f32,
        features: Features,
        text: &str,
        glyphs: &mut Vec<ShapedGlyph>,
    ) {
        glyphs.clear();
        self.key.face = face.0;
        self.key.size = size.to_bits();
        self.key.ligatures = features.ligatures;
        self.key.text.clear();
        self.key.text.push_str(text);
        if let Some(known) = self.newer.get(&self.key) {
            glyphs.extend_from_slice(known);
            return;
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
                shaped
            }
        };
        glyphs.extend_from_slice(&known);
        if self.newer.len() >= SHAPED_RUNS {
            self.older = std::mem::take(&mut self.newer);
        }
        self.newer.insert(self.key.clone(), known);
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
        Ok(Self {
            backend,
            config,
            pixels,
            scale,
            text: TextConfig::default(),
            cell,
            atlas: Atlas::new(),
            glyphs: Glyphs::new(&resolved.faces, non_ascii.as_ref()),
            shaped: ShapedRuns::default(),
            scratch: Vec::new(),
            found: Vec::new(),
            frame: 0,
            refused: FastMap::default(),
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
        let metrics = self.backend.metrics(self.resolved.faces.regular.id, pixels);
        self.cell = CellMetrics::new(&metrics, &self.config);
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

    /// Starts a frame: the glyphs that it uses stay in the atlas.
    pub fn begin_frame(&mut self) {
        self.frame += 1;
        self.atlas.begin_frame();
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
        self.draw(found, self.thin(shown.dark))
    }

    /// The glyph of `found`, an emoji, of the whole `cluster`, when the
    /// cluster is a sequence that its face shapes into one glyph.
    fn sequence(&mut self, found: Found, cluster: &str) -> Found {
        if !found.emoji || cluster.chars().nth(1).is_none() {
            return found;
        }
        let mut shaped = std::mem::take(&mut self.scratch);
        let features = Features {
            ligatures: self.config.ligatures,
        };
        self.shaped.shape(
            &mut self.backend,
            found.face,
            self.pixels,
            features,
            cluster,
            &mut shaped,
        );
        let whole = match shaped.as_slice() {
            [one] => Found {
                glyph: one.glyph,
                ..found
            },
            _ => found,
        };
        self.scratch = shaped;
        whole
    }

    /// The glyph `found` in the atlas, of thin strokes when `thin`, drawn
    /// into it the first time, antialiased as the configuration says.
    fn draw(&mut self, found: Found, thin: bool) -> Option<AtlasGlyph> {
        let key = GlyphKey {
            face: found.face,
            glyph: found.glyph,
            size: self.pixels.to_bits(),
            synthesis: found.synthesis,
            thin,
        };
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
                let bitmap = if found.face == SPRITES {
                    let character = char::from_u32(u32::from(found.glyph.0))?;
                    let mut bitmap = sprite::draw(character, &self.cell)?;
                    // Without antialiasing, a pixel is covered or not.
                    if !antialias {
                        for pixel in &mut bitmap.data {
                            *pixel = if *pixel >= 128 { 255 } else { 0 };
                        }
                    }
                    bitmap
                } else {
                    let rendering = Rendering { antialias, thin };
                    self.backend.rasterize(
                        found.face,
                        found.glyph,
                        self.pixels,
                        found.synthesis,
                        rendering,
                    )
                };
                let Some(placement) = self.atlas.insert(key, &bitmap) else {
                    // The refusals of long ago go, rather than pile up.
                    let frame = self.frame;
                    self.refused.retain(|_, at| frame < *at + REFUSED_FRAMES);
                    self.refused.insert(key, frame);
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
        Some(AtlasGlyph {
            region,
            left: placement.left,
            top: placement.top,
        })
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
                if let Some(glyph) = self.draw(found, thin) {
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
            if self.needs_shaping(text, end - index) {
                self.shape_cells(run, (index, end), found, text, thin, out);
            } else {
                for (cell, found) in cells.iter().enumerate().take(end).skip(index) {
                    if let Some(glyph) = found.and_then(|found| self.draw(found, thin)) {
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

    /// Whether `text`, of `cells`, can change once shaped: it has a cluster
    /// of several characters, or, with ligatures, several cells, as a font's
    /// contextual alternates can change letters too.
    fn needs_shaping(&self, text: &str, cells: usize) -> bool {
        if self.config.ligatures && cells > 1 {
            return true;
        }
        // A cluster takes a byte at least: as many bytes as cells are as
        // many clusters of one character.
        text.len() != cells && text.chars().nth(cells).is_some()
    }

    /// Shapes `text`, the clusters of the cells of `run` from `first` to
    /// `end`, in `found`'s face, and appends its glyphs to `out`, of thin
    /// strokes when `thin`, each at
    /// the cell where its cluster starts, moved from that cell's pen as the
    /// shaping moves it from the cluster's.
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
        let mut shaped = std::mem::take(&mut self.scratch);
        self.shaped.shape(
            &mut self.backend,
            found.face,
            self.pixels,
            features,
            text,
            &mut shaped,
        );
        let base = run.cells[first].start;
        let starts = &run.cells[first..end];
        let (mut pen, mut cell_pen, mut at) = (0.0_f32, 0.0_f32, usize::MAX);
        for glyph in &shaped {
            let cluster = base + glyph.cluster;
            let cell = first
                + starts
                    .partition_point(|cell| cell.start <= cluster)
                    .saturating_sub(1);
            if cell != at {
                at = cell;
                cell_pen = pen;
            }
            let drawn = Found {
                glyph: glyph.glyph,
                ..found
            };
            if let Some(atlas) = self.draw(drawn, thin) {
                out.push(RunGlyph {
                    cell,
                    glyph: atlas,
                    x: (pen - cell_pen + glyph.x_offset).round() as i32,
                    y: glyph.y_offset.round() as i32,
                });
            }
            pen += glyph.x_advance;
        }
        self.scratch = shaped;
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
            let found = self.look_up(style, character, Presentation::Text);
            self.glyphs.ascii[table][character as usize] = Some(found);
            return found;
        }
        if let Some(known) = self.glyphs.others.get(&(table, character, asked)) {
            return *known;
        }
        let presentation = asked.unwrap_or_else(|| Presentation::default_of(character));
        let found = self
            .look_up(style, character, presentation)
            .map(|found| Found {
                emoji: presentation == Presentation::Emoji,
                ..found
            });
        self.glyphs.others.insert((table, character, asked), found);
        found
    }

    /// Asks the backend where `style` finds `character` in `presentation`.
    /// As text: in the text's faces, the non-ASCII family's and then the
    /// text's family's, and else in the face of the system's cascade that
    /// draws its text. As an emoji: in the cascade's face of its emoji, and
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
        if drawn && let Ok(code) = u16::try_from(u32::from(character)) {
            return Some(Found {
                face: SPRITES,
                synthesis: Synthesis::default(),
                glyph: GlyphId(code),
                emoji: false,
            });
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
    let fallback = *glyphs
        .fallbacks
        .entry((face.id, character, presentation))
        .or_insert_with(|| {
            let cluster: String = std::iter::once(character).chain(selector).collect();
            backend.fallback(face.id, &cluster)
        });
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
        })
    })
}

impl<B: FontBackend> GlyphSource for Font<B> {
    fn glyph(&mut self, shown: &CellText<'_>) -> Option<AtlasGlyph> {
        Font::glyph(self, shown)
    }

    fn run(&mut self, run: &Run<'_>, out: &mut Vec<RunGlyph>) {
        self.lay_out(run, out);
    }
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
    /// The face of its family Wide.
    const WIDE: FaceId = FaceId(5);

    /// A backend of two families: Mono, of a regular and a bold face, which
    /// lack `A`, ✔, ⌚ and CJK, and Wide, of one face, which lacks `A` and
    /// U+E0A0; and of a cascade of a face that draws CJK, one of emoji, for
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
                    },
                    FaceInfo {
                        id: FaceId(1),
                        style: "Bold".to_owned(),
                        weight: 700.0,
                        italic: false,
                        oblique: false,
                        width: 0.0,
                        in_group: false,
                    },
                ]
            })
        }

        fn metrics(&mut self, _: FaceId, size: f32) -> FaceMetrics {
            FaceMetrics {
                ascent: size * 0.8,
                descent: size * 0.2,
                advance: size * 0.6,
                ..FaceMetrics::default()
            }
        }

        fn glyph(&mut self, face: FaceId, character: char) -> Option<GlyphId> {
            *self.looked_up.entry((face, character)).or_default() += 1;
            let has = match face {
                CJK => is_cjk(character),
                EMOJI => character != '\u{E000}',
                SYMBOLS => !is_cjk(character) && character != '\u{231A}',
                WIDE => character != 'A' && character != '\u{E0A0}',
                _ => !matches!(character, 'A' | '\u{2714}' | '\u{231A}') && !is_cjk(character),
            };
            u16::try_from(u32::from(character))
                .ok()
                .filter(|_| has)
                .map(GlyphId)
        }

        fn fallback(&mut self, _: FaceId, cluster: &str) -> Option<FaceId> {
            self.fallbacks += 1;
            self.asked.push(cluster.to_owned());
            if cluster.contains('\u{FE0F}') {
                Some(EMOJI)
            } else if cluster.contains('\u{FE0E}') {
                Some(SYMBOLS)
            } else {
                cluster.chars().next().filter(|&c| is_cjk(c)).map(|_| CJK)
            }
        }

        /// Shapes a glyph a character, of its code, a cell's advance each,
        /// but for its ligatures: `->` as two alternates of its cells, as a
        /// font's contextual alternates are, and `fi` as one glyph; an acute
        /// accent goes back over its base; and the emoji face shapes a ZWJ
        /// sequence into one glyph.
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
            let glyph = |glyph: u16, cluster: usize, x_advance: f32, x_offset: f32| ShapedGlyph {
                glyph: GlyphId(glyph),
                cluster: cluster as u32,
                x_advance,
                x_offset,
                y_offset: 0.0,
            };
            if face == EMOJI && text.contains('\u{200D}') {
                glyphs.push(glyph(0xF46A, 0, advance * 2.0, 0.0));
                return;
            }
            let mut characters = text.char_indices().peekable();
            let mut base = 0;
            while let Some((at, character)) = characters.next() {
                let next = characters.peek().map(|&(_, next)| next);
                match (character, next) {
                    ('-', Some('>')) if features.ligatures => {
                        glyphs.push(glyph(0xE100, at, advance, 0.0));
                        glyphs.push(glyph(0xE101, at + 1, advance, 0.0));
                        characters.next();
                    }
                    ('f', Some('i')) if features.ligatures => {
                        glyphs.push(glyph(0xFB01, at, advance * 2.0, 0.0));
                        characters.next();
                    }
                    ('\u{301}', _) => glyphs.push(glyph(0x301, base, 0.0, -advance / 2.0)),
                    _ => {
                        base = at;
                        let code = u16::try_from(u32::from(character)).unwrap_or(0);
                        glyphs.push(glyph(code, at, advance, 0.0));
                    }
                }
            }
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
                thin: false
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

    /// The glyphs that `font` lays out for a run of `clusters`, a cell each,
    /// as their cells and textures.
    fn laid_out(font: &mut Font<CoreText>, clusters: &[&str]) -> Vec<(usize, Texture)> {
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
            .map(|placed| (placed.cell, placed.glyph.region.texture))
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
            .map(|(cell, _)| cell)
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
