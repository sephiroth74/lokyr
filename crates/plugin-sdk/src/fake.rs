//! A host of the tests' own, for plugins' unit tests, as the `fake` feature
//! gives it.
//!
//! The test sets what the host holds: panes, each of its window, its tab,
//! its rows' lines and texts, its links, its styles, its selection and its
//! program's directory, the focused pane, the clipboard, the text's size,
//! the targets that are not there, and the plugin's config and state. The
//! host answers the plugin's requests as the app's does: a read with what it
//! holds as it handles it, a request that the plugin's capabilities lack,
//! or of a pane that is not there, with its error; a write and a request of
//! the app's queued, and applied as the call returns, the writes first,
//! then the app's requests, so that a read after a write, in the same call,
//! reads what was before, and a write to a tab or a pane that an earlier
//! write of the call opens finds it.
//!
//! Its workspace is the app's, of the panes that the test gives: the
//! windows, each of its tabs in their order and the one that it shows; the
//! tabs, each of its layout, the pane of its keyboard and its zoom; the
//! focused window, whose tab and pane the focused pane's are. The plugin's
//! writes change it as the app's workspace changes, a new tab, a split, a
//! close, a move, a focus, a zoom and a split's ratio, the events of each
//! owed to the plugin in the app's order; a drag of a pane's edge,
//! `resize_pane`, changes nothing of it, as it has no sizes.
//!
//! It keeps what the plugin declared, its commands, its bindings and its
//! subscriptions; what it did, its writes and its requests of the app's, as
//! the requests themselves, in their order, which a test asserts; its
//! decorations and its panels; the events that it is owed, which
//! [`FakeHost::deliver`] gives it; and the trees of its views, which
//! [`snapshot`] writes as text.
//!
//! ```ignore
//! let mut host = FakeHost::new("core-actions");
//! host.pane(PaneId(7)).selection = Some("ls -la".to_owned());
//! host.focus(Some(PaneId(7)));
//! host.command(&mut CoreActions, "pane.copy", Args::new())?;
//! assert_eq!(host.take_done(), [Request::SetClipboardText("ls -la".to_owned())]);
//! ```

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use crate::command::{Args, Capability, Code, Command, Error, Uses, is_name};
use crate::event::WorkspaceEvent;
use crate::host::{
    self, BarPlace, Binding, Color, DecorationLook, Direction, Layout, LineText, MOST_STATE,
    PaneId, Side, StyleRun, TabId, TabInfo, WindowId, WindowInfo,
};
use crate::keys::{When, parse_keys};
use crate::message::{Call, Data, Effect, Event, Request, Response};
use crate::plugin::{Cx, Plugin};
use crate::ui::{Anchor, PanelId, PanelOptions, Slot, UiEvent, ViewId};
use crate::view::{Key, Kind, Node};

/// The first id that the host gives a window, a tab, a pane, a decoration
/// or a read of its own, past those that a test gives its panes and its
/// tabs.
pub const FIRST_ID: u64 = 1001;

/// The name of the pointer's events, as a plugin subscribes to them.
const POINTER: &str = "pointer";

/// The most levels of a tree that a look for a key goes down, as the app's
/// host's.
const DEEPEST: usize = 64;

/// A pane of the fake host: where it is, and what its reads give.
#[derive(Clone, Debug, PartialEq)]
pub struct FakePane {
    /// Its window.
    pub window: WindowId,
    /// Its tab.
    pub tab: TabId,
    /// The logical line of each of its rows, by the row's number, as a read
    /// of a line's text gives it.
    pub lines: BTreeMap<i64, LineText>,
    /// The text of each of its rows, by the row's number, and whether the
    /// row wraps into the next, as a read of rows gives them: a row that
    /// it holds without one reads empty.
    pub rows: BTreeMap<i64, (String, bool)>,
    /// The targets of the OSC 8 links of its cells, by their columns and
    /// their rows' numbers.
    pub links: BTreeMap<(u16, i64), String>,
    /// The styles of each of its rows, by the row's number.
    pub styles: BTreeMap<i64, Vec<StyleRun>>,
    /// Its selection's text.
    pub selection: Option<String>,
    /// The working directory of its program in the foreground.
    pub directory: Option<String>,
    /// The numbers of the rows that it holds, of its scrollback and its
    /// screen, which a decoration's cells and a read of rows are of: a
    /// screen of 24 rows without a scrollback by default.
    pub held: std::ops::Range<i64>,
}

impl Default for FakePane {
    fn default() -> Self {
        Self {
            held: 0..24,
            window: WindowId(1),
            tab: TabId(1),
            lines: BTreeMap::new(),
            rows: BTreeMap::new(),
            links: BTreeMap::new(),
            styles: BTreeMap::new(),
            selection: None,
            directory: None,
        }
    }
}

/// A decoration that the plugin laid: its pane, its cells and its look.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FakeDecoration {
    /// Its id.
    pub id: u64,
    /// Its pane.
    pub pane: PaneId,
    /// Its first cell, by its column and its row's number.
    pub from: (u16, i64),
    /// Its last cell.
    pub to: (u16, i64),
    /// Its look.
    pub look: DecorationLook,
}

/// A window of the fake host: its tabs, in their order, and the one that
/// it shows.
#[derive(Clone, Debug, PartialEq)]
struct FakeWindow {
    tabs: Vec<TabId>,
    active: TabId,
}

/// A tab of the fake host: its window, its layout, the pane that has its
/// keyboard, the one that takes its whole room, if one does, and the title
/// and the color that the plugin gave it.
#[derive(Clone, Debug, PartialEq)]
struct FakeTab {
    window: WindowId,
    layout: Layout,
    active: PaneId,
    zoomed: Option<PaneId>,
    title: Option<String>,
    color: Option<Color>,
}

/// What a tab's leaving did to its window, which is told after the tab's
/// own change.
#[derive(Clone, Copy)]
enum Left {
    /// Nothing that the window shows.
    Kept,
    /// The window shows another tab.
    Shows(WindowId, TabId),
    /// The window closed, as it has no tab left.
    Closed(WindowId),
}

/// The ids that a write's request gave, of what it opens as it applies.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Opened {
    Nothing,
    Tab(TabId, PaneId),
    Pane(PaneId),
}

/// The host of a plugin's unit tests.
#[derive(Debug)]
pub struct FakeHost {
    caller: String,
    /// The capabilities that the plugin holds; `None` for every one, as a
    /// first-party plugin's.
    granted: Option<Vec<Capability>>,
    /// The commands that its manifest says it runs, of a plugin of the
    /// capabilities that its manifest declares.
    uses: Option<Uses>,
    /// The slots that its manifest fills; `None` for every one.
    slots: Option<Vec<Slot>>,
    /// The commands of the app's and of the other plugins', which its own
    /// join.
    others: Vec<Command>,
    /// Whether it was activated, as the config's changes reach an active
    /// plugin alone.
    activated: bool,
    panes: BTreeMap<PaneId, FakePane>,
    windows: BTreeMap<WindowId, FakeWindow>,
    tabs: BTreeMap<TabId, FakeTab>,
    /// The windows' numbers and the keys that select them, as the test
    /// gave them.
    numbers: BTreeMap<WindowId, (Option<u32>, Option<String>)>,
    /// The place of the plugin's tab bar.
    bar_place: BarPlace,
    /// The window that has the focus.
    focused: Option<WindowId>,
    /// The focus that the test gave last, which the next look at the
    /// panes takes, once the test gave them their tabs.
    given_focus: Option<Option<PaneId>>,
    clipboard: Option<String>,
    font_size: f32,
    missing: Vec<String>,
    config: Data,
    state: Data,
    commands: Vec<Command>,
    bindings: Vec<Binding>,
    subscribed: Vec<String>,
    done: Vec<Request>,
    decorations: Vec<FakeDecoration>,
    panels: BTreeMap<PanelId, PanelOptions>,
    /// The window that each open panel shows in.
    panel_windows: BTreeMap<PanelId, WindowId>,
    views: BTreeMap<ViewId, Option<Node>>,
    /// The views whose renders the plugin's calls asked for, as a panel's
    /// opening asks for its own.
    renders: Vec<ViewId>,
    owed: Vec<Event>,
    last_id: u64,
}

