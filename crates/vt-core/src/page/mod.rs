//! The paged grid: cells, rows, pages, the scrollback, and pins into it.

mod cell;
mod style;

pub use cell::{Cell, Content, Semantic, Width};
pub use style::{Rgb, StyleId};
