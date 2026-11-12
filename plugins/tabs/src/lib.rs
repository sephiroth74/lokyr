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

use std::collections::{BTreeMap, BTreeSet};

use lokyr_plugin_sdk::command::{Args, Category, Code, Command, Error, Kind, Param, Value};
use lokyr_plugin_sdk::event::WorkspaceEvent;
use lokyr_plugin_sdk::host::{
    BarPlace, Color, MenuItem, MenuPlace, ScreenPoint, TabId, TabInfo, WindowId,
};
use lokyr_plugin_sdk::message::{Data, Event, KEYS_CHANGED};
use lokyr_plugin_sdk::plugin::{Cx, Plugin};
use lokyr_plugin_sdk::ui::{Anchor, PanelId, PanelOptions, Role, UiEvent, ViewId};
use lokyr_plugin_sdk::view::{self, Node, UiEventKind};

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
/// A tab's context menu, a panel, and its items' keys; and the message of
/// its close of the other tabs.
const MENU: &str = "tab-menu";
const MENU_RENAME: &str = "menu-rename";
const MENU_DETACH: &str = "menu-detach";
const MENU_CLOSE: &str = "menu-close";
const MENU_CLOSE_OTHERS: &str = "menu-close-others";
/// The context menu's submenu of the tab's color: its title's message, its
/// items' keys' start, and the item of no color.
const MENU_COLOR: &str = "menu-color";
const MENU_COLOR_NONE: &str = "menu-color-none";

/// A color of a tab's context menu: its name, of its item's key and its
/// message, its English title, the dot before it, as a menu of the system's
/// shows no swatch, and its red, green and blue.
type MenuColor = (&'static str, &'static str, &'static str, (u8, u8, u8));

/// The colors of a tab's context menu, macOS's system colors.
const COLORS: [MenuColor; 7] = [
    ("red", "Red", "🔴", (0xFF, 0x3B, 0x30)),
    ("orange", "Orange", "🟠", (0xFF, 0x95, 0x00)),
    ("yellow", "Yellow", "🟡", (0xFF, 0xCC, 0x00)),
    ("green", "Green", "🟢", (0x34, 0xC7, 0x59)),
    ("blue", "Blue", "🔵", (0x00, 0x7A, 0xFF)),
    ("purple", "Purple", "🟣", (0xAF, 0x52, 0xDE)),
    ("gray", "Gray", "⚫", (0x8E, 0x8E, 0x93)),
];
const CLOSE_OTHERS: &str = "bar-close-others";

/// The messages of the bar's own texts: the button of a new tab's, a
/// tab's close button's, and the title of a tab of none, of its number.
const NEW_TAB: &str = "bar-new-tab";
const CLOSE_TAB: &str = "bar-close-tab";
const UNTITLED: &str = "bar-untitled";

/// The messages of what accessibility calls a tab's marks.
const MARKS: [&str; 8] = [
    "bar-mark-activity",
    "bar-mark-bell",
    "bar-mark-running",
    "bar-mark-secure",
    "bar-mark-working",
    "bar-mark-waiting",
    "bar-mark-done",
    "bar-mark-error",
];

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
    let place = if bar::titled(tabs.len(), &settings) {
        BarPlace::Title
    } else {
        settings.place(None)
    };
    (
        bar::bar_with(tabs, keys, window, (&settings, &bar::BarTexts::default())),
        place,
    )
}

/// The plugin of the tabs: the tabs whose titles it asked the user for, by
/// their questions; its bar's config, the width that the user's drag left
/// its column, and the config's width that the drag took the place of; and
/// the views that it rendered, each of its last tree, which it renders
/// again as their trees change.
#[derive(Debug, Default)]
pub struct Tabs {
    renaming: BTreeMap<u64, (TabId, String)>,
    /// The tab whose context menu is open, of its window.
    menu: Option<(WindowId, TabId)>,
    settings: Settings,
    width: Option<f32>,
    dragged_over: Option<f32>,
    views: Vec<(ViewId, Option<Node>)>,
    /// The windows whose bar shows their single tab's title in their
    /// titlebar.
    titled: BTreeSet<WindowId>,
}

