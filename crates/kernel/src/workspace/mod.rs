//! The workspace: the windows, their tabs, each tab's layout of panes, and
//! the panes with their sessions.
//!
//! ```text
//! Workspace
//! └── Window
//!     └── Tab (title, color, active pane)
//!         └── Layout: a leaf, its pane
//!             └── Pane ──▶ Session
//! ```
//!
//! The ids of windows, tabs and panes are never reused while the process
//! runs. A window has one tab, whose layout is a single pane, for now: tabs
//! and splits extend the model. The sessions' events come in on the main
//! thread, as [`Workspace::collect`] gathers them, and wait as
//! [`Event`]s until the embedder takes them.

mod pane;
#[cfg(test)]
mod tests;

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};

use lokyr_vt_core::page::Rgb;
use lokyr_vt_core::terminal::{self, ClipboardTarget};

pub use pane::{Environment, Exited, Grid, Pane, RESIZE_PACE, Settings};

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
            /// A new id, which nothing of the process had.
            fn new() -> Self {
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
    pub(crate) fn of(id: u64) -> Self {
        Self(id)
    }
}

impl PaneId {
    /// The id of the number `id`, as a plugin's handle gives it, which may
    /// be no pane's.
    pub(crate) fn of(id: u64) -> Self {
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

/// A tab: its title, its color, the layout of its panes, and the pane that
/// has the keyboard.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tab {
    id: TabId,
    window: WindowId,
    title: String,
    color: Option<Rgb>,
    layout: Layout,
    active: PaneId,
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
}

/// How a tab lays its panes out: a tree, whose leaves are panes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Layout {
    /// A pane, which takes the whole of its part.
    Leaf(PaneId),
}

impl Layout {
    /// The panes of the layout, in its order.
    pub fn panes(&self) -> Vec<PaneId> {
        match self {
            Self::Leaf(pane) => vec![*pane],
        }
    }
}

/// What happened in the workspace, for the embedder to act on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    /// A window opened.
    WindowOpened(WindowId),
    /// A window closed, with its tabs and panes.
    WindowClosed(WindowId),
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
}

impl Workspace {
    /// An empty workspace.
    pub fn new() -> Self {
        Self::default()
    }

    /// Opens a window with one tab, whose layout is `pane`, and returns its
    /// id.
    pub fn open_window(&mut self, pane: Pane) -> WindowId {
        let (window, tab, id) = (WindowId::new(), TabId::new(), pane.id());
        self.panes.insert(id, pane);
        self.tab_of.insert(id, tab);
        self.tabs.insert(
            tab,
            Tab {
                id: tab,
                window,
                title: String::new(),
                color: None,
                layout: Layout::Leaf(id),
                active: id,
            },
        );
        self.windows.insert(
            window,
            Window {
                id: window,
                tabs: vec![tab],
                active: tab,
            },
        );
        self.events.push(Event::WindowOpened(window));
        window
    }

    /// Closes `window`, with its tabs and its panes, whose programs are hung
    /// up; returns whether it was open.
    pub fn close_window(&mut self, window: WindowId) -> bool {
        let Some(closed) = self.windows.remove(&window) else {
            return false;
        };
        for tab in closed.tabs {
            let Some(tab) = self.tabs.remove(&tab) else {
                continue;
            };
            for pane in tab.layout.panes() {
                self.tab_of.remove(&pane);
                self.panes.remove(&pane);
            }
        }
        self.events.push(Event::WindowClosed(window));
        true
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

    /// Gathers what the panes' sessions tell, on the main thread, once a
    /// session woke it: the titles, which the tabs of their active panes
    /// take, the bells, the writes to the clipboards, the ends of the
    /// output, and the terminals' other events, as [`Event`]s.
    pub fn collect(&mut self) {
        for (&id, pane) in &mut self.panes {
            let Some(happened) = pane.happened() else {
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
    }

    /// Takes the events that wait, oldest first.
    pub fn take_events(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.events)
    }
}
