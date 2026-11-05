//! Built-in plugin of the tabs.
//!
//! Its commands open a tab in the focused window, close the focused one,
//! or its pane while it has splits, asking first while a program other
//! than a shell runs there, select the next, the previous or one by its
//! number, move it, rename it, give it a color, and move it out to a
//! window of its own; the system's keymap binds their keys. Its tab bar
//! shows each window's tabs, as its section of the config, `[plugins.tabs]`,
//! lays them out, a click selecting a tab or closing it. Like every
//! built-in plugin, it depends only on the plugin SDK.

#![forbid(unsafe_code)]

mod bar;

use std::collections::BTreeMap;

use lokyr_plugin_sdk::command::{Args, Category, Code, Command, Error, Kind, Param, Value};
use lokyr_plugin_sdk::event::WorkspaceEvent;
use lokyr_plugin_sdk::host::{BarPlace, Color, TabId, TabInfo, WindowId};
use lokyr_plugin_sdk::message::{Data, Event, KEYS_CHANGED};
use lokyr_plugin_sdk::plugin::{Cx, Plugin};
use lokyr_plugin_sdk::ui::{UiEvent, ViewId};
use lokyr_plugin_sdk::view::{Node, UiEventKind};

use crate::bar::Settings;

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

/// The most tabs whose keys a tab bar shows, from the first.
const NUMBERED: usize = 9;

/// The tab bar of `tabs`, each of the keys that select it of `keys`, and of
/// the keys that bring its window forward, `window`, as the section of the
/// config `config` lays it out, and its place: as a window shows it, for
/// the golden images of its looks.
#[doc(hidden)]
pub fn tab_bar(
    tabs: &[TabInfo],
    keys: &[Option<String>],
    window: Option<&str>,
    config: &Data,
) -> (Option<Node>, BarPlace) {
    let settings = Settings::of(config);
    (
        bar::bar(tabs, keys, window, &settings),
        settings.place(None),
    )
}

/// The plugin of the tabs: the tabs whose titles it asked the user for, by
/// their questions; its bar's config, the width that the user's drag left
/// its column, and the views that it rendered, each of its last tree, which
/// it renders again as their trees change.
#[derive(Debug, Default)]
pub struct Tabs {
    renaming: BTreeMap<u64, TabId>,
    settings: Settings,
    width: Option<f32>,
    views: Vec<(ViewId, Option<Node>)>,
}

impl Plugin for Tabs {
    fn activate(&mut self, cx: &mut Cx<'_>) -> Result<(), Error> {
        // The column's width of the user's last drag, as its state keeps it.
        if let Ok(Data::Map(state)) = cx.state() {
            self.width = match state.get("column-width") {
                Some(Data::Number(width)) => Some(*width as f32),
                Some(Data::Integer(width)) => Some(*width as f32),
                _ => None,
            };
        }
        self.configure(cx)?;
        let mut names: Vec<&str> = WorkspaceEvent::NAMES.to_vec();
        names.push(KEYS_CHANGED);
        cx.subscribe(&names)?;
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

    fn config_schema(&self) -> Option<&'static str> {
        Some(include_str!("../tabs.schema.json"))
    }

    /// A window's tab bar, of its tabs, each of the keys that select it.
    fn render(&mut self, cx: &mut Cx<'_>, view: &ViewId) -> Option<Node> {
        let node = self.tree(cx, view);
        match self.views.iter_mut().find(|(known, _)| known == view) {
            Some((_, last)) => last.clone_from(&node),
            None => self.views.push((view.clone(), node.clone())),
        }
        node
    }

    /// A click on a tab selects it; on its close button, closes it, asking
    /// first while a program other than a shell runs there; on the button
    /// of a new tab, opens one.
    fn on_ui_event(&mut self, cx: &mut Cx<'_>, event: &UiEvent) -> Result<(), Error> {
        let ViewId::TabBar(window) = &event.view else {
            return Ok(());
        };
        if event.kind != UiEventKind::Click {
            return Ok(());
        }
        let key = event.key.0.as_str();
        let id = |prefix: &str| key.strip_prefix(prefix)?.parse::<u64>().ok();
        if key == bar::NEW_KEY {
            let (tab, _) = cx.new_tab(*window, None)?;
            return cx.activate_tab(tab);
        }
        if let Some(id) = id("close-") {
            return cx.close_tab(TabId(id), true);
        }
        if let Some(id) = id("tab-") {
            return cx.activate_tab(TabId(id));
        }
        Ok(())
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
        let mut again = false;
        for event in events {
            match event {
                Event::Answered { id, text } => {
                    let (Some(tab), Some(text)) = (self.renaming.remove(id), text) else {
                        continue;
                    };
                    let title = text.trim();
                    // A tab that closed meanwhile takes nothing.
                    let _ = cx.set_tab_title(tab, (!title.is_empty()).then_some(title));
                }
                Event::ConfigChanged => {
                    self.configure(cx)?;
                    again = true;
                }
                // The user's drag of the column's edge, which the state
                // keeps for the next start.
                Event::TabBarResized { width } => {
                    self.width = Some(*width);
                    let state = Data::Map(BTreeMap::from([(
                        "column-width".to_owned(),
                        Data::Number(f64::from(*width)),
                    )]));
                    // Without a state directory, the width stays for this run.
                    let _ = cx.set_state(state);
                }
                Event::Workspace(_) | Event::KeysChanged => again = true,
                _ => {}
            }
        }
        if again {
            self.render_again(cx);
        }
        Ok(())
    }
}

impl Tabs {
    /// Takes its section of the config, and puts its bar at its place: a
    /// column's width that the config changes takes the place of the
    /// user's drag's.
    fn configure(&mut self, cx: &mut Cx<'_>) -> Result<(), Error> {
        let settings = Settings::of(&cx.config()?);
        if settings.column_width != self.settings.column_width && !self.views.is_empty() {
            self.width = None;
        }
        self.settings = settings;
        cx.set_tab_bar_place(self.settings.place(self.width))
    }

    /// The tree of `view`, a window's tab bar: its tabs, each of the keys
    /// that select it; none of another view, or of a window that closed.
    fn tree(&self, cx: &mut Cx<'_>, view: &ViewId) -> Option<Node> {
        let ViewId::TabBar(window) = view else {
            return None;
        };
        let tabs = cx.tabs(*window).ok()?;
        // The keys that bring the window forward, which a bar in the
        // titlebar shows after its buttons.
        let shortcut = cx
            .windows()
            .ok()?
            .into_iter()
            .find(|shown| shown.id == *window)
            .and_then(|shown| shown.keys);
        let keys: Vec<Option<String>> = (1..=NUMBERED)
            .take(tabs.len())
            .map(|index| {
                let args = Args::from([("index".to_owned(), Value::Integer(index as i64))]);
                cx.keys_of(GOTO, args).ok().flatten()
            })
            .collect();
        bar::bar(&tabs, &keys, shortcut.as_deref(), &self.settings)
    }

    /// Renders again its views whose trees changed, so that the host draws
    /// no frame of the same; those of windows that closed go.
    fn render_again(&mut self, cx: &mut Cx<'_>) {
        let views = std::mem::take(&mut self.views);
        for (view, last) in views {
            if self.tree(cx, &view) != last && cx.request_render(view.clone()).is_err() {
                continue;
            }
            self.views.push((view, last));
        }
    }

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
