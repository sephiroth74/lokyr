//! The window's chrome: the views of the plugins that fill its slots, its
//! tab bar, its strip, its status bar and its panes' headers, each a
//! surface of its own, laid out around and over the panes as `ui-core`'s
//! chrome places them, and painted over the panes in the window's one
//! drawable, of the theme's tokens.

use std::fmt;
use std::time::Instant;

use lokyr_kernel::plugins::host;
use lokyr_kernel::plugins::ui::{Slot, UiEvent, ViewId};
use lokyr_kernel::workspace::PaneId;
use lokyr_platform_core::{Platform, Point, Size, Sweep, SweepShape, WindowId, WindowSize};
use lokyr_render_core::{Item, Rect};
use lokyr_text_core::FontBackend;
use lokyr_ui_core::chrome::{self, Heights, Regions, TabBarPlace};
use lokyr_ui_core::font::Font;
use lokyr_ui_core::layout::Frame;
use lokyr_ui_core::pointer::NodeEvent;
use lokyr_ui_core::progress::{self, SweepArea};
use lokyr_ui_core::surface::Surface;
use lokyr_ui_core::theme::Tokens;

use super::App;
use crate::Backend;

/// A view of a plugin's that a window shows in one of its slots: its
/// surface, the number of the render that the surface holds, and where it
/// shows, once laid out.
pub(crate) struct SlotView {
    /// The plugin.
    pub(crate) plugin: String,
    /// The view.
    pub(crate) view: ViewId,
    /// The number of the render that the surface holds; `None` before one.
    shown: Option<u64>,
    /// The view's tree as the window shows it.
    pub(crate) surface: Surface,
    /// Where it shows, in pixels from the content's corner; `None` for a
    /// view that shows nothing.
    pub(crate) frame: Option<Frame>,
}

impl fmt::Debug for SlotView {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SlotView")
            .field("plugin", &self.plugin)
            .field("view", &self.view)
            .field("shown", &self.shown)
            .field("frame", &self.frame)
            .finish_non_exhaustive()
    }
}

impl SlotView {
    /// Whether it has a tree to show.
    fn shows(&self) -> bool {
        self.surface.tree.root().is_some()
    }
}

/// Where the tab bar goes: in the titlebar, past the window's buttons, or at
/// the window's top where it has no titlebar.
const PLACE: TabBarPlace = TabBarPlace::Titlebar;

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

    /// Takes into `window`'s surfaces, of `scale` pixels a point, the trees
    /// that their plugins rendered since they took one: a view that shows
    /// nothing has an empty surface, a tree that the host refuses leaves
    /// the last, as the log tells, and the views that the window no longer
    /// shows go.
    pub(super) fn take_trees(&mut self, window: WindowId, scale: f32) {
        let views = self.slot_views(window);
        let Some(state) = self.windows.get_mut(&window) else {
            return;
        };
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
            let mut slot = found.unwrap_or_else(|| SlotView {
                plugin: plugin.clone(),
                view: view.clone(),
                shown: None,
                surface: Surface::new(scale),
                frame: None,
            });
            if (slot.surface.layout.scale() - scale).abs() > f32::EPSILON {
                slot.surface.layout.set_scale(&slot.surface.tree, scale);
            }
            let rendered = self.plugins.rendered(&plugin, &view);
            if rendered != slot.shown {
                slot.shown = rendered;
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
            }
            state.chrome.push(slot);
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
        let tab_bar = if titlebar > 0.0 {
            if tab_bar_shows { titlebar } else { 0.0 }
        } else {
            natural(Slot::TabBar)
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
        let regions = chrome::regions(&shape, PLACE, heights);
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

/// The event of the plugin's that `event`, of a node of `view`, is.
fn ui_event(view: ViewId, event: NodeEvent) -> UiEvent {
    UiEvent {
        view,
        key: event.key,
        kind: event.kind,
    }
}

/// The items of the slots' views of `chrome`, each at its frame, over the
/// panes, of `tokens`, in a window of `room` pixels, of the dark appearance
/// or the light one, of their transitions at `now`.
pub(super) fn paint<F: FontBackend>(
    chrome: &[SlotView],
    font: &mut Font<F>,
    tokens: &Tokens,
    room: (f32, f32),
    (dark, now): (bool, Instant),
) -> Vec<Item> {
    chrome
        .iter()
        .filter_map(|view| Some((view, view.frame?)))
        .flat_map(|(view, frame)| {
            view.surface
                .paint(font, tokens, ((frame.x, frame.y), room), (dark, None, now))
        })
        .collect()
}

/// The indeterminate progresses of the slots' views of `chrome`, which the
/// window server moves, of `tokens`, at `now`, on a display of `scale`, of
/// their sweeps' `start`.
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
