//! A tab's floating panes: panes over its layout's, each in a frame of
//! shares of the tab's room, in an order from the back to the front, which
//! show or hide together. The one that takes the keyboard comes in front;
//! as they hide, the layout's pane that had it last takes it back. A
//! floating pane closes as any pane, and the one in front takes the place
//! of the layout's last pane as it closes, so that the tab stays. A popup
//! is a floating pane that runs a command, which closes as it ends, and
//! which no quit keeps.

use lokyr_plugin_sdk::event::WorkspaceEvent;

use super::{Pane, PaneId, TabId, Workspace};

/// The least share of the tab's room that a floating pane takes across and
/// down.
pub const LEAST_SHARE: f64 = 0.05;

/// The frame of a floating pane: its top left corner and its size, each a
/// share of its tab's room, from its top left corner, within the room,
/// kept in ten-thousandths, as a split's ratio is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FloatingFrame {
    x: u16,
    y: u16,
    width: u16,
    height: u16,
}

/// The ten-thousandths of the whole.
const WHOLE: f64 = 10_000.0;

impl FloatingFrame {
    /// The frame of the corner `x`, `y` and the size `width`, `height`,
    /// shares of the tab's room, within it: of [`LEAST_SHARE`] at least each
    /// way and the whole room at most, its corner where it stays inside, a
    /// share that is not a number taken as the least, or as the room's
    /// edge of a corner.
    pub fn new(x: f64, y: f64, width: f64, height: f64) -> Self {
        let size = |share: f64| {
            if share.is_nan() {
                LEAST_SHARE
            } else {
                share.clamp(LEAST_SHARE, 1.0)
            }
        };
        let (width, height) = (size(width), size(height));
        let place = |at: f64, size: f64| {
            if at.is_nan() {
                0.0
            } else {
                at.clamp(0.0, 1.0 - size)
            }
        };
        // Within 0 and 1, which ten thousand times fits.
        let kept = |share: f64| (share * WHOLE).round() as u16;
        Self {
            x: kept(place(x, width)),
            y: kept(place(y, height)),
            width: kept(width),
            height: kept(height),
        }
    }

    /// The frame in the tab's middle of `width` and `height`, shares of its
    /// room.
    pub fn centered(width: f64, height: f64) -> Self {
        Self::new((1.0 - width) / 2.0, (1.0 - height) / 2.0, width, height)
    }

    /// Its corner's share across.
    pub fn x(self) -> f64 {
        f64::from(self.x) / WHOLE
    }

    /// Its corner's share down.
    pub fn y(self) -> f64 {
        f64::from(self.y) / WHOLE
    }

    /// Its share across.
    pub fn width(self) -> f64 {
        f64::from(self.width) / WHOLE
    }

    /// Its share down.
    pub fn height(self) -> f64 {
        f64::from(self.height) / WHOLE
    }
}

/// A pane that floats over its tab's layout, in its frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Floating {
    /// The pane.
    pub pane: PaneId,
    /// Its frame over the tab's room.
    pub frame: FloatingFrame,
    /// Whether it is a popup, of a command that closes it as it ends.
    pub popup: bool,
}

impl Workspace {
    /// Floats `new` over `tab`'s layout, in `frame`, in front of its other
    /// floating panes, which show: it takes the tab's keyboard. Its id, or
    /// none of a tab that is not there or the settings'.
    pub fn float_pane(&mut self, tab: TabId, new: Pane, frame: FloatingFrame) -> Option<PaneId> {
        self.float(tab, new, frame, false)
    }

    /// Floats `new`, a popup of its command, over `tab`'s layout, as
    /// [`Workspace::float_pane`] floats a pane.
    pub fn open_popup(&mut self, tab: TabId, new: Pane, frame: FloatingFrame) -> Option<PaneId> {
        self.float(tab, new, frame, true)
    }

    /// Whether `pane` is a popup.
    pub fn is_popup(&self, pane: PaneId) -> bool {
        self.tab_of
            .get(&pane)
            .and_then(|tab| self.tabs.get(tab))
            .is_some_and(|tab| tab.is_popup(pane))
    }

