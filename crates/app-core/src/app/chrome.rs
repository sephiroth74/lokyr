//! The window's chrome: the views of the plugins that fill its slots, its
//! tab bar, its strip, its status bar and its panes' headers, each a
//! surface of its own, laid out around and over the panes as `ui-core`'s
//! chrome places them, and painted over the panes in the window's one
//! drawable, of the theme's tokens.

use std::fmt;
use std::time::Instant;

use lokyr_kernel::plugins::Plugins;
use lokyr_kernel::plugins::host::{self, BarPlace};
use lokyr_kernel::plugins::ui::{PanelId, PanelOptions, Role, Slot, UiEvent, ViewId};
use lokyr_kernel::workspace::PaneId;
use lokyr_platform_core::{Platform, Point, Size, Sweep, SweepShape, WindowId, WindowSize};
use lokyr_render_core::{Item, Rect};
use lokyr_text_core::FontBackend;
use lokyr_ui_core::chrome::{self, Heights, Regions, TabBarPlace};
use lokyr_ui_core::focus::Target;
use lokyr_ui_core::font::Font;
use lokyr_ui_core::layout::Frame;
use lokyr_ui_core::panel::{self, Panels};
use lokyr_ui_core::pointer::NodeEvent;
use lokyr_ui_core::progress::{self, SweepArea};
use lokyr_ui_core::surface::Surface;
use lokyr_ui_core::theme::Tokens;
use lokyr_ui_core::tree::NodeId;

use super::App;
use crate::Backend;
use crate::window::NativeMenu;

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
    /// Whether it has a tree to show.
    fn shows(&self) -> bool {
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
        }
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
    /// status bar's of each plugin that fills them, in their order; and the
    /// tab bar's, of the config's plugin or the first.
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
        let slots = &self.config.slots;
        let one = |slot, chosen: &Option<String>| {
            self.plugins
                .provider(slot, chosen.as_deref().unwrap_or(""))
                .map(str::to_owned)
        };
        let mut views = Vec::new();
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
        // The panels over them, from the lowest, a native menu's last.
        let panel = |id: u32| state.chrome.iter().find(|view| view.id == id);
        let placed = state.panels.order().filter_map(panel);
        let native = state
            .native
            .iter()
            .filter_map(|menu| match menu {
                NativeMenu::Panel(id, _) => Some(*id),
                NativeMenu::Window | NativeMenu::Closed => None,
            })
            .chain(state.native_due.iter().copied())
            .filter_map(panel)
            .filter(|view| !state.panels.is_open(view.id));
        for view in placed.chain(native) {
            views.push((view.plugin.clone(), view.view.clone()));
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
            .filter(|(plugin, view)| self.plugins.rendered(plugin, view).is_none())
            .collect();
        let mut rendered = false;
        for (plugin, view) in &due {
            let result = self.with_plugins(platform, |plugins, context| {
                plugins.render(plugin, view, context)
            });
            if let Err(error) = result {
                eprintln!("lokyr: {error}");
            }
            rendered |= self.plugins.rendered(plugin, view).is_some();
        }
        rendered
    }

    /// Asks for a frame of each window that shows a view whose plugin
    /// rendered it anew since the window took its tree.
    pub(super) fn frame_rendered(&self, platform: &mut dyn Platform) {
        for (&window, state) in &self.windows {
            let stale = state
                .chrome
                .iter()
                .any(|view| self.plugins.rendered(&view.plugin, &view.view) != view.shown);
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
                    }
                }
            };
            if (slot.surface.layout.scale() - scale).abs() > f32::EPSILON {
                slot.surface.layout.set_scale(&slot.surface.tree, scale);
            }
            let rendered = self.plugins.rendered(&plugin, &view);
            if rendered != slot.shown {
                let first = slot.shown.is_none();
                slot.shown = rendered;
                state.views_changed = true;
                let composing = slot.surface.fields.composing(&slot.surface.tree).is_some();
                match self.plugins.tree(&plugin, &view) {
                    Some(tree) => {
                        if let Err(error) = slot.surface.update(tree) {
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
                // keyboard takes the focus.
                if first && let Some(placed) = &slot.placed {
                    let role = placed.options.role;
                    if role == Role::Menu && panel::native_menu(&slot.surface.tree).is_some() {
                        state.panels.close(slot.id);
                        state.native_due.push(slot.id);
                    } else if role != Role::Tooltip {
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
        };
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
            TabBarPlace::Titlebar if titlebar > 0.0 => titlebar,
            TabBarPlace::Left(_) | TabBarPlace::Right(_) => 1.0,
            _ => natural(Slot::TabBar),
        };
        let heights = Heights {
            tab_bar,
            window_strip: natural(Slot::WindowStrip),
            status_bar: natural(Slot::StatusBar),
        };
        let shape = chrome::Window {
            size: (width, size.pixels.height as f32),
            titlebar,
            buttons: (size.buttons * size.scale) as f32,
        };
        let regions = chrome::regions(&shape, place, heights);
        state.overlay = regions.window_overlay;
        for slot in [Slot::TabBar, Slot::WindowStrip, Slot::StatusBar] {
            let region = match slot {
                Slot::TabBar => regions.tab_bar,
                Slot::WindowStrip => regions.window_strip,
                Slot::StatusBar => regions.status_bar,
                Slot::PaneHeader => None,
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
            )
        })
        .collect()
}

/// The indeterminate progresses of the slots' views and the panels of
/// `chrome`, which the window server moves, of `tokens`, at `now`, on a
/// display of `scale`, of their sweeps' `start`, but those that a panel
/// of `panels` over them covers.
pub(super) fn sweeps(
    chrome: &[SlotView],
    panels: &Panels<u32>,
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
            let below = view.placed.is_some().then_some(view.id);
            let moved = move |rect: Rect| Rect {
                x: rect.x + frame.x as i32,
                y: rect.y + frame.y as i32,
                ..rect
            };
            view.surface
                .sweeps(tokens, now)
                .into_iter()
                .filter(move |area| !panels.covers(moved(area.visible), below))
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

/// Whether `chrome`'s tab bar shows a view.
pub(super) fn has_tab_bar(chrome: &[SlotView]) -> bool {
    chrome
        .iter()
        .any(|view| view.view.slot() == Some(Slot::TabBar) && view.shows())
}

/// Where `chrome`'s tab bar goes, as its plugin, of `plugins`, gives it; in
/// the titlebar without one.
pub(super) fn tab_bar_place(chrome: &[SlotView], plugins: &Plugins) -> BarPlace {
    chrome
        .iter()
        .find(|view| view.view.slot() == Some(Slot::TabBar))
        .map(|view| plugins.tab_bar_place(&view.plugin))
        .unwrap_or_default()
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
