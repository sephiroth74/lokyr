//! The workspace: the windows, their tabs, each tab's layout of panes, and
//! the panes with their sessions.
//!
//! ```text
//! Workspace
//! └── Window (its tabs, in their order, and the one that it shows)
//!     └── Tab (title, color, active pane, zoomed pane)
//!         └── Layout: Split { direction, ratio, first, second } | Leaf
//!             └── Pane ──▶ Session
//! ```
//!
//! The ids of windows, tabs and panes are never reused while the process
//! runs. The workspace's operations are mechanisms, which plugins call:
//! opening, closing, showing and moving tabs; splitting, closing, focusing,
//! resizing and zooming panes, and moving them to a tab or a window of
//! their own. A tab that loses its last pane closes, and a window its last
//! tab. The sessions' events come in on the main thread, as
//! [`Workspace::collect`] gathers them, and wait as [`Event`]s until the
//! embedder takes them.

mod layout;
mod pane;
#[cfg(test)]
mod tests;

use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use lokyr_plugin_sdk::event::WorkspaceEvent;
use lokyr_plugin_sdk::host as sdk;
use lokyr_vt_core::page::Rgb;
use lokyr_vt_core::terminal::{self, ClipboardTarget};

pub use layout::{Direction, Edge, Layout, Ratio, Side};
pub use pane::{ANSWER_WAIT, Environment, Exited, Grid, Pane, RESIZE_PACE, Settings, TOLD_PACE};

/// The last id that the process gave, to a window, a tab or a pane.
static LAST_ID: AtomicU64 = AtomicU64::new(0);

/// An id that no other window, tab or pane of the process has had.
fn next_id() -> u64 {
    LAST_ID.fetch_add(1, Ordering::Relaxed) + 1
}

macro_rules! id {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(u64);

        impl $name {
            /// A new id, which nothing of the process had: a new pane's, or
            /// the one that a plugin's tab or pane takes as its call
            /// returns, which the host gives it as it asks.
            #[allow(clippy::new_without_default)]
            pub fn new() -> Self {
                Self(next_id())
            }

            /// The id's number.
            pub fn get(self) -> u64 {
                self.0
            }
        }
    };
}

id!(
    /// A window of the workspace.
    WindowId
);
id!(
    /// A tab of a window.
    TabId
);
id!(
    /// A pane of a tab.
    PaneId
);

impl WindowId {
    /// The id of the number `id`, as a plugin's handle gives it, which may
    /// be no window's.
    pub fn of(id: u64) -> Self {
        Self(id)
    }
}

impl TabId {
    /// The id of the number `id`, as a plugin's handle gives it, which may
    /// be no tab's.
    pub fn of(id: u64) -> Self {
        Self(id)
    }
}

impl From<WindowId> for sdk::WindowId {
    fn from(id: WindowId) -> Self {
        Self(id.0)
    }
}

impl From<TabId> for sdk::TabId {
    fn from(id: TabId) -> Self {
        Self(id.0)
    }
}

impl From<PaneId> for sdk::PaneId {
    fn from(id: PaneId) -> Self {
        Self(id.0)
    }
}

impl PaneId {
    /// The id of the number `id`, as a plugin's handle gives it, which may
    /// be no pane's.
    pub fn of(id: u64) -> Self {
        Self(id)
    }
}

/// A window: its tabs, in their order, and the one that it shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Window {
    id: WindowId,
    tabs: Vec<TabId>,
    active: TabId,
}

impl Window {
    /// The window's id.
    pub fn id(&self) -> WindowId {
        self.id
    }

    /// Its tabs, in their order.
    pub fn tabs(&self) -> &[TabId] {
        &self.tabs
    }

    /// The tab that it shows.
    pub fn active_tab(&self) -> TabId {
        self.active
    }
}

/// A tab: its title, its color, the layout of its panes, the pane that has
/// the keyboard, and the one that takes the whole room while it is zoomed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tab {
    id: TabId,
    window: WindowId,
    title: String,
    color: Option<Rgb>,
    layout: Layout,
    active: PaneId,
    zoomed: Option<PaneId>,
}

