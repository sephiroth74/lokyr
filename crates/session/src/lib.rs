//! The session behind a pane.
//!
//! Owns a PTY, its child process and the terminal state, feeds the output to
//! the parser from a dedicated I/O thread, and produces the snapshots the
//! renderer reads.

#![forbid(unsafe_code)]