impl FakeHost {
    /// A host of the plugin `caller`, as its calls name it, of every
    /// capability, without panes, of text of 13 points, and of an empty
    /// config.
    pub fn new(caller: &str) -> Self {
        Self {
            caller: caller.to_owned(),
            granted: None,
            uses: None,
            slots: None,
            others: Vec::new(),
            activated: false,
            panes: BTreeMap::new(),
            windows: BTreeMap::new(),
            tabs: BTreeMap::new(),
            numbers: BTreeMap::new(),
            bar_place: BarPlace::default(),
            focused: None,
            given_focus: None,
            clipboard: None,
            font_size: 13.0,
            missing: Vec::new(),
            config: Data::Map(BTreeMap::new()),
            state: Data::Nil,
            commands: Vec::new(),
            bindings: Vec::new(),
            subscribed: Vec::new(),
            done: Vec::new(),
            decorations: Vec::new(),
            panels: BTreeMap::new(),
            panel_windows: BTreeMap::new(),
            views: BTreeMap::new(),
            renders: Vec::new(),
            owed: Vec::new(),
            last_id: FIRST_ID - 1,
        }
    }

    /// The host, whose plugin holds the capabilities of `granted` alone, as
    /// another than a first-party plugin does, of a manifest that declares
    /// them: its commands are of its namespace, and it runs those of
    /// [`FakeHost::with_uses`] alone, beside its own.
    pub fn with_capabilities(mut self, granted: Vec<Capability>) -> Self {
        self.granted = Some(granted);
        self.uses.get_or_insert_with(Uses::default);
        self
    }

    /// The host, whose plugin's manifest says that it runs the commands of
    /// `patterns`, names and namespaces, as `tabs.*`, which a plugin of
    /// [`FakeHost::with_capabilities`] checks.
    pub fn with_uses(mut self, patterns: Vec<String>) -> Self {
        self.uses = Some(Uses::new(patterns));
        self
    }

    /// The host, whose plugin's manifest fills the slots of `slots` alone,
    /// which its views are of, beside its panels.
    pub fn with_slots(mut self, slots: Vec<Slot>) -> Self {
        self.slots = Some(slots);
        self
    }

    /// The host, of the commands of `commands` beside the plugin's, as the
    /// app's and the other plugins' registered them, which the plugin's
    /// calls run.
    pub fn with_commands(mut self, commands: Vec<Command>) -> Self {
        self.others = commands;
        self
    }

    /// The pane `id`, which the host holds from now, of the window 1 and
    /// the tab 1 if it did not hold it. A pane of a tab of its own, or of a
    /// window of its own, opens them, the tab after the window's others and
    /// the pane after the tab's others, side by side.
    pub fn pane(&mut self, id: PaneId) -> &mut FakePane {
        self.panes.entry(id).or_default()
    }

    /// Gives `window` its `number` and the `keys` that select it, as the
    /// platform writes them, which a read of the windows tells.
    pub fn number_window(
        &mut self,
        window: WindowId,
        number: Option<u32>,
        keys: Option<&str>,
    ) -> &mut Self {
        self.numbers
            .insert(window, (number, keys.map(str::to_owned)));
        self
    }

    /// Gives `pane` the focus, its window's, its tab's in that window and
    /// its own in that tab, as the app's focused pane has it, or takes it
    /// away with `None`.
    pub fn focus(&mut self, pane: Option<PaneId>) -> &mut Self {
        if let Some(pane) = pane {
            self.panes.entry(pane).or_default();
        }
        self.given_focus = Some(pane);
        self
    }

    /// Puts `text` on the clipboard, or empties it.
    pub fn set_clipboard(&mut self, text: Option<&str>) -> &mut Self {
        self.clipboard = text.map(str::to_owned);
        self
    }

    /// The clipboard's text.
    pub fn clipboard(&self) -> Option<&str> {
        self.clipboard.as_deref()
    }

    /// Gives the text `size`, in points, as its reads tell it.
    pub fn set_font_size(&mut self, size: f32) -> &mut Self {
        self.font_size = size;
        self
    }

    /// Has the targets of `missing` not there, which open nothing.
    pub fn set_missing(&mut self, missing: &[&str]) -> &mut Self {
        self.missing = missing.iter().map(|&target| target.to_owned()).collect();
        self
    }

    /// Gives the plugin the config `config`, a map of its keys' values, its
    /// schema's defaults among them as the app gives them, of which an
    /// active plugin hears as [`FakeHost::deliver`] goes on.
    pub fn set_config(&mut self, config: Data) -> &mut Self {
        if config != self.config {
            self.config = config;
            if self.activated {
                self.owed.push(Event::ConfigChanged);
            }
        }
        self
    }

    /// Gives the plugin the state `state`, as an earlier run kept it.
    pub fn set_state(&mut self, state: Data) -> &mut Self {
        self.state = state;
        self
    }

    /// The plugin's state, as it last kept it.
    pub fn state(&self) -> &Data {
        &self.state
    }

    /// Owes the plugin `event`, which [`FakeHost::deliver`] gives it if it
    /// subscribed to its name, or if it is one of those of the plugin
    /// alone.
    pub fn post(&mut self, event: Event) -> &mut Self {
        self.owed.push(event);
        self
    }

    /// The commands that the plugin registered.
    pub fn commands(&self) -> &[Command] {
        &self.commands
    }

    /// The bindings that it made.
    pub fn bindings(&self) -> &[Binding] {
        &self.bindings
    }

    /// The names of the events that it subscribed to.
    pub fn subscriptions(&self) -> &[String] {
        &self.subscribed
    }

    /// What it did since the last take: its writes, in their order, then
    /// its requests of the app's, in theirs, as the requests themselves.
    pub fn take_done(&mut self) -> Vec<Request> {
        std::mem::take(&mut self.done)
    }

    /// The decorations that it laid and did not take away, in their order.
    pub fn decorations(&self) -> &[FakeDecoration] {
        &self.decorations
    }

    /// Its open panels, by their ids.
    pub fn panels(&self) -> &BTreeMap<PanelId, PanelOptions> {
        &self.panels
    }

    /// The last tree of `view` that it rendered, if it gave one.
    pub fn tree(&self, view: &ViewId) -> Option<&Node> {
        self.views.get(view).and_then(Option::as_ref)
    }

    /// The views whose renders its calls asked for since the last take, as
    /// `request_render` and a panel's opening ask, in their order, each
    /// once, which a test renders with [`FakeHost::render`].
    pub fn take_renders(&mut self) -> Vec<ViewId> {
        std::mem::take(&mut self.renders)
    }

    /// Activates `plugin`.
    pub fn activate(&mut self, plugin: &mut dyn Plugin) -> Result<(), Error> {
        self.activated = true;
        self.call(true, |cx| plugin.activate(cx))
    }

    /// Runs `plugin`'s command `name` with `args`, as the registry passes
    /// them.
    pub fn command(
        &mut self,
        plugin: &mut dyn Plugin,
        name: &str,
        args: Args,
    ) -> Result<(), Error> {
        self.call(true, |cx| plugin.on_command(cx, name, args))
    }

    /// Whether `plugin`'s command `name` would do something with `args`, as
    /// a menu asks: what the call asks is dropped, its decorations, its
    /// panels, the events that it would be owed and the ids that it took.
    pub fn applies(&mut self, plugin: &mut dyn Plugin, name: &str, args: &Args) -> bool {
        let mut applies = false;
        let _ = self.call(false, |cx| {
            applies = plugin.applies(cx, name, args);
            Ok(())
        });
        applies
    }

    /// Gives `plugin` `events`, as a batch.
    pub fn events(&mut self, plugin: &mut dyn Plugin, events: &[Event]) -> Result<(), Error> {
        self.call(true, |cx| plugin.on_event(cx, events))
    }

    /// Gives `plugin` the events that it is owed, as a batch, if it is owed
    /// any: those of the plugin alone, as a panel's closing, a read's rows,
    /// a question's answer and its config's change, and those that it
    /// subscribed to, of the
    /// workspace's, its own writes' among them, and those that it emitted;
    /// the others go.
    pub fn deliver(&mut self, plugin: &mut dyn Plugin) -> Result<(), Error> {
        let owed = std::mem::take(&mut self.owed);
        let batch: Vec<Event> = owed
            .into_iter()
            .filter(|event| {
                matches!(
                    event,
                    Event::Lines { .. }
                        | Event::PanelClosed(_)
                        | Event::ConfigChanged
                        | Event::Answered { .. }
                        | Event::TabBarResized { .. }
                ) || self.subscribed.iter().any(|name| name == event.name())
            })
            .collect();
        if batch.is_empty() {
            return Ok(());
        }
        self.events(plugin, &batch)
    }

    /// Has `plugin` render `view`, whose tree the host keeps.
    pub fn render(&mut self, plugin: &mut dyn Plugin, view: &ViewId) -> Option<Node> {
        let mut tree = None;
        let _ = self.call(true, |cx| {
            tree = plugin.render(cx, view);
            Ok(())
        });
        self.views.insert(view.clone(), tree.clone());
        tree
    }

