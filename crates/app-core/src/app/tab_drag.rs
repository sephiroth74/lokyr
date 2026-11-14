//! A tab dragged out of its bar, into another window's bar, into a pane of
//! another tab, or out of every window: its image under the pointer, a card
//! of the tab over its panes as they showed as it left, which the platform
//! shows over every window; its window showing the tab next to it
//! meanwhile; the bar of another window opening a room for it under the
//! pointer, or the half of a pane under it lighting up, nearest the
//! pointer; and its drop, of which its plugin makes the tab's move, or its
//! way back, as Escape puts it.

use std::time::Instant;

use lokyr_kernel::plugins::host::{BarPlace, Edge};
use lokyr_kernel::plugins::ui::Slot;
use lokyr_kernel::plugins::view::{Key as NodeKey, UiEventKind};
use lokyr_kernel::workspace;
use lokyr_platform_core::{Platform, Point, ScreenPoint, ScreenRect, Size, WindowId};
use lokyr_render_core::{
    Color, DisplayList, Item, PictureId, Rect, Renderer, Shadow, Shape, Surface as _,
};
use lokyr_ui_core::pointer::NodeEvent;

use super::App;
use super::chrome::{self, SlotView};
use crate::drawing::Backend;

/// How far past its bar, across it, the pointer takes a tab out of it, in
/// points: that of a bar's height, about.
const MARGIN: f64 = 24.0;

/// The image's card: its width, the radius of its corners, and the room
/// between its edges and what it holds, in points.
const CARD_WIDTH: f64 = 320.0;
const CARD_RADIUS: f64 = 10.0;
const CARD_PADDING: f64 = 6.0;

/// The room around the card that its shadow takes, in points, and the
/// shadow's blur and drop.
const SHADOW_ROOM: f64 = 24.0;
const SHADOW_BLUR: f64 = 18.0;
const SHADOW_DROP: f64 = 6.0;

/// The opacity of the card's shadow, a black's.
const SHADOW_OPACITY: f32 = 0.35;

/// A tab dragged out of its bar, or back in it, until its drop.
pub(crate) struct TabDrag<B: Backend> {
    /// The window of the bar that it was dragged from.
    window: WindowId,
    /// The tab, and its node's key in the bar.
    tab: workspace::TabId,
    key: NodeKey,
    /// Whether its window showed it as it left the bar, which shows it
    /// again as it comes back.
    showed: bool,
    /// Whether it is out of its bar.
    out: bool,
    /// Where the pointer is in the image, from its corner, and the image's
    /// size, in points.
    grab: (f64, f64),
    size: Size,
    /// Where the pointer is in a window of the tab alone, from its
    /// content's corner, in points: within the tab as it grabbed it, at the
    /// place of the first tab of the bar.
    alone: (f64, f64),
    /// The size of its window's content below the titlebar, which a window
    /// of the tab alone takes.
    content: Size,
    /// The bar of another window that opened a room for it, and that bar's
    /// view.
    room: Option<(WindowId, u32)>,
    /// The half of a pane of another tab that it lit up, under the pointer.
    lit: Option<Lit>,
    /// The image's list, of the picture of the tab's panes, which the
    /// renderer holds during the drag, and the surface that shows it, at
    /// its window's scale.
    list: DisplayList,
    picture: Option<PictureId>,
    surface: Option<B::Surface>,
    scale: f64,
}

impl<B: Backend> std::fmt::Debug for TabDrag<B> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TabDrag")
            .field("window", &self.window)
            .field("tab", &self.tab)
            .field("key", &self.key)
            .field("showed", &self.showed)
            .field("out", &self.out)
            .field("room", &self.room)
            .finish_non_exhaustive()
    }
}

/// The half of a pane of another tab than a dragged one, under the
/// pointer, which lights up for the tab's drop there: its window, the pane,
/// its edge nearest the pointer, and that half, in pixels from the window's
/// content's corner.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Lit {
    window: WindowId,
    pane: workspace::PaneId,
    edge: Edge,
    half: Rect,
}

/// What a tab's drop does.
pub(super) enum Dropping {
    /// Its node takes the event, before its drag's end, of which its
    /// plugin moves the tab, and the window that takes it, if one does,
    /// comes forward once the drag ended.
    Event(UiEventKind, Option<WindowId>),
    /// It goes back to its place in its bar.
    Back,
}

