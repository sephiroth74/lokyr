//! The font that a window's text takes: its faces, its cell, and the atlas
//! that holds the glyphs drawn so far.

use bolt_render_core::{AtlasRegion, Texture, TextureUpdate};
use bolt_text_core::{
    Atlas, BitmapFormat, CellMetrics, Change, Face, FaceError, FaceId, Faces, FastMap, FontBackend,
    FontConfig, GlyphId, Resolved, Synthesis, resolve_faces,
};

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
    pixels: f32,
    cell: CellMetrics,
    atlas: Atlas<GlyphKey>,
    glyphs: Glyphs,
}

/// The glyph of each face for each character that it was asked for, so
/// that the backend is asked once: ASCII's in a table, the others' in a
/// map. The styles whose faces are the same share theirs.
#[derive(Debug)]
struct Glyphs {
    /// The table of each style.
    tables: [usize; 4],
    /// By table, then by character: `None` until asked for.
    ascii: [[Option<Option<GlyphId>>; 128]; 4],
    others: FastMap<(usize, char), Option<GlyphId>>,
}

impl Glyphs {
    fn new(faces: &Faces) -> Self {
        let id = |style| face_of(faces, style).id;
        Self {
            tables: [0, 1, 2, 3].map(|style| {
                (0..style)
                    .find(|&other| id(other) == id(style))
                    .unwrap_or(style)
            }),
            ascii: [[None; 128]; 4],
            others: FastMap::default(),
        }
    }
}

impl<B: FontBackend> Font<B> {
    /// The font that `config` names, through `backend`, at a display's
    /// `scale`.
    pub fn new(mut backend: B, config: FontConfig, scale: f32) -> Result<Self, FaceError> {
        let resolved = resolve_faces(&mut backend, &config)?;
        let pixels = config.size.points() * scale;
        let cell = CellMetrics::new(&backend.metrics(resolved.faces.regular.id, pixels), &config);
        Ok(Self {
            backend,
            config,
            pixels,
            cell,
            atlas: Atlas::new(),
            glyphs: Glyphs::new(&resolved.faces),
            resolved,
        })
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

    /// The glyph of `shown`, in the face of its style, drawn into the atlas
    /// the first time; `None` when the face lacks its character.
    pub fn glyph(&mut self, shown: &CellText<'_>) -> Option<AtlasGlyph> {
        let style = usize::from(shown.style.bold) | usize::from(shown.style.italic) << 1;
        let face = face_of(&self.resolved.faces, style);
        let character = shown.text.chars().next()?;
        let glyph = self.glyph_id(style, face.id, character)?;
        let key = GlyphKey {
            face: face.id,
            glyph,
            size: self.pixels.to_bits(),
            synthesis: face.synthesis,
        };
        let placement = match self.atlas.get(&key) {
            Some(placement) => placement,
            None => {
                let bitmap = self
                    .backend
                    .rasterize(face.id, glyph, self.pixels, face.synthesis);
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

    /// The glyph of `character` in `face`, the face of `style`, which the
    /// backend gives once.
    fn glyph_id(&mut self, style: usize, face: FaceId, character: char) -> Option<GlyphId> {
        let backend = &mut self.backend;
        let table = self.glyphs.tables[style];
        match self.glyphs.ascii[table].get_mut(character as usize) {
            Some(known) => *known.get_or_insert_with(|| backend.glyph(face, character)),
            None => *self
                .glyphs
                .others
                .entry((table, character))
                .or_insert_with(|| backend.glyph(face, character)),
        }
    }
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

    use bolt_text_core::{Bitmap, FaceInfo, FaceMetrics, Features, ShapedGlyph};
    use bolt_vt_core::page::Style;

    use super::*;

    /// A backend of one family, Mono, of a regular and a bold face, whose
    /// glyphs are their characters' codes, drawn as squares of the size.
    #[derive(Default)]
    struct Squares {
        drawn: HashMap<(FaceId, u16), usize>,
        looked_up: HashMap<(FaceId, char), usize>,
    }

    impl FontBackend for Squares {
        fn default_family(&mut self) -> String {
            "Mono".to_owned()
        }

        fn family(&mut self, name: &str) -> Option<Vec<FaceInfo>> {
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
            u16::try_from(u32::from(character))
                .ok()
                .filter(|&code| code != 0x41)
                .map(GlyphId)
        }

        fn fallback(&mut self, _: FaceId, _: &str) -> Option<FaceId> {
            None
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