    /// Gives `plugin` `event`, of a node of one of its views.
    pub fn ui_event(&mut self, plugin: &mut dyn Plugin, event: &UiEvent) -> Result<(), Error> {
        self.call(true, |cx| plugin.on_ui_event(cx, event))
    }

    /// Makes a call of the plugin's, `call`, on the host, and applies what
    /// it asked as it returns, with `apply`, its writes first, then its
    /// requests of the app's; or drops it, with what queuing it changed.
    fn call(
        &mut self,
        apply: bool,
        call: impl FnOnce(&mut Cx<'_>) -> Result<(), Error>,
    ) -> Result<(), Error> {
        self.settle();
        let caller = self.caller.clone();
        let kept = (!apply).then(|| {
            (
                self.decorations.clone(),
                (self.panels.clone(), self.panel_windows.clone()),
                (self.owed.len(), self.renders.len()),
                self.last_id,
            )
        });
        let mut calling = Calling {
            host: self,
            writes: Vec::new(),
            asked: Vec::new(),
        };
        let result = call(&mut Cx::new(&mut calling, &caller));
        let (writes, asked) = (calling.writes, calling.asked);
        if let Some((decorations, (panels, windows), (owed, renders), last_id)) = kept {
            self.decorations = decorations;
            self.panels = panels;
            self.panel_windows = windows;
            self.owed.truncate(owed);
            self.renders.truncate(renders);
            self.last_id = last_id;
            return result;
        }
        for (request, opened) in writes {
            self.apply(request, opened);
        }
        self.done.extend(asked);
        result
    }

    /// A new id of a window, a tab, a pane, a decoration or a read.
    fn next_id(&mut self) -> u64 {
        self.last_id += 1;
        self.last_id
    }

    /// The pane `pane`; `not-found` if the host does not hold it.
    fn held(&self, pane: PaneId) -> Result<&FakePane, Error> {
        self.panes
            .get(&pane)
            .ok_or_else(|| Error::new(Code::NotFound, format!("there is no pane {}", pane.0)))
    }

    /// The window that has the focus, if it is open.
    fn focused_window(&self) -> Option<WindowId> {
        self.focused
            .filter(|window| self.windows.contains_key(window))
    }

    /// The focused pane: that of the keyboard of the tab that the focused
    /// window shows.
    fn focused_pane(&self) -> Option<PaneId> {
        let window = self.windows.get(&self.focused_window()?)?;
        Some(self.tabs.get(&window.active)?.active)
    }

    /// Makes the workspace of the panes as the test gave them: a tab of no
    /// pane goes, and a window of no tab; a pane's tab, or its window, that
    /// the host did not hold opens, the tab after its window's others, the
    /// pane after its tab's others, side by side; a tab takes its first
    /// pane's window, as the test moved it. Then the focus that the test
    /// gave, if it gave one since.
    fn settle(&mut self) {
        let Self {
            panes,
            windows,
            tabs,
            ..
        } = self;
        let placed: Vec<(PaneId, WindowId, TabId)> = panes
            .iter()
            .map(|(&id, pane)| (id, pane.window, pane.tab))
            .collect();
        let held: BTreeSet<TabId> = placed.iter().map(|&(_, _, tab)| tab).collect();
        tabs.retain(|tab, _| held.contains(tab));
        let mut first = BTreeSet::new();
        for &(pane, window, tab) in &placed {
            let entry = tabs.entry(tab).or_insert_with(|| FakeTab {
                window,
                layout: Layout::Pane(pane),
                active: pane,
                zoomed: None,
                title: None,
                color: None,
            });
            if first.insert(tab) {
                entry.window = window;
            }
            if !leaves(&entry.layout).contains(&pane) {
                append(&mut entry.layout, pane);
            }
        }
        for (&id, tab) in tabs.iter_mut() {
            let stale: Vec<PaneId> = leaves(&tab.layout)
                .into_iter()
                .filter(|leaf| panes.get(leaf).is_none_or(|pane| pane.tab != id))
                .collect();
            for leaf in stale {
                remove(&mut tab.layout, leaf);
            }
            let kept = leaves(&tab.layout);
            if !kept.contains(&tab.active) {
                tab.active = kept[0];
            }
            if tab.zoomed.is_some_and(|zoomed| !kept.contains(&zoomed)) {
                tab.zoomed = None;
            }
        }
        for (&id, window) in windows.iter_mut() {
            window
                .tabs
                .retain(|tab| tabs.get(tab).is_some_and(|tab| tab.window == id));
        }
        for (&id, tab) in tabs.iter() {
            let window = windows.entry(tab.window).or_insert_with(|| FakeWindow {
                tabs: Vec::new(),
                active: id,
            });
            if !window.tabs.contains(&id) {
                window.tabs.push(id);
            }
        }
        windows.retain(|_, window| !window.tabs.is_empty());
        for window in windows.values_mut() {
            if !window.tabs.contains(&window.active) {
                window.active = window.tabs[0];
            }
        }
        match self.given_focus.take() {
            Some(Some(pane)) => {
                if let Some(&FakePane { window, tab, .. }) = self.panes.get(&pane) {
                    self.focused = Some(window);
                    if let Some(shown) = self.windows.get_mut(&window) {
                        shown.active = tab;
                    }
                    if let Some(tab) = self.tabs.get_mut(&tab) {
                        tab.active = pane;
                    }
                }
            }
            Some(None) => self.focused = None,
            None => {}
        }
    }

    /// Applies `request`, a write, which opens what `opened` gave, as the
    /// call returns: the workspace's writes as the app's workspace takes
    /// them, which owe the plugin their events.
    fn apply(&mut self, request: Request, opened: Opened) {
        match &request {
            Request::SetClipboardText(text) => self.clipboard = Some(text.clone()),
            Request::Undecorate { id, .. } => self.decorations.retain(|laid| laid.id != *id),
            Request::RequestRender(view) => self.ask_render(view.clone()),
            Request::SetState(state) => self.state = state.clone(),
            Request::Emit {
                name,
                schema,
                value,
            } => self.owed.push(Event::Custom {
                name: name.clone(),
                schema: schema.clone(),
                value: value.clone(),
            }),
            Request::NewTab { window, index } => {
                if let Opened::Tab(tab, pane) = opened {
                    self.open_tab(*window, index.unwrap_or(usize::MAX), tab, pane);
                }
            }
            Request::Split {
                pane,
                direction,
                side,
            } => {
                if let Opened::Pane(new) = opened {
                    self.split(*pane, *direction, *side, new);
                }
            }
            // A close that asks closes as the user's confirmation would,
            // which a test reads among the requests done.
            Request::CloseTab { tab, .. } => self.close_tab(*tab),
            Request::ClosePane { pane, .. } => self.close_pane(*pane),
            Request::SetTabTitle { tab, title } => {
                if let Some(tab) = self.tabs.get_mut(tab) {
                    tab.title.clone_from(title);
                }
            }
            Request::SetTabBarPlace(place) => self.bar_place = *place,
            Request::SetTabColor { tab, color } => {
                if let Some(tab) = self.tabs.get_mut(tab) {
                    tab.color = *color;
                }
            }
            Request::ActivateTab(tab) => self.activate_tab(*tab),
            Request::FocusPane(pane) => self.focus_pane(*pane),
            Request::MoveTab { tab, window, index } => self.move_tab(*tab, *window, *index),
            Request::TabToNewWindow(tab) => self.tab_to_new_window(*tab),
            Request::PaneToNewTab {
                pane,
                window,
                index,
            } => self.pane_to_new_tab(*pane, *window, *index),
            Request::PaneToNewWindow(pane) => self.pane_to_new_window(*pane),
            Request::Zoom { tab, pane } => self.zoom(*tab, *pane),
            Request::Equalize(tab) => self.relayout(*tab, equalize),
            Request::SetRatio { tab, path, ratio } => {
                let ratio = ratio.map(|ratio| {
                    // A split's share, of ten-thousandths, within 5 % of
                    // its parts' edges, as the app keeps it.
                    (ratio * 10_000.0).round().clamp(500.0, 9_500.0) / 10_000.0
                });
                self.relayout(*tab, |layout| set_ratio(layout, path, ratio));
            }
            _ => {}
        }
        self.done.push(request);
    }

    /// Owes the plugin the workspace's `event`.
    fn changed(&mut self, event: WorkspaceEvent) {
        self.owed.push(Event::Workspace(event));
    }

    /// Opens `tab` of `pane` in `window`, at `index` of its tabs or at
    /// their end past them; the window shows the tab that it showed.
    fn open_tab(&mut self, window: WindowId, index: usize, tab: TabId, pane: PaneId) {
        let Some(shown) = self.windows.get_mut(&window) else {
            return;
        };
        let index = index.min(shown.tabs.len());
        shown.tabs.insert(index, tab);
        self.tabs.insert(
            tab,
            FakeTab {
                window,
                layout: Layout::Pane(pane),
                active: pane,
                zoomed: None,
                title: None,
                color: None,
            },
        );
        self.panes.insert(
            pane,
            FakePane {
                window,
                tab,
                ..FakePane::default()
            },
        );
        self.changed(WorkspaceEvent::TabCreated { tab, window, index });
        self.changed(WorkspaceEvent::PaneCreated { pane, tab });
    }

    /// Splits `pane`'s part along `direction`, `new` on `side`, which ends
    /// its tab's zoom; the tab's keyboard stays where it was.
    fn split(&mut self, pane: PaneId, direction: Direction, side: Side, new: PaneId) {
        let Some(id) = self.panes.get(&pane).map(|pane| pane.tab) else {
            return;
        };
        let Some(tab) = self.tabs.get_mut(&id) else {
            return;
        };
        if !split(&mut tab.layout, pane, direction, new, side) {
            return;
        }
        tab.zoomed = None;
        let window = tab.window;
        self.panes.insert(
            new,
            FakePane {
                window,
                tab: id,
                ..FakePane::default()
            },
        );
        self.changed(WorkspaceEvent::PaneCreated { pane: new, tab: id });
        self.changed(WorkspaceEvent::LayoutChanged { tab: id });
    }

    /// Takes `tab` out of its window's tabs: the window shows the tab after
    /// it, or before it, if it showed this one, and closes if it has none
    /// left. The tab itself stays.
    fn detach_tab(&mut self, tab: TabId) -> Left {
        let Some(id) = self.tabs.get(&tab).map(|tab| tab.window) else {
            return Left::Kept;
        };
        let Some(window) = self.windows.get_mut(&id) else {
            return Left::Kept;
        };
        let Some(index) = window.tabs.iter().position(|&shown| shown == tab) else {
            return Left::Kept;
        };
        window.tabs.remove(index);
        if window.tabs.is_empty() {
            self.windows.remove(&id);
            return Left::Closed(id);
        }
        if window.active != tab {
            return Left::Kept;
        }
        let shown = window.tabs[index.min(window.tabs.len() - 1)];
        window.active = shown;
        Left::Shows(id, shown)
    }

    /// Takes `tab` and its panes away, the panes' closings and the tab's
    /// owed.
    fn drop_tab(&mut self, tab: TabId) {
        let Some(dropped) = self.tabs.remove(&tab) else {
            return;
        };
        for pane in leaves(&dropped.layout) {
            self.panes.remove(&pane);
            self.changed(WorkspaceEvent::PaneClosed { pane, tab });
        }
        self.changed(WorkspaceEvent::TabClosed {
            tab,
            window: dropped.window,
        });
    }

    /// Owes what a tab's leaving did to its window.
    fn tell_left(&mut self, left: Left) {
        match left {
            Left::Kept => {}
            Left::Shows(window, tab) => self.changed(WorkspaceEvent::TabActivated { tab, window }),
            Left::Closed(window) => self.changed(WorkspaceEvent::WindowClosed { window }),
        }
    }

    /// Closes `tab`, with its panes.
    fn close_tab(&mut self, tab: TabId) {
        if !self.tabs.contains_key(&tab) {
            return;
        }
        let left = self.detach_tab(tab);
        self.drop_tab(tab);
        self.tell_left(left);
    }

    /// Takes `pane` out of `tab`'s layout, its room to its sibling, and the
    /// tab's keyboard and zoom away from it, which the changes tell; a tab
    /// that it leaves without panes leaves its window, what that did to it
    /// returned for the caller to tell after the tab's end.
    fn detach_pane(&mut self, pane: PaneId, id: TabId) -> Option<Left> {
        let tab = self.tabs.get_mut(&id)?;
        let Some(nearest) = remove(&mut tab.layout, pane) else {
            return Some(self.detach_tab(id));
        };
        let refocused = tab.active == pane;
        if refocused {
            tab.active = nearest;
        }
        if tab.zoomed == Some(pane) {
            tab.zoomed = None;
        }
        self.changed(WorkspaceEvent::LayoutChanged { tab: id });
        if refocused {
            self.changed(WorkspaceEvent::PaneFocused {
                pane: nearest,
                tab: id,
            });
        }
        None
    }

    /// Closes `pane`: its room goes to its sibling, and its tab's keyboard,
    /// if it had it, to the sibling's pane nearest to it; a tab that it
    /// leaves without panes closes, and a window without tabs.
    fn close_pane(&mut self, pane: PaneId) {
        let Some(tab) = self.panes.remove(&pane).map(|pane| pane.tab) else {
            return;
        };
        self.changed(WorkspaceEvent::PaneClosed { pane, tab });
        if let Some(left) = self.detach_pane(pane, tab) {
            if let Some(gone) = self.tabs.remove(&tab) {
                self.changed(WorkspaceEvent::TabClosed {
                    tab,
                    window: gone.window,
                });
            }
            self.tell_left(left);
        }
    }

    /// Has `tab`'s window show it.
    fn activate_tab(&mut self, tab: TabId) {
        let Some(window) = self.tabs.get(&tab).map(|tab| tab.window) else {
            return;
        };
        if let Some(shown) = self.windows.get_mut(&window)
            && shown.active != tab
        {
            shown.active = tab;
            self.changed(WorkspaceEvent::TabActivated { tab, window });
        }
    }

    /// Gives `pane` its tab's keyboard.
    fn focus_pane(&mut self, pane: PaneId) {
        let Some(id) = self.panes.get(&pane).map(|pane| pane.tab) else {
            return;
        };
        if let Some(tab) = self.tabs.get_mut(&id)
            && tab.active != pane
        {
            tab.active = pane;
            self.changed(WorkspaceEvent::PaneFocused { pane, tab: id });
        }
    }

    /// Gives `tab`, and its panes, the window `window`.
    fn place_tab(&mut self, tab: TabId, window: WindowId) {
        if let Some(moved) = self.tabs.get_mut(&tab) {
            moved.window = window;
            for pane in leaves(&moved.layout) {
                if let Some(pane) = self.panes.get_mut(&pane) {
                    pane.window = window;
                }
            }
        }
    }

    /// Moves `tab` to `index` of `window`'s tabs, or to their end past
    /// them: the window that it leaves shows the tab after it, or before
    /// it, if it showed this one, and closes if it has none left; the
    /// window that takes it shows the tab that it showed.
    fn move_tab(&mut self, tab: TabId, window: WindowId, index: usize) {
        if !self.tabs.contains_key(&tab) || !self.windows.contains_key(&window) {
            return;
        }
        let from = self.tabs[&tab].window;
        let was = self.windows[&from].tabs.iter().position(|&id| id == tab);
        let left = if from == window {
            if let Some(shown) = self.windows.get_mut(&window) {
                shown.tabs.retain(|&id| id != tab);
            }
            Left::Kept
        } else {
            self.detach_tab(tab)
        };
        self.place_tab(tab, window);
        let mut placed = 0;
        if let Some(shown) = self.windows.get_mut(&window) {
            placed = index.min(shown.tabs.len());
            shown.tabs.insert(placed, tab);
        }
        if from != window || was != Some(placed) {
            self.changed(WorkspaceEvent::TabMoved {
                tab,
                from,
                window,
                index: placed,
            });
        }
        self.tell_left(left);
    }

    /// Opens a window whose one tab, which it shows, is `tab`, and returns
    /// it.
    fn window_for(&mut self, tab: TabId) -> WindowId {
        let window = WindowId(self.next_id());
        self.windows.insert(
            window,
            FakeWindow {
                tabs: vec![tab],
                active: tab,
            },
        );
        self.place_tab(tab, window);
        self.changed(WorkspaceEvent::WindowOpened { window });
        window
    }

    /// Moves `tab` to a window of its own; a tab alone in its window stays.
    fn tab_to_new_window(&mut self, tab: TabId) {
        let Some(from) = self.tabs.get(&tab).map(|tab| tab.window) else {
            return;
        };
        if self
            .windows
            .get(&from)
            .is_none_or(|shown| shown.tabs.len() == 1)
        {
            return;
        }
        let left = self.detach_tab(tab);
        let window = self.window_for(tab);
        self.changed(WorkspaceEvent::TabMoved {
            tab,
            from,
            window,
            index: 0,
        });
        self.tell_left(left);
    }

    /// Makes a tab of `pane` alone, which left its tab, in `window`, if it
    /// is given, at `index` of its tabs, or in a window of its own, which
    /// opens, and owes its opening and the pane's move from `from`.
    fn new_tab_of(&mut self, pane: PaneId, from: TabId, place: Option<(WindowId, usize)>) {
        let tab = TabId(self.next_id());
        self.tabs.insert(
            tab,
            FakeTab {
                window: WindowId(0),
                layout: Layout::Pane(pane),
                active: pane,
                zoomed: None,
                title: None,
                color: None,
            },
        );
        if let Some(moved) = self.panes.get_mut(&pane) {
            moved.tab = tab;
        }
        let (window, index) = match place {
            Some((window, index)) => {
                let shown = self.windows.get_mut(&window).expect("an open window");
                let index = index.min(shown.tabs.len());
                shown.tabs.insert(index, tab);
                self.place_tab(tab, window);
                (window, index)
            }
            None => (self.window_for(tab), 0),
        };
        self.changed(WorkspaceEvent::TabCreated { tab, window, index });
        self.changed(WorkspaceEvent::PaneMoved { pane, from, tab });
    }

    /// Moves `pane` out of its tab into a tab of its own at `index` of
    /// `window`'s tabs, or at their end past them; a pane alone in its tab
    /// moves with it.
    fn pane_to_new_tab(&mut self, pane: PaneId, window: WindowId, index: usize) {
        let Some(tab) = self.panes.get(&pane).map(|pane| pane.tab) else {
            return;
        };
        if !self.windows.contains_key(&window) {
            return;
        }
        if self
            .tabs
            .get(&tab)
            .is_some_and(|tab| leaves(&tab.layout).len() == 1)
        {
            self.move_tab(tab, window, index);
            return;
        }
        self.detach_pane(pane, tab);
        self.new_tab_of(pane, tab, Some((window, index)));
    }

    /// Moves `pane` out of its tab into a window of its own; a pane alone
    /// in its tab moves with it.
    fn pane_to_new_window(&mut self, pane: PaneId) {
        let Some(tab) = self.panes.get(&pane).map(|pane| pane.tab) else {
            return;
        };
        if self
            .tabs
            .get(&tab)
            .is_some_and(|tab| leaves(&tab.layout).len() == 1)
        {
            self.tab_to_new_window(tab);
            return;
        }
        self.detach_pane(pane, tab);
        self.new_tab_of(pane, tab, None);
    }

    /// Has `pane` take `tab`'s whole room, or ends its zoom with `None`.
    fn zoom(&mut self, id: TabId, pane: Option<PaneId>) {
        let Some(tab) = self.tabs.get_mut(&id) else {
            return;
        };
        if pane.is_some_and(|pane| !leaves(&tab.layout).contains(&pane)) {
            return;
        }
        if tab.zoomed != pane {
            tab.zoomed = pane;
            self.changed(WorkspaceEvent::LayoutChanged { tab: id });
        }
    }

    /// Changes `tab`'s layout with `change`, which owes the change if the
    /// layout is another.
    fn relayout(&mut self, id: TabId, change: impl FnOnce(&mut Layout)) {
        let Some(tab) = self.tabs.get_mut(&id) else {
            return;
        };
        let before = tab.layout.clone();
        change(&mut tab.layout);
        if tab.layout != before {
            self.changed(WorkspaceEvent::LayoutChanged { tab: id });
        }
    }

    /// Asks for a render of `view`, once.
    fn ask_render(&mut self, view: ViewId) {
        if !self.renders.contains(&view) {
            self.renders.push(view);
        }
    }

    /// The window of `pane`, as its tab's; `not-found` if the host does not
    /// hold it.
    fn pane_window(&self, pane: PaneId) -> Result<WindowId, Error> {
        self.panes
            .get(&pane)
            .and_then(|held| self.tabs.get(&held.tab))
            .map(|tab| tab.window)
            .ok_or_else(|| Error::new(Code::NotFound, format!("there is no pane {}", pane.0)))
    }

    /// The window that `view` shows in, if the view is one of the
    /// plugin's: of a slot that its manifest fills, of a window or a pane
    /// that is there, or one of its open panels; `not-found` otherwise.
    fn view_window(&self, view: &ViewId) -> Result<WindowId, Error> {
        let not_found = || Error::new(Code::NotFound, format!("the plugin has no view {view:?}"));
        if let (Some(slot), Some(slots)) = (view.slot(), &self.slots)
            && !slots.contains(&slot)
        {
            return Err(not_found());
        }
        let window = match view {
            ViewId::TabBar(window) | ViewId::WindowStrip(window) | ViewId::StatusBar(window) => {
                Some(*window).filter(|window| self.windows.contains_key(window))
            }
            ViewId::PaneHeader(pane) => self.pane_window(*pane).ok(),
            ViewId::Panel(id) => self.panel_windows.get(id).copied(),
        };
        window.ok_or_else(not_found)
    }

    /// The node of `key` in the last tree of `view`, one of the plugin's;
    /// `not-found` without one.
    fn node(&self, view: &ViewId, key: &Key) -> Result<&Node, Error> {
        self.view_window(view)?;
        self.views
            .get(view)
            .and_then(Option::as_ref)
            .and_then(|tree| keyed(tree, key, 0))
            .ok_or_else(|| {
                Error::new(
                    Code::NotFound,
                    format!("the view {view:?} has no node `{}`", key.0),
                )
            })
    }

    /// The window that a panel at `anchor` shows in: its pane's, its
    /// window, or its node's view's.
    fn anchor_window(&self, anchor: &Anchor) -> Result<WindowId, Error> {
        match anchor {
            Anchor::Cell { pane, .. } | Anchor::Cursor(pane) | Anchor::PaneCorner { pane, .. } => {
                self.pane_window(*pane)
            }
            Anchor::Pointer(window) | Anchor::WindowCorner { window, .. } => {
                if self.windows.contains_key(window) {
                    Ok(*window)
                } else {
                    Err(Error::new(
                        Code::NotFound,
                        format!("there is no window {}", window.0),
                    ))
                }
            }
            Anchor::Node { view, key } => {
                self.node(view, key)?;
                self.view_window(view)
            }
        }
    }

    /// The command `name`, the plugin's or another's, if one is registered.
    fn registered(&self, name: &str) -> Option<&Command> {
        self.commands
            .iter()
            .chain(&self.others)
            .find(|command| command.name == name)
    }

    /// Checks a call of the command `name` with `args` that the plugin
    /// makes, as the app's registry does: of a command that is registered,
    /// of the arguments that its parameters take; of a command that the
    /// plugin's manifest says it runs, and of the capabilities that it
    /// needs, if the plugin is of a manifest that declares them, its own
    /// commands needing neither.
    fn check_run(&self, name: &str, args: &Args) -> Result<(), Error> {
        let Some(command) = self.registered(name) else {
            return Err(Error::new(Code::NotFound, format!("no command `{name}`")));
        };
        command.check_call(args.clone())?;
        let (Some(uses), Some(granted)) = (&self.uses, &self.granted) else {
            return Ok(());
        };
        if self.commands.iter().any(|own| own.name == name) {
            return Ok(());
        }
        if !uses.allows(name) {
            return Err(Error::new(
                Code::Undeclared,
                format!("the plugin runs `{name}`, which its manifest does not declare"),
            ));
        }
        if let Some(missing) = command
            .capabilities
            .iter()
            .find(|capability| !granted.contains(capability))
        {
            return Err(Error::new(
                Code::MissingCapability,
                format!("`{name}` needs `{missing}`, which the plugin does not have"),
            ));
        }
        Ok(())
    }

    /// The plugin's namespace, its name's last part.
    fn namespace(&self) -> &str {
        self.caller.rsplit('/').next().unwrap_or_default()
    }

    /// The windows, each of its tabs and the one that it shows.
    fn windows(&self) -> Vec<WindowInfo> {
        self.windows
            .iter()
            .map(|(&id, window)| {
                let (number, keys) = self.numbers.get(&id).cloned().unwrap_or_default();
                WindowInfo {
                    id,
                    tabs: window.tabs.clone(),
                    active: window.active,
                    number,
                    keys,
                }
            })
            .collect()
    }
}

/// The panes of `layout`, in their order, the first leftmost or topmost.
fn leaves(layout: &Layout) -> Vec<PaneId> {
    match layout {
        Layout::Pane(pane) => vec![*pane],
        Layout::Split { first, second, .. } => {
            let mut panes = leaves(first);
            panes.extend(leaves(second));
            panes
        }
    }
}

/// Puts `pane` beside the last pane of `layout`, side by side, after it.
fn append(layout: &mut Layout, pane: PaneId) {
    match layout {
        Layout::Pane(last) => {
            *layout = Layout::Split {
                direction: Direction::Horizontal,
                ratio: None,
                first: Box::new(Layout::Pane(*last)),
                second: Box::new(Layout::Pane(pane)),
            };
        }
        Layout::Split { second, .. } => append(second, pane),
    }
}

/// Splits the part of `pane` along `direction`, with `new` on `side`;
/// whether `pane` is one of `layout`'s and `new` is not.
fn split(layout: &mut Layout, pane: PaneId, direction: Direction, new: PaneId, side: Side) -> bool {
    if leaves(layout).contains(&new) {
        return false;
    }
    match layout {
        Layout::Pane(leaf) if *leaf == pane => {
            let (first, second) = match side {
                Side::First => (new, pane),
                Side::Second => (pane, new),
            };
            *layout = Layout::Split {
                direction,
                ratio: None,
                first: Box::new(Layout::Pane(first)),
                second: Box::new(Layout::Pane(second)),
            };
            true
        }
        Layout::Pane(_) => false,
        Layout::Split { first, second, .. } => {
            split(first, pane, direction, new, side) || split(second, pane, direction, new, side)
        }
    }
}

/// Takes `pane`'s part away from `layout`, its split's room to its sibling,
/// which takes the split's place: the sibling's pane nearest to it; `None`
/// if `pane` is no part of a split of `layout`, as its one pane.
fn remove(layout: &mut Layout, pane: PaneId) -> Option<PaneId> {
    let Layout::Split { first, second, .. } = layout else {
        return None;
    };
    let (sibling, nearest) = match (first.as_ref(), second.as_ref()) {
        (Layout::Pane(leaf), sibling) if *leaf == pane => {
            let sibling = sibling.clone();
            let nearest = leaves(&sibling)[0];
            (sibling, nearest)
        }
        (sibling, Layout::Pane(leaf)) if *leaf == pane => {
            let sibling = sibling.clone();
            let nearest = *leaves(&sibling).last().expect("a part has a pane");
            (sibling, nearest)
        }
        _ => return remove(first, pane).or_else(|| remove(second, pane)),
    };
    *layout = sibling;
    Some(nearest)
}

/// Shares each split of `layout` by its panes again.
fn equalize(layout: &mut Layout) {
    if let Layout::Split {
        ratio,
        first,
        second,
        ..
    } = layout
    {
        *ratio = None;
        equalize(first);
        equalize(second);
    }
}

/// Gives the split at `path` of `layout` the share `to` of its first part,
/// if a split is there.
fn set_ratio(layout: &mut Layout, path: &[Side], to: Option<f64>) {
    let Layout::Split {
        ratio,
        first,
        second,
        ..
    } = layout
    else {
        return;
    };
    match path.split_first() {
        None => *ratio = to,
        Some((Side::First, rest)) => set_ratio(first, rest, to),
        Some((Side::Second, rest)) => set_ratio(second, rest, to),
    }
}

/// The node of `key` in `node`'s tree, `depth` levels down already, if it
/// has one within [`DEEPEST`] levels.
fn keyed<'a>(node: &'a Node, key: &Key, depth: usize) -> Option<&'a Node> {
    if node.key.as_ref() == Some(key) {
        return Some(node);
    }
    if depth >= DEEPEST {
        return None;
    }
    node.children
        .iter()
        .find_map(|child| keyed(child, key, depth + 1))
}

