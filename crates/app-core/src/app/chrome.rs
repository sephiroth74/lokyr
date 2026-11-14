//! The window's chrome: the views of the plugins that fill its slots, its
//! tab bar, its strip, its status bar and its panes' headers, each a
//! surface of its own, laid out around and over the panes as `ui-core`'s
//! chrome places them, and painted over the panes in the window's one
//! drawable, of the theme's tokens.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::time::Instant;

use lokyr_kernel::plugins::Plugins;
use lokyr_kernel::plugins::host::{self, BarPlace};
use lokyr_kernel::plugins::ui::{PanelId, PanelOptions, Role, Slot, UiEvent, ViewId};
use lokyr_kernel::plugins::view::{
    Align, Hover, Justify, Key as NodeKey, Layout as NodeLayout, Length, Node, Style, Token,
    Transition, UiEventKind,
};
use lokyr_kernel::workspace::PaneId;
use lokyr_platform_core::{
    ContextMenu, Fill, FillShape, MenuItem, Platform, Point, Size, Sweep, SweepShape, WindowId,
    WindowSize,
};
use lokyr_render_core::{Item, Rect};
use lokyr_text_core::FontBackend;
use lokyr_ui_core::chrome::{self, Heights, Regions, TabBarPlace};
use lokyr_ui_core::focus::Target;
use lokyr_ui_core::font::Font;
use lokyr_ui_core::layout::{Frame, Room};
use lokyr_ui_core::paint::{NO_PICTURES, Pictures};
use lokyr_ui_core::panel::{self, Panels};
use lokyr_ui_core::pointer::NodeEvent;
use lokyr_ui_core::progress::{self, FillArea, SweepArea};
use lokyr_ui_core::surface::Surface;
use lokyr_ui_core::theme::Tokens;
use lokyr_ui_core::tree::NodeId;

use super::App;
use crate::Backend;
use crate::window::NativeMenu;

/// The most buttons that the titlebar shows beside the window's: past them,
/// the last place's button lists the rest in a menu.
pub(crate) const MOST_TITLE_BUTTONS: usize = 3;

/// A titlebar button's side, in points.
const TITLE_BUTTON: f32 = 24.0;

/// The room that each titlebar button takes across, in points.
pub(super) const TITLE_BUTTON_ROOM: f32 = 28.0;

/// The plugin's name of the button of the titlebar's buttons that find no
/// room, the app's own, which no plugin's name is.
pub(crate) const OVERFLOW: &str = "";

/// The prefix of the ids of the items of the menu of the titlebar's
/// buttons that find no room, before their views' ids.
const OVERFLOWED: &str = "title-button:";

/// How long a titlebar button's hover and press fade, in seconds.
const FADE: f32 = 0.12;

/// What a panel keeps of its opening: its plugin's options, and where the
/// pointer was as it opened, in pixels, which an anchor at the pointer
/// takes.
#[derive(Clone, Debug)]
pub(crate) struct PanelPlace {
    /// Its anchor, its role and what closes it.
    pub(crate) options: PanelOptions,
    /// The pointer's place as it opened.
    pub(crate) pointer: (f32, f32),
}

/// A view of a plugin's that a window shows in one of its slots, or as one
/// of its panels: its surface, the number of the render that the surface
/// holds, and where it shows, once laid out.
pub(crate) struct SlotView {
    /// Its id in its window, which the keyboard's focus knows it by.
    pub(crate) id: u32,
    /// Its number in its window's tree of accessibility, which no other
    /// view of the window has as it shows, its nodes' ids of it.
    pub(crate) access: u8,
    /// The plugin.
    pub(crate) plugin: String,
    /// The view.
    pub(crate) view: ViewId,
    /// The number of the render that the surface holds; `None` before one.
    shown: Option<u64>,
    /// The view's tree as the window shows it.
    pub(crate) surface: Surface,
    /// Where it shows, in pixels from the content's corner; `None` for a
    /// view that shows nothing, or a panel that the platform shows as its
    /// own menu.
    pub(crate) frame: Option<Frame>,
    /// A panel's opening; `None` for a slot's view.
    pub(crate) placed: Option<PanelPlace>,
    /// The node that its plugin asked to show, by its key, which the
    /// nearest scroll area around it scrolls to as the view lays out, once
    /// its tree has it.
    pub(crate) reveal: Option<NodeKey>,
    /// The faces' generation that its text was measured of.
    pub(crate) faces: u64,
}

impl fmt::Debug for SlotView {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SlotView")
            .field("id", &self.id)
            .field("access", &self.access)
            .field("plugin", &self.plugin)
            .field("view", &self.view)
            .field("shown", &self.shown)
            .field("frame", &self.frame)
            .field("placed", &self.placed)
            .finish_non_exhaustive()
    }
}

impl SlotView {
    /// Scrolls to the node that its plugin asked to show, of the last
    /// layout, once its tree has it.
    pub(crate) fn settle_reveal(&mut self) {
        let surface = &mut self.surface;
        let found = self
            .reveal
            .as_ref()
            .and_then(|key| surface.tree.by_key(key));
        if let Some(node) = found {
            surface.layout.reveal(&surface.tree, node);
            self.reveal = None;
        }
    }

    /// Whether it has a tree to show.
    pub(crate) fn shows(&self) -> bool {
        self.surface.tree.root().is_some()
    }

    /// The view of `plugin`'s panel `id`, of the id `view` in its window,
    /// at `scale` pixels a point, as it opens, before its first tree.
    pub(crate) fn panel(
        (view, access): (u32, u8),
        (plugin, id): (String, PanelId),
        scale: f32,
        placed: PanelPlace,
    ) -> Self {
        Self {
            id: view,
            access,
            plugin,
            view: ViewId::Panel(id),
            shown: None,
            surface: Surface::new(scale),
            frame: None,
            placed: Some(placed),
            reveal: None,
            faces: 0,
        }
    }

