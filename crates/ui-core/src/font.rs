//! The font that a window's text takes: its faces, the family of the text
//! past ASCII when the configuration names one, the faces of the system that
//! draw what they lack, its cell, and the atlas that holds the glyphs drawn
//! so far.

use lokyr_render_core::{AtlasRegion, Texture, TextureUpdate};
use lokyr_text_core::{
    Atlas, BitmapFormat, CellMetrics, Change, Face, FaceError, FaceId, Faces, FastMap, FontBackend,
    FontConfig, GlyphId, Resolved, Synthesis, choose_faces, resolve_faces,
};
use lokyr_vt_core::unicode;

use crate::pane::{AtlasGlyph, CellText};

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

    /// The glyph of `shown`, in its presentation: as text, in the face of
    /// its style, of the non-ASCII family's style for a character past ASCII
    /// and else of the text's family, or else in the face that the system's
    /// cascade gives for its text; as an emoji, in the face that the cascade
    /// gives for its emoji, the system's emoji font, and else as text. Drawn
    /// into the atlas the first time. `None` when no face draws its
    /// character.
    pub fn glyph(&mut self, shown: &CellText<'_>) -> Option<AtlasGlyph> {
        let style = usize::from(shown.style.bold) | usize::from(shown.style.italic) << 1;
        let character = shown.text.chars().next()?;
        let asked = Presentation::asked(shown.text);
        let found = self.find(style, character, asked)?;
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
        let found = self.look_up(style, character, presentation);
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
        })
    })
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
    /// Its face of emoji, which lacks U+E0B0.
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
                EMOJI => character != '\u{E0B0}',
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

        fn shape(&mut self, _: FaceId, _: f32, _: &str, _: Features, _: &mut Vec<ShapedGlyph>) {}

        fn rasterize(
            &mut self,
            face: FaceId,
            glyph: GlyphId,
            size: f32,
            synthesis: Synthesis,
        ) -> Bitmap {
            *self.drawn.entry((face, glyph.0)).or_default() += 1;
            self.synthesized.insert((face, glyph.0), synthesis);
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
        font.glyph(&shown("\u{E0B0}\u{FE0F}", &Style::DEFAULT))
            .unwrap();
        assert_eq!(font.backend.drawn.get(&(FaceId(0), 0xE0B0)), Some(&1));
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
        for icon in ['\u{F015}', '\u{E0A0}', '\u{E0B0}'] {
            let found = font.find(0, icon, None);
            assert_eq!(found.map(|found| found.face), Some(regular), "{icon:?}");
        }
        // Japanese takes the non-ASCII family.
        let japanese = font.find(0, '日', None).map(|found| found.face);
        assert_ne!(japanese, Some(regular));
    }
}
