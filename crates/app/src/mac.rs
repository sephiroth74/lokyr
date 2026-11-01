//! The Mac's drawing: Metal's GPU, renderer and surfaces, and CoreText's
//! fonts.

use lokyr_app_core::Backend;
use lokyr_font_coretext::CoreText;
use lokyr_platform_core::WindowHandle;
use lokyr_render_metal::{Gpu, MetalRenderer, Surface};

/// The Mac's GPU, through Metal, and its fonts, through CoreText.
#[derive(Debug)]
pub struct Mac {
    gpu: Gpu,
}

impl Backend for Mac {
    type Fonts = CoreText;
    type Surface = Surface;
    type Renderer = MetalRenderer;

    fn new() -> Result<Self, String> {
        Gpu::new()
            .map(|gpu| Self { gpu })
            .map_err(|error| error.to_string())
    }

    fn fonts() -> CoreText {
        CoreText::new()
    }

    fn renderer(&self, coverage: u32, color: u32) -> Result<MetalRenderer, String> {
        MetalRenderer::new(&self.gpu, coverage, color).map_err(|error| error.to_string())
    }

    fn surface(&self, handle: WindowHandle<'_>) -> Result<Surface, String> {
        Surface::new(&self.gpu, handle).map_err(|error| error.to_string())
    }

    fn warm_up(family: &str) {
        lokyr_font_coretext::warm_up(family);
    }
}
