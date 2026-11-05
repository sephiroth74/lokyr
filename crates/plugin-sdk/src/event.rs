//! The events that the host tells: what changed in the workspace, its
//! windows, their tabs, the tabs' layouts and their panes, as the
//! workspace's own operations record each change.

use serde::{Deserialize, Serialize};

use crate::host::{PaneId, TabId, WindowId};

/// A change of the workspace.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WorkspaceEvent {
    /// A window opened.
    WindowOpened {
        /// The window.
        window: WindowId,
    },
    /// A window closed, its tabs and panes before it.
    WindowClosed {
        /// The window.
        window: WindowId,
    },
    /// A tab opened in `window`, at `index` of its tabs.
    TabCreated {
        /// The tab.
        tab: TabId,
        /// Its window.
        window: WindowId,
        /// Its place among the window's tabs, from the first.
        index: usize,
    },
    /// A tab closed, its panes before it.
    TabClosed {
        /// The tab.
        tab: TabId,
        /// The window that it was in.
        window: WindowId,
    },
    /// `window` shows `tab`.
    TabActivated {
        /// The tab.
        tab: TabId,
        /// Its window.
        window: WindowId,
    },
    /// A tab moved, within its window or to another, to `index` of
    /// `window`'s tabs.
    TabMoved {
        /// The tab.
        tab: TabId,
        /// The window that it left, or its own.
        from: WindowId,
        /// The window that it is in.
        window: WindowId,
        /// Its place among that window's tabs, from the first.
        index: usize,
    },
    /// A pane opened in `tab`.
    PaneCreated {
        /// The pane.
        pane: PaneId,
        /// Its tab.
        tab: TabId,
    },
    /// A pane closed, whose program was hung up.
    PaneClosed {
        /// The pane.
        pane: PaneId,
        /// The tab that it was in.
        tab: TabId,
    },
    /// A pane took its tab's keyboard.
    PaneFocused {
        /// The pane.
        pane: PaneId,
        /// Its tab.
        tab: TabId,
    },
    /// A pane moved from a tab to another, its program running on.
    PaneMoved {
        /// The pane.
        pane: PaneId,
        /// The tab that it left.
        from: TabId,
        /// The tab that it is in.
        tab: TabId,
    },
    /// A tab's layout changed: a pane split or closed, a split's ratio, or
    /// its zoom.
    LayoutChanged {
        /// The tab.
        tab: TabId,
    },
    /// A tab's title or color changed, as a read of its window's tabs gives
    /// them: its own, its program's, or its active pane's folder and
    /// program, as its pane's output changes them, or as another pane takes
    /// its keyboard.
    TabChanged {
        /// The tab.
        tab: TabId,
    },
}

impl WorkspaceEvent {
    /// The names of the workspace's events, as plugins subscribe to them.
    pub const NAMES: [&'static str; 12] = [
        "window-opened",
        "window-closed",
        "tab-created",
        "tab-closed",
        "tab-activated",
        "tab-moved",
        "pane-created",
        "pane-closed",
        "pane-focused",
        "pane-moved",
        "layout-changed",
        "tab-changed",
    ];

    /// The event's name, as a plugin subscribes to it.
    pub fn name(&self) -> &'static str {
        match self {
            Self::WindowOpened { .. } => "window-opened",
            Self::WindowClosed { .. } => "window-closed",
            Self::TabCreated { .. } => "tab-created",
            Self::TabClosed { .. } => "tab-closed",
            Self::TabActivated { .. } => "tab-activated",
            Self::TabMoved { .. } => "tab-moved",
            Self::PaneCreated { .. } => "pane-created",
            Self::PaneClosed { .. } => "pane-closed",
            Self::PaneFocused { .. } => "pane-focused",
            Self::PaneMoved { .. } => "pane-moved",
            Self::LayoutChanged { .. } => "layout-changed",
            Self::TabChanged { .. } => "tab-changed",
        }
    }
}
