//! The Lokyr plugin API and its Rust SDK.
//!
//! Defines the `HostApi` trait that the host implements, with its types, and
//! the Rust front end that built-in plugins use. Third-party plugins reach the
//! same API from Luau. Both follow the same rules: plugins receive owned data,
//! are never re-entered, and neither spawn threads nor block.
//!
//! The host's API and the plugins' trait come with the `unstable` feature,
//! as they can change in a minor release.

#![forbid(unsafe_code)]

pub mod command;
#[cfg(feature = "unstable")]
pub mod event;
#[cfg(feature = "unstable")]
pub mod host;
#[cfg(feature = "unstable")]
pub mod plugin;
