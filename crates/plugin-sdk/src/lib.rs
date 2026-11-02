//! The Lokyr plugin API and its Rust SDK.
//!
//! Defines the plugin API as messages, the requests of the plugins, the
//! host's responses and its events, with their types, the view tree of
//! their UI as data, and the Rust front end that built-in plugins use. Third-party plugins reach the same
//! messages from Luau. Both follow the same rules: plugins receive owned
//! data, are never re-entered, and neither spawn threads nor block.
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
pub mod message;
#[cfg(feature = "unstable")]
pub mod plugin;
#[cfg(feature = "unstable")]
pub mod view;
