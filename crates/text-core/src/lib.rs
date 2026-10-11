//! Text pipeline shared by the font backends.
//!
//! Defines the `FontBackend` trait (discovery, fallback, shaping and
//! rasterization) and holds the parts that do not depend on the platform:
//! the shaping cache, the glyph atlas, the procedurally drawn box-drawing and
//! powerline sprites, and the Unicode width tables.

#![forbid(unsafe_code)]
