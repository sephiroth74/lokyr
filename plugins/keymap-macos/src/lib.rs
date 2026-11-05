//! Built-in plugin with the default shortcuts of macOS.
//!
//! It binds the keys of the app's commands and of the built-in plugins' as
//! macOS's apps and iTerm2 have them: ⌘N opens a window, ⌘C copies, a
//! ⌘-click opens a link. It registers no command of its own, and a binding
//! of a command that no plugin registers, as one of a plugin turned off,
//! takes no key. The user's `[keys]` come after its bindings, and win. Like
//! every built-in plugin, it depends only on the plugin SDK.

#![forbid(unsafe_code)]

use lokyr_plugin_sdk::command::{Args, Error};
use lokyr_plugin_sdk::host::Binding;
use lokyr_plugin_sdk::plugin::{Cx, Plugin};

lokyr_plugin_sdk::export_plugin!(KeymapMacos);

/// The bindings, each of its keys and of the command that they run, in
/// their order: a menu's item shows the first of its command's keys that no
/// later binding takes.
pub const BINDINGS: [(&str, &str); 24] = [
    // The application's.
    ("cmd+,", "app.settings"),
    ("cmd+q", "app.quit"),
    // The windows'.
    ("cmd+n", "window.new"),
    ("cmd+w", "window.close"),
    ("cmd+shift+w", "window.close"),
    ("cmd+shift+i", "window.set-title"),
    // The clipboard's and the editing's.
    ("cmd+c", "pane.copy"),
    ("cmd+v", "pane.paste"),
    ("cmd+a", "pane.select-all"),
    ("cmd+k", "pane.clear"),
    // The text's size: ⌘+ on a layout of its own `+` key, as the Italian,
    // and with Shift on the US layout's `=`.
    ("cmd+=", "font.bigger"),
    ("cmd+shift+=", "font.bigger"),
    ("cmd++", "font.bigger"),
    ("cmd+-", "font.smaller"),
    ("cmd+0", "font.reset"),
    // The scrollback's.
    ("cmd+up", "pane.scroll-line-up"),
    ("cmd+down", "pane.scroll-line-down"),
    ("cmd+pageup", "pane.scroll-page-up"),
    ("shift+pageup", "pane.scroll-page-up"),
    ("cmd+pagedown", "pane.scroll-page-down"),
    ("shift+pagedown", "pane.scroll-page-down"),
    ("cmd+home", "pane.scroll-top"),
    ("cmd+end", "pane.scroll-bottom"),
    // The links'.
    ("cmd+click", "links.open"),
];

/// The plugin of macOS's default shortcuts.
#[derive(Debug, Default)]
pub struct KeymapMacos;

impl Plugin for KeymapMacos {
    fn activate(&mut self, cx: &mut Cx<'_>) -> Result<(), Error> {
        for (keys, command) in BINDINGS {
            cx.bind(Binding {
                keys: keys.to_owned(),
                command: command.to_owned(),
                args: Args::new(),
                when: None,
            })?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