impl Tab {
    /// The tab's id.
    pub fn id(&self) -> TabId {
        self.id
    }

    /// The window that it is in.
    pub fn window(&self) -> WindowId {
        self.window
    }

    /// Its title, its active pane's program's, as the program sets it.
    pub fn title(&self) -> &str {
        &self.title
    }

    /// Its color, as its active pane's program sets it, if it set one.
    pub fn color(&self) -> Option<Rgb> {
        self.color
    }

    /// The layout of its panes.
    pub fn layout(&self) -> &Layout {
        &self.layout
    }

    /// The pane that has the keyboard.
    pub fn active_pane(&self) -> PaneId {
        self.active
    }

    /// The pane that takes the tab's whole room, while one is zoomed.
    pub fn zoomed(&self) -> Option<PaneId> {
        self.zoomed
    }
}

/// What a tab's leaving its window did to the window.
#[derive(Clone, Copy, Debug)]
enum Left {
    /// The window shows the tab that it showed.
    Kept,
    /// The window shows this tab, as it showed the one that left.
    Shows(WindowId, TabId),
    /// The window closed, as it has no tab left.
    Closed(WindowId),
}

/// What happened in the workspace, for the embedder to act on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    /// A change of the workspace's windows, tabs, layouts or panes, as the
    /// operation that made it records it.
    Changed(WorkspaceEvent),
    /// The title of a pane's program changed, which its tab takes when the
    /// pane is its active one.
    TitleChanged(PaneId),
    /// A pane's bell rang.
    Bell(PaneId),
    /// A pane's program wrote to the clipboards.
    ClipboardWrite {
        /// The pane.
        pane: PaneId,
        /// The clipboards.
        target: ClipboardTarget,
        /// What it wrote.
        data: Vec<u8>,
    },
    /// A pane's program's output ended: the program has, with the
    /// processes that it left.
    Ended(PaneId),
    /// Another event of a pane's terminal, as the terminal tells it.
    Terminal {
        /// The pane.
        pane: PaneId,
        /// The event.
        event: terminal::Event,
    },
}

/// The windows, their tabs and panes, and the events that wait.
#[derive(Debug, Default)]
pub struct Workspace {
    windows: BTreeMap<WindowId, Window>,
    tabs: BTreeMap<TabId, Tab>,
    panes: BTreeMap<PaneId, Pane>,
    /// The tab of each pane.
    tab_of: BTreeMap<PaneId, TabId>,
    events: Vec<Event>,
    /// The panes whose sessions told something within the pace of their
    /// last look, which wait for the next.
    waiting: BTreeSet<PaneId>,
}

impl Workspace {
    /// An empty workspace.
    pub fn new() -> Self {
        Self::default()
    }

    /// Opens a window with one tab, whose layout is `pane`, and returns its
    /// id.
    pub fn open_window(&mut self, pane: Pane) -> WindowId {
        let id = pane.id();
        self.panes.insert(id, pane);
        let tab = self.new_tab(id);
        let window = self.window_for(tab);
        self.created(tab, &[id]);
        window
    }

    /// Closes `window`, with its tabs and its panes, whose programs are hung
    /// up; returns whether it was open.
    pub fn close_window(&mut self, window: WindowId) -> bool {
        let Some(closed) = self.windows.remove(&window) else {
            return false;
        };
        for tab in closed.tabs {
            self.drop_tab(tab);
        }
        self.changed(WorkspaceEvent::WindowClosed {
            window: window.into(),
        });
        true
    }

    /// Takes `tab` and its panes away, which the window that held it no
    /// longer lists, the panes' closings and the tab's told.
    fn drop_tab(&mut self, tab: TabId) {
        let Some(dropped) = self.tabs.remove(&tab) else {
            return;
        };
        for pane in dropped.layout.panes() {
            self.tab_of.remove(&pane);
            self.panes.remove(&pane);
            self.waiting.remove(&pane);
            self.changed(WorkspaceEvent::PaneClosed {
                pane: pane.into(),
                tab: tab.into(),
            });
        }
        self.changed(WorkspaceEvent::TabClosed {
            tab: tab.into(),
            window: dropped.window.into(),
        });
    }