impl<B: Backend> App<B> {
    /// The pointer moved to `position` over `window` in a drag of its tab
    /// bar's tab that carries a tab: past the bar's margin, or out of the
    /// window, the tab leaves the bar, its image under the pointer, the
    /// window showing the tab next to it, and a bar of another window under
    /// the pointer opens a room for it; back in the bar, it goes back.
    pub(super) fn drag_tab(
        &mut self,
        platform: &mut dyn Platform,
        window: WindowId,
        position: Point,
    ) {
        let Some(size) = platform.window_size(window) else {
            return;
        };
        let across = !matches!(
            self.windows
                .get(&window)
                .map(|state| chrome::tab_bar_place(&state.chrome, &self.plugins)),
            Some(BarPlace::Left { .. } | BarPlace::Right { .. } | BarPlace::Sidebar { .. })
        );
        let Some(state) = self.windows.get_mut(&window) else {
            return;
        };
        let Some((tab, frame)) = tab_bar(&state.chrome).and_then(|view| {
            let (_, tab) = view.surface.carried()?;
            Some((workspace::TabId::of(tab.0), view.frame?))
        }) else {
            // Its tab gone, as its program closed it, the drag ends.
            if self
                .tab_drag
                .as_ref()
                .is_some_and(|drag| drag.window == window)
            {
                self.end_tab_drag(platform, false);
            }
            return;
        };
        let scale = size.scale;
        let margin = (MARGIN * scale) as f32;
        let (x, y) = ((position.x * scale) as f32, (position.y * scale) as f32);
        let inside = position.x >= 0.0
            && position.y >= 0.0
            && position.x < size.points.width
            && position.y < size.points.height;
        let out = !inside
            || if across {
                y < frame.y - margin || y >= frame.y + frame.height + margin
            } else {
                x < frame.x - margin || x >= frame.x + frame.width + margin
            };
        let ours = self
            .tab_drag
            .as_ref()
            .filter(|drag| drag.window == window && drag.tab == tab)
            .map(|drag| drag.out);
        match (ours, out) {
            (None, false) => return,
            (None, true) => {
                if !self.start_tab_drag(platform, window, tab) {
                    return;
                }
            }
            (Some(false), true) => self.take_tab_out(platform),
            (Some(true), false) => self.bring_tab_back(platform),
            _ => {}
        }
        if out {
            self.follow_tab(platform, window, position);
        }
    }

