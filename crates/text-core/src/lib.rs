//! Text pipeline shared by the font backends.
//!
//! Defines the [`FontBackend`] trait (discovery, fallback, shaping and
//! rasterization), the `[font]` section of the configuration, and the choice
//! of the faces that the styles of text take; and holds the parts that do
//! not depend on the platform: the shaping cache, the glyph atlas, the
//! procedurally drawn box-drawing and powerline sprites, and the Unicode
//! width tables.

#![forbid(unsafe_code)]

mod backend;
mod config;
mod faces;

pub use backend::{
    Bitmap, BitmapFormat, FaceId, FaceInfo, FaceMetrics, Features, FontBackend, GlyphId,
    ShapedGlyph, Synthesis,
};
pub use config::{Factor, FontConfig, Name, Size};
pub use faces::{Face, Faces, MissingStyle, choose as choose_faces};