/// The keys of the plugin's state: the column's width of the user's last
/// drag, and the config's width that it took the place of.
const DRAGGED: &str = "column-width";
const DRAGGED_OVER: &str = "config-width";

impl Plugin for Tabs {
    fn activate(&mut self, cx: &mut Cx<'_>) -> Result<(), Error> {
        // The column's width of the user's last drag, as its state keeps it,
        // and the config's width that it took the place of.
        if let Ok(Data::Map(state)) = cx.state() {
            let width = |key: &str| match state.get(key) {
                Some(Data::Number(width)) => Some(*width as f32),
                Some(Data::Integer(width)) => Some(*width as f32),
                _ => None,
            };
            self.width = width(DRAGGED);
            self.dragged_over = width(DRAGGED_OVER);
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
        // The pane's context menu renames its tab.
        cx.add_menu_item(MenuItem {
            place: MenuPlace::PaneContext,
            command: RENAME.to_owned(),
            args: Args::new(),
            title: None,
            submenu: None,
            ticked: false,
        })?;
        Ok(())
    }

    fn catalogs(&self) -> lokyr_plugin_sdk::plugin::Catalogs {
        lokyr_plugin_sdk::plugin::built_in(&CATALOGS)
    }

    fn config_schema(&self) -> Option<std::borrow::Cow<'static, str>> {
        Some(include_str!("../tabs.schema.json").into())
    }

    /// A window's tab bar, of its tabs, each of the keys that select it;
    /// and a tab's context menu.
    fn render(&mut self, cx: &mut Cx<'_>, view: &ViewId) -> Option<Node> {
        if *view == ViewId::Panel(PanelId::from(MENU)) {
            return Some(self.menu_tree(cx));
        }
        let node = self.tree(cx, view);
        if let ViewId::TabBar(window) = view {
            self.place_window(cx, *window);
        }
        match self.views.iter_mut().find(|(known, _)| known == view) {
            Some((_, last)) => last.clone_from(&node),
            None => self.views.push((view.clone(), node.clone())),
        }
        node
    }

