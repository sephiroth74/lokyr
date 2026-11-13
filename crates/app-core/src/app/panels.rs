//! The plugins' floating panels in a window: each a view of its plugin's,
//! a surface over the window's content and its slots' views, as `ui-core`'s
//! panels place it, of its tree's size, at its anchor as the window's
//! layout puts it, within the window; a click outside it and Escape close it
//! where its plugin asks so, and its keyboard's focus goes back to where it
//! took it from. A menu's panel of menu items alone shows as the platform's
//! own menu, whose choice and closing its plugin hears.

use std::collections::BTreeMap;

use lokyr_kernel::plugins::host;
use lokyr_kernel::plugins::ui::{
    Anchor as PanelAnchor, Corner as PanelCorner, PanelId, PanelOptions, Role as PanelRole,
    UiEvent, ViewId,
};
use lokyr_kernel::plugins::view::{Key as NodeKey, UiEventKind};
use lokyr_kernel::workspace::{self, PaneId, Workspace};
use lokyr_platform_core::{ContextMenu, Menu, MenuItem, Platform, Point, WindowId, WindowSize};
use lokyr_ui_core::layout::Frame;
use lokyr_ui_core::panel::{self, Anchor, Corner, Dismiss, MenuEntry, Options, Role};

use super::App;
use super::chrome::{PanelPlace, SlotView, ui_event};
use super::input::shown_top;
use super::surfaces;
use crate::drawing::Backend;
use crate::window::{NativeMenu, Window};

/// The room between a panel in a corner and its rectangle's edges, in
/// points.
const MARGIN: f32 = 12.0;

impl<B: Backend> App<B> {
    /// Whether `window`'s keys and text that no binding takes go nowhere,
    /// rather than to the program, while a pane has its keyboard: a panel
    /// that holds them back shows, of the pane that has the keyboard, or of
    /// an anchor of no pane.
    pub(super) fn keys_held_back(&self, window: WindowId) -> bool {
        let Some(state) = self.windows.get(&window) else {
            return false;
        };
        if !state.pane_has_focus() {
            return false;
        }
        let focused = self
            .workspace
            .focused(state.window)
            .map(workspace::Pane::id);
        state
            .chrome
            .iter()
            .filter_map(|view| view.placed.as_ref())
            .any(|placed| {
                let anchor = &placed.options.anchor;
                placed.options.takes_keys
                    && anchor_shows(state, anchor)
                    && anchor_pane(anchor).is_none_or(|pane| Some(PaneId::of(pane.0)) == focused)
            })
    }

    /// Closes the panels anchored to a pane that closed, or that another
    /// window shows now, as its tab or itself moved there, telling their
    /// plugins, which can open them anew where the pane is.
    pub(super) fn close_strayed_panels(&mut self, platform: &mut dyn Platform) {
        let panes = &self.workspace;
        let mut strayed: Vec<(WindowId, u32)> = Vec::new();
        for (&window, state) in &self.windows {
            for view in &state.chrome {
                let Some(pane) = view
                    .placed
                    .as_ref()
                    .and_then(|placed| anchor_pane(&placed.options.anchor))
                else {
                    continue;
                };
                let shown = panes
                    .tab_of(PaneId::of(pane.0))
                    .and_then(|tab| panes.tab(tab))
                    .map(workspace::Tab::window);
                if shown != Some(state.window) {
                    strayed.push((window, view.id));
                }
            }
        }
        for (window, view) in strayed {
            self.close_panel(platform, window, view, true);
        }
    }

    /// Opens `plugin`'s panel `id` over the platform's window that shows
    /// `shown`, of `options`, over the panels open before it, its anchor
    /// taken anew at each layout, the pointer's place as it opens kept;
    /// the plugins' host renders it. A window that shows none closes it at
    /// once, as its plugin hears.
    pub(super) fn open_panel(
        &mut self,
        platform: &mut dyn Platform,
        (plugin, id): (String, PanelId),
        shown: workspace::WindowId,
        options: PanelOptions,
    ) {
        let found = self
            .windows
            .iter()
            .find(|(_, state)| state.window == shown)
            .map(|(&window, _)| window);
        let Some(window) = found else {
            self.plugins.panel_closed(&plugin, &id);
            return;
        };
        let scale = platform.window_size(window).map_or(1.0, |size| size.scale);
        let pointer = platform.pointer_position(window).map_or((0.0, 0.0), |at| {
            ((at.x * scale) as f32, (at.y * scale) as f32)
        });
        let Some(state) = self.windows.get_mut(&window) else {
            return;
        };
        state.next_view += 1;
        let view = state.next_view;
        let access = super::chrome::free_access(state.chrome.iter());
        state.panels.open(
            view,
            Options {
                anchor: Anchor::At(pointer),
                role: role_of(options.role),
                dismiss: Dismiss {
                    outside: options.close_outside,
                    escape: options.close_escape,
                },
            },
        );
        state.chrome.push(SlotView::panel(
            (view, access),
            (plugin, id),
            scale as f32,
            PanelPlace { options, pointer },
        ));
        platform.request_frame(window);
    }

