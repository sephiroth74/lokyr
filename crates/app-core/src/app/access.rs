//! The windows' accessibility: each window's tree, of its panes' screens
//! and cursors, which goes to the platform once an accessibility client,
//! as VoiceOver, asked for it, whole, then again as the panes' screens
//! change or the keyboard moves to another pane, at most once a [`RATE`];
//! nothing before a client asks, so that a window that no client reads
//! costs nothing. A client's focus of a pane gives it the keyboard.

use std::time::Duration;

use accesskit::{Action, ActionRequest};
use lokyr_kernel::workspace::PaneId;
use lokyr_platform_core::{Platform, WindowId};
use lokyr_ui_core::access::{self, PaneAccess};

use super::App;
use crate::Backend;

/// The least time between two of a window's trees, as its panes' screens
/// change.
pub(super) const RATE: Duration = Duration::from_millis(200);

impl<B: Backend> App<B> {
    /// Gives the platform the tree of `window`, whole, as a client asked
    /// for it, or as its time came.
    pub(super) fn give_accessibility(&mut self, platform: &mut dyn Platform, window: WindowId) {
        let Some(size) = platform.window_size(window) else {
            return;
        };
        let views = self.lay_out_panes(window, size);
        let title = self.title_of(window);
        let label = self.texts.text("access-pane");
        let Some(state) = self.windows.get_mut(&window) else {
            return;
        };
        let focused = self.workspace.active_pane(state.window);
        state.access_changed = false;
        state.access_focus = focused;
        let panes: Vec<PaneAccess<'_>> = views
            .iter()
            .filter_map(|view| {
                Some(PaneAccess {
                    number: number(view.pane)?,
                    label: &label,
                    snapshot: self.workspace.pane(view.pane)?.snapshot(),
                    grid: (view.rect.x as f32, view.rect.y as f32),
                    cell: (
                        f32::from(view.grid.cell_width),
                        f32::from(view.grid.cell_height),
                    ),
                })
            })
            .collect();
        let update = access::window(&title, &panes, focused.and_then(number));
        platform.update_accessibility(window, update);
    }

    /// The time of the windows' trees came: those whose panes changed go.
    pub(super) fn accessibility_due(&mut self, platform: &mut dyn Platform) {
        self.access_timer = None;
        let due: Vec<WindowId> = self
            .windows
            .iter()
            .filter(|(_, state)| state.access_changed)
            .map(|(&window, _)| window)
            .collect();
        for window in due {
            self.give_accessibility(platform, window);
        }
    }

    /// A client asked for `request`'s action in `window`: a focus of one of
    /// its panes, or of a row of one, gives that pane the keyboard.
    pub(super) fn accessibility_action(
        &mut self,
        platform: &mut dyn Platform,
        window: WindowId,
        request: &ActionRequest,
    ) {
        if request.action != Action::Focus {
            return;
        }
        if let Some(pane) = access::pane_of(request.target_node) {
            self.focus_pane(platform, window, PaneId::of(u64::from(pane)));
        }
    }
}

/// The number of `pane` in its window's tree.
fn number(pane: PaneId) -> Option<u32> {
    u32::try_from(pane.get()).ok()
}
