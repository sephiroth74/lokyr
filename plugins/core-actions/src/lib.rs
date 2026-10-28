//! Built-in plugin with the basic terminal actions.
//!
//! Copy and paste, select all, clearing the scrollback, scrolling, font size,
//! and opening and closing windows. Like every built-in plugin, it depends
//! only on the plugin SDK.

#![forbid(unsafe_code)]

use lokyr_plugin_sdk::plugin::Plugin;

/// The plugin of the basic terminal actions.
#[derive(Debug, Default)]
pub struct CoreActions;

impl Plugin for CoreActions {}