    /// `plugin`'s panel `id` closed as its plugin asked, which the plugins'
    /// host forgot and told already: its window lets it go.
    pub(super) fn panel_gone(&mut self, platform: &mut dyn Platform, plugin: &str, id: &PanelId) {
        let view = ViewId::Panel(id.clone());
        let found = self.windows.iter().find_map(|(&window, state)| {
            state
                .chrome
                .iter()
                .find(|shown| shown.plugin == plugin && shown.view == view)
                .map(|shown| (window, shown.id))
        });
        if let Some((window, view)) = found {
            self.close_panel(platform, window, view, false);
        }
    }

    /// Closes `window`'s panel of the view `view`: a composition of its
    /// field ends as it stands, which the input method forgets; the
    /// keyboard's focus that it had goes back to where it took it from, as
    /// its program hears; and its plugin hears that it closed, where `tell`,
    /// as a click outside it, Escape or the platform's menu closed it.
    pub(super) fn close_panel(
        &mut self,
        platform: &mut dyn Platform,
        window: WindowId,
        view: u32,
        tell: bool,
    ) {
        let Some(state) = self.windows.get_mut(&window) else {
            return;
        };
        let Some(at) = state.chrome.iter().position(|shown| shown.id == view) else {
            return;
        };
        let before = state.focus.target();
        let mut events = Vec::new();
        let closing = &mut state.chrome[at];
        if let Some(node) = closing.surface.fields.composing(&closing.surface.tree) {
            let kept = closing
                .surface
                .fields
                .keep_composition(&closing.surface.tree, node)
                .unwrap_or_default();
            events.extend(kept.into_iter().map(|event| {
                (
                    closing.plugin.clone(),
                    ui_event(closing.view.clone(), event),
                )
            }));
            platform.end_composition(window);
        }
        let closed = state.chrome.remove(at);
        state.panels.close(view);
        // The platform's menu of the panel closes, if the platform holds it
        // still; its choice, if one comes before its closing, does nothing.
        let last = state.native.len().checked_sub(1);
        if let Some(shown) = state
            .native
            .iter()
            .position(|menu| matches!(menu, NativeMenu::Panel(id, _) if *id == view))
        {
            state.native[shown] = NativeMenu::Closed;
            if Some(shown) == last {
                platform.close_context_menu(window);
            }
        }
        state.native_due.retain(|&due| due != view);
        {
            let (focus, chrome) = (&mut state.focus, &state.chrome);
            focus.closed(view, |id| {
                chrome
                    .iter()
                    .find(|shown| shown.id == id)
                    .map(|shown| &shown.surface.tree)
            });
        }
        if state.captured == Some(view) {
            state.captured = None;
        }
        if state.over == Some(view) {
            state.over = None;
        }
        if state.focus.target() != before {
            surfaces::tell_pane(&self.workspace, state, before);
        }
        events.extend(surfaces::focus_events(state));
        self.ui_events.extend(events);
        if tell && let ViewId::Panel(id) = &closed.view {
            self.plugins.panel_closed(&closed.plugin, id);
        }
        self.views_changed(window);
        platform.request_frame(window);
        self.wake_for_tooltips(platform);
    }

