//! The Lokyr plugin API and its Rust SDK.
//!
//! Defines the `HostApi` trait that the host implements, with its types, and
//! the Rust front end that built-in plugins use. Third-party plugins reach the
//! same API from Luau. Both follow the same rules: plugins receive owned data,
//! are never re-entered, and neither spawn threads nor block.

#![forbid(unsafe_code)]

pub mod command;
