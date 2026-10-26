//! Platform-independent UI engine.
//!
//! Lays out view trees, provides the widgets the chrome needs (text fields
//! with IME support, lists, menus), handles hit testing and accessibility,
//! and turns a window's chrome and panes into a single display list: a
//! pane's grid of cells comes from a snapshot of its terminal, in [`pane`],
//! with the glyphs of its [`font`].

#![forbid(unsafe_code)]

pub mod font;
pub mod pane;
pub mod theme;