    /// A click on a tab selects it; on its close button, closes it, asking
    /// first while a program other than a shell runs there; on the button
    /// of a new tab, opens one.
    /// A click selects a tab, closes it on its close button, and opens one
    /// on the button of a new tab; a middle click closes a tab, a double
    /// click renames it, and the context's button opens its menu, whose
    /// items act on it; a tab dropped goes to the bar, into the pane or to
    /// the place where it fell.
    fn on_ui_event(&mut self, cx: &mut Cx<'_>, event: &UiEvent) -> Result<(), Error> {
        let key = event.key.0.as_str();
        if event.view == ViewId::Panel(PanelId::from(MENU)) {
            return self.chosen(cx, key);
        }
        let ViewId::TabBar(window) = &event.view else {
            return Ok(());
        };
        let window = *window;
        let id = |prefix: &str| key.strip_prefix(prefix)?.parse::<u64>().ok();
        let tab = id("tab-").map(TabId);
        match (&event.kind, tab) {
            (UiEventKind::Click, _) if key == bar::NEW_KEY => {
                let (tab, _) = cx.new_tab(window, None)?;
                cx.activate_tab(tab)
            }
            (UiEventKind::Click, _) if let Some(id) = id("close-") => cx.close_tab(TabId(id), true),
            (UiEventKind::Click, Some(tab)) => cx.activate_tab(tab),
            (UiEventKind::MiddleClick, Some(tab)) => cx.close_tab(tab, true),
            // A tab dragged to another place in its bar, which the bar's
            // next render shows.
            (UiEventKind::Reorder { index }, Some(tab)) => cx.move_tab(tab, window, *index),
            // A tab dragged out of its bar, into another window's, which
            // shows it, or out of every window, into one of its own under
            // the pointer, its window moving there of a tab alone.
            (UiEventKind::DropOnBar { window, index }, Some(tab)) => {
                cx.move_tab(tab, *window, *index)?;
                cx.activate_tab(tab)
            }
            // Into a pane of another tab, beside it, at its edge.
            (UiEventKind::DropOnPane { pane, edge }, Some(tab)) => {
                cx.tab_into_pane(tab, *pane, *edge)
            }
            (UiEventKind::DropOutside { x, y }, Some(tab)) => {
                cx.tab_to_new_window_at(tab, ScreenPoint { x: *x, y: *y })
            }
            (UiEventKind::DoubleClick, Some(tab)) => {
                match cx.tabs(window)?.into_iter().find(|shown| shown.id == tab) {
                    Some(shown) => self.rename(cx, window, &shown, &Args::new()),
                    None => Ok(()),
                }
            }
            (UiEventKind::ContextMenu, Some(tab)) => {
                self.menu = Some((window, tab));
                // A menu of another tab's, open still, gives way.
                let _ = cx.close_panel(MENU);
                cx.open_panel(
                    MENU,
                    PanelOptions {
                        anchor: Anchor::Pointer(window),
                        role: Role::Menu,
                        close_outside: true,
                        close_escape: true,
                        takes_keys: false,
                        close_typing: false,
                    },
                )
            }
            _ => Ok(()),
        }
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
                match index
                    .checked_sub(1)
                    .and_then(|index| usize::try_from(index).ok())
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
        let mut shown = Vec::new();
        for event in events {
            match event {
                Event::Answered { id, text } => {
                    let (Some((tab, asked)), Some(text)) = (self.renaming.remove(id), text) else {
                        continue;
                    };
                    let title = text.trim();
                    // The text that it asked with, unchanged, leaves the
                    // tab as it was, its folder and program its own.
                    if title == asked.trim() {
                        continue;
                    }
                    // A tab that closed meanwhile takes nothing.
                    let _ = cx.set_tab_title(tab, (!title.is_empty()).then_some(title));
                }
                Event::ConfigChanged => {
                    self.configure(cx)?;
                    again = true;
                }
                // The user's drag of the column's edge, which the state
                // keeps for the next start, with the config's width that it
                // takes the place of.
                Event::TabBarResized { width } => {
                    self.width = Some(*width);
                    self.dragged_over = Some(self.settings.column_width);
                    let state = Data::Map(BTreeMap::from([
                        (DRAGGED.to_owned(), Data::Number(f64::from(*width))),
                        (
                            DRAGGED_OVER.to_owned(),
                            Data::Number(f64::from(self.settings.column_width)),
                        ),
                    ]));
                    // Without a state directory, the width stays for this run.
                    let _ = cx.set_state(state);
                }
                // A tab shown, which its bar scrolls to, as its render
                // has it.
                Event::Workspace(WorkspaceEvent::TabActivated { tab, window }) => {
                    shown.push((*window, *tab));
                    again = true;
                }
                // A closed window's view goes.
                Event::Workspace(WorkspaceEvent::WindowClosed { window }) => {
                    let closed = ViewId::TabBar(*window);
                    self.views.retain(|(view, _)| *view != closed);
                    self.titled.remove(window);
                }
                Event::Workspace(_) | Event::KeysChanged => again = true,
                _ => {}
            }
        }
        if again {
            self.render_again(cx);
        }
        for (window, tab) in shown {
            // A bar of no view, as of a single tab, has nothing to show.
            let _ = cx.reveal(ViewId::TabBar(window), bar::tab_key(tab.0).as_str());
        }
        Ok(())
    }
}

