//! Terminal emulation core.
//!
//! Turns the bytes coming from the PTY into terminal state (grid, cursor,
//! modes), and keys and mouse events into the bytes sent back to the PTY.
//! The crate does no I/O and knows nothing about windows or GPUs, so it can
//! be tested headless and fuzzed.
//!
//! - [`parser`] turns bytes into calls on a [`parser::Handler`].
//! - [`terminal`] holds the screens, the cursor and the modes, and applies
//!   what the parser finds.
//! - [`page`] stores the grid and its scrollback.
//! - [`unicode`] splits text into grapheme clusters and measures their width.
//! - [`input`] encodes keys and mouse events for the program.

#![forbid(unsafe_code)]

pub mod input;
pub mod page;
pub mod parser;
pub mod terminal;
pub mod unicode;