    /// Lays `window`'s panels out in a window of `size`, from the lowest:
    /// each of its tree's own size, of the window's width at most, or of its
    /// pane's width across its top, at its anchor as the window's layout
    /// now has it, within the window. The
    /// events of the lists' rows that ask for their items go to their
    /// plugins as the turn ends.
    pub(super) fn lay_out_panels(&mut self, window: WindowId, size: WindowSize) {
        let (Some(drawing), Some(state)) = (self.drawing.as_mut(), self.windows.get_mut(&window))
        else {
            return;
        };
        let room = (size.pixels.width as f32, size.pixels.height as f32);
        let scale = size.scale as f32;
        let font = &mut drawing.font;
        let mut events = Vec::new();
        let ids: Vec<u32> = state.panels.order().collect();
        for id in ids {
            let Some(at) = state.chrome.iter().position(|shown| shown.id == id) else {
                continue;
            };
            let Some(placed) = state.chrome[at].placed.clone() else {
                continue;
            };
            // A panel at a pane's corner or across its top shows while its
            // pane does, and hides behind another tab or a zoomed pane.
            if !anchor_shows(state, &placed.options.anchor) {
                state.panels.hide(id);
                state.chrome[at].frame = None;
                continue;
            }
            let plugin = state.chrome[at].plugin.clone();
            // The settings tab's view takes the panes' room whole.
            if plugin == super::chrome::OVERFLOW
                && state.chrome[at].view == super::settings_tab::settings_view()
            {
                let Some(room) = state.panes_room else {
                    continue;
                };
                let frame = Frame {
                    x: room.x as f32,
                    y: room.y as f32,
                    width: room.width as f32,
                    height: room.height as f32,
                };
                let view = &mut state.chrome[at];
                let laid = view
                    .surface
                    .lay_out((Some(frame.width), Some(frame.height)), font);
                view.frame = state.panels.place_at(id, frame);
                events.extend(
                    laid.into_iter()
                        .map(|event| (view.plugin.clone(), ui_event(view.view.clone(), event))),
                );
                continue;
            }
            let anchor = anchor_of(&self.workspace, state, (&plugin, &placed), scale);
            let view = &mut state.chrome[at];
            // A panel across a pane's top takes the pane's width.
            let across = match (&placed.options.anchor, anchor) {
                (PanelAnchor::PaneTop(_), Anchor::Corner(rect, ..)) => Some(rect.width),
                _ => None,
            };
            let mut laid = view.surface.lay_out((across, None), font);
            let mut natural = view.surface.size().unwrap_or_default();
            if natural.0 > room.0 {
                laid.extend(view.surface.lay_out((Some(room.0), None), font));
                natural = view.surface.size().unwrap_or_default();
            }
            state.panels.anchor(id, anchor);
            view.frame = state.panels.place(id, natural, room);
            if let Some(frame) = view.frame
                && (frame.width, frame.height) != natural
            {
                laid.extend(
                    view.surface
                        .lay_out((Some(frame.width), Some(frame.height)), font),
                );
            }
            events.extend(
                laid.into_iter()
                    .map(|event| (view.plugin.clone(), ui_event(view.view.clone(), event))),
            );
        }
        self.ui_events.extend(events);
    }

    /// Shows `window`'s menus' panels that wait as the platform's own
    /// menus, in their order, each at its anchor, its items' choices kept
    /// by their commands: each closes the one before it, as its closing
    /// tells.
    pub(super) fn show_native(
        &mut self,
        platform: &mut dyn Platform,
        window: WindowId,
        size: WindowSize,
    ) {
        if self
            .windows
            .get(&window)
            .is_some_and(|state| !state.native_due.is_empty())
        {
            self.release_capture(platform, window);
        }
        let Some(state) = self.windows.get_mut(&window) else {
            return;
        };
        for id in std::mem::take(&mut state.native_due) {
            let Some(view) = state.chrome.iter().find(|shown| shown.id == id) else {
                continue;
            };
            let (Some(entries), Some(placed)) =
                (panel::native_menu(&view.surface.tree), view.placed.as_ref())
            else {
                continue;
            };
            let scale = size.scale as f32;
            let (x, y) = point(&anchor_of(
                &self.workspace,
                state,
                (&view.plugin, placed),
                scale,
            ));
            let mut menu = ContextMenu::default();
            let mut choices = BTreeMap::new();
            menu.items = menu_items(&entries, &mut menu, &mut choices);
            let position = Point {
                x: f64::from(x / scale),
                y: f64::from(y / scale),
            };
            platform.show_context_menu(window, position, &menu);
            state.native.push_back(NativeMenu::Panel(id, choices));
        }
    }

