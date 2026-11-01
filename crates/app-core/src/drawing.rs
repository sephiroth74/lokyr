//! What draws the windows: a backend's GPU, its renderer and its surfaces,
//! and the font of the text, of the backend's fonts.

use std::fmt::{Debug, Display};

use lokyr_platform_core::WindowHandle;
use lokyr_render_core::{Renderer, Surface};
use lokyr_text_core::{FontBackend, FontConfig};
use lokyr_ui_core::font::Font;

/// The platform's drawing: its GPU, which makes the renderers and the
/// surfaces, and its fonts.
pub trait Backend: Debug + Sized + 'static {
    /// The backend of the fonts.
    type Fonts: FontBackend + Debug;
    /// What the frames of a window, and of its layers, are drawn into.
    type Surface: Surface<Error: Display> + Debug;
    /// What draws the frames.
    type Renderer: Renderer<Target = Self::Surface, Error: Display> + Debug;

    /// The system's GPU; an error where it has none.
    fn new() -> Result<Self, String>;

    /// The system's fonts.
    fn fonts() -> Self::Fonts;

    /// A renderer of the atlas's textures, whose sides are `coverage` and
    /// `color`.
    fn renderer(&self, coverage: u32, color: u32) -> Result<Self::Renderer, String>;

    /// The surface of the window, or of the layer of one, that `handle`
    /// names.
    fn surface(&self, handle: WindowHandle<'_>) -> Result<Self::Surface, String>;

    /// Readies the system's fallback of fonts for the text of `family`, on
    /// the thread that calls it, so that the first characters that the
    /// fonts lack find their faces sooner; nothing, for a backend that has
    /// nothing to ready.
    fn warm_up(family: &str) {
        let _ = family;
    }
}

/// What draws the windows: the GPU, its renderer, and the font of the text.
#[derive(Debug)]
pub struct Drawing<B: Backend> {
    pub(crate) backend: B,
    pub(crate) renderer: B::Renderer,
    pub(crate) font: Font<B::Fonts>,
    /// How many fonts it took, the first one too: a layer draws its cells
    /// again of a new one, whose atlas gives their regions to other pixels.
    pub(crate) fonts: u64,
    /// Whether a thread readies the system's fallback of fonts, as one does
    /// once the first frame is drawn.
    pub(crate) warmed_up: bool,
}

impl<B: Backend> Drawing<B> {
    /// The system's GPU, and the font that `config` gives, which tells of a
    /// family that is not installed.
    pub fn new(config: FontConfig) -> Result<Self, String> {
        let backend = B::new()?;
        let font = font::<B>(config)?;
        let (coverage, color) = font.texture_sizes();
        let renderer = backend.renderer(coverage, color)?;
        Ok(Self {
            backend,
            renderer,
            font,
            fonts: 1,
            warmed_up: false,
        })
    }

    /// Takes the font that `config` gives, with an atlas of its own, and a
    /// renderer of the atlas's textures, in place of those before; or, if
    /// it fails, keeps them.
    pub(crate) fn set_font(&mut self, config: FontConfig) -> Result<(), String> {
        let font = font::<B>(config)?;
        let (coverage, color) = font.texture_sizes();
        let renderer = self.backend.renderer(coverage, color)?;
        self.font = font;
        self.renderer = renderer;
        self.fonts += 1;
        Ok(())
    }

    /// What the font lacks of the config's: a family that is not
    /// installed, of the text or of the text past ASCII, and the one that
    /// it takes instead.
    pub(crate) fn missing(&self) -> Option<String> {
        let taken = self.font.family();
        let missing = |family: &str, of: &str| {
            format!("the font family `{family}` is not installed; the {of} takes `{taken}`")
        };
        let text = self
            .font
            .missing_family()
            .map(|family| missing(family, "text"));
        let past_ascii = self
            .font
            .missing_non_ascii_family()
            .map(|family| missing(family, "text past ASCII"));
        match (text, past_ascii) {
            (Some(text), Some(past)) => Some(format!("{text}; {past}")),
            (text, past) => text.or(past),
        }
    }
}

/// The font that `config` gives, of the backend's fonts.
pub(crate) fn font<B: Backend>(config: FontConfig) -> Result<Font<B::Fonts>, String> {
    Font::new(B::fonts(), config, 1.0).map_err(|error| error.to_string())
}