    /// Whether it is a panel's view of the role of an overlay, over a pane,
    /// which a scroll goes through but where its view would scroll.
    pub(crate) fn is_overlay(&self) -> bool {
        self.placed
            .as_ref()
            .is_some_and(|placed| placed.options.role == Role::Overlay)
    }

    /// Whether it is a panel's view of the role of a tooltip, which the
    /// pointer goes through.
    pub(crate) fn is_tooltip(&self) -> bool {
        self.placed
            .as_ref()
            .is_some_and(|placed| placed.options.role == Role::Tooltip)
    }
}

impl<B: Backend> App<B> {
    /// The views that `window` shows in its slots, in the order that they
    /// draw: the headers of the panes that its active tab shows, of the
    /// config's plugin or the first that fills them; the strip's and the
    /// status bar's of each plugin that fills them, in their order; the tab
    /// bar's, of the config's plugin or the first; the titlebar's buttons
    /// over it, as a title or a sidebar through the titlebar lies under
    /// them; and the panels over them, the settings tab's chosen page right
    /// over the tab's view.
    pub(super) fn slot_views(&self, window: WindowId) -> Vec<(String, ViewId)> {
        let Some(state) = self.windows.get(&window) else {
            return Vec::new();
        };
        let shown = host::WindowId::from(state.window);
        let panes: Vec<PaneId> = self
            .workspace
            .window(state.window)
            .and_then(|shown| self.workspace.tab(shown.active_tab()))
            .map(|tab| match tab.zoomed() {
                Some(zoomed) => vec![zoomed],
                None => tab.layout().panes(),
            })
            .unwrap_or_default();
        // The settings' window of their own shows none of the plugins'
        // views of the terminals' windows: no slot's, and no panel but the
        // app's own and its pages'.
        let settings = self.workspace.settings_window() == Some(state.window);
        let slots = &self.config.slots;
        let one = |slot, chosen: &Option<String>| {
            self.plugins
                .provider(slot, chosen.as_deref().unwrap_or(""))
                .map(str::to_owned)
        };
        let mut views = Vec::new();
        // The panels, from the lowest, a native menu's last, which the
        // platform shows as its own, its view kept for its choice and its
        // close.
        let panel = |id: u32| state.chrome.iter().find(|view| view.id == id);
        let native = || {
            state
                .native
                .iter()
                .filter_map(|menu| match menu {
                    NativeMenu::Panel(id, _) => Some(*id),
                    NativeMenu::Window | NativeMenu::Closed | NativeMenu::Overflow(_) => None,
                })
                .chain(state.native_due.iter().copied())
                .filter_map(panel)
                .filter(|view| !state.panels.is_open(view.id))
        };
        if settings {
            for view in state.panels.order().filter_map(panel).chain(native()) {
                let of_page = view.placed.as_ref().is_some_and(|placed| {
                    matches!(
                        placed.options.anchor,
                        lokyr_kernel::plugins::ui::Anchor::Node {
                            view: ViewId::Page(_),
                            ..
                        }
                    )
                });
                if view.plugin != OVERFLOW && !of_page {
                    continue;
                }
                views.push((view.plugin.clone(), view.view.clone()));
                if view.plugin == OVERFLOW
                    && view.view == super::settings_tab::settings_view()
                    && let Some((plugin, page)) = self.settings.chosen_page()
                {
                    views.push((plugin.to_owned(), page));
                }
            }
            return views;
        }
        if let Some(plugin) = one(Slot::PaneHeader, &slots.pane_header) {
            for pane in panes {
                views.push((plugin.clone(), ViewId::PaneHeader(pane.into())));
            }
        }
        for plugin in self.plugins.providers(Slot::WindowStrip) {
            views.push((plugin.to_owned(), ViewId::WindowStrip(shown)));
        }
        for plugin in self.plugins.providers(Slot::StatusBar) {
            views.push((plugin.to_owned(), ViewId::StatusBar(shown)));
        }
        if let Some(plugin) = one(Slot::TabBar, &slots.tab_bar) {
            views.push((plugin, ViewId::TabBar(shown)));
        }
        // The titlebar's buttons of the plugins that are not stopped, as a
        // stopped plugin's button would show a state that no longer holds
        // and take no click, those that wait for their slot among them,
        // and the app's of those that find no room as they are more than
        // the titlebar shows.
        let buttons: Vec<&str> = self
            .plugins
            .providers(Slot::TitleButtons)
            .into_iter()
            .filter(|plugin| {
                self.plugins.status(plugin) != Some(lokyr_kernel::plugins::Status::Disabled)
            })
            .collect();
        let more = buttons.len() > MOST_TITLE_BUTTONS;
        for plugin in buttons {
            views.push((plugin.to_owned(), ViewId::TitleButtons(shown)));
        }
        if more {
            views.push((OVERFLOW.to_owned(), ViewId::TitleButtons(shown)));
        }
        // The panels over them.
        for view in state.panels.order().filter_map(panel).chain(native()) {
            views.push((view.plugin.clone(), view.view.clone()));
            if view.plugin == OVERFLOW
                && view.view == super::settings_tab::settings_view()
                && let Some((plugin, page)) = self.settings.chosen_page()
            {
                views.push((plugin.to_owned(), page));
            }
        }
        views
    }