    /// The platform's menu of `window` that is open gave `command`: of a
    /// menu's panel, its item's node's event to the panel's plugin, as its
    /// press would tell, or nothing once the panel closed. Whether the menu
    /// was a panel's, whose items are no commands of the app's.
    pub(super) fn native_chosen(&mut self, window: WindowId, command: &str) -> bool {
        let Some(state) = self.windows.get(&window) else {
            return false;
        };
        match state.native.front() {
            Some(NativeMenu::Panel(id, choices)) => {
                let view = state.chrome.iter().find(|shown| shown.id == *id);
                if let (Some((key, kind)), Some(view)) = (choices.get(command), view) {
                    let event = UiEvent {
                        view: view.view.clone(),
                        key: key.clone(),
                        kind: kind.clone(),
                    };
                    self.ui_events.push((view.plugin.clone(), event));
                }
                true
            }
            Some(NativeMenu::Overflow(choices)) => {
                let choices = choices.clone();
                self.overflow_chosen(window, &choices, command);
                true
            }
            Some(NativeMenu::Closed) => true,
            Some(NativeMenu::Window) | None => false,
        }
    }

    /// The platform's menu of `window` that was open closed: a menu's panel
    /// that it showed closes, as its plugin hears, after its choice where
    /// one was made.
    pub(super) fn native_closed(&mut self, platform: &mut dyn Platform, window: WindowId) {
        self.release_capture(platform, window);
        let shown = self
            .windows
            .get_mut(&window)
            .and_then(|state| state.native.pop_front());
        if let Some(NativeMenu::Panel(view, _)) = shown {
            self.close_panel(platform, window, view, true);
        }
    }
}

/// The anchor of a panel of `plugin`'s, of `placed`, in `state`'s window,
/// in pixels, as its last layout placed the panes and the views, of `scale`
/// pixels a point: below a cell of a pane, its cursor's, or a node of a
/// view of the plugin's, the button of the titlebar's buttons that find no
/// room for one of them; at the pointer's place as it opened; in a corner
/// of a pane, or of the window's room below its tab bar. An anchor gone, as
/// of a pane that closed, is the pointer's.
fn anchor_of<B: Backend>(
    workspace: &Workspace,
    state: &Window<B>,
    (plugin, placed): (&str, &PanelPlace),
    scale: f32,
) -> Anchor {
    let pointer = Anchor::At(placed.pointer);
    let pane_view = |pane: host::PaneId| {
        state
            .views
            .iter()
            .find(|view| view.pane == PaneId::of(pane.0))
    };
    let cell = |view: &crate::window::PaneView, column: u16, row: i64| {
        let (width, height) = (
            f32::from(view.grid.cell_width),
            f32::from(view.grid.cell_height),
        );
        Frame {
            x: view.rect.x as f32 + f32::from(column) * width,
            y: view.rect.y as f32 + row as f32 * height,
            width,
            height,
        }
    };
    let margin = MARGIN * scale;
    match &placed.options.anchor {
        PanelAnchor::Cell { pane, column, row } => pane_view(*pane)
            .and_then(|view| {
                let top = shown_top(workspace.pane(view.pane)?);
                Some(Anchor::Below(cell(view, *column, row - top)))
            })
            .unwrap_or(pointer),
        PanelAnchor::Cursor(pane) => pane_view(*pane)
            .and_then(|view| {
                let at = workspace.pane(view.pane)?.snapshot().cursor().position;
                Some(Anchor::Below(cell(view, at.x, i64::from(at.y))))
            })
            .unwrap_or(pointer),
        PanelAnchor::Pointer(_) => pointer,
        PanelAnchor::Node { view, key } => {
            let of = |plugin: &str| {
                state
                    .chrome
                    .iter()
                    .find(|shown| &shown.view == view && shown.plugin == plugin)
            };
            let shown = of(plugin);
            let node = shown.and_then(|shown| {
                let frame = shown.frame?;
                let node = shown.surface.tree.by_key(key)?;
                let at = shown.surface.layout.frame(&shown.surface.tree, node)?;
                Some(Anchor::Below(Frame {
                    x: at.x + frame.x,
                    y: at.y + frame.y,
                    ..at
                }))
            });
            // A titlebar button that finds no room: below the button of
            // those.
            let hidden = || {
                shown
                    .filter(|shown| shown.frame.is_none())
                    .and_then(|_| of(super::chrome::OVERFLOW)?.frame)
                    .map(Anchor::Below)
            };
            node.or_else(hidden).unwrap_or(pointer)
        }
        PanelAnchor::PaneCorner { pane, corner } => pane_view(*pane).map_or(pointer, |view| {
            let rect = view.rect;
            let frame = Frame {
                x: rect.x as f32,
                y: rect.y as f32,
                width: rect.width as f32,
                height: rect.height as f32,
            };
            Anchor::Corner(frame, corner_of(*corner), margin)
        }),
        // Across the pane's top, of its width, at its edge.
        PanelAnchor::PaneTop(pane) => pane_view(*pane).map_or(pointer, |view| {
            let rect = view.rect;
            let frame = Frame {
                x: rect.x as f32,
                y: rect.y as f32,
                width: rect.width as f32,
                height: rect.height as f32,
            };
            Anchor::Corner(frame, Corner::TopLeft, 0.0)
        }),
        PanelAnchor::WindowCorner { corner, .. } => {
            Anchor::Corner(state.overlay, corner_of(*corner), margin)
        }
    }
}

