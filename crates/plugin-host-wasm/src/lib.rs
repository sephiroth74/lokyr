//! Runs third-party plugins as WebAssembly components.
//!
//! Links only the capabilities a plugin was granted, enforces memory and CPU
//! limits, and keeps a cache of ahead-of-time compiled components. Calls are
//! forwarded to the same `HostApi` that native plugins use.

// `deny` rather than `forbid`: loading a precompiled component from the cache
// goes through wasmtime's `deserialize`, which is `unsafe`, and that single
// call site is allowed explicitly.
#![deny(unsafe_code)]
