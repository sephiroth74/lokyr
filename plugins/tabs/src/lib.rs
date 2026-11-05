//! Built-in plugin of the tabs.
//!
//! Its commands open a tab in the focused window, close the focused one,
//! or its pane while it has splits, asking first while a program other
//! than a shell runs there, select the next, the previous or one by its
//! number, move it, rename it, give it a color, and move it out to a
//! window of its own; the system's keymap binds their keys. Like every
//! built-in plugin, it depends only on the plugin SDK.

#![forbid(unsafe_code)]

use std::collections::BTreeMap;

use lokyr_plugin_sdk::command::{Args, Category, Code, Command, Error, Kind, Param, Value};
use lokyr_plugin_sdk::host::{Color, TabId, TabInfo, WindowId};
use lokyr_plugin_sdk::message::Event;
use lokyr_plugin_sdk::plugin::{Cx, Plugin};

lokyr_plugin_sdk::export_plugin!(Tabs);

/// Opens a tab in the focused window, after its others, which it shows.
const NEW: &str = "tabs.new";

/// Closes the focused tab, or its focused pane while it has others.
const CLOSE: &str = "tabs.close";

/// Shows the next tab, the first after the last.
const NEXT: &str = "tabs.next";

/// Shows the previous tab, the last before the first.
const PREV: &str = "tabs.prev";

/// Shows the tab of its `index`, from 1.
const GOTO: &str = "tabs.goto";

/// Moves the focused tab by its `delta` among its window's tabs.
const MOVE: &str = "tabs.move";

/// Gives the focused tab its `title`, or asks for one without it.
const RENAME: &str = "tabs.rename";

/// Gives the focused tab its `color`, as `#rrggbb`, or its program's back
/// with an empty one.
const SET_COLOR: &str = "tabs.set-color";

/// Moves the focused tab out to a window of its own.
const DETACH: &str = "tabs.detach";

/// The plugin's catalogs, of its commands' titles and its question's
/// texts.
static CATALOGS: [(&str, &str); 2] = [
    ("en", include_str!("../locales/en/tabs.ftl")),
    ("it", include_str!("../locales/it/tabs.ftl")),
];

