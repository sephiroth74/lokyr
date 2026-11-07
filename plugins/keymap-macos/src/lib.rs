//! Built-in plugin with the default shortcuts of macOS.
//!
//! It binds the keys of the app's commands and of the built-in plugins' as
//! macOS's apps and iTerm2 have them: ⌘N opens a window, ⌘C copies, a
//! ⌘-click opens a link. It registers no command of its own, and a binding
//! of a command that no plugin registers, as one of a plugin turned off,
//! takes no key. The user's `[keys]` come after its bindings, and win. Like
//! every built-in plugin, it depends only on the plugin SDK.

#![forbid(unsafe_code)]

use lokyr_plugin_sdk::command::{Args, Error, Value};
use lokyr_plugin_sdk::host::Binding;
use lokyr_plugin_sdk::plugin::{Cx, Plugin};

lokyr_plugin_sdk::export_plugin!(KeymapMacos);

/// The bindings, each of its keys and of the command that they run, in
/// their order: a menu's item shows the first of its command's keys that no
/// later binding takes.
pub const BINDINGS: [(&str, &str); 37] = [
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
    // The search's.
    ("cmd+f", "search.open"),
    ("cmd+g", "search.next"),
    ("cmd+shift+g", "search.previous"),
    // The command palette's.
    ("cmd+shift+p", "command-palette.open"),
    // The tabs': Command-W closes the tab, or its pane, once a tab bar
    // runs, the window's close keeping Command-Shift-W.
    ("cmd+t", "tabs.new"),
    ("cmd+w", "tabs.close"),
    ("cmd+shift+]", "tabs.next"),
    ("cmd+shift+[", "tabs.prev"),
    ("ctrl+tab", "tabs.next"),
    ("ctrl+shift+tab", "tabs.prev"),
    // The splits'.
    ("cmd+]", "splits.focus-next"),
    ("cmd+[", "splits.focus-prev"),
    ("cmd+shift+enter", "splits.zoom-toggle"),
];

/// The bindings of a command of an argument, a whole number, after the
/// others: each of its keys, the command, the argument's name and its
/// value.
pub const ARGUED: [(&str, &str, &str, i64); 20] = [
    // A tab a place to the left or to the right.
    ("cmd+shift+left", "tabs.move", "delta", -1),
    ("cmd+shift+right", "tabs.move", "delta", 1),
    // The tabs of the numbers from 1 to 9.
    ("cmd+1", "tabs.goto", "index", 1),
    ("cmd+2", "tabs.goto", "index", 2),
    ("cmd+3", "tabs.goto", "index", 3),
    ("cmd+4", "tabs.goto", "index", 4),
    ("cmd+5", "tabs.goto", "index", 5),
    ("cmd+6", "tabs.goto", "index", 6),
    ("cmd+7", "tabs.goto", "index", 7),
    ("cmd+8", "tabs.goto", "index", 8),
    ("cmd+9", "tabs.goto", "index", 9),
    // The windows of the numbers from 1 to 9.
    ("alt+cmd+1", "window.goto", "index", 1),
    ("alt+cmd+2", "window.goto", "index", 2),
    ("alt+cmd+3", "window.goto", "index", 3),
    ("alt+cmd+4", "window.goto", "index", 4),
    ("alt+cmd+5", "window.goto", "index", 5),
    ("alt+cmd+6", "window.goto", "index", 6),
    ("alt+cmd+7", "window.goto", "index", 7),
    ("alt+cmd+8", "window.goto", "index", 8),
    ("alt+cmd+9", "window.goto", "index", 9),
];

/// The bindings of a command of a direction, after the others: each of its
/// keys, the command, and the direction, `left`, `right`, `up` or `down`,
/// as iTerm2 binds its splits' keys.
pub const DIRECTED: [(&str, &str, &str); 10] = [
    // A pane split right or down.
    ("cmd+d", "splits.split", "right"),
    ("cmd+shift+d", "splits.split", "down"),
    // The keyboard to the pane beside.
    ("alt+cmd+left", "splits.focus", "left"),
    ("alt+cmd+right", "splits.focus", "right"),
    ("alt+cmd+up", "splits.focus", "up"),
    ("alt+cmd+down", "splits.focus", "down"),
    // A divider a cell along.
    ("ctrl+cmd+left", "splits.resize", "left"),
    ("ctrl+cmd+right", "splits.resize", "right"),
    ("ctrl+cmd+up", "splits.resize", "up"),
    ("ctrl+cmd+down", "splits.resize", "down"),
];

/// The bindings of a command of an argument, a whole number, that hold
/// while a plugin's panel has the keyboard, after the others: each of its
/// keys, the command, the argument's name and its value, and the context.
pub const IN_PANELS: [(&str, &str, &str, i64, &str); 2] = [
    // The command palette's arrows, through its list while its field has
    // the keys.
    (
        "up",
        "command-palette.move",
        "delta",
        -1,
        "command-palette.focused",
    ),
    (
        "down",
        "command-palette.move",
        "delta",
        1,
        "command-palette.focused",
    ),
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
        for (keys, command, name, value) in ARGUED {
            cx.bind(Binding {
                keys: keys.to_owned(),
                command: command.to_owned(),
                args: Args::from([(name.to_owned(), Value::Integer(value))]),
                when: None,
            })?;
        }
        for (keys, command, direction) in DIRECTED {
            cx.bind(Binding {
                keys: keys.to_owned(),
                command: command.to_owned(),
                args: Args::from([("direction".to_owned(), Value::String(direction.to_owned()))]),
                when: None,
            })?;
        }
        for (keys, command, name, value, when) in IN_PANELS {
            cx.bind(Binding {
                keys: keys.to_owned(),
                command: command.to_owned(),
                args: Args::from([(name.to_owned(), Value::Integer(value))]),
                when: Some(when.to_owned()),
            })?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