    /// Renders the views of `window`'s slots that their plugins have not
    /// rendered yet, as the window or a pane shows the slot first; whether
    /// a plugin rendered one.
    pub(super) fn render_slots(&mut self, platform: &mut dyn Platform, window: WindowId) -> bool {
        let due: Vec<(String, ViewId)> = self
            .slot_views(window)
            .into_iter()
            .filter(|(plugin, view)| {
                plugin != OVERFLOW && self.plugins.rendered(plugin, view).is_none()
            })
            .collect();
        let mut rendered = false;
        // The window's title, which a single tab's title in the titlebar
        // draws, as the system's would show it now, before its bar's first
        // render, which would otherwise draw an older one and render again
        // as the title comes, a frame later.
        if !due.is_empty() {
            let title = self.title_of(window);
            if let Some(state) = self.windows.get(&window) {
                self.workspace.set_window_title(state.window, &title);
            }
        }
        for (plugin, view) in &due {
            let written = self.plugins.written();
            let result = self.with_plugins(platform, |plugins, context| {
                plugins.render(plugin, view, context)
            });
            if let Err(error) = result {
                eprintln!("lokyr: {error}");
            }
            rendered |= self.plugins.rendered(plugin, view).is_some();
            // The writes of a render of the main thread's, as the place of
            // the window's tab bar, which the frame under way lays out
            // after it, show in that frame; the other windows draw them.
            if !written && self.plugins.take_written() {
                for &other in self.windows.keys() {
                    if other != window {
                        platform.request_frame(other);
                    }
                }
            }
        }
        rendered
    }

    /// Asks for a frame of each window that shows a view whose plugin
    /// rendered it anew since the window took its tree.
    pub(super) fn frame_rendered(&self, platform: &mut dyn Platform) {
        for (&window, state) in &self.windows {
            let stale = state.chrome.iter().any(|view| {
                view.plugin != OVERFLOW
                    && self.plugins.rendered(&view.plugin, &view.view) != view.shown
            });
            if stale {
                platform.request_frame(window);
            }
        }
    }

    /// Takes into `window`'s surfaces, of `scale` pixels a point, the trees
    /// that their plugins rendered since they took one: a view that shows
    /// nothing has an empty surface, a tree that the host refuses leaves
    /// the last, as the log tells, and the views that the window no longer
    /// shows go. The keyboard's focus follows its node's key, or leaves a
    /// node gone, or a view gone, to where it came from, and a field's
    /// composition that a render took away ends.
    pub(super) fn take_trees(&mut self, window: WindowId, scale: f32) {
        let views = self.slot_views(window);
        // The notice's card, of its revision, made where it changed.
        let revision = self.notice.revision;
        let card = super::notice_card::notice_view();
        let due = self.windows.get(&window).is_some_and(|state| {
            state.chrome.iter().any(|view| {
                view.plugin == OVERFLOW && view.view == card && view.shown != Some(revision)
            })
        });
        let notice = if due { self.notice_tree() } else { None };
        // The settings tab's tree, made as its view first shows.
        let settings_revision = self.settings.revision;
        let settings_due = self.windows.get(&window).is_some_and(|state| {
            state.chrome.iter().any(|view| {
                view.plugin == OVERFLOW
                    && view.view == super::settings_tab::settings_view()
                    && view.shown != Some(settings_revision)
            })
        });
        let settings = settings_due.then(|| self.settings_tree());
        let paneless = self.shows_settings(window);
        let Some(state) = self.windows.get_mut(&window) else {
            return;
        };
        let before = state.focus.target();
        let kept = std::mem::take(&mut state.chrome);
        let mut kept: Vec<Option<SlotView>> = kept.into_iter().map(Some).collect();
        for (plugin, view) in views {
            let found = kept
                .iter_mut()
                .find(|slot| {
                    slot.as_ref()
                        .is_some_and(|slot| slot.plugin == plugin && slot.view == view)
                })
                .and_then(Option::take);
            let mut slot = match found {
                Some(slot) => slot,
                None => {
                    state.next_view += 1;
                    let access = free_access(state.chrome.iter().chain(kept.iter().flatten()));
                    SlotView {
                        id: state.next_view,
                        access,
                        plugin: plugin.clone(),
                        view: view.clone(),
                        shown: None,
                        surface: Surface::new(scale),
                        frame: None,
                        placed: None,
                        reveal: None,
                        faces: 0,
                    }
                }
            };
            if (slot.surface.layout.scale() - scale).abs() > f32::EPSILON {
                slot.surface.layout.set_scale(&slot.surface.tree, scale);
            }
            // The app's button of the titlebar's buttons that find no room
            // is of one tree, its first.
            let rendered = if plugin == OVERFLOW {
                Some(if view == card {
                    revision
                } else if view == super::settings_tab::settings_view() {
                    settings_revision
                } else {
                    0
                })
            } else {
                self.plugins.rendered(&plugin, &view)
            };
            if rendered != slot.shown {
                let first = slot.shown.is_none();
                slot.shown = rendered;
                state.views_changed = true;
                let composing = slot.surface.fields.composing(&slot.surface.tree).is_some();
                let tree = if plugin == OVERFLOW && view == card {
                    notice.clone()
                } else if plugin == OVERFLOW && view == super::settings_tab::settings_view() {
                    settings.clone()
                } else if plugin == OVERFLOW {
                    Some(overflow_button(self.texts.text("title-buttons-more")))
                } else {
                    let tree = self.plugins.tree(&plugin, &view);
                    match view {
                        ViewId::TitleButtons(_) => tree.cloned().map(sized_title_button),
                        _ => tree.cloned(),
                    }
                };
                match tree {
                    Some(tree) => {
                        if let Err(error) = slot.surface.update(&tree) {
                            eprintln!(
                                "lokyr: the view {view:?} of `{plugin}` keeps its last tree, as its new one is not one: {error:?}"
                            );
                        }
                    }
                    None => slot.surface = Surface::new(scale),
                }
                state.focus.updated(slot.id, &slot.surface.tree);
                state.composition_dropped |=
                    composing && slot.surface.fields.composing(&slot.surface.tree).is_none();
                // A panel's first tree: a menu of menu items alone shows as
                // the platform's own, and draws nothing; one that takes the
                // keyboard, a menu's or a dialog's, takes the focus.
                if first && let Some(placed) = &slot.placed {
                    let role = placed.options.role;
                    if role == Role::Menu && panel::native_menu(&slot.surface.tree).is_some() {
                        state.panels.close(slot.id);
                        state.native_due.push(slot.id);
                    } else if role.takes_focus() {
                        state.focus.first(slot.id, &slot.surface.tree);
                    }
                }
            }
            state.chrome.push(slot);
        }
        // The views that the window no longer shows: the focus that one had
        // goes back to where its view took it from, or to the pane.
        for gone in kept.into_iter().flatten() {
            state.views_changed = true;
            state.composition_dropped |=
                gone.surface.fields.composing(&gone.surface.tree).is_some();
            let (focus, chrome) = (&mut state.focus, &state.chrome);
            focus.closed(gone.id, |id| {
                chrome
                    .iter()
                    .find(|view| view.id == id)
                    .map(|view| &view.surface.tree)
            });
            if state.captured == Some(gone.id) {
                state.captured = None;
            }
            if state.over == Some(gone.id) {
                state.over = None;
            }
        }
        // The settings' window has no pane: the focus on none, as the
        // settings open, or gone back as a page's view went, goes to the
        // page's first field.
        if paneless {
            focus_page(state);
        }
        // A move of the focus that a render made, as back to the pane of a
        // field gone, or to a panel's field as the panel shows, the pane's
        // program learns of; the composition of where the focus was ends
        // as it stands, which the input method forgets.
        let mut events = Vec::new();
        if state.focus.target() != before {
            state.composition_dropped |=
                super::surfaces::end_composition(state, before, &mut events);
            super::surfaces::tell_pane(&self.workspace, state, before);
        }
        events.extend(super::surfaces::focus_events(state));
        self.ui_events.extend(events);
        // Tab goes through the settings tab's sidebar and its page as one.
        let id_of = |found: &dyn Fn(&SlotView) -> bool| {
            state
                .chrome
                .iter()
                .find(|view| found(view))
                .map(|view| view.id)
        };
        let settings = id_of(&|view| {
            view.plugin == OVERFLOW && view.view == super::settings_tab::settings_view()
        });
        let page = id_of(&|view| matches!(view.view, ViewId::Page(_)));
        match (settings, page) {
            (Some(settings), Some(page)) => state.focus.set_chain(&[settings, page]),
            _ => state.focus.set_chain(&[]),
        }
    }