/// A command: its name, its English title, and its parameter, if it has
/// one: its name, its kind, and its default.
type Spec = (
    &'static str,
    &'static str,
    Option<(&'static str, Kind, Option<Value>)>,
);

/// The commands, in the order of the menus.
fn commands() -> [Spec; 9] {
    let empty = || Some(Value::String(String::new()));
    [
        (NEW, "New Tab", None),
        (CLOSE, "Close Tab", None),
        (NEXT, "Select Next Tab", None),
        (PREV, "Select Previous Tab", None),
        (GOTO, "Select Tab", Some(("index", Kind::Integer, None))),
        (MOVE, "Move Tab", Some(("delta", Kind::Integer, None))),
        (
            RENAME,
            "Rename Tab…",
            Some(("title", Kind::String, empty())),
        ),
        (
            SET_COLOR,
            "Set Tab Color",
            Some(("color", Kind::String, None)),
        ),
        (DETACH, "Move Tab to New Window", None),
    ]
}

/// The plugin of the tabs, and the tabs whose titles it asked the user
/// for, by their questions.
#[derive(Debug, Default)]
pub struct Tabs {
    renaming: BTreeMap<u64, TabId>,
}

impl Plugin for Tabs {
    fn activate(&mut self, cx: &mut Cx<'_>) -> Result<(), Error> {
        for (name, title, param) in commands() {
            cx.register_command(Command {
                name: name.to_owned(),
                title: title.to_owned(),
                category: Category::Tabs,
                capabilities: Vec::new(),
                params: param
                    .into_iter()
                    .map(|(name, kind, default)| Param {
                        name: name.to_owned(),
                        kind,
                        default,
                    })
                    .collect(),
                when: None,
            })?;
        }
        Ok(())
    }

    fn catalogs(&self) -> &'static [(&'static str, &'static str)] {
        &CATALOGS
    }

    fn on_command(&mut self, cx: &mut Cx<'_>, name: &str, args: Args) -> Result<(), Error> {
        // Without a focused window, there is nothing to do.
        let Some(window) = cx.focused_window() else {
            return Ok(());
        };
        if name == NEW {
            let (tab, _) = cx.new_tab(window, None)?;
            return cx.activate_tab(tab);
        }
        let tabs = cx.tabs(window)?;
        let Some(at) = tabs.iter().position(|tab| tab.active) else {
            return Ok(());
        };
        let tab = &tabs[at];
        match name {
            CLOSE if tab.panes.len() > 1 => cx.close_pane(tab.focused, true),
            CLOSE => cx.close_tab(tab.id, true),
            NEXT => cx.activate_tab(tabs[(at + 1) % tabs.len()].id),
            PREV => cx.activate_tab(tabs[(at + tabs.len() - 1) % tabs.len()].id),
            GOTO => {
                let index = integer(&args, "index");
                match usize::try_from(index - 1)
                    .ok()
                    .and_then(|index| tabs.get(index))
                {
                    Some(shown) => cx.activate_tab(shown.id),
                    None => Ok(()),
                }
            }
            MOVE => self.move_by(cx, window, &tabs, at, integer(&args, "delta")),
            RENAME => self.rename(cx, window, tab, &args),
            SET_COLOR => {
                let text = match args.get("color") {
                    Some(Value::String(text)) => text.trim(),
                    _ => "",
                };
                let color = if text.is_empty() {
                    None
                } else {
                    Some(color(text).ok_or_else(|| {
                        Error::new(
                            Code::Failed,
                            format!("`{text}` is not a color, as `#ff8800`"),
                        )
                    })?)
                };
                cx.set_tab_color(tab.id, color)
            }
            DETACH if tabs.len() > 1 => cx.tab_to_new_window(tab.id),
            DETACH => Ok(()),
            _ => Err(Error::new(
                Code::NotFound,
                format!("tabs has no command `{name}`"),
            )),
        }
    }

    /// The user's title of a tab that it asked for, which the tab takes;
    /// an empty one gives it its program's back, and none leaves it as it
    /// was.
    fn on_event(&mut self, cx: &mut Cx<'_>, events: &[Event]) -> Result<(), Error> {
        for event in events {
            let Event::Answered { id, text } = event else {
                continue;
            };
            let (Some(tab), Some(text)) = (self.renaming.remove(id), text) else {
                continue;
            };
            let title = text.trim();
            // A tab that closed meanwhile takes nothing.
            let _ = cx.set_tab_title(tab, (!title.is_empty()).then_some(title));
        }
        Ok(())
    }
}

impl Tabs {
    /// Moves the tab at `at` of `window`'s `tabs` by `delta`, as far as
    /// their ends.
    fn move_by(
        &mut self,
        cx: &mut Cx<'_>,
        window: WindowId,
        tabs: &[TabInfo],
        at: usize,
        delta: i64,
    ) -> Result<(), Error> {
        let last = i64::try_from(tabs.len() - 1).unwrap_or(i64::MAX);
        let from = i64::try_from(at).unwrap_or(i64::MAX);
        let to = from.saturating_add(delta).clamp(0, last);
        if to == from {
            return Ok(());
        }
        // Within the window's tabs, which a `usize` counts.
        cx.move_tab(tabs[at].id, window, to as usize)
    }

    /// Gives `tab` the title of `args`, or, without one, asks the user
    /// for it, in a field of its title as it is.
    fn rename(
        &mut self,
        cx: &mut Cx<'_>,
        window: WindowId,
        tab: &TabInfo,
        args: &Args,
    ) -> Result<(), Error> {
        let title = match args.get("title") {
            Some(Value::String(title)) => title.trim(),
            _ => "",
        };
        if !title.is_empty() {
            return cx.set_tab_title(tab.id, Some(title));
        }
        let id = cx.ask_text(Some(window), "rename-message", "rename-detail", &tab.title)?;
        self.renaming.insert(id, tab.id);
        Ok(())
    }
}

/// The whole number `name` of `args`, which the registry checked; 0
/// without it.
fn integer(args: &Args, name: &str) -> i64 {
    match args.get(name) {
        Some(&Value::Integer(value)) => value,
        _ => 0,
    }
}

/// The color that `text` writes, as `#ff8800` or `ff8800`, if it writes
/// one.
fn color(text: &str) -> Option<Color> {
    let hex = text.strip_prefix('#').unwrap_or(text);
    if hex.len() != 6 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let part = |at: usize| u8::from_str_radix(&hex[at..at + 2], 16).ok();
    Some(Color::Rgb(part(0)?, part(2)?, part(4)?))
}

#[cfg(test)]
mod tests;