    /// Records `event`, for the embedder.
    fn changed(&mut self, event: WorkspaceEvent) {
        self.events.push(Event::Changed(event));
    }

    /// Records that `tab`, at its place in its window, and its `panes`
    /// opened.
    fn created(&mut self, tab: TabId, panes: &[PaneId]) {
        let Some(window) = self.tabs.get(&tab).map(|tab| tab.window) else {
            return;
        };
        let index = self
            .windows
            .get(&window)
            .and_then(|shown| shown.tabs.iter().position(|&id| id == tab))
            .unwrap_or(0);
        self.changed(WorkspaceEvent::TabCreated {
            tab: tab.into(),
            window: window.into(),
            index,
        });
        for &pane in panes {
            self.changed(WorkspaceEvent::PaneCreated {
                pane: pane.into(),
                tab: tab.into(),
            });
        }
    }

    /// The window `id`, if it is open.
    pub fn window(&self, id: WindowId) -> Option<&Window> {
        self.windows.get(&id)
    }

    /// The windows, in the order they opened.
    pub fn windows(&self) -> impl Iterator<Item = &Window> {
        self.windows.values()
    }

    /// The tab `id`, if it is open.
    pub fn tab(&self, id: TabId) -> Option<&Tab> {
        self.tabs.get(&id)
    }

    /// The pane `id`, if it is open.
    pub fn pane(&self, id: PaneId) -> Option<&Pane> {
        self.panes.get(&id)
    }

    /// The pane `id`, to change, if it is open.
    pub fn pane_mut(&mut self, id: PaneId) -> Option<&mut Pane> {
        self.panes.get_mut(&id)
    }

    /// The panes, in the order they opened.
    pub fn panes(&self) -> impl Iterator<Item = &Pane> {
        self.panes.values()
    }

    /// The panes, to change, in the order they opened.
    pub fn panes_mut(&mut self) -> impl Iterator<Item = &mut Pane> {
        self.panes.values_mut()
    }

    /// The pane that has the keyboard in `window`: its active tab's.
    pub fn active_pane(&self, window: WindowId) -> Option<PaneId> {
        let window = self.windows.get(&window)?;
        Some(self.tabs.get(&window.active)?.active)
    }

    /// The pane that has the keyboard in `window`, as
    /// [`Workspace::active_pane`] names it.
    pub fn focused(&self, window: WindowId) -> Option<&Pane> {
        self.panes.get(&self.active_pane(window)?)
    }

    /// The pane that has the keyboard in `window`, to change.
    pub fn focused_mut(&mut self, window: WindowId) -> Option<&mut Pane> {
        let pane = self.active_pane(window)?;
        self.panes.get_mut(&pane)
    }

    /// The panes of every tab of `window`, in the tabs' order and their
    /// layouts'.
    pub fn panes_of(&self, window: WindowId) -> Vec<PaneId> {
        self.windows.get(&window).map_or_else(Vec::new, |window| {
            window
                .tabs
                .iter()
                .filter_map(|tab| self.tabs.get(tab))
                .flat_map(|tab| tab.layout.panes())
                .collect()
        })
    }

