//! Pseudo-terminals and child processes.
//!
//! Opens a PTY, starts the shell as its child process and resizes the
//! terminal. The public API exposes no raw file descriptors, so other
//! platforms can be supported behind the same interface.