    /// Lays out `window`'s slots' views in a window of `size`, as `ui-core`'s
    /// chrome places them, but the panes' headers, which the panes' rooms
    /// take: the regions of the slots, and the room that the panes share;
    /// `None` before the app draws. The events of the lists' rows that ask
    /// for their items go to their plugins as the turn ends.
    pub(super) fn lay_out_chrome(&mut self, window: WindowId, size: WindowSize) -> Option<Regions> {
        let drawing = self.drawing.as_mut()?;
        let state = self.windows.get_mut(&window)?;
        let font = &mut drawing.font;
        let width = size.pixels.width as f32;
        let titlebar = size.bar_pixels() as f32;
        let mut events = Vec::new();
        let tab_bar_shows = has_tab_bar(&state.chrome);
        // The place that the bar's plugin gives it, a column's width in
        // pixels.
        let place = match tab_bar_place(&state.chrome, &self.plugins) {
            BarPlace::Titlebar => TabBarPlace::Titlebar,
            BarPlace::BelowTitlebar => TabBarPlace::BelowTitlebar,
            BarPlace::Bottom => TabBarPlace::Bottom,
            BarPlace::Left { width } => TabBarPlace::Left(width * size.scale as f32),
            BarPlace::Right { width } => TabBarPlace::Right(width * size.scale as f32),
            BarPlace::Sidebar { width } => TabBarPlace::Sidebar(width * size.scale as f32),
            BarPlace::Title => TabBarPlace::Title,
        };
        // The titlebar's buttons that show, three at most, of the plugins'
        // order: of more, two, and the app's button of the rest.
        let scale = size.scale as f32;
        let shown_buttons = title_buttons_shown(&state.chrome);
        let room_across = (TITLE_BUTTON_ROOM * scale).round();
        // The views' lengths of the titlebar and of the window's buttons,
        // as a sidebar leaves them their room; a view through the titlebar,
        // a sidebar's or a title's, leaves the plugins' buttons theirs too,
        // which lie right after the window's.
        let room = Room {
            titlebar,
            buttons: (size.buttons * size.scale) as f32,
        };
        let through = Room {
            buttons: room.buttons + shown_buttons.len() as f32 * room_across,
            ..room
        };
        let crosses = matches!(place, TabBarPlace::Sidebar(_) | TabBarPlace::Title);
        for view in &mut state.chrome {
            let tab_bar = view.view.slot() == Some(Slot::TabBar);
            view.surface
                .set_room(if crosses && tab_bar { through } else { room });
        }
        // Each slot's height, of its views laid out across the window, or
        // its share of it for a slot of many.
        let mut natural = |slot: Slot| {
            let mut views: Vec<&mut SlotView> = state
                .chrome
                .iter_mut()
                .filter(|view| view.view.slot() == Some(slot) && view.shows())
                .collect();
            let share = width / views.len().max(1) as f32;
            views
                .iter_mut()
                .map(|view| {
                    let laid = view.surface.lay_out((Some(share), None), font);
                    events.extend(
                        laid.into_iter()
                            .map(|event| (view.plugin.clone(), view.view.clone(), event)),
                    );
                    view.surface.size().map_or(0.0, |size| size.1)
                })
                .fold(0.0_f32, f32::max)
        };
        // The titlebar's height in the titlebar, a column's whole height,
        // and across the window its view's.
        let tab_bar = match place {
            _ if !tab_bar_shows => 0.0,
            TabBarPlace::Titlebar | TabBarPlace::Title if titlebar > 0.0 => titlebar,
            TabBarPlace::Left(_) | TabBarPlace::Right(_) | TabBarPlace::Sidebar(_) => 1.0,
            _ => natural(Slot::TabBar),
        };
        let window_strip = natural(Slot::WindowStrip);
        let status_bar = natural(Slot::StatusBar);
        let heights = Heights {
            tab_bar,
            window_strip,
            status_bar,
            title_buttons: shown_buttons.len() as f32 * (TITLE_BUTTON_ROOM * scale).round(),
        };
        let shape = chrome::Window {
            size: (width, size.pixels.height as f32),
            titlebar,
            buttons: room.buttons,
        };
        let regions = chrome::regions(&shape, place, heights);
        state.overlay = regions.window_overlay;
        // The buttons side by side, each a square in the middle of its room
        // and of the titlebar's height; the others hide.
        let side = (TITLE_BUTTON * scale).round();
        for view in state
            .chrome
            .iter_mut()
            .filter(|view| view.view.slot() == Some(Slot::TitleButtons))
        {
            view.frame = None;
            let (Some(region), Some(place)) = (
                regions.title_buttons,
                shown_buttons.iter().position(|id| *id == view.id),
            ) else {
                continue;
            };
            let frame = Frame {
                x: region.x + room_across * place as f32 + ((room_across - side) / 2.0).round(),
                y: region.y + ((region.height - side) / 2.0).round(),
                width: side,
                height: side,
            };
            view.frame = Some(frame);
            let laid = view
                .surface
                .lay_out((Some(frame.width), Some(frame.height)), font);
            events.extend(
                laid.into_iter()
                    .map(|event| (view.plugin.clone(), view.view.clone(), event)),
            );
        }
        for slot in [Slot::TabBar, Slot::WindowStrip, Slot::StatusBar] {
            let region = match slot {
                Slot::TabBar => regions.tab_bar,
                Slot::WindowStrip => regions.window_strip,
                Slot::StatusBar => regions.status_bar,
                Slot::PaneHeader | Slot::TitleButtons => None,
            };
            let mut views: Vec<&mut SlotView> = state
                .chrome
                .iter_mut()
                .filter(|view| view.view.slot() == Some(slot))
                .collect();
            let count = views.iter().filter(|view| view.shows()).count().max(1);
            let mut shown = 0;
            for view in &mut views {
                view.frame = region.filter(|_| view.shows()).map(|region| {
                    let share = region.width / count as f32;
                    let frame = Frame {
                        x: region.x + share * shown as f32,
                        width: share,
                        ..region
                    };
                    shown += 1;
                    frame
                });
                if let Some(frame) = view.frame {
                    let laid = view
                        .surface
                        .lay_out((Some(frame.width), Some(frame.height)), font);
                    view.settle_reveal();
                    events.extend(
                        laid.into_iter()
                            .map(|event| (view.plugin.clone(), view.view.clone(), event)),
                    );
                }
            }
        }
        self.ui_events.extend(
            events
                .into_iter()
                .map(|(plugin, view, event)| (plugin, ui_event(view, event))),
        );
        Some(regions)
    }