    /// Gathers what the sessions of `panes` tell, on the main thread, once
    /// they woke it, at `now`, and of the panes that waited for their
    /// turn: the titles, which the tabs of their active panes take, the
    /// bells, the writes to the clipboards, the ends of the output, and the
    /// terminals' other events, as [`Event`]s. A pane looked at within
    /// [`TOLD_PACE`] waits for its turn, as its session wakes the main
    /// thread for nothing it tells meanwhile; this returns when the first
    /// that waits has it, for the app to collect again.
    pub fn collect(&mut self, panes: &[PaneId], now: Instant) -> Option<Instant> {
        let mut asked: BTreeSet<PaneId> = std::mem::take(&mut self.waiting);
        asked.extend(panes);
        for id in asked {
            let Some(pane) = self.panes.get_mut(&id) else {
                continue;
            };
            if pane.told_due().is_some_and(|due| due > now) {
                self.waiting.insert(id);
                continue;
            }
            let Some(happened) = pane.happened(now) else {
                continue;
            };
            // The tab that takes the pane's title and color, as its active
            // pane.
            let mut tab = self
                .tab_of
                .get(&id)
                .and_then(|tab| self.tabs.get_mut(tab))
                .filter(|tab| tab.active == id);
            for event in happened.events {
                let event = match event {
                    terminal::Event::TitleChanged => {
                        if let (Some(tab), Some(title)) = (tab.as_deref_mut(), &happened.title) {
                            tab.title.clone_from(title);
                        }
                        Event::TitleChanged(id)
                    }
                    terminal::Event::ClipboardWrite { target, data } => Event::ClipboardWrite {
                        pane: id,
                        target,
                        data,
                    },
                    terminal::Event::TabColor(color) => {
                        if let Some(tab) = tab.as_deref_mut() {
                            tab.color = color;
                        }
                        Event::Terminal {
                            pane: id,
                            event: terminal::Event::TabColor(color),
                        }
                    }
                    event => Event::Terminal { pane: id, event },
                };
                self.events.push(event);
            }
            if happened.bell {
                self.events.push(Event::Bell(id));
            }
            if happened.ended {
                self.events.push(Event::Ended(id));
            }
        }
        self.waiting
            .iter()
            .filter_map(|id| self.panes.get(id)?.told_due())
            .min()
    }

    /// Every pane's id, for a look at them all.
    pub fn pane_ids(&self) -> Vec<PaneId> {
        self.panes.keys().copied().collect()
    }

