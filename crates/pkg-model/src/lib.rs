//! Package formats shared by the app, Sleip and the registry.
//!
//! Covers plugin manifests (`plugin.toml`), the user's plugin list
//! (`plugins.toml`), the lockfile (`plugins.lock`), the registry index format
//! and the distribution files that list the built-in plugins.

#![forbid(unsafe_code)]

pub mod manifest;
pub mod schema;
