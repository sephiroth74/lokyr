//! The kernel of the app.
//!
//! Owns the workspace model (windows, tabs, layout trees, panes), commands,
//! the keymap, events, the config and the fast services (search, decorations,
//! triggers). It implements the plugin `HostApi` and hosts the built-in
//! plugins, but decides nothing about how things look: plugins do.

#![forbid(unsafe_code)]