    /// Gives the panes' rooms of `window`, in pixels, their headers: each
    /// room's header, of its view laid out across it, at its top, and the
    /// grid's room below it, which the room becomes.
    pub(super) fn lay_out_headers(&mut self, window: WindowId, rooms: &mut [(PaneId, Rect)]) {
        let (Some(drawing), Some(state)) = (self.drawing.as_mut(), self.windows.get_mut(&window))
        else {
            return;
        };
        let font = &mut drawing.font;
        let mut events = Vec::new();
        for view in &mut state.chrome {
            if view.view.slot() == Some(Slot::PaneHeader) {
                view.frame = None;
            }
        }
        for (pane, rect) in rooms.iter_mut() {
            let header = ViewId::PaneHeader(host::PaneId::from(*pane));
            let Some(view) = state
                .chrome
                .iter_mut()
                .find(|view| view.view == header && view.shows())
            else {
                continue;
            };
            let room = frame_of(*rect);
            let laid = view.surface.lay_out((Some(room.width), None), font);
            events.extend(
                laid.into_iter()
                    .map(|event| (view.plugin.clone(), view.view.clone(), event)),
            );
            let height = view.surface.size().map_or(0.0, |size| size.1);
            let regions = chrome::pane_regions(room, height);
            view.frame = regions.header;
            if let Some(frame) = regions.header {
                let laid = view
                    .surface
                    .lay_out((Some(frame.width), Some(frame.height)), font);
                events.extend(
                    laid.into_iter()
                        .map(|event| (view.plugin.clone(), view.view.clone(), event)),
                );
            }
            *rect = rect_of(regions.grid);
        }
        self.ui_events.extend(
            events
                .into_iter()
                .map(|(plugin, view, event)| (plugin, ui_event(view, event))),
        );
    }
}

impl<B: Backend> App<B> {
    /// `window`'s slots' views took their places: the hover follows the
    /// nodes that the layout moved under a pointer that stayed, as a tab
    /// that slides under it as the one before it closes, whose events go to
    /// their plugins as the turn ends; the field with the keyboard's focus
    /// scrolls across to its caret, the others to their starts.
    pub(super) fn settle_views(&mut self, window: WindowId) {
        let (Some(drawing), Some(state)) = (self.drawing.as_mut(), self.windows.get_mut(&window))
        else {
            return;
        };
        let focused = match state.focus.target() {
            Target::Node { panel, node } => Some((panel, node)),
            Target::Pane => None,
        };
        let mut events = Vec::new();
        for view in &mut state.chrome {
            if view.frame.is_some() {
                let moved = view.surface.laid_out();
                events.extend(
                    moved
                        .into_iter()
                        .map(|event| (view.plugin.clone(), ui_event(view.view.clone(), event))),
                );
            }
            let node = focused.and_then(|(id, node)| (id == view.id).then_some(node));
            view.surface.follow(node, &mut drawing.font);
        }
        self.ui_events.extend(events);
    }
}

/// The lowest number of accessibility that no view of `views` has.
pub(super) fn free_access<'v>(views: impl Iterator<Item = &'v SlotView>) -> u8 {
    let taken: Vec<u8> = views.map(|view| view.access).collect();
    (0..=u8::MAX)
        .find(|number| !taken.contains(number))
        .unwrap_or(u8::MAX)
}

