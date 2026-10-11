//! SDK for Saetta plugins.
//!
//! The same plugin source builds either as a statically linked native plugin
//! or as a WebAssembly component for `wasm32-wasip2`. Both targets expose the
//! same types and follow the same rules: plugins receive owned data, are never
//! re-entered, and neither spawn threads nor block.

#![forbid(unsafe_code)]