    /// The tab of `window`'s bar dragged leaves the bar: its image, of the
    /// picture of its panes and of its node as it shows, grabbed where the
    /// press grabbed the node; whether it did.
    fn start_tab_drag(
        &mut self,
        platform: &mut dyn Platform,
        window: WindowId,
        tab: workspace::TabId,
    ) -> bool {
        let Some(size) = platform.window_size(window) else {
            return false;
        };
        let scale = size.scale;
        // A size kept for a window that an earlier drop did not open goes.
        self.placing_size = None;
        // The picture of its panes, where the app draws, of their room.
        let pictured = self.tab_picture(platform, window, tab);
        let room = self.windows.get(&window).and_then(|state| state.panes_room);
        let (width, height) = pictured
            .map(|(_, pixels, _)| pixels)
            .or(room.map(|room| (room.width, room.height)))
            .unwrap_or((size.pixels.width, size.pixels.height));
        let background = pictured
            .map(|(_, _, background)| background)
            .or_else(|| {
                let shown = self.workspace.tab(tab)?;
                let pane = self.workspace.pane(shown.active_pane())?;
                Some(lokyr_ui_core::pane::background(pane.snapshot()))
            })
            .unwrap_or(Color::BLACK);
        let picture = pictured.map(|(picture, ..)| picture);
        let now = Instant::now();
        let mut drawing = self.drawing.as_mut();
        let Some(state) = self.windows.get_mut(&window) else {
            return false;
        };
        let Some(view) = tab_bar(&state.chrome) else {
            return false;
        };
        let Some((key, _)) = view.surface.carried() else {
            return false;
        };
        let (Some(frame), Some(node)) = (view.frame, view.surface.tree.by_key(&key)) else {
            return false;
        };
        let Some(laid) = view.surface.layout.frame(&view.surface.tree, node) else {
            return false;
        };
        // The node as it shows, under the pointer, in pixels of the window.
        let (dx, dy) = view.surface.animations.offset(&key, now);
        let shown = (frame.x + laid.x + dx, frame.y + laid.y + dy);
        let points = |pixels: f32| f64::from(pixels) / scale;
        let pixels = |points: f64| (points * scale).round() as f32;
        // Where the press grabbed it, within it.
        let pressed = view
            .surface
            .pointer
            .drag_start()
            .unwrap_or((laid.x + laid.width / 2.0, laid.y + laid.height / 2.0));
        let within = (
            points(pressed.0 - laid.x).clamp(0.0, points(laid.width)),
            points(pressed.1 - laid.y).clamp(0.0, points(laid.height)),
        );
        // A window of the tab alone shows it at its bar's first tab's place.
        let first = view
            .surface
            .tree
            .children(view.surface.tree.parent(node).unwrap_or(node))
            .iter()
            .find(|&&child| view.surface.tree.carries(child).is_some())
            .and_then(|&child| view.surface.layout.frame(&view.surface.tree, child))
            .map_or(shown, |first| (frame.x + first.x, frame.y + first.y));
        let alone = (points(first.0) + within.0, points(first.1) + within.1);
        // The card: the tab at its top, the panes under it, scaled to its
        // width.
        let (tab_width, tab_height) = (points(laid.width), points(laid.height));
        let inner = CARD_WIDTH - 2.0 * CARD_PADDING;
        let panes_height = inner * f64::from(height) / f64::from(width.max(1));
        let strip = tab_height + 2.0 * CARD_PADDING;
        let card = Size {
            width: CARD_WIDTH.max(tab_width + 2.0 * CARD_PADDING),
            height: strip + panes_height + CARD_PADDING,
        };
        let image = Size {
            width: card.width + 2.0 * SHADOW_ROOM,
            height: card.height + 2.0 * SHADOW_ROOM,
        };
        let tab_at = (SHADOW_ROOM + CARD_PADDING, SHADOW_ROOM + CARD_PADDING);
        let grab = (tab_at.0 + within.0, tab_at.1 + within.1);
        let rect = |x: f64, y: f64, width: f64, height: f64| Rect {
            x: pixels(x) as i32,
            y: pixels(y) as i32,
            width: pixels(width) as u32,
            height: pixels(height) as u32,
        };
        let mut items = vec![Item::Shapes(vec![Shape {
            radius: pixels(CARD_RADIUS),
            shadow: Some(Shadow {
                x: 0.0,
                y: pixels(SHADOW_DROP),
                blur: pixels(SHADOW_BLUR),
                color: Color::BLACK.with_alpha(SHADOW_OPACITY),
            }),
            ..Shape::filled(
                rect(SHADOW_ROOM, SHADOW_ROOM, card.width, card.height),
                background.with_alpha(1.0),
            )
        }])];
        if let Some(picture) = picture {
            items.push(Item::Picture {
                rect: rect(
                    SHADOW_ROOM + CARD_PADDING,
                    SHADOW_ROOM + strip,
                    card.width - 2.0 * CARD_PADDING,
                    panes_height,
                ),
                picture,
            });
        }
        // The tab's node as its bar paints it, moved to the card's top.
        let tokens = state
            .chrome_tokens
            .as_ref()
            .map(|(look, tokens)| (look.0, tokens));
        if let (Some((dark, tokens)), Some(drawing)) = (tokens, drawing.as_deref_mut()) {
            let to = (pixels(tab_at.0), pixels(tab_at.1));
            let origin = (to.0 - laid.x - dx, to.1 - laid.y - dy);
            let room = (size.pixels.width as f32, size.pixels.height as f32);
            items.push(Item::PushClip(Rect {
                x: to.0 as i32,
                y: to.1 as i32,
                width: laid.width.round() as u32,
                height: laid.height.round() as u32,
            }));
            items.extend(
                view.surface.paint(
                    &mut drawing.font,
                    tokens,
                    (origin, room),
                    (dark, None, now),
                    self.pictures
                        .get(&view.plugin)
                        .unwrap_or(&lokyr_ui_core::paint::NO_PICTURES),
                ),
            );
            items.push(Item::PopClip);
        }
        let mut list = DisplayList {
            background: Color::CLEAR,
            items,
            frame: 0,
        };
        if let Some(drawing) = drawing {
            list.frame = drawing.font.atlas_frame();
            let updates = drawing.font.texture_updates();
            if let Err(error) = drawing.renderer.update_textures(&updates) {
                eprintln!("lokyr: {error}");
            }
        }
        let showed = self
            .workspace
            .window(state.window)
            .is_some_and(|shown| shown.active_tab() == tab);
        let content = Size {
            width: size.points.width,
            height: size.points.height - size.bar,
        };
        self.tab_drag = Some(TabDrag {
            window,
            tab,
            key,
            showed,
            out: false,
            grab,
            size: image,
            alone,
            content,
            room: None,
            lit: None,
            list,
            picture,
            surface: None,
            scale,
        });
        self.take_tab_out(platform);
        true
    }