/// The event of the plugin's that `event`, of a node of `view`, is.
pub(super) fn ui_event(view: ViewId, event: NodeEvent) -> UiEvent {
    UiEvent {
        view,
        key: event.key,
        kind: event.kind,
    }
}

/// The items of the slots' views of `chrome`, each at its frame, over the
/// panes, of `tokens`, in a window of `room` pixels, of the dark appearance
/// or the light one, of the focus's ring round `focused`, a node of a view
/// by its id, of their transitions at `now`.
pub(super) fn paint<F: FontBackend>(
    chrome: &[SlotView],
    font: &mut Font<F>,
    tokens: &Tokens,
    room: (f32, f32),
    (dark, focused, now): (bool, Option<(u32, NodeId)>, Instant),
    pictures: &BTreeMap<String, Pictures>,
) -> Vec<Item> {
    chrome
        .iter()
        .filter_map(|view| Some((view, view.frame?)))
        .flat_map(|(view, frame)| {
            let focused = focused.and_then(|(id, node)| (id == view.id).then_some(node));
            view.surface.paint(
                font,
                tokens,
                ((frame.x, frame.y), room),
                (dark, focused, now),
                pictures.get(&view.plugin).unwrap_or(&NO_PICTURES),
            )
        })
        .collect()
}

/// The progresses of `view` that something drawn over them covers, whose
/// fills and moving parts its frame draws, under what covers them, rather
/// than the window server over the frame, of `tokens`, at `now`: those
/// under a panel of `panels` above the view, or under one of `tooltips`,
/// their boxes in the window's pixels, and, while a node of its own is
/// dragged among its siblings, those of every other node, which it goes
/// over.
pub(super) fn covered(
    view: &SlotView,
    panels: &Panels<u32>,
    tooltips: &[Rect],
    tokens: &Tokens,
    now: Instant,
) -> BTreeSet<NodeId> {
    let Some(frame) = view.frame else {
        return BTreeSet::new();
    };
    let moved = |rect: Rect| Rect {
        x: rect.x + frame.x as i32,
        y: rect.y + frame.y as i32,
        ..rect
    };
    let whole = Rect {
        x: frame.x as i32,
        y: frame.y as i32,
        width: frame.width as u32,
        height: frame.height as u32,
    };
    let below = view.placed.is_some().then_some(view.id);
    let lifted = view.surface.lifted_node();
    let tooltips: Vec<Rect> = tooltips
        .iter()
        .copied()
        .filter(|&tooltip| overlaps(tooltip, whole))
        .collect();
    // Nothing over the view, of no walk of its tree.
    if lifted.is_none() && tooltips.is_empty() && !panels.covers(whole, below) {
        return BTreeSet::new();
    }
    let tree = &view.surface.tree;
    let within_lifted = |node: NodeId| {
        let mut at = Some(node);
        while let Some(node) = at {
            if Some(node) == lifted {
                return true;
            }
            at = tree.parent(node);
        }
        false
    };
    let fills = view
        .surface
        .fills(tokens, now)
        .into_iter()
        .map(|area| (area.node, area.visible));
    let sweeps = view
        .surface
        .sweeps(tokens, now)
        .into_iter()
        .map(|area| (area.node, area.visible));
    fills
        .chain(sweeps)
        .filter(|&(node, visible)| {
            let shown = moved(visible);
            panels.covers(shown, below)
                || tooltips.iter().any(|&tooltip| overlaps(tooltip, shown))
                || lifted.is_some() && !within_lifted(node)
        })
        .map(|(node, _)| node)
        .collect()
}

/// Whether `a` and `b`, in pixels, share any of their area.
fn overlaps(a: Rect, b: Rect) -> bool {
    let (a_right, b_right) = (
        i64::from(a.x) + i64::from(a.width),
        i64::from(b.x) + i64::from(b.width),
    );
    let (a_bottom, b_bottom) = (
        i64::from(a.y) + i64::from(a.height),
        i64::from(b.y) + i64::from(b.height),
    );
    i64::from(a.x) < b_right
        && i64::from(b.x) < a_right
        && i64::from(a.y) < b_bottom
        && i64::from(b.y) < a_bottom
}