    /// Takes the events that wait, oldest first.
    pub fn take_events(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.events)
    }

    /// The tab that `pane` is in.
    pub fn tab_of(&self, pane: PaneId) -> Option<TabId> {
        self.tab_of.get(&pane).copied()
    }

    /// Opens a tab in `window`, at `index` of its tabs, or at their end
    /// past them, whose layout is `pane`, and returns its id; the window
    /// shows the tab that it showed.
    pub fn open_tab(&mut self, window: WindowId, pane: Pane, index: usize) -> Option<TabId> {
        self.open_tab_as(window, pane, index, TabId::new())
    }

    /// Opens a tab of the id `tab`, which nothing of the process took, as
    /// [`Workspace::open_tab`] opens one.
    pub(crate) fn open_tab_as(
        &mut self,
        window: WindowId,
        pane: Pane,
        index: usize,
        tab: TabId,
    ) -> Option<TabId> {
        if !self.windows.contains_key(&window) {
            return None;
        }
        let id = pane.id();
        self.panes.insert(id, pane);
        let tab = self.insert_tab_as(window, id, index, tab);
        self.created(tab, &[id]);
        Some(tab)
    }

    /// Makes a tab of `pane`, which the workspace holds, in `window`, which
    /// exists, at `index` of its tabs, and returns its id.
    fn insert_tab(&mut self, window: WindowId, pane: PaneId, index: usize) -> TabId {
        self.insert_tab_as(window, pane, index, TabId::new())
    }

    /// Makes a tab of the id `tab` as [`Workspace::insert_tab`] makes one.
    fn insert_tab_as(&mut self, window: WindowId, pane: PaneId, index: usize, tab: TabId) -> TabId {
        let tab = self.new_tab_as(pane, tab);
        if let Some(moved) = self.tabs.get_mut(&tab) {
            moved.window = window;
        }
        if let Some(window) = self.windows.get_mut(&window) {
            let index = index.min(window.tabs.len());
            window.tabs.insert(index, tab);
        }
        tab
    }

    /// Makes a tab whose layout is `pane`, which the workspace holds, and
    /// returns its id; its window is to be set, as it goes into one.
    fn new_tab(&mut self, pane: PaneId) -> TabId {
        self.new_tab_as(pane, TabId::new())
    }

    /// Makes a tab of the id `tab` as [`Workspace::new_tab`] makes one.
    fn new_tab_as(&mut self, pane: PaneId, tab: TabId) -> TabId {
        self.tab_of.insert(pane, tab);
        self.tabs.insert(
            tab,
            Tab {
                id: tab,
                window: WindowId::of(0),
                title: String::new(),
                color: None,
                layout: Layout::Leaf(pane),
                active: pane,
                zoomed: None,
            },
        );
        tab
    }

    /// Opens a window whose one tab, which it shows, is `tab`, and returns
    /// its id.
    fn window_for(&mut self, tab: TabId) -> WindowId {
        let window = WindowId::new();
        if let Some(moved) = self.tabs.get_mut(&tab) {
            moved.window = window;
        }
        self.windows.insert(
            window,
            Window {
                id: window,
                tabs: vec![tab],
                active: tab,
            },
        );
        self.changed(WorkspaceEvent::WindowOpened {
            window: window.into(),
        });
        window
    }

    /// Closes `tab`, with its panes, whose programs are hung up; its window
    /// shows the tab after it, or before it, if it showed that one, and
    /// closes if it has no tab left. Whether the tab was open.
    pub fn close_tab(&mut self, tab: TabId) -> bool {
        if !self.tabs.contains_key(&tab) {
            return false;
        }
        let left = self.detach_tab(tab);
        self.drop_tab(tab);
        self.tell_left(left);
        true
    }

    /// Takes `tab` out of its window's tabs, which shows the tab after it,
    /// or before it, if it showed that one, and which closes if it has none
    /// left; what it did, for the caller to tell after the tab's own
    /// change. The tab itself stays.
    fn detach_tab(&mut self, tab: TabId) -> Left {
        let Some(window_id) = self.tabs.get(&tab).map(|tab| tab.window) else {
            return Left::Kept;
        };
        let Some(window) = self.windows.get_mut(&window_id) else {
            return Left::Kept;
        };
        let Some(index) = window.tabs.iter().position(|&id| id == tab) else {
            return Left::Kept;
        };
        window.tabs.remove(index);
        if window.tabs.is_empty() {
            self.windows.remove(&window_id);
            return Left::Closed(window_id);
        }
        if window.active != tab {
            return Left::Kept;
        }
        let shown = window.tabs[index.min(window.tabs.len() - 1)];
        window.active = shown;
        Left::Shows(window_id, shown)
    }

    /// Tells what a tab's leaving did to its window.
    fn tell_left(&mut self, left: Left) {
        match left {
            Left::Kept => {}
            Left::Shows(window, tab) => self.changed(WorkspaceEvent::TabActivated {
                tab: tab.into(),
                window: window.into(),
            }),
            Left::Closed(window) => self.changed(WorkspaceEvent::WindowClosed {
                window: window.into(),
            }),
        }
    }

    /// Shows `tab` in its window; whether it is open.
    pub fn activate_tab(&mut self, tab: TabId) -> bool {
        let Some(window) = self.tabs.get(&tab).map(|tab| tab.window) else {
            return false;
        };
        let Some(shown) = self.windows.get_mut(&window) else {
            return false;
        };
        if shown.active != tab {
            shown.active = tab;
            self.changed(WorkspaceEvent::TabActivated {
                tab: tab.into(),
                window: window.into(),
            });
        }
        true
    }

    /// Moves `tab` to `window`, its own or another, at `index` of that
    /// window's tabs, or at their end past them; the window that it leaves
    /// shows the tab after it, or before it, if it showed this one, and
    /// closes if it has none left; the window that takes it shows the tab
    /// that it showed. Whether both are open.
    pub fn move_tab(&mut self, tab: TabId, window: WindowId, index: usize) -> bool {
        if !self.tabs.contains_key(&tab) || !self.windows.contains_key(&window) {
            return false;
        }
        let from = self.tabs[&tab].window;
        let was = self.windows[&from].tabs.iter().position(|&id| id == tab);
        let left = if from == window {
            if let Some(window) = self.windows.get_mut(&window) {
                window.tabs.retain(|&id| id != tab);
            }
            Left::Kept
        } else {
            self.detach_tab(tab)
        };
        if let Some(moved) = self.tabs.get_mut(&tab) {
            moved.window = window;
        }
        let mut placed = 0;
        if let Some(window) = self.windows.get_mut(&window) {
            placed = index.min(window.tabs.len());
            window.tabs.insert(placed, tab);
        }
        if from != window || was != Some(placed) {
            self.changed(WorkspaceEvent::TabMoved {
                tab: tab.into(),
                from: from.into(),
                window: window.into(),
                index: placed,
            });
        }
        self.tell_left(left);
        true
    }

    /// Moves `tab` to a window of its own, which shows it, and returns that
    /// window; a tab alone in its window stays there.
    pub fn tab_to_new_window(&mut self, tab: TabId) -> Option<WindowId> {
        let from = self.tabs.get(&tab)?.window;
        if self.windows.get(&from)?.tabs.len() == 1 {
            return Some(from);
        }
        let left = self.detach_tab(tab);
        let window = self.window_for(tab);
        self.changed(WorkspaceEvent::TabMoved {
            tab: tab.into(),
            from: from.into(),
            window: window.into(),
            index: 0,
        });
        self.tell_left(left);
        Some(window)
    }

    /// Splits the part of `pane` along `direction`, with `new` on `side`,
    /// in `pane`'s tab, whose zoom it ends, and returns the new pane's id;
    /// the tab's keyboard stays where it was.
    pub fn split(
        &mut self,
        pane: PaneId,
        direction: Direction,
        new: Pane,
        side: Side,
    ) -> Option<PaneId> {
        let tab_id = *self.tab_of.get(&pane)?;
        let tab = self.tabs.get_mut(&tab_id)?;
        let id = new.id();
        if !tab.layout.split(pane, direction, id, side) {
            return None;
        }
        tab.zoomed = None;
        self.panes.insert(id, new);
        self.tab_of.insert(id, tab_id);
        self.changed(WorkspaceEvent::PaneCreated {
            pane: id.into(),
            tab: tab_id.into(),
        });
        self.changed(WorkspaceEvent::LayoutChanged { tab: tab_id.into() });
        Some(id)
    }

    /// Takes `pane` out of its tab's layout, its room to its sibling, and
    /// the tab's keyboard and zoom away from it, and tells the change of the
    /// layout; a tab that it leaves without panes leaves its window, which
    /// closes if it has no tab left. The pane itself stays. Returns the tab
    /// that it left, if it was in one, and, if that tab is left empty, what
    /// its leaving did to its window, which the caller tells after the
    /// tab's end.
    fn detach_pane(&mut self, pane: PaneId) -> Option<(TabId, Option<Left>)> {
        let tab_id = self.tab_of.remove(&pane)?;
        let tab = self.tabs.get_mut(&tab_id)?;
        let Some(nearest) = tab.layout.remove(pane) else {
            // Its one pane.
            return Some((tab_id, Some(self.detach_tab(tab_id))));
        };
        let refocused = tab.active == pane;
        if refocused {
            tab.active = nearest;
        }
        if tab.zoomed == Some(pane) {
            tab.zoomed = None;
        }
        self.changed(WorkspaceEvent::LayoutChanged { tab: tab_id.into() });
        if refocused {
            self.changed(WorkspaceEvent::PaneFocused {
                pane: nearest.into(),
                tab: tab_id.into(),
            });
        }
        Some((tab_id, None))
    }

    /// Closes `pane`, whose program is hung up: its room goes to its
    /// sibling, and its tab's keyboard, if it had it, to the sibling's pane
    /// nearest to it; a tab that it leaves without panes closes, and a
    /// window without tabs. Whether it was open.
    pub fn close_pane(&mut self, pane: PaneId) -> bool {
        let Some(&tab) = self.tab_of.get(&pane) else {
            return false;
        };
        self.panes.remove(&pane);
        self.waiting.remove(&pane);
        self.changed(WorkspaceEvent::PaneClosed {
            pane: pane.into(),
            tab: tab.into(),
        });
        if let Some((tab, Some(left))) = self.detach_pane(pane) {
            if let Some(gone) = self.tabs.remove(&tab) {
                self.changed(WorkspaceEvent::TabClosed {
                    tab: tab.into(),
                    window: gone.window.into(),
                });
            }
            self.tell_left(left);
        }
        true
    }

    /// Gives `pane` its tab's keyboard; whether it is open.
    pub fn focus_pane(&mut self, pane: PaneId) -> bool {
        let Some(&tab_id) = self.tab_of.get(&pane) else {
            return false;
        };
        let Some(tab) = self.tabs.get_mut(&tab_id) else {
            return false;
        };
        if tab.active != pane {
            tab.active = pane;
            self.changed(WorkspaceEvent::PaneFocused {
                pane: pane.into(),
                tab: tab_id.into(),
            });
        }
        true
    }

    /// Sets the ratio of the split at `path` of `tab`'s layout, or, with
    /// `None`, shares its room by its parts' weights; whether a split is
    /// there.
    pub fn set_ratio(&mut self, tab: TabId, path: &[Side], ratio: Option<Ratio>) -> bool {
        self.relayout(tab, |layout| layout.set_ratio(path, ratio))
    }

    /// Moves the `edge` of `pane` by `by`, a share of the room of the split
    /// whose divider it is: right and down for a positive one; whether
    /// `pane` has such an edge.
    pub fn resize_pane(&mut self, pane: PaneId, edge: Edge, by: f64) -> bool {
        let Some(&tab) = self.tab_of.get(&pane) else {
            return false;
        };
        self.relayout(tab, |layout| layout.resize(pane, edge, by))
    }

    /// Shares the room of every split of `tab` by its parts' weights;
    /// whether the tab is open.
    pub fn equalize(&mut self, tab: TabId) -> bool {
        self.relayout(tab, |layout| {
            layout.equalize();
            true
        })
    }

    /// Changes `tab`'s layout with `change`, and tells it if the layout is
    /// another; what `change` returns, or false without the tab.
    fn relayout(&mut self, tab: TabId, change: impl FnOnce(&mut Layout) -> bool) -> bool {
        let Some(shown) = self.tabs.get_mut(&tab) else {
            return false;
        };
        let before = shown.layout.clone();
        let done = change(&mut shown.layout);
        if shown.layout != before {
            self.changed(WorkspaceEvent::LayoutChanged { tab: tab.into() });
        }
        done
    }

    /// Zooms `pane`, which takes its tab's whole room, or, with `None`,
    /// ends `tab`'s zoom; whether `pane` is one of `tab`'s.
    pub fn zoom(&mut self, tab: TabId, pane: Option<PaneId>) -> bool {
        let Some(shown) = self.tabs.get_mut(&tab) else {
            return false;
        };
        if pane.is_some_and(|pane| !shown.layout.contains(pane)) {
            return false;
        }
        if shown.zoomed != pane {
            shown.zoomed = pane;
            self.changed(WorkspaceEvent::LayoutChanged { tab: tab.into() });
        }
        true
    }

    /// Moves `pane` out of its tab into a tab of its own in `window`, at
    /// `index` of its tabs, or at their end past them, and returns that
    /// tab; a pane alone in its tab moves with it.
    pub fn pane_to_new_tab(
        &mut self,
        pane: PaneId,
        window: WindowId,
        index: usize,
    ) -> Option<TabId> {
        let tab = *self.tab_of.get(&pane)?;
        if !self.windows.contains_key(&window) {
            return None;
        }
        if self.tabs.get(&tab)?.layout.panes().len() == 1 {
            return self.move_tab(tab, window, index).then_some(tab);
        }
        self.detach_pane(pane);
        let created = self.insert_tab(window, pane, index);
        self.created(created, &[]);
        self.changed(WorkspaceEvent::PaneMoved {
            pane: pane.into(),
            from: tab.into(),
            tab: created.into(),
        });
        Some(created)
    }

    /// Moves `pane` out of its tab into a window of its own, and returns
    /// that window; a pane alone in its tab moves with it, as
    /// [`Workspace::tab_to_new_window`] moves a tab.
    pub fn pane_to_new_window(&mut self, pane: PaneId) -> Option<WindowId> {
        let tab = *self.tab_of.get(&pane)?;
        if self.tabs.get(&tab)?.layout.panes().len() == 1 {
            return self.tab_to_new_window(tab);
        }
        self.detach_pane(pane);
        let created = self.new_tab(pane);
        let window = self.window_for(created);
        self.created(created, &[]);
        self.changed(WorkspaceEvent::PaneMoved {
            pane: pane.into(),
            from: tab.into(),
            tab: created.into(),
        });
        Some(window)
    }
}
