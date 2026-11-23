//! A window's broadcast of the typed input from the menus: the Shell menu's
//! Broadcast Input, ticked as the window in front broadcasts, and a pane's
//! Leave Broadcast and Join Broadcast.

use lokyr_kernel::plugins::host::BroadcastTo;
use lokyr_platform_core::{Platform, WindowId};

use super::{App, BROADCAST_OFF, BROADCAST_TAB, BROADCAST_WINDOW, BROADCASTS};
use crate::Backend;

impl<B: Backend> App<B> {
    /// Sends the typed input of the window that acts, or has the focus, as
    /// the broadcast's command `name` says: to the pane with its keyboard
    /// alone, or to its tab's panes or the window's too, which the same
    /// command again turns off.
    pub(super) fn broadcast_to(&mut self, platform: &mut dyn Platform, name: &str) {
        let Some(window) = self.acting.or_else(|| platform.focused_window()) else {
            return;
        };
        let Some(shown) = self.windows.get(&window).map(|state| state.window) else {
            return;
        };
        if self.workspace.focused(shown).is_none() {
            return;
        }
        let wanted = match name {
            BROADCAST_TAB => BroadcastTo::Tab,
            BROADCAST_WINDOW => BroadcastTo::Window,
            _ => BroadcastTo::Off,
        };
        let to = if wanted != BroadcastTo::Off && self.workspace.broadcast(shown) == wanted {
            BroadcastTo::Off
        } else {
            wanted
        };
        self.workspace.set_broadcast(shown, to);
        self.show_broadcast(platform, Some(window));
        platform.request_frame(window);
    }

    /// Leaves the pane with the keyboard of the window that acts, or has
    /// the focus, out of its window's broadcast, of `leave`, or takes it
    /// back in.
    pub(super) fn leave_broadcast(&mut self, platform: &mut dyn Platform, leave: bool) {
        let Some(window) = self.acting.or_else(|| platform.focused_window()) else {
            return;
        };
        let Some(pane) = self
            .windows
            .get(&window)
            .and_then(|state| self.workspace.focused(state.window))
            .map(lokyr_kernel::workspace::Pane::id)
        else {
            return;
        };
        self.workspace.set_pane_broadcast(pane, leave);
        platform.request_frame(window);
    }

    /// Ticks the Shell menu's Broadcast Input as `window`, the one in
    /// front, broadcasts, its items dimmed of a window of no pane, as the
    /// settings', and of none.
    pub(super) fn show_broadcast(&self, platform: &mut dyn Platform, window: Option<WindowId>) {
        let to = window
            .and_then(|window| self.windows.get(&window))
            .map(|state| state.window)
            .filter(|&shown| self.workspace.focused(shown).is_some())
            .map(|shown| self.workspace.broadcast(shown));
        for (name, ticked) in [
            (BROADCAST_OFF, BroadcastTo::Off),
            (BROADCAST_TAB, BroadcastTo::Tab),
            (BROADCAST_WINDOW, BroadcastTo::Window),
        ] {
            platform.set_command_checked(name, to == Some(ticked));
        }
        for name in BROADCASTS {
            platform.set_command_enabled(name, to.is_some());
        }
    }

    /// Whether `pane` takes its window's broadcast of the typed input, as
    /// its ring and what accessibility calls it tell.
    pub(super) fn takes_broadcast(&self, pane: lokyr_kernel::workspace::PaneId) -> bool {
        let Some(shown) = self.workspace.pane(pane) else {
            return false;
        };
        !shown.broadcast_left()
            && !shown.is_kept()
            && self
                .workspace
                .tab_of(pane)
                .is_some_and(|tab| self.workspace.broadcasts_tab(tab))
    }
}