/// The pane of a panel's anchor, if it is a pane's: of a cell, the cursor,
/// a corner or the top.
fn anchor_pane(anchor: &PanelAnchor) -> Option<host::PaneId> {
    match anchor {
        PanelAnchor::Cell { pane, .. }
        | PanelAnchor::Cursor(pane)
        | PanelAnchor::PaneCorner { pane, .. }
        | PanelAnchor::PaneTop(pane) => Some(*pane),
        PanelAnchor::Pointer(_) | PanelAnchor::Node { .. } | PanelAnchor::WindowCorner { .. } => {
            None
        }
    }
}

/// Whether a panel of `anchor` shows in `state`'s window: one at a pane's
/// corner or across its top while the window shows its pane, behind no
/// other tab nor a zoomed pane; any other always.
pub(super) fn anchor_shows<B: Backend>(state: &Window<B>, anchor: &PanelAnchor) -> bool {
    match anchor {
        PanelAnchor::PaneCorner { pane, .. } | PanelAnchor::PaneTop(pane) => state
            .views
            .iter()
            .any(|view| view.pane == PaneId::of(pane.0)),
        _ => true,
    }
}

/// The point of `anchor`, in pixels, where a menu that the platform places
/// opens: below its rectangle, at its point, or at its corner within the
/// margin.
fn point(anchor: &Anchor) -> (f32, f32) {
    match *anchor {
        Anchor::At(at) => at,
        Anchor::Below(rect) => (rect.x, rect.y + rect.height),
        Anchor::Corner(rect, corner, margin) => {
            let (left, right) = (rect.x + margin, rect.x + rect.width - margin);
            let (top, bottom) = (rect.y + margin, rect.y + rect.height - margin);
            match corner {
                Corner::TopLeft => (left, top),
                Corner::TopRight => (right, top),
                Corner::BottomLeft => (left, bottom),
                Corner::BottomRight => (right, bottom),
                Corner::Top => (
                    rect.x + rect.width / 2.0,
                    rect.y + (rect.height / 8.0).max(margin),
                ),
            }
        }
    }
}

/// The platform's items of a native menu's `entries`, each of its node's
/// key as its command, the dimmed ones' and the ticked ones' in `menu`, and
/// the events of their choices in `choices`.
fn menu_items(
    entries: &[MenuEntry],
    menu: &mut ContextMenu,
    choices: &mut BTreeMap<String, (NodeKey, UiEventKind)>,
) -> Vec<MenuItem> {
    entries
        .iter()
        .map(|entry| match entry {
            MenuEntry::Item {
                key,
                title,
                ticked,
                event,
            } => {
                match event {
                    Some(event) => {
                        choices.insert(key.0.clone(), (key.clone(), event.clone()));
                    }
                    None => menu.dimmed.push(key.0.clone()),
                }
                if *ticked {
                    menu.ticked.push(key.0.clone());
                }
                MenuItem::Command {
                    title: title.clone(),
                    command: key.0.clone(),
                    shortcut: None,
                }
            }
            MenuEntry::Separator => MenuItem::Separator,
            MenuEntry::Submenu { title, entries } => MenuItem::Submenu(Menu {
                title: title.clone(),
                items: menu_items(entries, menu, choices),
                role: None,
            }),
        })
        .collect()
}

/// `ui-core`'s role of a panel's `role`.
fn role_of(role: PanelRole) -> Role {
    match role {
        PanelRole::Tooltip => Role::Tooltip,
        PanelRole::Menu => Role::Menu,
        PanelRole::Dialog => Role::Dialog,
        PanelRole::Overlay => Role::Overlay,
    }
}

/// `ui-core`'s corner of a panel's `corner`.
fn corner_of(corner: PanelCorner) -> Corner {
    match corner {
        PanelCorner::TopLeft => Corner::TopLeft,
        PanelCorner::TopRight => Corner::TopRight,
        PanelCorner::BottomLeft => Corner::BottomLeft,
        PanelCorner::BottomRight => Corner::BottomRight,
        PanelCorner::Top => Corner::Top,
    }
}