/// The JSON of `data`, as the app keeps a plugin's state; `None` of a
/// number that is not finite, as JSON has none.
fn json(data: &Data) -> Option<serde_json::Value> {
    use serde_json::Value;
    Some(match data {
        Data::Nil => Value::Null,
        Data::Boolean(boolean) => Value::Bool(*boolean),
        Data::Integer(integer) => Value::from(*integer),
        Data::Number(number) => Value::Number(serde_json::Number::from_f64(*number)?),
        Data::Text(text) => Value::String(text.clone()),
        Data::List(items) => Value::Array(items.iter().map(json).collect::<Option<_>>()?),
        Data::Map(map) => Value::Object(
            map.iter()
                .map(|(key, value)| Some((key.clone(), json(value)?)))
                .collect::<Option<_>>()?,
        ),
    })
}

/// The host of one call, which queues its writes, each with the ids that
/// its request gave, and its requests of the app's.
struct Calling<'h> {
    host: &'h mut FakeHost,
    writes: Vec<(Request, Opened)>,
    asked: Vec<Request>,
}

impl Calling<'_> {
    /// Whether the call's writes so far open the pane `pane`.
    fn opens_pane(&self, pane: PaneId) -> bool {
        self.writes.iter().any(|(_, opened)| match opened {
            Opened::Tab(_, opened) | Opened::Pane(opened) => *opened == pane,
            Opened::Nothing => false,
        })
    }

    /// The pane of the tab `tab` that the call's writes so far open, if
    /// they open it.
    fn opened_tab(&self, tab: TabId) -> Option<PaneId> {
        self.writes.iter().find_map(|(_, opened)| match opened {
            Opened::Tab(opened, pane) if *opened == tab => Some(*pane),
            _ => None,
        })
    }

    /// Whether `pane` is held, or the call's writes so far open it;
    /// `not-found` otherwise.
    fn known_pane(&self, pane: PaneId) -> Result<(), Error> {
        if self.opens_pane(pane) {
            return Ok(());
        }
        self.host.held(pane).map(|_| ())
    }

    /// Whether `tab` is held, or the call's writes so far open it;
    /// `not-found` otherwise.
    fn known_tab(&self, tab: TabId) -> Result<(), Error> {
        if self.opened_tab(tab).is_some() || self.host.tabs.contains_key(&tab) {
            return Ok(());
        }
        Err(Error::new(
            Code::NotFound,
            format!("there is no tab {}", tab.0),
        ))
    }

    /// Whether `window` is open; `not-found` otherwise.
    fn known_window(&self, window: WindowId) -> Result<(), Error> {
        if self.host.windows.contains_key(&window) {
            return Ok(());
        }
        Err(Error::new(
            Code::NotFound,
            format!("there is no window {}", window.0),
        ))
    }

    /// Answers `request`, a read, with what the host holds.
    fn read(&mut self, request: &Request) -> Result<Response, Error> {
        let host = &*self.host;
        Ok(match request {
            Request::ClipboardText => Response::Text(host.clipboard.clone()),
            Request::FocusedWindow => Response::Window(host.focused_window()),
            Request::FocusedPane => Response::Pane(host.focused_pane()),
            Request::SelectionText(pane) => Response::Text(host.held(*pane)?.selection.clone()),
            Request::LineText { pane, row, .. } => {
                Response::Line(host.held(*pane)?.lines.get(row).cloned())
            }
            Request::LinkAt { pane, column, row } => {
                Response::Text(host.held(*pane)?.links.get(&(*column, *row)).cloned())
            }
            Request::WorkingDirectory(pane) => Response::Text(host.held(*pane)?.directory.clone()),
            Request::Styles { pane, row } => {
                Response::Styles(host.held(*pane)?.styles.get(row).cloned())
            }
            Request::ReadLines { pane, first, count } => {
                // Every row that it holds from the first, or from its
                // first if that is later, a blank one empty.
                let held = host.held(*pane)?;
                let lines: Vec<host::Line> = ((*first).max(held.held.start)..held.held.end)
                    .take((*count).min(host::MOST_LINES))
                    .map(|row| {
                        let (text, wrapped) = held.rows.get(&row).cloned().unwrap_or_default();
                        host::Line { row, text, wrapped }
                    })
                    .collect();
                let (pane, id) = (*pane, self.host.next_id());
                self.host.owed.push(Event::Lines {
                    id,
                    pane,
                    lines,
                    complete: true,
                });
                Response::Reading(id)
            }
            Request::Opens(target) => {
                Response::Boolean(opening(target).is_ok() && !host.missing.contains(target))
            }
            Request::FontSize => Response::FontSize(host.font_size),
            Request::TabBarPlace => Response::TabBarPlace(host.bar_place),
            Request::Windows => Response::Windows(host.windows()),
            Request::Tabs(window) => {
                let Some(found) = host.windows.get(window) else {
                    return Err(Error::new(
                        Code::NotFound,
                        format!("there is no window {}", window.0),
                    ));
                };
                Response::Tabs(
                    found
                        .tabs
                        .iter()
                        .filter_map(|&id| {
                            let tab = host.tabs.get(&id)?;
                            Some(TabInfo {
                                id,
                                window: *window,
                                title: tab.title.clone().unwrap_or_default(),
                                color: tab.color,
                                active: id == found.active,
                                focused: tab.active,
                                panes: leaves(&tab.layout),
                                zoomed: tab.zoomed,
                            })
                        })
                        .collect(),
                )
            }
            Request::Layout(tab) => match host.tabs.get(tab) {
                Some(tab) => Response::Layout(tab.layout.clone()),
                None => {
                    return Err(Error::new(
                        Code::NotFound,
                        format!("there is no tab {}", tab.0),
                    ));
                }
            },
            Request::Config => Response::Data(host.config.clone()),
            Request::State => Response::Data(host.state.clone()),
            other => {
                return Err(Error::new(Code::Fault, format!("{other:?} is not a read")));
            }
        })
    }

    /// Checks `request`, a write or a request of the app's, and queues it:
    /// one of a pane, a tab or a window that the host does not hold, nor
    /// the call's writes so far open, or of a target that does not open,
    /// fails now.
    fn queue(&mut self, request: Request) -> Result<Response, Error> {
        let mut opened = Opened::Nothing;
        let response = match &request {
            Request::SelectAll(pane)
            | Request::ClearScrollback(pane)
            | Request::Clear(pane)
            | Request::Paste { pane, .. }
            | Request::SendText { pane, .. }
            | Request::Scroll { pane, .. }
            | Request::FocusPane(pane)
            | Request::ClosePane { pane, .. }
            | Request::PaneToNewWindow(pane) => {
                self.known_pane(*pane)?;
                Response::Done
            }
            Request::SetTabTitle { tab, .. } => {
                self.known_tab(*tab)?;
                Response::Done
            }
            Request::SetTabBarPlace(place) => {
                place
                    .check()
                    .map_err(|message| Error::new(Code::Failed, message))?;
                Response::Done
            }
            Request::SetTabColor { tab, color } => {
                self.known_tab(*tab)?;
                if matches!(color, Some(Color::Palette(_))) {
                    return Err(Error::new(
                        Code::Failed,
                        "a tab's color is an RGB one, not one of the palette's",
                    ));
                }
                Response::Done
            }
            Request::AskText { window, .. } => {
                if let Some(window) = window {
                    self.known_window(*window)?;
                }
                Response::Asking(self.host.next_id())
            }
            Request::ResizePane { pane, by, .. } => {
                self.known_pane(*pane)?;
                if !(-1.0..=1.0).contains(by) {
                    return Err(Error::new(
                        Code::Failed,
                        format!("a resize of {by} is not a share from -1 to 1"),
                    ));
                }
                Response::Done
            }
            Request::CloseTab { tab, .. }
            | Request::ActivateTab(tab)
            | Request::TabToNewWindow(tab)
            | Request::Equalize(tab) => {
                self.known_tab(*tab)?;
                Response::Done
            }
            Request::SetRatio { tab, ratio, .. } => {
                self.known_tab(*tab)?;
                if ratio.is_some_and(|ratio| !(0.0..=1.0).contains(&ratio)) {
                    return Err(Error::new(
                        Code::Failed,
                        "a split's ratio is a share from 0 to 1",
                    ));
                }
                Response::Done
            }
            Request::SetFontSize(Some(size)) => {
                // The text's sizes, in points, as the app takes them.
                let (smallest, largest) = (4.0_f32, 200.0_f32);
                let points = f64::from(*size);
                if !(f64::from(smallest)..=f64::from(largest)).contains(&points) {
                    return Err(Error::new(
                        Code::Failed,
                        format!(
                            "the size, in points, is from {smallest} to {largest}, not {points}"
                        ),
                    ));
                }
                Response::Done
            }
            Request::SetState(value) => {
                let failed = |message: String| Error::new(Code::Failed, message);
                let text = json(value)
                    .ok_or_else(|| {
                        failed("a plugin's state holds finite numbers alone".to_owned())
                    })?
                    .to_string();
                if text.len() > MOST_STATE {
                    return Err(failed(format!(
                        "a plugin's state is {MOST_STATE} bytes at most as JSON, not {}",
                        text.len()
                    )));
                }
                Response::Done
            }
            Request::RunCommand { name, args } => {
                self.host.check_run(name, args)?;
                Response::Done
            }
            Request::RequestRender(view) => {
                self.host.view_window(view)?;
                Response::Done
            }
            Request::SetText { view, key, .. } => {
                if !matches!(self.host.node(view, key)?.kind, Kind::Field(_)) {
                    return Err(Error::new(
                        Code::NotFound,
                        format!("`{}` is not a field", key.0),
                    ));
                }
                Response::Done
            }
            Request::Select { view, key, .. } => {
                if !matches!(self.host.node(view, key)?.kind, Kind::List(_)) {
                    return Err(Error::new(
                        Code::NotFound,
                        format!("`{}` is not a list", key.0),
                    ));
                }
                Response::Done
            }
            Request::MoveTab { tab, window, .. } => {
                self.known_tab(*tab)?;
                self.known_window(*window)?;
                Response::Done
            }
            Request::CloseWindow(window) => {
                self.known_window(*window)?;
                Response::Done
            }
            Request::PaneToNewTab { pane, window, .. } => {
                self.known_pane(*pane)?;
                self.known_window(*window)?;
                Response::Done
            }
            Request::Zoom { tab, pane } => {
                let panes = match self.opened_tab(*tab) {
                    Some(opened) => vec![opened],
                    None => match self.host.tabs.get(tab) {
                        Some(found) => leaves(&found.layout),
                        None => {
                            return Err(Error::new(
                                Code::NotFound,
                                format!("there is no tab {}", tab.0),
                            ));
                        }
                    },
                };
                if let Some(pane) = pane
                    && !panes.contains(pane)
                {
                    return Err(Error::new(
                        Code::NotFound,
                        format!("the tab {} has no pane {}", tab.0, pane.0),
                    ));
                }
                Response::Done
            }
            Request::Decorate {
                pane,
                from,
                to,
                look,
            } => {
                let held = &self.host.held(*pane)?.held;
                if !held.contains(&from.1)
                    || !held.contains(&to.1)
                    || (to.1, to.0) < (from.1, from.0)
                {
                    return Err(Error::new(
                        Code::Failed,
                        "a decoration's cells are not the pane's, or not in reading order",
                    ));
                }
                let id = self.host.next_id();
                self.host.decorations.push(FakeDecoration {
                    id,
                    pane: *pane,
                    from: *from,
                    to: *to,
                    look: *look,
                });
                Response::Decoration(id)
            }
            Request::Undecorate { pane, id } => {
                // A decoration of a pane that closed since is the plugin's
                // still, as the app's host keeps it.
                let own = self
                    .host
                    .decorations
                    .iter()
                    .any(|laid| laid.id == *id && laid.pane == *pane);
                if !own {
                    return Err(Error::new(
                        Code::NotFound,
                        format!("the plugin has no decoration {id} of the pane {}", pane.0),
                    ));
                }
                Response::Done
            }
            Request::Open(target) => {
                opening(target).map_err(|message| Error::new(Code::Failed, message))?;
                Response::Done
            }
            Request::NewTab { window, .. } => {
                self.known_window(*window)?;
                let (tab, pane) = (TabId(self.host.next_id()), PaneId(self.host.next_id()));
                opened = Opened::Tab(tab, pane);
                Response::Tab { tab, pane }
            }
            Request::Split { pane, .. } => {
                self.known_pane(*pane)?;
                let new = PaneId(self.host.next_id());
                opened = Opened::Pane(new);
                Response::Pane(Some(new))
            }
            Request::OpenPanel { id, options } => {
                if id.0.is_empty() {
                    return Err(Error::new(Code::InvalidName, "a panel's id is not empty"));
                }
                if self.host.panels.contains_key(id) {
                    return Err(Error::new(
                        Code::AlreadyRegistered,
                        format!("the plugin has a panel `{}` open", id.0),
                    ));
                }
                let window = self.host.anchor_window(&options.anchor)?;
                self.host.panels.insert(id.clone(), options.clone());
                self.host.panel_windows.insert(id.clone(), window);
                self.host.ask_render(ViewId::Panel(id.clone()));
                Response::Done
            }
            Request::ClosePanel(id) => {
                if self.host.panels.remove(id).is_none() {
                    return Err(Error::new(
                        Code::NotFound,
                        format!("the plugin has no panel `{}` open", id.0),
                    ));
                }
                // Forgotten now, so that the same call can open it again,
                // of no tree kept.
                self.host.panel_windows.remove(id);
                let view = ViewId::Panel(id.clone());
                self.host.views.remove(&view);
                self.host.renders.retain(|asked| *asked != view);
                self.host.owed.push(Event::PanelClosed(id.clone()));
                Response::Done
            }
            Request::Emit { name, schema, .. } => {
                let namespace = self.host.namespace();
                let own = name
                    .strip_prefix(namespace)
                    .is_some_and(|rest| rest.starts_with('.'));
                if !is_name(name) || !own {
                    return Err(Error::new(
                        Code::InvalidName,
                        format!("`{name}` is not an event of the plugin's, `{namespace}.event`"),
                    ));
                }
                if schema.is_empty() {
                    return Err(Error::new(
                        Code::InvalidName,
                        format!("the event `{name}` names no schema"),
                    ));
                }
                Response::Done
            }
            _ => Response::Done,
        };
        match request.effect() {
            Effect::App => self.asked.push(request),
            _ => self.writes.push((request, opened)),
        }
        Ok(response)
    }

    /// Checks `request`, a declaration, as the app's host does, and takes
    /// it at once.
    fn declare(&mut self, request: Request) -> Result<Response, Error> {
        match request {
            Request::RegisterCommand(command) => {
                let host = &*self.host;
                if host.uses.is_some() {
                    let namespace = host.namespace();
                    let prefix = format!("{namespace}.");
                    if !command.name.starts_with(&prefix) {
                        return Err(Error::new(
                            Code::InvalidName,
                            format!(
                                "`{}` is not of the plugin's namespace, `{namespace}`",
                                command.name
                            ),
                        ));
                    }
                    if host
                        .others
                        .iter()
                        .any(|other| other.name.starts_with(&prefix))
                    {
                        return Err(Error::new(
                            Code::AlreadyRegistered,
                            format!(
                                "the namespace `{namespace}` has another plugin's commands, or the app's"
                            ),
                        ));
                    }
                }
                if host.registered(&command.name).is_some() {
                    return Err(Error::new(
                        Code::AlreadyRegistered,
                        format!("`{}` is registered already", command.name),
                    ));
                }
                command.check()?;
                self.host.commands.push(command);
            }
            Request::Bind(binding) => {
                let invalid = |message: String| Error::new(Code::InvalidBinding, message);
                parse_keys(&binding.keys).map_err(invalid)?;
                if !is_name(&binding.command) {
                    return Err(invalid(format!(
                        "`{}` is not a command's name, `namespace.action`",
                        binding.command
                    )));
                }
                if let Some(when) = &binding.when {
                    When::parse(when).map_err(invalid)?;
                }
                self.host.bindings.push(binding);
            }
            Request::Subscribe(names) => {
                if let Some(name) = names.iter().find(|name| {
                    !WorkspaceEvent::NAMES.contains(&name.as_str())
                        && name.as_str() != POINTER
                        && !is_name(name)
                }) {
                    return Err(Error::new(
                        Code::InvalidName,
                        format!(
                            "`{name}` is no event: one of the workspace's, as `tab-created`, or a plugin's, `namespace.event`"
                        ),
                    ));
                }
                for name in names {
                    if !self.host.subscribed.contains(&name) {
                        self.host.subscribed.push(name);
                    }
                }
            }
            other => {
                return Err(Error::new(
                    Code::Fault,
                    format!("{other:?} is not a declaration"),
                ));
            }
        }
        Ok(Response::Done)
    }
}

