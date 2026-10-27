//! The font that a window's text takes: its faces, the family of the text
//! past ASCII when the configuration names one, the faces of the system that
//! draw what they lack, its cell, and the atlas that holds the glyphs drawn
//! so far.

use lokyr_render_core::{AtlasRegion, Texture, TextureUpdate};
use lokyr_text_core::{
    Atlas, BitmapFormat, CellMetrics, Change, Face, FaceError, FaceId, Faces, FastMap, FontBackend,
    FontConfig, GlyphId, Resolved, Synthesis, choose_faces, resolve_faces,
};

/// VARIATION SELECTOR-16, which asks for a character's emoji.
const EMOJI_PRESENTATION: char = '\u{FE0F}';

use crate::pane::{AtlasGlyph, CellText};

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
    cell: CellMetrics,
    atlas: Atlas<GlyphKey>,
    glyphs: Glyphs,
}

/// Where a character's glyph is: the face that draws it, what its style
/// draws heavier or slanted, and the glyph.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Found {
    face: FaceId,
    synthesis: Synthesis,
    glyph: GlyphId,
}

/// Where each style finds each character that it was asked for, in its
/// face, the non-ASCII family's or a face of the system's cascade, so that
/// a frame finds them without the backend: ASCII's in a table, the others'
/// in a map. The styles whose faces are the same share theirs. The
/// backend's answers are kept too, so that it is asked once for each face
/// and character.
#[derive(Debug)]
struct Glyphs {
    /// The table of each style.
    tables: [usize; 4],
    /// By table, then by character: `None` until asked for.
    ascii: [[Option<Option<Found>>; 128]; 4],
    /// By table, character, and whether VS16 asks for its emoji.
    others: FastMap<(usize, char, bool), Option<Found>>,
    /// The backend's glyph of each face and character.
    ids: FastMap<(FaceId, char), Option<GlyphId>>,
    /// The backend's fallback of each face, character, and whether VS16
    /// asks for its emoji.
    fallbacks: FastMap<(FaceId, char, bool), Option<FaceId>>,
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
            cell,
            atlas: Atlas::new(),
            glyphs: Glyphs::new(&resolved.faces, non_ascii.as_ref()),
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
        let pixels = self.config.size.points() * scale;
        if pixels == self.pixels {
            return;
        }
        self.pixels = pixels;
        let metrics = self.backend.metrics(self.resolved.faces.regular.id, pixels);
        self.cell = CellMetrics::new(&metrics, &self.config);
    }

    /// Starts a frame: the glyphs that it uses stay in the atlas.
    pub fn begin_frame(&mut self) {
        self.atlas.begin_frame();
    }

    /// The glyph of `shown`, in the face of its style, or of the non-ASCII
    /// family's style for a character past ASCII, or else in the face that
    /// the system's cascade gives for it, as emoji take the system's emoji
    /// font; drawn into the atlas the first time. `None` when no face draws
    /// its character.
    pub fn glyph(&mut self, shown: &CellText<'_>) -> Option<AtlasGlyph> {
        let style = usize::from(shown.style.bold) | usize::from(shown.style.italic) << 1;
        let character = shown.text.chars().next()?;
        let emoji = shown.text.contains(EMOJI_PRESENTATION);
        let found = self.find(style, character, emoji)?;
        let key = GlyphKey {
            face: found.face,
            glyph: found.glyph,
            size: self.pixels.to_bits(),
            synthesis: found.synthesis,
        };
        let placement = match self.atlas.get(&key) {
            Some(placement) => placement,
            None => {
                let bitmap =
                    self.backend
                        .rasterize(found.face, found.glyph, self.pixels, found.synthesis);
                self.atlas.insert(key, &bitmap)?
            }
        };
        Some(AtlasGlyph {
            region: AtlasRegion {
                texture: texture(placement.format),
                x: placement.x,
                y: placement.y,
                width: placement.width,
                height: placement.height,
            },
            left: placement.left,
            top: placement.top,
        })
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

    /// Where `style` finds `character`, an emoji when `emoji` asks for it,
    /// found once.
    fn find(&mut self, style: usize, character: char, emoji: bool) -> Option<Found> {
        let table = self.glyphs.tables[style];
        if let Some(Some(known)) = self.glyphs.ascii[table].get(character as usize) {
            return *known;
        }
        if let Some(known) = self.glyphs.others.get(&(table, character, emoji)) {
            return *known;
        }
        let found = self.look_up(style, character, emoji);
        match self.glyphs.ascii[table].get_mut(character as usize) {
            Some(slot) => *slot = Some(found),
            None => {
                self.glyphs.others.insert((table, character, emoji), found);
            }
        }
        found
    }

    /// Asks the backend where `style` finds `character`: in its face, unless
    /// that lacks it, or `emoji` asks for its emoji, and then in the face of
    /// the system's cascade, which draws it as the style does.
    fn look_up(&mut self, style: usize, character: char, emoji: bool) -> Option<Found> {
        let face = match &self.non_ascii {
            Some(non_ascii) if !character.is_ascii() => face_of(non_ascii, style),
            _ => face_of(&self.resolved.faces, style),
        };
        let Glyphs { ids, fallbacks, .. } = &mut self.glyphs;
        let backend = &mut self.backend;
        if !emoji && let Some(glyph) = glyph_of(ids, backend, face.id, character) {
            return Some(Found {
                face: face.id,
                synthesis: face.synthesis,
                glyph,
            });
        }
        let cluster = if emoji {
            format!("{character}{EMOJI_PRESENTATION}")
        } else {
            character.to_string()
        };
        let fallback = *fallbacks
            .entry((face.id, character, emoji))
            .or_insert_with(|| backend.fallback(face.id, &cluster));
        // An emoji that no face of the cascade draws keeps the face's glyph,
        // as text.
        let drawing = match fallback {
            Some(fallback) => fallback,
            None if emoji => face.id,
            None => return None,
        };
        let glyph = glyph_of(ids, backend, drawing, character)?;
        Some(Found {
            face: drawing,
            synthesis: face.synthesis,
            glyph,
        })
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

    use lokyr_text_core::{Bitmap, FaceInfo, FaceMetrics, Features, ShapedGlyph};
    use lokyr_vt_core::page::Style;

    use super::*;

    /// The face of the cascade of `Squares` that draws CJK.
    const CJK: FaceId = FaceId(9);
    /// Its face of emoji.
    const EMOJI: FaceId = FaceId(8);
    /// The face of its family Wide.
    const WIDE: FaceId = FaceId(5);

    /// A backend of two families: Mono, of a regular and a bold face, which
    /// lack `A` and CJK, and Wide, of one face, which lacks `A` alone; and of
    /// a cascade of a face that draws CJK, and one of emoji. Their glyphs are
    /// their characters' codes, drawn as squares of the size.
    #[derive(Default)]
    struct Squares {
        drawn: HashMap<(FaceId, u16), usize>,
        looked_up: HashMap<(FaceId, char), usize>,
        fallbacks: usize,
    }

    fn is_cjk(character: char) -> bool {
        character >= '\u{3000}'
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
                        in_group: false,
                    },
                    FaceInfo {
                        id: FaceId(1),
                        style: "Bold".to_owned(),
                        weight: 700.0,
                        italic: false,
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
                EMOJI => true,
                WIDE => character != 'A',
                _ => character != 'A' && !is_cjk(character),
            };
            u16::try_from(u32::from(character))
                .ok()
                .filter(|_| has)
                .map(GlyphId)
        }

        fn fallback(&mut self, _: FaceId, cluster: &str) -> Option<FaceId> {
            self.fallbacks += 1;
            if cluster.contains('\u{FE0F}') {
                Some(EMOJI)
            } else {
                cluster.chars().next().filter(|&c| is_cjk(c)).map(|_| CJK)
            }
        }

        fn shape(&mut self, _: FaceId, _: f32, _: &str, _: Features, _: &mut Vec<ShapedGlyph>) {}

        fn rasterize(&mut self, face: FaceId, glyph: GlyphId, size: f32, _: Synthesis) -> Bitmap {
            *self.drawn.entry((face, glyph.0)).or_default() += 1;
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
        // A family that is not installed leaves that text to the text's
        // family.
        let mut font = Font::new(Squares::default(), config("Nope"), 1.0).unwrap();
        assert_eq!(font.missing_non_ascii_family(), Some("Nope"));
        font.glyph(&shown("é", &Style::DEFAULT)).unwrap();
        assert_eq!(font.backend.drawn.get(&(FaceId(0), 0xE9)), Some(&1));
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
