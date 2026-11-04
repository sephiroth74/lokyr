//! A drawing for the tests, which draws nothing and counts its frames: a
//! GPU, its renderer and surfaces, and fonts of one face, which has every
//! character of the Basic Multilingual Plane, `Mono` by default, `Menlo`
//! too.

use std::sync::atomic::{AtomicUsize, Ordering};

use lokyr_platform_core::WindowHandle;
use lokyr_render_core::{Color, DisplayList, TextureUpdate};
use lokyr_text_core::{
    Bitmap, BitmapFormat, FaceId, FaceInfo, FaceMetrics, Features, FontBackend, GlyphId, Rendering,
    ShapedGlyph, Synthesis,
};

use crate::drawing::Backend;

/// The families that the fonts have.
const FAMILIES: [&str; 2] = ["Mono", "Menlo"];

/// The GPU of the tests.
#[derive(Debug)]
pub(crate) struct Gpu;

impl Backend for Gpu {
    type Fonts = Fonts;
    type Surface = Surface;
    type Renderer = Renderer;

    fn new() -> Result<Self, String> {
        Ok(Self)
    }

    fn fonts() -> Fonts {
        Fonts
    }

    fn renderer(&self, _coverage: u32, _color: u32) -> Result<Renderer, String> {
        Ok(Renderer::default())
    }

    fn surface(&self, _handle: WindowHandle<'_>) -> Result<Surface, String> {
        Ok(Surface::default())
    }
}

/// A renderer that draws nothing, and counts the frames that it draws.
#[derive(Debug, Default)]
pub(crate) struct Renderer {
    pub(crate) frames: usize,
}

impl lokyr_render_core::Renderer for Renderer {
    type Target = Surface;
    type Error = String;

    fn update_textures(&mut self, _updates: &[TextureUpdate]) -> Result<(), String> {
        Ok(())
    }

    fn draw(&mut self, target: &Surface, _list: &DisplayList) -> Result<bool, String> {
        self.frames += 1;
        target.frames.fetch_add(1, Ordering::Relaxed);
        Ok(true)
    }

    fn drawing_from(&mut self) -> Option<u64> {
        None
    }
}

/// A surface that shows nothing, and counts the frames drawn into it.
#[derive(Debug, Default)]
pub(crate) struct Surface {
    pub(crate) frames: AtomicUsize,
}

impl lokyr_render_core::Surface for Surface {
    type Error = String;

    fn resize(&self, _width: u32, _height: u32, _scale: f64) {}

    fn set_presents_with_transaction(&self, _with: bool) {}

    fn release(&self) {}

    fn clear(&self, _color: Color) -> Result<bool, String> {
        Ok(true)
    }
}

/// Fonts of one face, whose glyphs are their characters' codes, an advance
/// of 0.6 of the size each, drawn as squares of half the size.
#[derive(Debug)]
pub(crate) struct Fonts;

impl FontBackend for Fonts {
    fn default_family(&mut self) -> String {
        FAMILIES[0].to_owned()
    }

    fn family(&mut self, name: &str) -> Option<Vec<FaceInfo>> {
        FAMILIES.contains(&name).then(|| {
            vec![FaceInfo {
                id: FaceId(0),
                style: "Regular".to_owned(),
                weight: 400.0,
                italic: false,
                oblique: false,
                width: 0.0,
                in_group: false,
                preferred: false,
            }]
        })
    }

    fn metrics(&mut self, _face: FaceId, size: f32) -> FaceMetrics {
        FaceMetrics {
            ascent: size * 0.8,
            descent: size * 0.2,
            advance: size * 0.6,
            ..FaceMetrics::default()
        }
    }

    fn glyph(&mut self, _face: FaceId, character: char) -> Option<GlyphId> {
        u16::try_from(u32::from(character)).ok().map(GlyphId)
    }

    fn fallback(&mut self, _face: FaceId, _cluster: &str) -> Option<FaceId> {
        None
    }

    fn shape(
        &mut self,
        face: FaceId,
        size: f32,
        text: &str,
        _features: Features,
        glyphs: &mut Vec<ShapedGlyph>,
    ) {
        let mut x = 0.0;
        for (at, character) in text.char_indices() {
            glyphs.push(ShapedGlyph {
                face,
                glyph: GlyphId(u16::try_from(u32::from(character)).unwrap_or(0)),
                cluster: u32::try_from(at).unwrap_or(u32::MAX),
                x,
                y: 0.0,
                right_to_left: false,
            });
            x += size * 0.6;
        }
    }

    fn rasterize(
        &mut self,
        _face: FaceId,
        _glyph: GlyphId,
        size: f32,
        _synthesis: Synthesis,
        _rendering: Rendering,
    ) -> Bitmap {
        let side = (size / 2.0) as u32;
        Bitmap {
            format: BitmapFormat::Gray,
            width: side,
            height: side,
            left: 0,
            top: i32::try_from(side).unwrap_or(0),
            data: vec![255; (side * side) as usize],
        }
    }
}
