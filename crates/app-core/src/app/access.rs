//! The windows' accessibility: each window's tree, of its panes' screens
//! and cursors, then its slots' views and its panels, in the order that
//! they draw, which goes to the platform once an accessibility client,
//! as VoiceOver, asked for it, whole, then again as the panes' screens
//! change, their cursors move, as a shell's Left at its prompt moves one
//! without a row's change, or the keyboard moves to another pane, at most
//! once a [`RATE`], and as the views change;
//! nothing before a client asks, so that a window that no client reads
//! costs nothing. A client's focus of a pane gives it the keyboard, and its
//! actions on a view's nodes go to them, as their plugins hear.

use std::time::Duration;

use accesskit::{Action, ActionRequest};
use lokyr_kernel::plugins::ui::Role as PanelRole;
use lokyr_kernel::workspace::{Pane, PaneId};
use lokyr_platform_core::{Platform, WindowId};
use lokyr_ui_core::access::view::{self as views, SurfaceAccess};
use lokyr_ui_core::access::{self, PaneAccess, WindowTree};
use lokyr_ui_core::focus::Target;
use lokyr_ui_core::panel::Role;

use super::chrome::ui_event;

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
        state.views_changed = false;
        state.access_focus = focused;
        state.access_cursors = views
            .iter()
            .filter_map(|view| self.workspace.pane(view.pane).map(tree_cursor))
            .collect();
        state.access_cursors.sort_unstable();
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
        let mut tree = WindowTree::new();
        for pane in &panes {
            tree.pane(pane);
        }
        // The slots' views and the panels that show, over the panes.
        for view in &state.chrome {
            let Some(frame) = view.frame else {
                continue;
            };
            tree.surface(&SurfaceAccess {
                number: view.access,
                surface: &view.surface,
                corner: (frame.x, frame.y),
                panel: view
                    .placed
                    .as_ref()
                    .map(|placed| match placed.options.role {
                        PanelRole::Tooltip => Role::Tooltip,
                        PanelRole::Menu => Role::Menu,
                        PanelRole::Dialog => Role::Dialog,
                        PanelRole::Overlay => Role::Overlay,
                    }),
            });
        }
        // The keyboard's focus at a view's node, or the pane's.
        let focus = match state.focus.target() {
            Target::Node { panel, node } => state
                .chrome
                .iter()
                .find(|view| view.id == panel)
                .map(|view| views::node_id(view.access, node)),
            Target::Pane => focused.and_then(number).map(access::pane_id),
        };
        platform.update_accessibility(window, tree.finish(&title, focus));
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

    /// A client asked for `request`'s action in `window`: one on a view's
    /// node, its row or its tab, as a press, a focus or a value set, goes to
    /// it, whose plugin hears its events; a focus of one of its panes, or of
    /// a row of one, gives that pane the keyboard.
    pub(super) fn accessibility_action(
        &mut self,
        platform: &mut dyn Platform,
        window: WindowId,
        request: &ActionRequest,
    ) {
        if let Some(number) = views::surface_of(request.target_node) {
            let Some(state) = self.windows.get_mut(&window) else {
                return;
            };
            let before = state.focus.target();
            let Some(view) = state.chrome.iter_mut().find(|view| view.access == number) else {
                return;
            };
            let events = views::perform(request, (view.id, &mut view.surface), &mut state.focus);
            let told: Vec<_> = events
                .into_iter()
                .map(|event| (view.plugin.clone(), ui_event(view.view.clone(), event)))
                .collect();
            self.ui_events.extend(told);
            self.focus_moved(platform, window, before);
            self.views_changed(window);
            platform.request_frame(window);
            return;
        }
        if request.action != Action::Focus {
            return;
        }
        if let Some(pane) = access::pane_of(request.target_node) {
            self.keys_to_pane(platform, window);
            self.focus_pane(platform, window, PaneId::of(u64::from(pane)));
        }
    }

    /// `window`'s slots' views or panels changed, of a new tree, a node's
    /// state or the focus: the next frame gives a client the tree again,
    /// within its rate.
    pub(super) fn views_changed(&mut self, window: WindowId) {
        if let Some(state) = self.windows.get_mut(&window) {
            state.views_changed = true;
        }
    }
}

/// Where a pane's cursor shows in its window's tree: the pane, its cell's
/// column and row, and the scroll of its view, which moves it.
pub(crate) type TreeCursor = (PaneId, u16, u16, usize);

/// Where `pane`'s cursor shows in its window's tree, as its last snapshot
/// has it.
pub(super) fn tree_cursor(pane: &Pane) -> TreeCursor {
    let snapshot = pane.snapshot();
    let at = snapshot.cursor().position;
    (pane.id(), at.x, at.y, snapshot.offset())
}

/// The number of `pane` in its window's tree.
fn number(pane: PaneId) -> Option<u32> {
    u32::try_from(pane.get()).ok()
}
