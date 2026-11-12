//! Text pipeline shared by the font backends.
//!
//! Defines the [`FontBackend`] trait (discovery, fallback, shaping and
//! rasterization), the `[font]` section of the configuration, the choice
//! of the faces that the styles of text take, and the list of the families
//! that the system has; and holds the parts that do not depend on the
//! platform: the shaping cache, the glyph atlas, the procedurally drawn
//! box-drawing and powerline sprites, and the Unicode width tables.

#![forbid(unsafe_code)]

mod atlas;
mod backend;
pub mod catalog;
mod cell;
mod config;
mod faces;
mod hash;
pub mod sprite;

pub use atlas::{Atlas, COLOR_MAX, Change, GRAY_MAX, Placement, START};
pub use backend::{
    Bitmap, BitmapFormat, FaceId, FaceInfo, FaceMetrics, Features, FontBackend, GlyphId, Raster,
    Rendering, ShapedGlyph, Synthesis,
};
pub use cell::CellMetrics;
pub use config::{Factor, FontConfig, Name, Size, ThinStrokes, as_written};
pub use faces::{
    Face, FaceError, Faces, Resolved, choose as choose_faces, resolve as resolve_faces,
    styles_within,
};
pub use hash::{FastHasher, FastMap};

/// Symbols Nerd Font Mono, of Nerd Fonts v3.5.1, which draws the icons of
/// the private use areas that prompts and tools show, where the configured
/// font lacks them: the backends take it as a face of their own.
pub const SYMBOLS_FONT: &[u8] = include_bytes!("../fonts/SymbolsNerdFontMono-Regular.ttf");
