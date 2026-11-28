//! The kernel of the app.
//!
//! Owns the workspace model (windows, tabs, layout trees, panes), commands,
//! the keymap, events, the config and the fast services (search, decorations,
//! triggers). It serves the plugins' messages and hosts the built-in
//! plugins, but decides nothing about how things look: plugins do.

#![forbid(unsafe_code)]

pub mod claude_code;
pub mod commands;
pub mod config;
pub mod diagnostics;
mod git;
pub mod keymap;
pub mod l10n;
pub mod layouts;
pub mod paste;
pub mod plugins;
pub mod session_folders;
pub mod state;
pub mod themes;
pub mod typed;
pub mod workspace;
