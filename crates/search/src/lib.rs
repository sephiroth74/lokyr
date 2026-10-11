//! Literal and regex search over the terminal grid, scrollback included.
//!
//! Soft-wrapped rows are searched as a single logical line, so matches that
//! cross the right margin are still found.

#![forbid(unsafe_code)]
