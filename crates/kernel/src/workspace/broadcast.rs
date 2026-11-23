//! A window's broadcast of the typed input: the keys, the input method's
//! text and the user's pastes go to the pane that has its keyboard, or to
//! every pane of its tab or of the window too, each encoded of its own
//! terminal's modes, but the panes left out, which take their own alone.

use lokyr_plugin_sdk::event::WorkspaceEvent;
use lokyr_plugin_sdk::host::BroadcastTo;

use super::{PaneId, TabId, WindowId, Workspace};

impl Workspace {
    /// Where `window`'s typed input goes beside the pane that has its
    /// keyboard; off of a window that is not there.
    pub fn broadcast(&self, window: WindowId) -> BroadcastTo {
        self.windows
            .get(&window)
            .map_or(BroadcastTo::Off, |shown| shown.broadcast)
    }

    /// Sends `window`'s typed input to `to`; whether the window is there.
    pub fn set_broadcast(&mut self, window: WindowId, to: BroadcastTo) -> bool {
        let Some(shown) = self.windows.get_mut(&window) else {
            return false;
        };
        if shown.broadcast != to {
            shown.broadcast = to;
            self.broadcast_changed(window);
        }
        true
    }

    /// Leaves `pane` out of its window's broadcast, or takes it back in;
    /// whether it is there.
    pub fn set_pane_broadcast(&mut self, pane: PaneId, left_out: bool) -> bool {
        let Some(shown) = self.panes.get_mut(&pane) else {
            return false;
        };
        if shown.broadcast_left != left_out {
            shown.broadcast_left = left_out;
            if let Some(window) = self.window_of_pane(pane) {
                self.broadcast_changed(window);
            }
        }
        true
    }

    /// Whether the panes of `tab` take its window's broadcast: every tab's
    /// of a broadcast to the window's panes, the window's active tab's of
    /// a broadcast to the tab's.
    pub fn broadcasts_tab(&self, tab: TabId) -> bool {
        let Some(window) = self.window_of_tab(tab) else {
            return false;
        };
        match self.broadcast(window) {
            BroadcastTo::Off => false,
            BroadcastTo::Tab => self
                .windows
                .get(&window)
                .is_some_and(|shown| shown.active == tab),
            BroadcastTo::Window => true,
        }
    }

    /// The panes of `window` left out of its broadcast, in their order.
    pub fn broadcast_left_out(&self, window: WindowId) -> Vec<PaneId> {
        self.panes_of(window)
            .into_iter()
            .filter(|pane| {
                self.panes
                    .get(pane)
                    .is_some_and(|shown| shown.broadcast_left)
            })
            .collect()
    }

    /// The panes that the input typed into `pane` goes to: `pane` first,
    /// then the others of its tab or of its window, in their order, as the
    /// window's broadcast says, of a program that runs and not left out;
    /// `pane` alone of a broadcast that is off, or of a pane left out.
    pub fn broadcast_panes(&self, pane: PaneId) -> Vec<PaneId> {
        let mut panes = vec![pane];
        let Some(window) = self.window_of_pane(pane) else {
            return panes;
        };
        if self
            .panes
            .get(&pane)
            .is_none_or(|shown| shown.broadcast_left)
        {
            return panes;
        }
        let others = match self.broadcast(window) {
            BroadcastTo::Off => return panes,
            BroadcastTo::Tab => self
                .tab_of(pane)
                .and_then(|tab| self.tabs.get(&tab))
                .map_or_else(Vec::new, super::Tab::shown_panes),
            BroadcastTo::Window => self.windows.get(&window).map_or_else(Vec::new, |shown| {
                shown
                    .tabs
                    .iter()
                    .filter_map(|tab| self.tabs.get(tab))
                    .flat_map(super::Tab::shown_panes)
                    .collect()
            }),
        };
        panes.extend(others.into_iter().filter(|&other| {
            other != pane
                && self
                    .panes
                    .get(&other)
                    .is_some_and(|shown| !shown.broadcast_left && !shown.is_kept())
        }));
        panes
    }

    /// The window that holds `pane`, if one does.
    fn window_of_pane(&self, pane: PaneId) -> Option<WindowId> {
        self.tab_of(pane).and_then(|tab| self.window_of_tab(tab))
    }

    /// Records that `window`'s broadcast changed, for the embedder.
    fn broadcast_changed(&mut self, window: WindowId) {
        self.changed(WorkspaceEvent::BroadcastChanged {
            window: window.into(),
        });
    }
}