    /// The tab dragged leaves its bar: its node shows nowhere, its window
    /// shows the tab next to it, and its image shows.
    fn take_tab_out(&mut self, platform: &mut dyn Platform) {
        let Some(drag) = self.tab_drag.as_mut() else {
            return;
        };
        drag.out = true;
        let (window, tab, showed) = (drag.window, drag.tab, drag.showed);
        if let Some(view) = self
            .windows
            .get_mut(&window)
            .and_then(|state| tab_bar_mut(&mut state.chrome))
        {
            view.surface.carry_out(true, Instant::now());
        }
        // The window shows the tab after it, or before it at the end, as if
        // it had closed; a window of the tab alone shows it on.
        let shown = self
            .windows
            .get(&window)
            .and_then(|state| self.workspace.window(state.window));
        if showed && let Some(shown) = shown {
            let tabs = shown.tabs();
            let next = tabs
                .iter()
                .position(|&at| at == tab)
                .and_then(|index| {
                    tabs.get(index + 1)
                        .or_else(|| index.checked_sub(1).and_then(|before| tabs.get(before)))
                })
                .copied();
            if let Some(next) = next {
                self.workspace.activate_tab(next);
            }
        }
        platform.request_frame(window);
    }

    /// The tab dragged comes back into its bar: its image hides, its window
    /// shows it again, and a room that another bar opened closes.
    fn bring_tab_back(&mut self, platform: &mut dyn Platform) {
        let Some(drag) = self.tab_drag.as_mut() else {
            return;
        };
        drag.out = false;
        drag.surface = None;
        let (window, tab, showed, room) = (drag.window, drag.tab, drag.showed, drag.room.take());
        platform.hide_drag_image(None);
        self.close_tab_room(platform, room);
        if let Some(view) = self
            .windows
            .get_mut(&window)
            .and_then(|state| tab_bar_mut(&mut state.chrome))
        {
            view.surface.carry_out(false, Instant::now());
        }
        if showed {
            self.workspace.activate_tab(tab);
        }
        platform.request_frame(window);
    }

    /// The tab out of its bar follows the pointer at `position` over
    /// `window`: its image moves under it, drawn as it first shows, and a
    /// bar of another window under it opens a room for it, which the one
    /// that it leaves closes.
    fn follow_tab(&mut self, platform: &mut dyn Platform, window: WindowId, position: Point) {
        let Some(at) = on_screens(platform, window, position) else {
            return;
        };
        let Some(drag) = self.tab_drag.as_mut() else {
            return;
        };
        platform.show_drag_image(ScreenRect {
            x: at.x - drag.grab.0,
            y: at.y - drag.grab.1,
            width: drag.size.width,
            height: drag.size.height,
        });
        if drag.surface.is_none()
            && let (Some(drawing), Some(handle)) =
                (self.drawing.as_mut(), platform.drag_image_handle())
        {
            match drawing.backend.surface(handle) {
                Ok(surface) => {
                    let pixels = |points: f64| (points * drag.scale).round() as u32;
                    surface.resize(
                        pixels(drag.size.width),
                        pixels(drag.size.height),
                        drag.scale,
                    );
                    if let Err(error) = drawing.renderer.draw_untimed(&surface, &drag.list) {
                        eprintln!("lokyr: {error}");
                    }
                    drag.surface = Some(surface);
                }
                Err(error) => eprintln!("lokyr: {error}"),
            }
        }
        let room = self.bar_under(platform, window, at);
        // Past every bar, the half of a pane under the pointer lights up.
        let lit = room
            .is_none()
            .then(|| self.pane_half_under(platform, at))
            .flatten();
        let Some(drag) = self.tab_drag.as_mut() else {
            return;
        };
        if drag.lit != lit {
            let before = std::mem::replace(&mut drag.lit, lit);
            for shown in [before, lit].into_iter().flatten() {
                platform.request_frame(shown.window);
            }
        }
        let left = drag.room.filter(|&before| Some(before) != room);
        drag.room = room;
        self.close_tab_room(platform, left);
        if let Some((dest, id)) = room
            && let Some(position) = local_of(platform, dest, at)
            && let Some(size) = platform.window_size(dest)
            && let Some(view) = self
                .windows
                .get_mut(&dest)
                .and_then(|state| state.chrome.iter_mut().find(|view| view.id == id))
            && let Some(frame) = view.frame
        {
            let at = (
                (position.x * size.scale) as f32 - frame.x,
                (position.y * size.scale) as f32 - frame.y,
            );
            view.surface.open_room(at, Instant::now());
            platform.request_frame(dest);
        }
    }