impl host::Host for Calling<'_> {
    fn handle(&mut self, call: Call) -> Response {
        if call.caller != self.host.caller {
            return Response::Failed(Error::new(
                Code::Fault,
                format!(
                    "a call of the plugin `{}` came as `{}`'s",
                    self.host.caller, call.caller
                ),
            ));
        }
        let request = match crate::message::within_depth(call.request) {
            Ok(request) => request,
            Err(error) => return Response::Failed(error),
        };
        if let (Some(needed), Some(granted)) = (request.capability(), &self.host.granted)
            && !granted.contains(&needed)
        {
            return Response::Failed(Error::new(
                Code::MissingCapability,
                format!(
                    "the plugin `{}` lacks the capability `{needed}`",
                    self.host.caller
                ),
            ));
        }
        let answered = match request.effect() {
            Effect::Read => self.read(&request),
            Effect::Write | Effect::App => self.queue(request),
            Effect::Declaration => self.declare(request),
        };
        answered.unwrap_or_else(Response::Failed)
    }
}

/// The schemes of the URLs that open, as the app's host has them.
const SCHEMES: [&str; 4] = ["http", "https", "mailto", "file"];

/// Whether `target` opens, as the app's host takes it: a URL of one of
/// [`SCHEMES`], or a path from `/` or `~`; the error of what does not.
fn opening(target: &str) -> Result<(), String> {
    if target.starts_with('/') || target == "~" || target.starts_with("~/") {
        return Ok(());
    }
    match target.split_once(':') {
        Some((scheme, _)) if SCHEMES.contains(&scheme.to_ascii_lowercase().as_str()) => Ok(()),
        Some((scheme, _)) => Err(format!(
            "`{scheme}:` does not open: only {}",
            SCHEMES.join(", ")
        )),
        None => Err(format!(
            "`{target}` is neither a URL nor a path that is absolute or from `~`"
        )),
    }
}

