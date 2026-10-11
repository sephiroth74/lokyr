//! Terminal emulation core.
//!
//! Turns the bytes coming from the PTY into terminal state (grid, cursor,
//! modes), and keys and mouse events into the bytes sent back to the PTY.
//! The crate does no I/O and knows nothing about windows or GPUs, so it can
//! be tested headless and fuzzed.