impl Tabs {
    /// Takes its section of the config, and puts its bar at its place: the
    /// user's drag's width stands while the config's is the one that the
    /// drag took the place of, at a start too, as the file may change while
    /// Lokyr is closed; a new one takes the drag's place, which the state
    /// forgets.
    fn configure(&mut self, cx: &mut Cx<'_>) -> Result<(), Error> {
        let settings = Settings::of(&cx.config()?);
        if self.width.is_some() {
            // A state of a drag, from before its config's width was kept,
            // was of the config of now.
            let over = *self.dragged_over.get_or_insert(settings.column_width);
            if over != settings.column_width {
                self.width = None;
                self.dragged_over = None;
                // Without a state directory, nothing was kept.
                let _ = cx.set_state(Data::Map(BTreeMap::new()));
            }
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
        let mut tabs = cx.tabs(*window).ok()?;
        let shown = cx
            .windows()
            .ok()?
            .into_iter()
            .find(|shown| shown.id == *window);
        // The keys that bring the window forward, which a bar in the
        // titlebar shows after its buttons.
        let shortcut = shown.as_ref().and_then(|shown| shown.keys.clone());
        // The window's bell, which its shown tab shows where the bar hides
        // the system's title that would: a single tab's title, or a bar in
        // the titlebar, at the top or down a sidebar.
        let hides_title = bar::titled(tabs.len(), &self.settings)
            || matches!(
                self.settings.place(None),
                BarPlace::Titlebar | BarPlace::Sidebar { .. }
            );
        if shown.is_some_and(|shown| shown.bell)
            && hides_title
            && let Some(tab) = tabs.iter_mut().find(|tab| tab.active)
        {
            tab.bell = true;
        }
        let keys: Vec<Option<String>> = (1..=NUMBERED)
            .take(tabs.len())
            .map(|index| {
                let args = Args::from([("index".to_owned(), Value::Integer(index as i64))]);
                cx.keys_of(GOTO, args).ok().flatten()
            })
            .collect();
        // Its own texts, of its catalogs in the language shown, English's
        // where a host gives none.
        let message = |cx: &mut Cx<'_>, id: &str, args: &[(&str, Data)]| {
            cx.message(id, args).ok().filter(|text| text != id)
        };
        let english = bar::BarTexts::default();
        let marked = |cx: &mut Cx<'_>, at: usize, english: String| {
            message(cx, MARKS[at], &[]).unwrap_or(english)
        };
        let marks = english.marks.clone();
        let marks = bar::MarkTexts {
            activity: marked(cx, 0, marks.activity),
            bell: marked(cx, 1, marks.bell),
            running: marked(cx, 2, marks.running),
            secure: marked(cx, 3, marks.secure),
            working: marked(cx, 4, marks.working),
            waiting: marked(cx, 5, marks.waiting),
            done: marked(cx, 6, marks.done),
            error: marked(cx, 7, marks.error),
        };
        let texts = bar::BarTexts {
            marks,
            new_tab: message(cx, NEW_TAB, &[]).unwrap_or(english.new_tab),
            close_tab: message(cx, CLOSE_TAB, &[]).unwrap_or(english.close_tab),
            untitled: (1..)
                .zip(&tabs)
                .filter(|(_, tab)| tab.title.is_empty())
                .filter_map(|(number, _)| {
                    let index = Data::Integer(i64::try_from(number).unwrap_or(i64::MAX));
                    Some((number, message(cx, UNTITLED, &[("index", index)])?))
                })
                .collect(),
        };
        bar::bar_with(&tabs, &keys, shortcut.as_deref(), (&self.settings, &texts))
    }

    /// The context menu of a tab: its rename, its color, its move to a
    /// window of its own, its close and the others', of their titles in the
    /// language shown.
    fn menu_tree(&self, cx: &mut Cx<'_>) -> Node {
        // The color of the tab that the menu opened over, which its item
        // ticks.
        let current = self.menu.and_then(|(window, tab)| {
            cx.tabs(window)
                .ok()?
                .into_iter()
                .find(|shown| shown.id == tab)
        });
        let current = current.and_then(|shown| shown.color);
        let mut title = |id: &str, english: &str| {
            cx.message(id, &[])
                .ok()
                .filter(|text| text != id)
                .unwrap_or_else(|| english.to_owned())
        };
        let ticked = |label: String, on: bool, key: &str| {
            Node::new(view::Kind::Toggle(view::Toggle {
                label,
                on,
                ..view::Toggle::default()
            }))
            .key(key)
        };
        let mut colors = vec![Node::text(title(MENU_COLOR, "Color"))];
        for (name, english, dot, (red, green, blue)) in COLORS {
            let key = format!("{MENU_COLOR}-{name}");
            let label = format!("{dot} {}", title(&key, english));
            let on = current == Some(Color::Rgb(red, green, blue));
            colors.push(ticked(label, on, &key));
        }
        colors.push(Node::separator());
        let none = title(MENU_COLOR_NONE, "No Color");
        colors.push(ticked(none, current.is_none(), MENU_COLOR_NONE));
        Node::container(vec![
            Node::button(title("command-tabs-rename", "Rename Tab…")).key(MENU_RENAME),
            Node::container(colors),
            Node::button(title("command-tabs-detach", "Move Tab to New Window")).key(MENU_DETACH),
            Node::separator(),
            Node::button(title("command-tabs-close", "Close Tab")).key(MENU_CLOSE),
            Node::button(title(CLOSE_OTHERS, "Close Other Tabs")).key(MENU_CLOSE_OTHERS),
        ])
    }

    /// The item `key` of a tab's context menu, chosen: of the tab that the
    /// menu opened over, if it is open still.
    fn chosen(&mut self, cx: &mut Cx<'_>, key: &str) -> Result<(), Error> {
        let Some((window, tab)) = self.menu.take() else {
            return Ok(());
        };
        let _ = cx.close_panel(MENU);
        let Ok(tabs) = cx.tabs(window) else {
            return Ok(());
        };
        let Some(shown) = tabs.iter().find(|shown| shown.id == tab) else {
            return Ok(());
        };
        match key {
            MENU_RENAME => self.rename(cx, window, shown, &Args::new()),
            MENU_DETACH if tabs.len() > 1 => cx.tab_to_new_window(tab),
            MENU_CLOSE => cx.close_tab(tab, true),
            MENU_CLOSE_OTHERS => {
                for other in tabs.iter().filter(|other| other.id != tab) {
                    cx.close_tab(other.id, true)?;
                }
                Ok(())
            }
            MENU_COLOR_NONE => cx.set_tab_color(tab, None),
            _ => match COLORS.iter().find(|(name, ..)| {
                key.strip_prefix(MENU_COLOR)
                    .and_then(|rest| rest.strip_prefix('-'))
                    == Some(name)
            }) {
                Some(&(.., (red, green, blue))) => {
                    cx.set_tab_color(tab, Some(Color::Rgb(red, green, blue)))
                }
                None => Ok(()),
            },
        }
    }

    /// Puts the bar of `window` in its titlebar as its title while it has a
    /// single tab whose bar hides, at the bar's place otherwise, as its
    /// tree, which this render gives, shows it, so that the place and the
    /// tree come together.
    fn place_window(&mut self, cx: &mut Cx<'_>, window: WindowId) {
        let count = cx.tabs(window).map_or(0, |tabs| tabs.len());
        let titled = count > 0 && bar::titled(count, &self.settings);
        if titled == self.titled.contains(&window) {
            return;
        }
        let place = titled.then_some(BarPlace::Title);
        // A window that closed meanwhile keeps nothing.
        if cx.set_window_tab_bar_place(window, place).is_ok() && titled {
            self.titled.insert(window);
        } else {
            self.titled.remove(&window);
        }
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
        self.renaming.insert(id, (tab.id, tab.title.clone()));
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
