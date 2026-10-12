//! Runs third-party plugins written in Luau.
//!
//! Each plugin gets a Luau VM of its own, with only the libraries that
//! compute, a memory limit and a deadline per call. Everything outside the
//! computation goes through host functions that check the plugin's
//! capabilities, and calls are forwarded to the same `HostApi` that native
//! plugins use.

// `deny` rather than `forbid`: stopping a plugin at its deadline and reading
// bulk results go through Lua's C API, which is `unsafe`, and each of those
// call sites is allowed explicitly.
#![deny(unsafe_code)]