    /// The bar of another window than `window` at `at` on the screens, of
    /// its view's id, that shows and takes a tab: the pointer over it, or
    /// past it by the margin across it.
    fn bar_under(
        &self,
        platform: &dyn Platform,
        window: WindowId,
        at: ScreenPoint,
    ) -> Option<(WindowId, u32)> {
        let dest = platform.window_at(at).filter(|&dest| dest != window)?;
        let size = platform.window_size(dest)?;
        let position = local_of(platform, dest, at)?;
        let state = self.windows.get(&dest)?;
        // A title has no tabs to open a room among; the drop over it goes
        // after the window's tab.
        if chrome::tab_bar_place(&state.chrome, &self.plugins) == BarPlace::Title {
            return None;
        }
        let view = tab_bar(&state.chrome).filter(|view| view.shows())?;
        let frame = view.frame?;
        let across = !matches!(
            chrome::tab_bar_place(&state.chrome, &self.plugins),
            BarPlace::Left { .. } | BarPlace::Right { .. } | BarPlace::Sidebar { .. }
        );
        let margin = (MARGIN * size.scale) as f32;
        let (x, y) = (
            (position.x * size.scale) as f32,
            (position.y * size.scale) as f32,
        );
        let over = if across {
            x >= frame.x
                && x < frame.x + frame.width
                && y >= frame.y - margin
                && y < frame.y + frame.height + margin
        } else {
            y >= frame.y
                && y < frame.y + frame.height
                && x >= frame.x - margin
                && x < frame.x + frame.width + margin
        };
        over.then_some((dest, view.id))
    }

