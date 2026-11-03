//! Platform-independent UI engine.
//!
//! Keeps the plugins' view trees, as a flat arena that a new render
//! updates by a diff, in [`tree`]; lays them out, with `taffy`, in
//! [`layout`], their texts measured in the [`interface`]'s face; paints
//! them into display list items, of the [`theme`]'s tokens, in [`paint`];
//! finds the nodes under the pointer, and tells them what it did and how it
//! looks over them, in [`pointer`](mod@pointer); keeps the keyboard's focus
//! on a pane or a panel's node, in [`focus`]; does what accessibility asks
//! of a node, in [`action`]; provides the widgets the chrome needs (text
//! fields with IME support, lists, menus), handles accessibility, and turns
//! a window's chrome and panes into a single display list: a pane's grid of
//! cells comes from a snapshot of its terminal, in [`pane`], with the
//! glyphs of its [`font`].

#![forbid(unsafe_code)]

pub mod action;
pub mod cursor;
pub mod focus;
pub mod font;
pub mod interface;
pub mod layout;
pub mod paint;
pub mod pane;
pub mod pointer;
pub mod text;
pub mod theme;
pub mod tree;