/// `tree` as text: a line of each node, two spaces deeper for each level,
/// of its kind, its text, and its key, label, role, tooltip and focus when
/// it has them; a list of its count and its row's template, and a view of
/// tabs of its tabs and the one chosen. Its layout and its style are not
/// told, as a snapshot tells the tree's content.
pub fn snapshot(tree: &Node) -> String {
    let mut out = String::new();
    write_node(&mut out, tree, 0);
    out
}

fn write_node(out: &mut String, node: &Node, depth: usize) {
    let _ = write!(out, "{}", "  ".repeat(depth));
    let quoted = |text: &str| format!("{text:?}");
    match &node.kind {
        Kind::Container => out.push_str("container"),
        Kind::Text(text) => {
            let _ = write!(out, "text {}", quoted(text));
        }
        Kind::Icon(name) => {
            let _ = write!(out, "icon {}", quoted(name));
        }
        Kind::Spacer => out.push_str("spacer"),
        Kind::Field(field) => {
            let _ = write!(out, "field {}", quoted(&field.placeholder));
        }
        Kind::Button(button) => {
            let _ = write!(out, "button {}", quoted(&button.label));
            if !button.icon.is_empty() {
                let _ = write!(out, " icon={}", quoted(&button.icon));
            }
            if button.disabled {
                out.push_str(" disabled");
            }
        }
        Kind::Toggle(toggle) => {
            let _ = write!(out, "toggle {}", quoted(&toggle.label));
            if !toggle.icon.is_empty() {
                let _ = write!(out, " icon={}", quoted(&toggle.icon));
            }
            out.push_str(if toggle.on { " on" } else { " off" });
            if toggle.disabled {
                out.push_str(" disabled");
            }
        }
        Kind::Separator => out.push_str("separator"),
        Kind::List(list) => {
            let _ = write!(
                out,
                "list of {}, {} given from {}",
                list.count,
                list.items.len(),
                list.first
            );
        }
        Kind::Scroll => out.push_str("scroll"),
        Kind::Progress(progress) => {
            let _ = write!(out, "progress {:?} {:?}", progress.shape, progress.state);
            if progress.state != crate::view::ProgressState::Indeterminate {
                let _ = write!(out, " {}", progress.value);
            }
        }
        Kind::Tabs(tabs) => {
            let labels: Vec<String> = tabs.tabs.iter().map(|tab| quoted(&tab.label)).collect();
            let _ = write!(out, "tabs {} chosen {}", labels.join(" | "), tabs.chosen);
        }
    }
    if let Some(key) = &node.key {
        let _ = write!(out, " key={}", key.0);
    }
    if let Some(label) = &node.label {
        let _ = write!(out, " label={}", quoted(label));
    }
    if let Some(role) = &node.role {
        let _ = write!(out, " role={role:?}");
    }
    if let Some(tooltip) = &node.tooltip {
        let _ = write!(out, " tooltip={}", quoted(tooltip));
    }
    if node.focusable {
        out.push_str(" focusable");
    }
    out.push('\n');
    if let Kind::List(list) = &node.kind {
        let _ = writeln!(out, "{}row:", "  ".repeat(depth + 1));
        write_node(out, &list.row, depth + 2);
    }
    for child in &node.children {
        write_node(out, child, depth + 1);
    }
}

#[cfg(test)]
mod tests;