    /// The half of a pane of another tab than the one dragged, at `at` on
    /// the screens, of a window of the app, the drag's own too, as it lays
    /// its panes out: the half of the pane's edge nearest the pointer.
    fn pane_half_under(&self, platform: &dyn Platform, at: ScreenPoint) -> Option<Lit> {
        let drag = self.tab_drag.as_ref()?;
        let dest = platform.window_at(at)?;
        let size = platform.window_size(dest)?;
        let position = local_of(platform, dest, at)?;
        let (x, y) = (position.x * size.scale, position.y * size.scale);
        let view = self.windows.get(&dest)?.views.iter().find(|view| {
            let rect = view.rect;
            x >= f64::from(rect.x)
                && y >= f64::from(rect.y)
                && x < f64::from(rect.x) + f64::from(rect.width)
                && y < f64::from(rect.y) + f64::from(rect.height)
        })?;
        if self.workspace.tab_of(view.pane) == Some(drag.tab) {
            return None;
        }
        let rect = view.rect;
        let across = (x - f64::from(rect.x)) / f64::from(rect.width.max(1));
        let down = (y - f64::from(rect.y)) / f64::from(rect.height.max(1));
        let edge = [
            (across, Edge::Left),
            (1.0 - across, Edge::Right),
            (down, Edge::Top),
            (1.0 - down, Edge::Bottom),
        ]
        .into_iter()
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, edge)| edge)?;
        let (wide, high) = (rect.width / 2, rect.height / 2);
        let half = match edge {
            Edge::Left => Rect {
                width: wide,
                ..rect
            },
            Edge::Right => Rect {
                x: rect.x.saturating_add_unsigned(rect.width - wide),
                width: wide,
                ..rect
            },
            Edge::Top => Rect {
                height: high,
                ..rect
            },
            Edge::Bottom => Rect {
                y: rect.y.saturating_add_unsigned(rect.height - high),
                height: high,
                ..rect
            },
        };
        Some(Lit {
            window: dest,
            pane: view.pane,
            edge,
            half,
        })
    }

    /// The half of a pane of `window` that the tab dragged lights up, if
    /// it lights one there.
    pub(super) fn lit_half(&self, window: WindowId) -> Option<Rect> {
        self.tab_drag
            .as_ref()
            .and_then(|drag| drag.lit)
            .filter(|lit| lit.window == window)
            .map(|lit| lit.half)
    }

    /// Closes the room that the bar of `room`, a window's view, opened.
    fn close_tab_room(&mut self, platform: &mut dyn Platform, room: Option<(WindowId, u32)>) {
        let Some((dest, id)) = room else {
            return;
        };
        if let Some(view) = self
            .windows
            .get_mut(&dest)
            .and_then(|state| state.chrome.iter_mut().find(|view| view.id == id))
            && view.surface.close_room(Instant::now())
        {
            platform.request_frame(dest);
        }
    }

    /// What the drop of the tab dragged out of `window`'s bar at `position`
    /// does: in the room of another window's bar, or at the end of the tabs
    /// of a window whose bar hides, over its titlebar, the tab goes there;
    /// on a pane of another tab, beside it, in its half that lit up;
    /// out of every window, to a window of its own, under the pointer; over
    /// a window elsewhere, or the settings' window, back. `None` of no tab
    /// out of its bar.
    pub(super) fn tab_drop(
        &mut self,
        platform: &mut dyn Platform,
        window: WindowId,
        position: Point,
    ) -> Option<Dropping> {
        let drag = self
            .tab_drag
            .as_ref()
            .filter(|drag| drag.window == window && drag.out)?;
        let (alone, room, content, tab) = (drag.alone, drag.room, drag.content, drag.tab);
        let Some(at) = on_screens(platform, window, position) else {
            return Some(Dropping::Back);
        };
        let Some(dest) = platform.window_at(at) else {
            self.placing_size = Some((tab, content));
            let place = UiEventKind::DropOutside {
                x: at.x - alone.0,
                y: at.y - alone.1,
            };
            return Some(Dropping::Event(place, None));
        };
        // On a pane of another tab, of its own window too, outside the room
        // of a bar, it goes in there, at the half that lit up.
        if room.is_none_or(|(over, _)| over != dest)
            && let Some(lit) = self.pane_half_under(platform, at)
        {
            let there = UiEventKind::DropOnPane {
                pane: lit.pane.into(),
                edge: lit.edge,
            };
            return Some(Dropping::Event(there, Some(lit.window)));
        }
        if dest == window {
            return Some(Dropping::Back);
        }
        // The settings' window of their own takes no tab.
        if self.shows_settings(dest) {
            return Some(Dropping::Back);
        }
        let now = Instant::now();
        let shown = self.windows.get(&dest).map(|state| state.window);
        let index = match room {
            Some((over, id)) if over == dest => self
                .windows
                .get_mut(&dest)
                .and_then(|state| state.chrome.iter_mut().find(|view| view.id == id))
                .and_then(|view| view.surface.drop_room(now)),
            _ => {
                // A window whose bar hides, or shows its title, takes it at
                // its tabs' end, over its titlebar.
                let size = platform.window_size(dest)?;
                let position = local_of(platform, dest, at)?;
                let hidden = self.windows.get(&dest).is_some_and(|state| {
                    !chrome::has_tab_bar(&state.chrome)
                        || chrome::tab_bar_place(&state.chrome, &self.plugins) == BarPlace::Title
                });
                (hidden && position.y < size.bar)
                    .then(|| shown.and_then(|shown| self.workspace.window(shown)))
                    .flatten()
                    .map(|shown| shown.tabs().len())
            }
        };
        let (Some(index), Some(shown)) = (index, shown) else {
            return Some(Dropping::Back);
        };
        // The room that the drop takes stays open for the tab.
        if room.is_some_and(|(over, _)| over == dest)
            && let Some(drag) = self.tab_drag.as_mut()
        {
            drag.room = None;
        }
        // The window that takes it comes forward, its plugin's move showing
        // it there.
        let there = UiEventKind::DropOnBar {
            window: shown.into(),
            index,
        };
        Some(Dropping::Event(there, Some(dest)))
    }

    /// The tab's drag ends: dropped, its image hides at once; otherwise it
    /// slides back to the tab's place in its bar, as the window shows the
    /// tab again. The room that another bar opened closes, but the one
    /// that the drop took, as does the picture of its panes.
    pub(super) fn end_tab_drag(&mut self, platform: &mut dyn Platform, dropped: bool) {
        let Some(mut drag) = self.tab_drag.take() else {
            return;
        };
        if let Some(lit) = drag.lit.take() {
            platform.request_frame(lit.window);
        }
        self.close_tab_room(platform, drag.room.take());
        if drag.out {
            let back = (!dropped && !platform.reduce_motion())
                .then(|| self.tab_home(platform, &drag))
                .flatten();
            platform.hide_drag_image(back);
            if !dropped && drag.showed {
                self.workspace.activate_tab(drag.tab);
            }
        }
        if let (Some(picture), Some(drawing)) = (drag.picture.take(), self.drawing.as_mut()) {
            drawing.renderer.drop_picture(picture);
        }
        platform.request_frame(drag.window);
    }

    /// Whether a tab of `window`'s bar is dragged, out of it or back in it.
    pub(super) fn tab_dragged_of(&self, window: WindowId) -> bool {
        self.tab_drag
            .as_ref()
            .is_some_and(|drag| drag.window == window)
    }

    /// The place of the tab of `drag` in its bar, on the screens, which its
    /// image slides back to.
    fn tab_home(&self, platform: &dyn Platform, drag: &TabDrag<B>) -> Option<ScreenRect> {
        let size = platform.window_size(drag.window)?;
        let corner = platform.window_frame(drag.window)?;
        let view = tab_bar(&self.windows.get(&drag.window)?.chrome)?;
        let frame = view.frame?;
        let node = view.surface.tree.by_key(&drag.key)?;
        let laid = view.surface.layout.frame(&view.surface.tree, node)?;
        let points = |pixels: f32| f64::from(pixels) / size.scale;
        Some(ScreenRect {
            x: corner.x + points(frame.x + laid.x),
            y: corner.y + points(frame.y + laid.y),
            width: points(laid.width),
            height: points(laid.height),
        })
    }
}