    /// Floats `new` over `tab`'s layout, in `frame`, a popup or not.
    fn float(
        &mut self,
        tab: TabId,
        new: Pane,
        frame: FloatingFrame,
        popup: bool,
    ) -> Option<PaneId> {
        let shown = self.tabs.get_mut(&tab)?;
        let id = new.id();
        shown.floating.push(Floating {
            pane: id,
            frame,
            popup,
        });
        shown.floating_shown = true;
        self.panes.insert(id, new);
        self.tab_of.insert(id, tab);
        self.changed(WorkspaceEvent::PaneCreated {
            pane: id.into(),
            tab: tab.into(),
        });
        self.changed(WorkspaceEvent::LayoutChanged { tab: tab.into() });
        self.focus_pane(id);
        Some(id)
    }

    /// Shows `tab`'s floating panes, or hides them, the layout's pane that
    /// had the keyboard last taking it back from a floating one; whether the
    /// tab is there.
    pub fn set_floating_shown(&mut self, tab: TabId, shown: bool) -> bool {
        let Some(held) = self.tabs.get_mut(&tab) else {
            return false;
        };
        if held.floating_shown == shown {
            return true;
        }
        held.floating_shown = shown;
        let back = (!shown && held.is_floating(held.active)).then_some(held.split_active);
        let front = held
            .floating
            .last()
            .map(|front| front.pane)
            .filter(|_| shown);
        self.changed(WorkspaceEvent::LayoutChanged { tab: tab.into() });
        // The keyboard on a pane that shows: the layout's as they hide, the
        // one in front as they show.
        if let Some(pane) = back.or(front) {
            self.focus_pane(pane);
        }
        true
    }

    /// Floats `pane`, of its tab's layout, over the layout in `frame`, in
    /// front, with the keyboard: its room goes to its sibling. Whether it is
    /// a pane of a layout of more than one, which a tab keeps.
    pub fn float_split_pane(&mut self, pane: PaneId, frame: FloatingFrame) -> bool {
        let Some(&tab) = self.tab_of.get(&pane) else {
            return false;
        };
        let Some(shown) = self.tabs.get_mut(&tab) else {
            return false;
        };
        if !shown.layout.contains(pane) {
            return false;
        }
        let Some(nearest) = shown.layout.remove(pane) else {
            return false;
        };
        if shown.split_active == pane {
            shown.split_active = nearest;
        }
        if shown.zoomed == Some(pane) {
            shown.zoomed = None;
        }
        shown.floating.push(Floating {
            pane,
            frame,
            popup: false,
        });
        shown.floating_shown = true;
        self.changed(WorkspaceEvent::LayoutChanged { tab: tab.into() });
        self.focus_pane(pane);
        true
    }

    /// Moves the floating pane `pane` into its tab's layout, on the right
    /// of the layout's pane that had the keyboard last, half and half, with
    /// the keyboard. Whether it is a floating pane.
    pub fn dock_floating(&mut self, pane: PaneId) -> bool {
        let Some(&tab) = self.tab_of.get(&pane) else {
            return false;
        };
        let Some(shown) = self.tabs.get_mut(&tab) else {
            return false;
        };
        let Some(at) = shown.floating.iter().position(|of| of.pane == pane) else {
            return false;
        };
        let beside = shown.split_active;
        if !shown.layout.split(
            beside,
            super::Direction::Horizontal,
            pane,
            super::Side::Second,
        ) {
            return false;
        }
        shown.floating.remove(at);
        shown.zoomed = None;
        self.changed(WorkspaceEvent::LayoutChanged { tab: tab.into() });
        self.focus_pane(pane);
        true
    }

    /// Moves or resizes the floating pane `pane` to `frame`, within its
    /// tab's room; whether it is a floating pane.
    pub fn move_floating(&mut self, pane: PaneId, frame: FloatingFrame) -> bool {
        let Some(&tab) = self.tab_of.get(&pane) else {
            return false;
        };
        let Some(floating) = self
            .tabs
            .get_mut(&tab)
            .and_then(|shown| shown.floating.iter_mut().find(|of| of.pane == pane))
        else {
            return false;
        };
        if floating.frame != frame {
            floating.frame = frame;
            self.changed(WorkspaceEvent::LayoutChanged { tab: tab.into() });
        }
        true
    }
}