/// The indeterminate progresses of the slots' views and the panels of
/// `chrome`, which the window server moves, of `tokens`, at `now`, on a
/// display of `scale`, of their sweeps' `start`, but those that something
/// drawn over them covers, as each view's `covered` says, which its frame
/// draws.
pub(super) fn sweeps(
    chrome: &[SlotView],
    tokens: &Tokens,
    (now, start): (Instant, Instant),
    scale: f64,
) -> Vec<Sweep> {
    let points = |pixels: f32| f64::from(pixels) / scale;
    let place = |rect: Rect, (x, y): (f32, f32)| {
        (
            Point {
                x: points(rect.x as f32 + x),
                y: points(rect.y as f32 + y),
            },
            Size {
                width: points(rect.width as f32),
                height: points(rect.height as f32),
            },
        )
    };
    chrome
        .iter()
        .filter_map(|view| Some((view, view.frame?)))
        .flat_map(|(view, frame)| {
            view.surface
                .sweeps(tokens, now)
                .into_iter()
                .filter(|area| !view.surface.covered.contains(&area.node))
                .map(move |area: SweepArea| {
                    let at = (frame.x, frame.y);
                    let (origin, size) = place(area.rect, at);
                    Sweep {
                        origin,
                        size,
                        within: place(area.visible, at),
                        radius: points(area.radius),
                        shape: match area.shape {
                            progress::SweepShape::Bar { width } => SweepShape::Bar {
                                width: points(width),
                            },
                            progress::SweepShape::Ring { span, thickness } => SweepShape::Ring {
                                span: f64::from(span),
                                thickness: points(thickness),
                            },
                            progress::SweepShape::Outline { span, thickness } => {
                                SweepShape::Outline {
                                    span: f64::from(span),
                                    thickness: points(thickness),
                                }
                            }
                            progress::SweepShape::Dots { diameter, centers } => SweepShape::Dots {
                                diameter: points(diameter),
                                centers: centers.map(|(x, y)| (points(x), points(y))),
                            },
                        },
                        color: [
                            f64::from(area.color.red),
                            f64::from(area.color.green),
                            f64::from(area.color.blue),
                            f64::from(area.color.alpha),
                        ],
                        period: area.period,
                        start,
                    }
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

/// The fills of the progresses of the slots' views and the panels of
/// `chrome` that are not indeterminate, which the window server draws, of
/// `tokens`, at `now`, on a display of `scale`, but those that something
/// drawn over them covers, as each view's `covered` says, which its frame
/// draws: each of an id of its view and its node, which stays as their
/// renders keep the node.
pub(super) fn fills(chrome: &[SlotView], tokens: &Tokens, now: Instant, scale: f64) -> Vec<Fill> {
    let points = |pixels: f32| f64::from(pixels) / scale;
    let place = |rect: Rect, (x, y): (f32, f32)| {
        (
            Point {
                x: points(rect.x as f32 + x),
                y: points(rect.y as f32 + y),
            },
            Size {
                width: points(rect.width as f32),
                height: points(rect.height as f32),
            },
        )
    };
    chrome
        .iter()
        .filter_map(|view| Some((view, view.frame?)))
        .flat_map(|(view, frame)| {
            view.surface
                .fills(tokens, now)
                .into_iter()
                .filter(|area| !view.surface.covered.contains(&area.node))
                .map(move |area: FillArea| {
                    let at = (frame.x, frame.y);
                    let (origin, size) = place(area.rect, at);
                    Fill {
                        id: (u64::from(view.id) << 48) ^ area.node.bits(),
                        origin,
                        size,
                        within: place(area.visible, at),
                        radius: points(area.radius),
                        shape: match area.shape {
                            progress::FillShape::Bar => FillShape::Bar,
                            progress::FillShape::Ring { thickness } => FillShape::Ring {
                                thickness: points(thickness),
                            },
                            progress::FillShape::Outline { thickness } => FillShape::Outline {
                                thickness: points(thickness),
                            },
                        },
                        color: [
                            f64::from(area.color.red),
                            f64::from(area.color.green),
                            f64::from(area.color.blue),
                            f64::from(area.color.alpha),
                        ],
                        value: f64::from(area.value),
                    }
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

/// Whether `chrome`'s tab bar shows a view.
pub(super) fn has_tab_bar(chrome: &[SlotView]) -> bool {
    chrome
        .iter()
        .any(|view| view.view.slot() == Some(Slot::TabBar) && view.shows())
}

/// The room either side of the inner edge of a tab bar's column, in
/// points, that takes the edge's drag.
const EDGE: f64 = 3.0;

/// The inner edge of `chrome`'s tab bar's column, as its frame and its
/// plugin's place, of `plugins`, have it: its plugin, whether the column
/// is on the left, and the room around the edge, in pixels of `scale`,
/// that takes its drag; `None` but for a column that shows.
pub(super) fn column_edge(
    chrome: &[SlotView],
    plugins: &Plugins,
    scale: f64,
) -> Option<(String, bool, Frame)> {
    let view = chrome
        .iter()
        .find(|view| view.view.slot() == Some(Slot::TabBar) && view.shows())?;
    let frame = view.frame?;
    let left = match bar_place_of(view, plugins) {
        BarPlace::Left { .. } | BarPlace::Sidebar { .. } => true,
        BarPlace::Right { .. } => false,
        _ => return None,
    };
    let edge = if left { frame.x + frame.width } else { frame.x };
    let half = (EDGE * scale) as f32;
    let zone = Frame {
        x: edge - half,
        y: frame.y,
        width: 2.0 * half,
        height: frame.height,
    };
    Some((view.plugin.clone(), left, zone))
}

/// Where `chrome`'s tab bar goes, as its plugin, of `plugins`, gives it; in
/// the titlebar without one.
pub(super) fn tab_bar_place(chrome: &[SlotView], plugins: &Plugins) -> BarPlace {
    chrome
        .iter()
        .find(|view| view.view.slot() == Some(Slot::TabBar))
        .map(|view| bar_place_of(view, plugins))
        .unwrap_or_default()
}

/// Where the tab bar of `view` goes, as its plugin, of `plugins`, gives it
/// its window, or every window.
fn bar_place_of(view: &SlotView, plugins: &Plugins) -> BarPlace {
    match &view.view {
        ViewId::TabBar(window) => plugins.window_tab_bar_place(&view.plugin, *window),
        _ => plugins.tab_bar_place(&view.plugin),
    }
}

/// Whether a transition of `chrome`'s views runs at `now`.
pub(super) fn animating(chrome: &[SlotView], now: Instant) -> bool {
    chrome.iter().any(|view| view.surface.animating(now))
}

/// The frame of `rect`.
fn frame_of(rect: Rect) -> Frame {
    Frame {
        x: rect.x as f32,
        y: rect.y as f32,
        width: rect.width as f32,
        height: rect.height as f32,
    }
}

/// The rectangle of `frame`, its edges at whole pixels.
pub(super) fn rect_of(frame: Frame) -> Rect {
    let (left, top) = (frame.x.round(), frame.y.round());
    let (right, bottom) = (
        (frame.x + frame.width).round(),
        (frame.y + frame.height).round(),
    );
    Rect {
        x: left as i32,
        y: top as i32,
        width: (right - left).max(0.0) as u32,
        height: (bottom - top).max(0.0) as u32,
    }
}

/// The ids of the titlebar's buttons of `chrome` that show, in their order:
/// every plugin's that has a tree, three at most, or of more, the first two
/// and the app's button of the rest.
pub(super) fn title_buttons_shown(chrome: &[SlotView]) -> Vec<u32> {
    let buttons = || {
        chrome
            .iter()
            .filter(|view| view.view.slot() == Some(Slot::TitleButtons) && view.shows())
    };
    let plugins: Vec<u32> = buttons()
        .filter(|view| view.plugin != OVERFLOW)
        .map(|view| view.id)
        .collect();
    if plugins.len() <= MOST_TITLE_BUTTONS {
        return plugins;
    }
    let mut shown = plugins[..MOST_TITLE_BUTTONS - 1].to_vec();
    shown.extend(
        buttons()
            .filter(|view| view.plugin == OVERFLOW)
            .map(|view| view.id),
    );
    shown
}

/// `button`, a plugin's titlebar button, of the host's size: a square of
/// [`TITLE_BUTTON`] points, its icon in its middle.
fn sized_title_button(mut button: Node) -> Node {
    button.layout = NodeLayout {
        width: Length::Points(TITLE_BUTTON),
        min_width: Length::Points(TITLE_BUTTON),
        height: Length::Points(TITLE_BUTTON),
        justify: Justify::Center,
        align: Align::Center,
        ..NodeLayout::default()
    };
    button
}

/// The app's button of the titlebar's buttons that find no room, of the
/// name `more`: an ellipsis, of no box at rest.
fn overflow_button(more: String) -> Node {
    sized_title_button(
        Node::icon_button("ellipsis")
            .key("overflow")
            .label(more.clone())
            .tooltip(more)
            .style(Style {
                background: Some(Token::from("transparent")),
                radius: 5.0,
                hover: Some(Hover {
                    background: Some(Token::from("button.hover.bg")),
                    ..Hover::default()
                }),
                pressed: Some(Hover {
                    background: Some(Token::from("button.pressed.bg")),
                    ..Hover::default()
                }),
                transition: Some(Transition {
                    duration: FADE,
                    ..Transition::default()
                }),
                ..Style::default()
            }),
    )
}

impl<B: Backend> App<B> {
    /// Shows the menu of `window`'s titlebar buttons that find no room, below
    /// the app's button of them: each by its name, dimmed where it is, whose
    /// choice its plugin hears as its button's click.
    pub(super) fn show_overflow(&mut self, platform: &mut dyn Platform, window: WindowId) {
        self.release_capture(platform, window);
        let Some(state) = self.windows.get_mut(&window) else {
            return;
        };
        let Some((frame, scale)) = state
            .chrome
            .iter()
            .find(|view| view.plugin == OVERFLOW && view.view.slot() == Some(Slot::TitleButtons))
            .and_then(|view| Some((view.frame?, view.surface.layout.scale())))
        else {
            return;
        };
        let mut menu = ContextMenu::default();
        let mut choices = BTreeMap::new();
        let hidden = state.chrome.iter().filter(|view| {
            view.view.slot() == Some(Slot::TitleButtons)
                && view.plugin != OVERFLOW
                && view.shows()
                && view.frame.is_none()
        });
        for view in hidden {
            let Some(button) = self.plugins.tree(&view.plugin, &view.view) else {
                continue;
            };
            let title = [&button.tooltip, &button.label]
                .into_iter()
                .flatten()
                .find(|name| !name.trim().is_empty())
                .cloned()
                .unwrap_or_default();
            let id = format!("{OVERFLOWED}{}", view.id);
            let disabled = matches!(
                &button.kind,
                lokyr_kernel::plugins::view::Kind::Button(button) if button.disabled
            );
            if disabled {
                menu.dimmed.push(id.clone());
            }
            menu.items.push(MenuItem::Command {
                title,
                command: id.clone(),
                shortcut: None,
            });
            choices.insert(id, view.id);
        }
        if menu.items.is_empty() {
            return;
        }
        let position = Point {
            x: f64::from(frame.x / scale),
            y: f64::from((frame.y + frame.height) / scale),
        };
        platform.show_context_menu(window, position, &menu);
        state.native.push_back(NativeMenu::Overflow(choices));
    }

    /// The event of the app's button of the titlebar's buttons that find no
    /// room, of `view`: its click shows their menu.
    pub(super) fn overflow_event(&mut self, platform: &mut dyn Platform, event: &UiEvent) {
        let ViewId::TitleButtons(shown) = event.view else {
            return;
        };
        if !matches!(event.kind, UiEventKind::Click) {
            return;
        }
        let window = self
            .windows
            .iter()
            .find(|(_, state)| host::WindowId::from(state.window) == shown)
            .map(|(window, _)| *window);
        if let Some(window) = window {
            self.show_overflow(platform, window);
        }
    }

    /// The item `command` of the menu of `window`'s titlebar buttons that
    /// find no room, of `choices`: its button's click, which its plugin
    /// hears.
    pub(super) fn overflow_chosen(
        &mut self,
        window: WindowId,
        choices: &BTreeMap<String, u32>,
        command: &str,
    ) {
        let Some(state) = self.windows.get(&window) else {
            return;
        };
        let chosen = choices
            .get(command)
            .and_then(|id| state.chrome.iter().find(|view| view.id == *id));
        let Some(view) = chosen else {
            return;
        };
        let Some(key) = self
            .plugins
            .tree(&view.plugin, &view.view)
            .and_then(|button| button.key.clone())
        else {
            return;
        };
        let event = UiEvent {
            view: view.view.clone(),
            key,
            kind: UiEventKind::Click,
        };
        self.ui_events.push((view.plugin.clone(), event));
    }
}

/// Gives the focus of the settings' window, `state`'s, which has no pane, to
/// its page's first field, or its first node that takes the keyboard, where
/// it is on no node, as on the pane that the window lacks.
pub(super) fn focus_page<B: crate::drawing::Backend>(state: &mut crate::window::Window<B>) {
    if state.focus.target() != Target::Pane {
        return;
    }
    if let Some(page) = state
        .chrome
        .iter()
        .find(|view| matches!(view.view, ViewId::Page(_)))
    {
        state.focus.first(page.id, &page.surface.tree);
    }
}