/// `events`, of a drag's end, with `kind` of its node before its end.
pub(super) fn before_end(events: &mut Vec<NodeEvent>, kind: UiEventKind) {
    let end = events.iter().position(|event| {
        matches!(
            event.kind,
            UiEventKind::DragEnd { .. } | UiEventKind::DragCancel
        )
    });
    if let Some(end) = end {
        let dropped = NodeEvent {
            kind,
            ..events[end].clone()
        };
        events.insert(end, dropped);
    }
}

/// The tab bar's view of `chrome`.
fn tab_bar(chrome: &[SlotView]) -> Option<&SlotView> {
    chrome
        .iter()
        .find(|view| view.view.slot() == Some(Slot::TabBar))
}

fn tab_bar_mut(chrome: &mut [SlotView]) -> Option<&mut SlotView> {
    chrome
        .iter_mut()
        .find(|view| view.view.slot() == Some(Slot::TabBar))
}

/// `position`, of `window`'s content, on the screens.
fn on_screens(platform: &dyn Platform, window: WindowId, position: Point) -> Option<ScreenPoint> {
    let frame = platform.window_frame(window)?;
    Some(ScreenPoint {
        x: frame.x + position.x,
        y: frame.y + position.y,
    })
}

/// `at`, on the screens, of `window`'s content.
fn local_of(platform: &dyn Platform, window: WindowId, at: ScreenPoint) -> Option<Point> {
    let frame = platform.window_frame(window)?;
    Some(Point {
        x: at.x - frame.x,
        y: at.y - frame.y,
    })
}
