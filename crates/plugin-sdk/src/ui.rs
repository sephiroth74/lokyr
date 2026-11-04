//! Where the plugins' views show: the slots of a window and of its panes,
//! which a plugin fills as its manifest declares, and its floating panels,
//! which it opens and closes; and the events of their nodes.
//!
//! The host asks a plugin for a view's tree, `render`, as a window or a
//! pane shows the slot, as the plugin asks, and as the theme changes, and
//! keeps the last one: a view keeps its tree as its plugin is disabled.

use serde::{Deserialize, Serialize};

use crate::host::{PaneId, WindowId};
use crate::view::{Key, UiEventKind};

/// A slot of the window's chrome, which the views of the plugins that
/// declare it fill: one plugin's, chosen by the config, or many, in the
/// plugins' order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Slot {
    /// The tab bar, in the titlebar or below it: one plugin's.
    TabBar,
    /// The strip across the panes' top, over them, as the programs'
    /// progress: many.
    WindowStrip,
    /// The status bar across the window's bottom: many.
    StatusBar,
    /// A pane's header across its top, above its grid: one plugin's.
    PaneHeader,
}

impl Slot {
    /// The slots, in the order that they draw.
    pub const ALL: [Self; 4] = [
        Self::PaneHeader,
        Self::WindowStrip,
        Self::StatusBar,
        Self::TabBar,
    ];

    /// Its name, as a manifest's `contributes.slots` and the config write
    /// it.
    pub fn name(self) -> &'static str {
        match self {
            Self::TabBar => "tab-bar",
            Self::WindowStrip => "window-strip",
            Self::StatusBar => "status-bar",
            Self::PaneHeader => "pane-header",
        }
    }

    /// The slot of `name`, if it is one.
    pub fn of(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|slot| slot.name() == name)
    }

    /// Whether many plugins' views fill it, in their order, rather than one
    /// plugin's.
    pub fn of_many(self) -> bool {
        matches!(self, Self::WindowStrip | Self::StatusBar)
    }
}

/// A panel of a plugin's, by the id that the plugin gives it, which no
/// other of its open panels has.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct PanelId(pub String);

impl From<&str> for PanelId {
    fn from(id: &str) -> Self {
        Self(id.to_owned())
    }
}

/// A view of a plugin's: a window's slot, a pane's header, or one of its
/// panels.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ViewId {
    /// A window's tab bar.
    TabBar(WindowId),
    /// A window's strip.
    WindowStrip(WindowId),
    /// A window's status bar.
    StatusBar(WindowId),
    /// A pane's header.
    PaneHeader(PaneId),
    /// One of its panels.
    Panel(PanelId),
}

impl ViewId {
    /// The view of `slot` of `window`, or of `pane` for a pane's header.
    pub fn of(slot: Slot, window: WindowId, pane: PaneId) -> Self {
        match slot {
            Slot::TabBar => Self::TabBar(window),
            Slot::WindowStrip => Self::WindowStrip(window),
            Slot::StatusBar => Self::StatusBar(window),
            Slot::PaneHeader => Self::PaneHeader(pane),
        }
    }

    /// Its slot, if it is a slot's.
    pub fn slot(&self) -> Option<Slot> {
        match self {
            Self::TabBar(_) => Some(Slot::TabBar),
            Self::WindowStrip(_) => Some(Slot::WindowStrip),
            Self::StatusBar(_) => Some(Slot::StatusBar),
            Self::PaneHeader(_) => Some(Slot::PaneHeader),
            Self::Panel(_) => None,
        }
    }
}

/// A corner of a rectangle.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Corner {
    /// The top left one.
    TopLeft,
    /// The top right one.
    TopRight,
    /// The bottom left one.
    BottomLeft,
    /// The bottom right one.
    BottomRight,
}

/// Where a panel goes, and the window that it shows in.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Anchor {
    /// Below a cell of a pane, by its column and its row's number, above it
    /// where the window has no room below.
    Cell {
        /// The pane.
        pane: PaneId,
        /// The cell's column.
        column: u16,
        /// Its row's number.
        row: i64,
    },
    /// Below a pane's cursor.
    Cursor(PaneId),
    /// At the pointer, where it is in a window as the panel opens.
    Pointer(WindowId),
    /// Below a node of a view of the plugin's, by its key.
    Node {
        /// The view.
        view: ViewId,
        /// The node's key.
        key: Key,
    },
    /// In a corner of a pane.
    PaneCorner {
        /// The pane.
        pane: PaneId,
        /// The corner.
        corner: Corner,
    },
    /// In a corner of a window's room below its tab bar.
    WindowCorner {
        /// The window.
        window: WindowId,
        /// The corner.
        corner: Corner,
    },
}

/// What a panel is to the system.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Role {
    /// A tooltip: it takes neither the pointer, which goes through it, nor
    /// the keyboard.
    Tooltip,
    /// A menu: it takes the keyboard as it opens, and a click outside it
    /// that closes it goes no further. One of menu items alone shows as
    /// the platform's own menu, where it has one.
    Menu,
    /// A dialog, as a find bar: it takes the keyboard as it opens.
    #[default]
    Dialog,
}

/// A panel's anchor, its role and what closes it, besides its plugin.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PanelOptions {
    /// Where it goes.
    pub anchor: Anchor,
    /// What it is.
    #[serde(default)]
    pub role: Role,
    /// Whether a click outside it closes it.
    #[serde(default)]
    pub close_outside: bool,
    /// Whether Escape closes it, while it has the keyboard's focus.
    #[serde(default)]
    pub close_escape: bool,
}

/// What happened to a node of a key of one of a plugin's views.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct UiEvent {
    /// The view.
    pub view: ViewId,
    /// The node's key.
    pub key: Key,
    /// What happened.
    pub kind: UiEventKind,
}
