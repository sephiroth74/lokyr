//! The gallery's view as a window's host shows it: the gallery's content,
//! as a pane's, the window's chrome around it, and its floating panels,
//! each a surface of its own, the panels over the rest, and the keyboard's
//! focus across them; what the pointer, the keyboard and the input method
//! do to them, and the panels that a click outside them or Escape closes.
//! The window's own pieces, the platform's timers, clipboard and
//! composition, are the caller's.

use std::collections::BTreeMap;
use std::time::Instant;

use accesskit::{ActionRequest, TreeUpdate};
use lokyr_plugin_sdk::view::{Key as NodeKey, UiEventKind};
use lokyr_render_core::Item;
use lokyr_text_core::FontBackend;
use lokyr_ui_core::access::WindowTree;
use lokyr_ui_core::access::view::{self as access, SurfaceAccess};
use lokyr_ui_core::chrome::{self, Heights, TabBarPlace, Window};
use lokyr_ui_core::field::{Carets, Clipboard};
use lokyr_ui_core::focus::{Focus, Routed, Target};
use lokyr_ui_core::font::Font;
use lokyr_ui_core::layout::{Frame, Measure};
use lokyr_ui_core::panel::{self, Anchor, Corner, Dismiss, MenuEntry, Options, Panels, Role};
use lokyr_ui_core::pointer::{self, Area, Button, NodeEvent};
use lokyr_ui_core::progress::SweepArea;
use lokyr_ui_core::surface::Surface;
use lokyr_ui_core::theme::Tokens;
use lokyr_ui_core::tree::{NodeId, Tree};
use lokyr_vt_core::input::{Key, KeyAction, KeyEvent};

use crate::gallery::{BELOW, FIND, Gallery, MENU, PanelAsk, Slot};

/// The gallery's content, as a panel of the focus's.
pub const MAIN: u8 = 0;

/// The chrome's surfaces, past the panels', by their slots, in the order
/// that they draw: the pane's header, the strip over the page, the status
/// bar and the tab bar.
const SLOTS: [(u8, Slot); 4] = [
    (16, Slot::PaneHeader),
    (17, Slot::WindowStrip),
    (18, Slot::StatusBar),
    (19, Slot::TabBar),
];

/// The rounds of events that a change of the trees can bring, as a hover
/// that moves what is under the pointer, past which it waits for the next.
const ROUNDS: usize = 4;

/// The room between a panel in a corner and the window's edges, in points.
const MARGIN: f32 = 12.0;

/// Where a panel of the gallery's goes, as its anchor follows it.
#[derive(Clone, Debug)]
enum Place {
    /// At the pointer, where it was as the panel opened.
    Pointer((f32, f32)),
    /// Below the content's node of a key.
    Below(NodeKey),
    /// In a corner of the window.
    Corner(Corner),
}

/// The gallery's surfaces, and the focus across them.
pub struct View {
    /// The gallery.
    pub gallery: Gallery,
    /// The keyboard's focus.
    pub focus: Focus<u8>,
    /// The gallery's content.
    pub main: Surface,
    /// The open panels, over it.
    pub panels: Panels<u8>,
    surfaces: BTreeMap<u8, (Surface, Place)>,
    /// The surface that the pointer's button went down over, whose moves
    /// it takes until the button goes up, and the one that it is over.
    captured: Option<u8>,
    over: Option<u8>,
    /// Where the pointer is, in pixels from the window's content's corner.
    at: (f32, f32),
    /// The window's room, in pixels, and the page's in it.
    room: (f32, f32),
    page: Frame,
    /// The window's chrome, once the window told its titlebar: its
    /// titlebar's height and its buttons' room, in pixels, and the slots'
    /// surfaces and regions, by their ids.
    chrome: Option<(f32, f32)>,
    slots: BTreeMap<u8, (Surface, Option<Frame>)>,
    scale: f32,
    /// Whether the platform shows a menu's panel of menu items as its own.
    pub native_menus: bool,
    /// The native menu to show, its entries and its place in pixels of the
    /// content, and the events of its items' choices, by their keys.
    native: Option<(Vec<MenuEntry>, (f32, f32))>,
    choices: BTreeMap<String, (NodeKey, UiEventKind)>,
}

/// What a key did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Keyed {
    /// A surface's node, or the focus, took it.
    Taken,
    /// Nothing took it, which the window's own keys may.
    Left,
}

impl View {
    /// The view of `gallery`, at `scale` pixels a point.
    pub fn new(gallery: Gallery, scale: f32) -> Self {
        let mut view = Self {
            gallery,
            focus: Focus::new(),
            main: Surface::new(scale),
            panels: Panels::new(),
            surfaces: BTreeMap::new(),
            captured: None,
            over: None,
            at: (0.0, 0.0),
            room: (0.0, 0.0),
            page: Frame::default(),
            chrome: None,
            slots: BTreeMap::new(),
            scale,
            native_menus: false,
            native: None,
            choices: BTreeMap::new(),
        };
        view.render();
        view
    }

    /// The native menu that a menu's panel became, to show at its place, in
    /// pixels of the content, once.
    pub fn take_native(&mut self) -> Option<(Vec<MenuEntry>, (f32, f32))> {
        self.native.take()
    }

    /// The item `command` of the native menu shown last was chosen: its
    /// node's event, as its press would tell.
    pub fn chosen(&mut self, command: &str) {
        let Some((key, kind)) = self.choices.get(command).cloned() else {
            return;
        };
        self.gallery.event(&key.0, kind);
        self.render();
    }

    /// The surface of `panel`, the content's of `MAIN`, or of a slot's.
    pub fn surface(&self, panel: u8) -> Option<&Surface> {
        match panel {
            MAIN => Some(&self.main),
            _ => self
                .surfaces
                .get(&panel)
                .map(|(surface, _)| surface)
                .or_else(|| self.slots.get(&panel).map(|(surface, _)| surface)),
        }
    }

    fn surface_mut(&mut self, panel: u8) -> Option<&mut Surface> {
        match panel {
            MAIN => Some(&mut self.main),
            _ => match self.surfaces.get_mut(&panel) {
                Some((surface, _)) => Some(surface),
                None => self.slots.get_mut(&panel).map(|(surface, _)| surface),
            },
        }
    }

    /// Gives the view the window's chrome, of a titlebar `titlebar` pixels
    /// high over the window's top and the window's buttons `buttons` across
    /// its left, the page in the pane's grid's region; the window's
    /// coordinates are then the whole content's, the titlebar's too.
    pub fn set_titlebar(&mut self, titlebar: f32, buttons: f32) {
        self.chrome = Some((titlebar, buttons));
        if self.slots.is_empty() {
            for (id, _) in SLOTS {
                self.slots.insert(id, (Surface::new(self.scale), None));
            }
            self.render();
        }
    }

    /// The region of the slot of `id`, if it shows.
    pub fn region(&self, id: u8) -> Option<Frame> {
        self.slots.get(&id).and_then(|(_, frame)| *frame)
    }

    /// The page's region, in pixels of the window.
    #[cfg(test)]
    pub fn page(&self) -> Frame {
        self.page
    }

    /// The parts of the titlebar that are the tab bar's own, in pixels of
    /// the window, where a press goes to it: none but in the titlebar.
    pub fn titlebar_areas(&self) -> Vec<Frame> {
        let Some((surface, Some(frame))) = self.slots.get(&slot_id(Slot::TabBar)) else {
            return Vec::new();
        };
        if self.gallery.tab_bar_place() != TabBarPlace::Titlebar
            || self.chrome.is_none_or(|(titlebar, _)| titlebar <= 0.0)
        {
            return Vec::new();
        }
        chrome::titlebar_areas(&surface.tree, &surface.layout)
            .into_iter()
            .map(|area| Frame {
                x: area.x + frame.x,
                y: area.y + frame.y,
                ..area
            })
            .collect()
    }

    /// The slot's surface under `at`, in pixels of the window, the topmost.
    fn slot_under(&self, at: (f32, f32)) -> Option<u8> {
        SLOTS.iter().rev().map(|&(id, _)| id).find(|id| {
            self.region(*id).is_some_and(|frame| {
                at.0 >= frame.x
                    && at.1 >= frame.y
                    && at.0 < frame.x + frame.width
                    && at.1 < frame.y + frame.height
            })
        })
    }

    /// The page's surface and the chrome's that show, in the order that
    /// they draw, each of its id and its region in the window.
    fn placed(&self) -> Vec<(u8, &Surface, Frame)> {
        let mut placed = vec![(MAIN, &self.main, self.page)];
        for (id, _) in SLOTS {
            if let (Some((surface, _)), Some(frame)) = (self.slots.get(&id), self.region(id)) {
                placed.push((id, surface, frame));
            }
        }
        placed
    }

    /// The corner of `panel`'s surface in the window, in pixels.
    fn corner(&self, panel: u8) -> (f32, f32) {
        if panel == MAIN {
            return (self.page.x, self.page.y);
        }
        match self.panels.frame(panel).or_else(|| self.region(panel)) {
            Some(frame) => (frame.x, frame.y),
            None => (0.0, 0.0),
        }
    }

    /// The tree of `panel`'s surface.
    fn tree(&self, panel: u8) -> Option<&Tree> {
        self.surface(panel).map(|surface| &surface.tree)
    }

    /// Takes the gallery's views anew, its content's and its panels', the
    /// texts that it sets in its fields, and the panels that it opens and
    /// closes.
    pub fn render(&mut self) {
        let _ = self.main.update(&self.gallery.view());
        self.focus.updated(MAIN, &self.main.tree);
        // Each text to the field of its key, of whichever surface.
        for (key, text) in self.gallery.take_texts() {
            let key = NodeKey(key);
            if let Some(surface) = self
                .all_mut()
                .find(|surface| surface.tree.by_key(&key).is_some())
            {
                surface.fields.set_text(&key, &text);
            } else {
                self.main.fields.set_text(&key, &text);
            }
        }
        for ask in self.gallery.take_panels() {
            match ask {
                PanelAsk::Open(panel) => self.open(panel),
                PanelAsk::Close(panel) => self.close(panel),
            }
        }
        for (&panel, (surface, _)) in &mut self.surfaces {
            let _ = surface.update(&self.gallery.panel(panel));
            self.focus.updated(panel, &surface.tree);
        }
        for (id, slot) in SLOTS {
            if let Some((surface, _)) = self.slots.get_mut(&id) {
                let _ = surface.update(&self.gallery.slot(slot));
            }
        }
    }

    /// Opens `panel`, over the others, of its anchor, its role and what
    /// closes it; a panel that takes the keyboard takes the focus.
    fn open(&mut self, panel: u8) {
        let (place, role, dismiss) = match panel {
            MENU => (
                Place::Pointer(self.at),
                Role::Menu,
                Dismiss {
                    outside: true,
                    escape: true,
                },
            ),
            FIND => (
                Place::Corner(Corner::TopRight),
                Role::Dialog,
                Dismiss {
                    outside: false,
                    escape: true,
                },
            ),
            BELOW => (
                Place::Below(NodeKey::from("open-below")),
                Role::Dialog,
                Dismiss {
                    outside: true,
                    escape: true,
                },
            ),
            _ => (
                Place::Corner(Corner::BottomLeft),
                Role::Tooltip,
                Dismiss::default(),
            ),
        };
        let anchor = self.anchor(&place);
        let mut surface = Surface::new(self.scale);
        let _ = surface.update(&self.gallery.panel(panel));
        // A menu of menu items alone shows as the platform's own, at its
        // anchor, which the system places, and draws nothing.
        if role == Role::Menu
            && self.native_menus
            && let Some(entries) = panel::native_menu(&surface.tree)
        {
            self.choices.clear();
            collect(&entries, &mut self.choices);
            self.native = Some((entries, point(&anchor)));
            return;
        }
        self.panels.open(
            panel,
            Options {
                anchor,
                role,
                dismiss,
            },
        );
        self.surfaces.insert(panel, (surface, place));
        if role.takes_focus()
            && let Some((surface, _)) = self.surfaces.get(&panel)
        {
            self.focus.first(panel, &surface.tree);
            self.tell_focus();
        }
    }

    /// Closes `panel`, whose focus goes back to where it took it from.
    fn close(&mut self, panel: u8) {
        if self.panels.close(panel) || self.surfaces.contains_key(&panel) {
            self.surfaces.remove(&panel);
            let (main, surfaces) = (&self.main, &self.surfaces);
            self.focus.closed(panel, |of| match of {
                MAIN => Some(&main.tree),
                _ => surfaces.get(&of).map(|(surface, _)| &surface.tree),
            });
            if self.captured == Some(panel) {
                self.captured = None;
            }
            if self.over == Some(panel) {
                self.over = None;
            }
            self.tell_focus();
        }
    }

    /// The anchor of `place`, in pixels of the window: a corner's of the
    /// page, the pane's overlays' region.
    fn anchor(&self, place: &Place) -> Anchor {
        match place {
            Place::Pointer(at) => Anchor::At(*at),
            Place::Below(key) => self
                .main
                .tree
                .by_key(key)
                .and_then(|node| self.main.layout.frame(&self.main.tree, node))
                .map_or(Anchor::At(self.at), |frame| {
                    Anchor::Below(Frame {
                        x: frame.x + self.page.x,
                        y: frame.y + self.page.y,
                        ..frame
                    })
                }),
            Place::Corner(corner) => Anchor::Corner(self.page, *corner, MARGIN * self.scale),
        }
    }

    /// Every surface: the content's, the chrome's, then the open panels'.
    fn all(&self) -> impl Iterator<Item = &Surface> {
        std::iter::once(&self.main)
            .chain(self.slots.values().map(|(surface, _)| surface))
            .chain(self.surfaces.values().map(|(surface, _)| surface))
    }

    /// Every surface, to change.
    fn all_mut(&mut self) -> impl Iterator<Item = &mut Surface> {
        std::iter::once(&mut self.main)
            .chain(self.slots.values_mut().map(|(surface, _)| surface))
            .chain(self.surfaces.values_mut().map(|(surface, _)| surface))
    }

    /// Lays the chrome out in the window's room, each slot in its region,
    /// and gives the page the pane's grid's region; without a chrome, the
    /// whole room, `height` `None` for the page's own: the events of the
    /// slots' layouts.
    fn lay_out_chrome(
        &mut self,
        measure: &mut impl Measure,
        height: Option<f32>,
    ) -> Vec<NodeEvent> {
        let width = self.room.0;
        let Some((titlebar, buttons)) = self.chrome else {
            self.page = Frame {
                x: 0.0,
                y: 0.0,
                width,
                height: height.unwrap_or(f32::MAX),
            };
            return Vec::new();
        };
        let place = self.gallery.tab_bar_place();
        let mut events = Vec::new();
        // Each slot's height, of its view laid out across the window.
        let mut natural = |slots: &mut BTreeMap<u8, (Surface, Option<Frame>)>, slot: Slot| {
            let (surface, _) = slots.get_mut(&slot_id(slot))?;
            surface.tree.root()?;
            events.extend(surface.lay_out((Some(width), None), measure));
            surface.size().map(|size| size.1)
        };
        let tab_bar = match place {
            TabBarPlace::Titlebar if titlebar > 0.0 => self
                .slots
                .get(&slot_id(Slot::TabBar))
                .and_then(|(surface, _)| surface.tree.root())
                .map(|_| titlebar),
            _ => natural(&mut self.slots, Slot::TabBar),
        };
        let heights = Heights {
            tab_bar: tab_bar.unwrap_or(0.0),
            window_strip: natural(&mut self.slots, Slot::WindowStrip).unwrap_or(0.0),
            status_bar: natural(&mut self.slots, Slot::StatusBar).unwrap_or(0.0),
        };
        let header = self
            .slots
            .get_mut(&slot_id(Slot::PaneHeader))
            .filter(|(surface, _)| surface.tree.root().is_some())
            .and_then(|(surface, _)| {
                events.extend(surface.lay_out((Some(width), None), measure));
                surface.size().map(|size| size.1)
            })
            .unwrap_or(0.0);
        let window = Window {
            size: (width, height.unwrap_or(self.room.1)),
            titlebar,
            buttons,
        };
        let regions = chrome::regions(&window, place, heights);
        let pane = chrome::pane_regions(regions.panes, header);
        self.page = pane.grid;
        for (id, slot) in SLOTS {
            let frame = match slot {
                Slot::TabBar => regions.tab_bar,
                Slot::WindowStrip => regions.window_strip,
                Slot::StatusBar => regions.status_bar,
                Slot::PaneHeader => pane.header,
            };
            if let Some((surface, region)) = self.slots.get_mut(&id) {
                *region = frame;
                if let Some(frame) = frame {
                    events
                        .extend(surface.lay_out((Some(frame.width), Some(frame.height)), measure));
                }
            }
        }
        events
    }

    /// The scale changed, to `scale` pixels a point.
    pub fn set_scale(&mut self, scale: f32) {
        self.scale = scale;
        for surface in self.all_mut() {
            surface.layout.set_scale(&surface.tree, scale);
        }
    }

    /// Lays the content out in the window's `room`, in pixels, its height
    /// `None` for its own, and the panels each of its content's size within
    /// it, placed as their anchors say; the events of the lists' items that
    /// they ask for and of the hover that the layout moves go to the
    /// gallery, a few rounds at most; the fields scrolled to their carets.
    pub fn lay_out(&mut self, measure: &mut (impl Measure + Carets), room: (f32, Option<f32>)) {
        self.room = (room.0, room.1.unwrap_or(f32::MAX));
        for _ in 0..ROUNDS {
            let mut events = self.lay_out_chrome(measure, room.1);
            let page = (
                Some(self.page.width),
                room.1.map(|_| self.page.height.max(0.0)),
            );
            events.extend(self.main.lay_out(page, measure));
            let panels: Vec<u8> = self.panels.order().collect();
            for panel in panels {
                let Some(place) = self.surfaces.get(&panel).map(|(_, place)| place.clone()) else {
                    continue;
                };
                let anchor = self.anchor(&place);
                let window = self.room;
                let Some((surface, _)) = self.surfaces.get_mut(&panel) else {
                    continue;
                };
                events.extend(surface.lay_out((None, None), measure));
                let mut size = surface.size().unwrap_or_default();
                if size.0 > window.0 {
                    events.extend(surface.lay_out((Some(window.0), None), measure));
                    size = surface.size().unwrap_or_default();
                }
                self.panels.anchor(panel, anchor);
                self.panels.place(panel, size, window);
                events.extend(surface.laid_out());
            }
            events.extend(self.main.laid_out());
            for (surface, region) in self.slots.values_mut() {
                if region.is_some() {
                    events.extend(surface.laid_out());
                }
            }
            if events.is_empty() {
                break;
            }
            self.apply(events);
        }
        let focused = self.focused();
        for panel in std::iter::once(MAIN).chain(self.panels.order().collect::<Vec<_>>()) {
            let node = focused.and_then(|(at, node)| (at == panel).then_some(node));
            if let Some(surface) = self.surface_mut(panel) {
                surface.follow(node, measure);
            }
        }
    }

    /// The last layout, at `now`: the nodes of transitions that it moved
    /// slide, and those whose opacity changed fade, unless `reduced`, as
    /// the system reduces motion.
    pub fn animate(&mut self, now: Instant, reduced: bool) {
        for surface in self.all_mut() {
            surface.animate(now, reduced);
        }
    }

    /// Whether a transition runs at `now`, for which the window draws a
    /// frame at each refresh of the display, and none once they end.
    pub fn animating(&self, now: Instant) -> bool {
        self.all().any(|surface| surface.animating(now))
    }

    /// The window's tree of accessibility, titled `title`: the content,
    /// the chrome's slots and the open panels, in the order that they draw,
    /// the keyboard's focus at its node.
    pub fn accessibility(&self, title: &str) -> TreeUpdate {
        let mut tree = WindowTree::new();
        let mut add = |number: u8, surface: &Surface, panel: Option<Role>| {
            tree.surface(&SurfaceAccess {
                number,
                surface,
                corner: self.corner(number),
                panel,
            });
        };
        add(MAIN, &self.main, None);
        for (&slot, (surface, _)) in &self.slots {
            add(slot, surface, None);
        }
        for panel in self.panels.order() {
            if let Some((surface, _)) = self.surfaces.get(&panel) {
                add(
                    panel,
                    surface,
                    self.panels.options(panel).map(|options| options.role),
                );
            }
        }
        let focus = self
            .focused()
            .map(|(panel, node)| access::node_id(panel, node));
        tree.finish(title, focus)
    }

    /// Does a client's `request` of accessibility to the node, the row or
    /// the tab that it names, whose events the gallery takes, and those of
    /// the focus that it moved.
    pub fn access(&mut self, request: &ActionRequest) {
        let Some(panel) = access::surface_of(request.target_node) else {
            return;
        };
        let surface = match panel {
            MAIN => Some(&mut self.main),
            _ => match self.surfaces.get_mut(&panel) {
                Some((surface, _)) => Some(surface),
                None => self.slots.get_mut(&panel).map(|(surface, _)| surface),
            },
        };
        let Some(surface) = surface else {
            return;
        };
        let events = access::perform(request, (panel, surface), &mut self.focus);
        self.apply(events);
        self.tell_focus();
    }

    /// The events of the surfaces to the gallery, which renders anew.
    fn apply(&mut self, events: Vec<NodeEvent>) {
        if events.is_empty() {
            return;
        }
        for event in events {
            self.gallery.event(&event.key.0, event.kind);
        }
        self.render();
    }

    /// The panel and the node that have the keyboard's focus, if a node has
    /// it.
    pub fn focused(&self) -> Option<(u8, NodeId)> {
        match self.focus.target() {
            Target::Node { panel, node } => Some((panel, node)),
            Target::Pane => None,
        }
    }

    /// Tells the gallery where the focus is, and the focus's and the blur's
    /// events of its move.
    fn tell_focus(&mut self) {
        let key = self
            .focused()
            .and_then(|(panel, node)| self.tree(panel)?.key(node))
            .map(|key| key.0.clone());
        self.gallery.focused(key.as_deref());
        let trees: BTreeMap<u8, &Tree> = std::iter::once((MAIN, &self.main.tree))
            .chain(
                self.surfaces
                    .iter()
                    .map(|(&panel, (surface, _))| (panel, &surface.tree)),
            )
            .collect();
        let events = self.focus.events(|panel| trees.get(&panel).copied());
        let events: Vec<NodeEvent> = events.into_iter().map(|(_, event)| event).collect();
        self.apply(events);
    }

    /// The surface under `at`, in pixels of the window, the captured one
    /// while a button is down, and `at` from its corner.
    fn target(&self, at: (f32, f32)) -> (u8, (f32, f32)) {
        let panel = self.captured.or_else(|| self.under(at)).unwrap_or(MAIN);
        (panel, self.local(panel, at))
    }

    /// The panel under `at`, or the chrome's slot, if one is.
    fn under(&self, at: (f32, f32)) -> Option<u8> {
        self.panels.under(at).or_else(|| self.slot_under(at))
    }

    /// `at`, in pixels of the window, from `panel`'s corner.
    fn local(&self, panel: u8, at: (f32, f32)) -> (f32, f32) {
        let corner = self.corner(panel);
        (at.0 - corner.0, at.1 - corner.1)
    }

    /// The pointer moved to `at`, in pixels of the content, at `now`: the
    /// surface that it left learns of it, the one that it is over, of its
    /// move.
    pub fn moved(&mut self, at: (f32, f32), carets: &mut dyn Carets, now: Instant) {
        self.at = at;
        let (panel, local) = self.target(at);
        let mut events = Vec::new();
        if self.over != Some(panel) {
            if let Some(left) = self.over.and_then(|left| self.surface_mut(left)) {
                events.extend(left.left(now));
            }
            self.over = Some(panel);
        }
        if let Some(surface) = self.surface_mut(panel) {
            events.extend(surface.moved(local, carets, now).0);
        }
        self.apply(events);
    }

    /// `button` went down at `at`, in pixels of the content, of `clicks` in
    /// a row, Shift held or not: the panels that a click outside them
    /// closes close, a menu's closing taking the press; the composition of
    /// any field kept as it stands; the surface under it takes the press,
    /// and its node the focus. Whether a composition was kept, which the
    /// platform's input method is to forget.
    pub fn pressed(
        &mut self,
        at: (f32, f32),
        (button, clicks, shift): (Button, u32, bool),
        carets: &mut dyn Carets,
    ) -> bool {
        self.at = at;
        let mut events = Vec::new();
        let mut kept = false;
        for panel in std::iter::once(MAIN).chain(self.panels.order().collect::<Vec<_>>()) {
            if let Some(surface) = self.surface_mut(panel)
                && let Some(node) = surface.fields.composing(&surface.tree)
            {
                events.extend(
                    surface
                        .fields
                        .keep_composition(&surface.tree, node)
                        .unwrap_or_default(),
                );
                kept = true;
            }
        }
        if self.captured.is_none() {
            let outside = self.panels.pressed(at);
            for panel in &outside.closed {
                self.close(*panel);
                self.gallery.closed(*panel);
            }
            if outside.taken {
                self.apply(events);
                return kept;
            }
        }
        let (panel, local) = self.target(at);
        let Some(surface) = self.surface_mut(panel) else {
            return kept;
        };
        let pressed = surface.pressed(local, (button, clicks, shift), carets);
        events.extend(pressed.events);
        let path = pressed.path;
        self.captured = Some(panel);
        let tree = match panel {
            MAIN => Some(&self.main.tree),
            _ => self.surfaces.get(&panel).map(|(surface, _)| &surface.tree),
        };
        if let Some(tree) = tree {
            self.focus.pressed(panel, tree, &path);
        }
        self.apply(events);
        self.tell_focus();
        kept
    }

    /// `button` went up at `at`, of `clicks` in a row: the surface that it
    /// went down over takes it.
    pub fn released(&mut self, at: (f32, f32), (button, clicks): (Button, u32)) {
        let (panel, local) = self.target(at);
        self.captured = None;
        let events = self
            .surface_mut(panel)
            .map(|surface| surface.released(local, (button, clicks)))
            .unwrap_or_default();
        self.apply(events);
    }

    /// A wheel or a trackpad scrolled at `at`, `delta` points across and
    /// towards the content above: the surface under it takes it.
    pub fn scrolled(&mut self, at: (f32, f32), delta: (f32, f32)) {
        let panel = self.under(at).unwrap_or(MAIN);
        let local = self.local(panel, at);
        let events = self
            .surface_mut(panel)
            .map(|surface| surface.scrolled(local, delta))
            .unwrap_or_default();
        self.apply(events);
    }

    /// `key`, which the window's bindings left: the focus routes it, Tab
    /// reaching every control as `every_control` says; the node that has the
    /// focus takes it, a field cutting, copying and pasting of `clipboard`;
    /// Escape over a panel closes it, if it asks so.
    pub fn key(
        &mut self,
        key: &KeyEvent<'_>,
        clipboard: &mut dyn Clipboard,
        every_control: bool,
    ) -> Keyed {
        self.focus.set_every_control(every_control);
        let before = self.focused();
        let (main, surfaces) = (&self.main, &self.surfaces);
        let routed = self.focus.key(
            |panel| match panel {
                MAIN => Some(&main.tree),
                _ => surfaces.get(&panel).map(|(surface, _)| &surface.tree),
            },
            key,
        );
        match routed {
            Routed::Node { panel, node } => {
                let taken = self
                    .surface_mut(panel)
                    .and_then(|surface| surface.key(node, key, clipboard));
                if let Some(events) = taken {
                    self.apply(events);
                    return Keyed::Taken;
                }
                // Space and Return press the gallery's nodes that take the
                // focus.
                if panel == MAIN && key.action == KeyAction::Press {
                    let name = match key.key {
                        Key::Char(' ') => "Space",
                        Key::Enter => "Return",
                        _ => return Keyed::Left,
                    };
                    if let Some(pressed) = self.main.tree.key(node).map(|key| key.0.clone()) {
                        self.gallery.pressed(&pressed, name);
                        self.render();
                        return Keyed::Taken;
                    }
                }
                Keyed::Left
            }
            Routed::Focus => {
                if key.key == Key::Escape && key.action == KeyAction::Press {
                    self.escape(before.map(|(panel, _)| panel));
                }
                // A key that moves the focus takes the pointer back, as of
                // a drag.
                let mut events = Vec::new();
                for surface in self.all_mut() {
                    events.extend(surface.cancel());
                }
                self.captured = None;
                self.apply(events);
                self.tell_focus();
                // A field that Tab gives the focus selects all of its text.
                if let Some((panel, node)) = self.focused()
                    && let Some(surface) = self.surface_mut(panel)
                {
                    surface.fields.select_all(&surface.tree, node);
                }
                Keyed::Taken
            }
            Routed::Pane => {
                // Escape over no node closes the topmost menu.
                if key.key == Key::Escape && key.action == KeyAction::Press && self.escape(None) {
                    return Keyed::Taken;
                }
                Keyed::Left
            }
        }
    }

    /// Escape went to `panel`, which had the focus, or to none: the panel
    /// closes if it asks so; without one, the topmost menu that asks so.
    /// Whether one closed.
    fn escape(&mut self, panel: Option<u8>) -> bool {
        let closed = match panel {
            Some(MAIN) => None,
            Some(panel) => self.panels.escaped(panel).then_some(panel),
            None => self.panels.escaped_menu(),
        };
        let Some(panel) = closed else {
            return false;
        };
        self.close(panel);
        self.gallery.closed(panel);
        true
    }

    /// The input method composes `text` in the field that has the focus, of
    /// which it selects `selected`; or commits `text` where `commit`.
    pub fn ime(&mut self, text: &str, selected: std::ops::Range<usize>, commit: bool) {
        let Some((panel, node)) = self.focused() else {
            return;
        };
        let events = self.surface_mut(panel).and_then(|surface| {
            if commit {
                surface.commit(node, text)
            } else {
                surface.compose(node, text, selected)
            }
        });
        if let Some(events) = events {
            self.apply(events);
        }
    }

    /// Time passed, to `now`: whether a tooltip that waited shows now.
    pub fn tick(&mut self, now: Instant) -> bool {
        let mut shown = false;
        for surface in self.all_mut() {
            shown |= surface.tooltip.tick(now);
        }
        shown
    }

    /// The time that a tooltip waits for, the soonest, if one waits.
    pub fn deadline(&self) -> Option<Instant> {
        self.all()
            .filter_map(|surface| surface.tooltip.deadline())
            .min()
    }

    /// A key or the window's loss of its focus: the tooltips hide; whether
    /// one showed.
    pub fn dismiss_tooltips(&mut self) -> bool {
        let mut shown = false;
        for surface in self.all_mut() {
            shown |= surface.tooltip.dismiss();
        }
        shown
    }

    /// The window lost the keyboard's focus: the pointer's drags cancel.
    pub fn cancel(&mut self) {
        let mut events = Vec::new();
        for surface in self.all_mut() {
            events.extend(surface.cancel());
        }
        self.captured = None;
        self.dismiss_tooltips();
        self.apply(events);
    }

    /// The display list's items of the content, of the chrome and of the
    /// panels over them, from `origin` in the window's pixels, of `tokens`,
    /// in the window's `room` of pixels, of the transitions at `now`.
    pub fn paint<B: FontBackend>(
        &self,
        font: &mut Font<B>,
        tokens: &Tokens,
        (origin, room): ((f32, f32), (f32, f32)),
        (dark, now): (bool, Instant),
    ) -> Vec<Item> {
        let focused = self.focused();
        let focus_of = |panel: u8| focused.and_then(|(at, node)| (at == panel).then_some(node));
        let mut items = Vec::new();
        for (id, surface, frame) in self.placed() {
            let at = (origin.0 + frame.x, origin.1 + frame.y);
            items.extend(surface.paint(font, tokens, (at, room), (dark, focus_of(id), now)));
        }
        for panel in self.panels.order() {
            let (Some(frame), Some(surface)) = (self.panels.frame(panel), self.surface(panel))
            else {
                continue;
            };
            let at = (origin.0 + frame.x, origin.1 + frame.y);
            items.extend(surface.paint(font, tokens, (at, room), (dark, focus_of(panel), now)));
        }
        items
    }

    /// The areas of the pointer's shapes, in pixels of the window, the
    /// topmost panel's first, as the pointer goes through a tooltip, then
    /// the chrome's and the page's.
    pub fn areas(&self) -> Vec<Area> {
        let mut areas = Vec::new();
        for panel in self.panels.order().rev() {
            let tooltip = self
                .panels
                .options(panel)
                .is_some_and(|options| options.role == Role::Tooltip);
            let (Some(frame), Some(surface), false) =
                (self.panels.frame(panel), self.surface(panel), tooltip)
            else {
                continue;
            };
            areas.extend(
                pointer::areas(&surface.tree, &surface.layout)
                    .into_iter()
                    .map(|area| Area {
                        rect: lokyr_render_core::Rect {
                            x: area.rect.x + frame.x as i32,
                            y: area.rect.y + frame.y as i32,
                            ..area.rect
                        },
                        ..area
                    }),
            );
        }
        let moved = |area: Area, (x, y): (f32, f32)| Area {
            rect: lokyr_render_core::Rect {
                x: area.rect.x + x as i32,
                y: area.rect.y + y as i32,
                ..area.rect
            },
            ..area
        };
        for (_, surface, frame) in self.placed().into_iter().rev() {
            areas.extend(
                pointer::areas(&surface.tree, &surface.layout)
                    .into_iter()
                    .map(|area| moved(area, (frame.x, frame.y))),
            );
        }
        areas
    }

    /// The sweeps of the indeterminate progresses, in pixels of the
    /// window, of their nodes' transitions at `now`, but those that a panel
    /// over them covers.
    pub fn sweeps(&self, tokens: &Tokens, now: Instant) -> Vec<SweepArea> {
        let moved = |sweep: SweepArea, (x, y): (f32, f32)| {
            let moved = |rect: lokyr_render_core::Rect| lokyr_render_core::Rect {
                x: rect.x + x as i32,
                y: rect.y + y as i32,
                ..rect
            };
            SweepArea {
                rect: moved(sweep.rect),
                visible: moved(sweep.visible),
                ..sweep
            }
        };
        let mut sweeps = Vec::new();
        for (_, surface, frame) in self.placed() {
            sweeps.extend(
                surface
                    .sweeps(tokens, now)
                    .into_iter()
                    .map(|sweep| moved(sweep, (frame.x, frame.y))),
            );
        }
        sweeps.retain(|sweep| !self.panels.covers(sweep.visible, None));
        for panel in self.panels.order() {
            let (Some(frame), Some(surface)) = (self.panels.frame(panel), self.surface(panel))
            else {
                continue;
            };
            sweeps.extend(
                surface
                    .sweeps(tokens, now)
                    .into_iter()
                    .map(|sweep| moved(sweep, (frame.x, frame.y)))
                    .filter(|sweep| !self.panels.covers(sweep.visible, Some(panel))),
            );
        }
        sweeps
    }

    /// The input method's rectangle of the field that has the focus, in
    /// pixels of the window.
    pub fn text_area(&self, carets: &mut dyn Carets) -> Option<Frame> {
        let (panel, node) = self.focused()?;
        let area = self.surface(panel)?.text_area(node, carets)?;
        let (x, y) = self.corner(panel);
        Some(Frame {
            x: area.x + x,
            y: area.y + y,
            ..area
        })
    }
}

/// The id of `slot`'s surface.
fn slot_id(slot: Slot) -> u8 {
    SLOTS
        .iter()
        .find(|(_, of)| *of == slot)
        .map_or(0, |(id, _)| *id)
}

/// The choices of `entries`, of their submenus' too, by their keys: their
/// nodes' events, but a disabled item's.
fn collect(entries: &[MenuEntry], choices: &mut BTreeMap<String, (NodeKey, UiEventKind)>) {
    for entry in entries {
        match entry {
            MenuEntry::Item {
                key,
                event: Some(event),
                ..
            } => {
                choices.insert(key.0.clone(), (key.clone(), event.clone()));
            }
            MenuEntry::Submenu { entries, .. } => collect(entries, choices),
            MenuEntry::Item { .. } | MenuEntry::Separator => {}
        }
    }
}

/// The point of `anchor` where a native menu goes, its top left corner.
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
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    use lokyr_ui_core::theme::Theme;
    use lokyr_vt_core::input::Mods;

    use super::*;
    use crate::gallery::NOTE as NOTE_PANEL;

    /// A font of the tests': a character 7 points wide, a text 14 high, an
    /// icon 16.
    struct Fixed;

    impl Measure for Fixed {
        fn text(&mut self, text: &str) -> (f32, f32) {
            (7.0 * text.chars().count() as f32, 14.0)
        }

        fn icon(&mut self, _: &str) -> (f32, f32) {
            (16.0, 16.0)
        }
    }

    impl Carets for Fixed {
        fn carets(&mut self, text: &str) -> Rc<[(usize, f32)]> {
            let mut carets: Vec<(usize, f32)> = text
                .char_indices()
                .enumerate()
                .map(|(index, (at, _))| (at, index as f32 * 7.0))
                .collect();
            carets.push((text.len(), text.chars().count() as f32 * 7.0));
            carets.into()
        }
    }

    struct Board;

    impl Clipboard for Board {
        fn read(&mut self) -> Option<String> {
            None
        }

        fn write(&mut self, _: &str) {}
    }

    const ROOM: (f32, f32) = (1120.0, 860.0);

    /// The gallery's view, laid out in the window's room, at a scale of 1.
    fn view() -> View {
        let mut view = View::new(Gallery::new(false), 1.0);
        view.lay_out(&mut Fixed, (ROOM.0, Some(ROOM.1)));
        view
    }

    /// The middle of the content's node of `key`, scrolled into the page's
    /// view first.
    fn middle(view: &mut View, key: &str) -> (f32, f32) {
        let tree = &view.main.tree;
        let node = tree.by_key(&key.into()).unwrap();
        let page = tree.by_key(&"page".into()).unwrap();
        let frame = view.main.layout.frame(tree, node).unwrap();
        let shown = view.main.layout.frame(tree, page).unwrap();
        if frame.y + frame.height > shown.y + shown.height {
            let down = frame.y + frame.height - (shown.y + shown.height) + 20.0;
            view.main.layout.scroll_by(&view.main.tree, page, down);
            view.lay_out(&mut Fixed, (ROOM.0, Some(ROOM.1)));
        }
        let frame = view.main.layout.frame(&view.main.tree, node).unwrap();
        let page = view.page();
        (
            page.x + frame.x + frame.width / 2.0,
            page.y + frame.y + frame.height / 2.0,
        )
    }

    /// The middle of the node of `key` of `slot`'s view, in the window.
    fn slot_middle(view: &View, slot: Slot, key: &str) -> (f32, f32) {
        let id = slot_id(slot);
        let surface = view.surface(id).unwrap();
        let node = surface.tree.by_key(&key.into()).unwrap();
        let frame = surface.layout.frame(&surface.tree, node).unwrap();
        let region = view.region(id).unwrap();
        (
            region.x + frame.x + frame.width / 2.0,
            region.y + frame.y + frame.height / 2.0,
        )
    }

    /// The background of the tab bar's node of `key`.
    fn tab_background(view: &View, key: &str) -> Option<String> {
        let surface = view.surface(slot_id(Slot::TabBar))?;
        let node = surface.tree.by_key(&key.into())?;
        Some(surface.tree.style(node)?.background.clone()?.0)
    }

    fn frame(x: f32, y: f32, width: f32, height: f32) -> Frame {
        Frame {
            x,
            y,
            width,
            height,
        }
    }

    #[test]
    fn the_chrome_s_slots_take_their_regions_and_the_page_the_rest() {
        let mut view = view();
        view.set_titlebar(40.0, 80.0);
        view.lay_out(&mut Fixed, (ROOM.0, Some(ROOM.1)));
        // The tab bar in the titlebar, past the window's buttons.
        assert_eq!(
            view.region(slot_id(Slot::TabBar)),
            Some(frame(80.0, 0.0, ROOM.0 - 80.0, 40.0))
        );
        // The strip over the pane's top, its header there too, and the
        // status bar across the bottom.
        assert_eq!(
            view.region(slot_id(Slot::WindowStrip)),
            Some(frame(0.0, 40.0, ROOM.0, 2.0))
        );
        let header = view.region(slot_id(Slot::PaneHeader)).unwrap();
        let status = view.region(slot_id(Slot::StatusBar)).unwrap();
        assert_eq!((header.x, header.y, header.width), (0.0, 40.0, ROOM.0));
        assert_eq!(status.y + status.height, ROOM.1);
        assert!(header.height > 0.0 && status.height > 0.0);
        // The page between them, its root of its room.
        let page = view.page();
        assert_eq!(
            page,
            frame(
                0.0,
                40.0 + header.height,
                ROOM.0,
                status.y - 40.0 - header.height
            )
        );
        assert_eq!(view.main.size(), Some((page.width, page.height)));
        // The tab bar's tabs and its button are its own in the titlebar,
        // its bare room the window's.
        let areas = view.titlebar_areas();
        assert_eq!(areas.len(), 4, "{areas:?}");
        for area in &areas {
            assert!(
                area.x >= 80.0 && area.y >= 0.0 && area.y + area.height <= 40.0,
                "{area:?}"
            );
        }
        let bar = view.region(slot_id(Slot::TabBar)).unwrap();
        let last = areas.last().unwrap();
        assert!(last.x + last.width < bar.x + bar.width);
    }

    #[test]
    fn a_click_on_a_tab_chooses_it_and_the_page_s_nodes_take_theirs_below_the_chrome() {
        let mut view = view();
        view.set_titlebar(40.0, 80.0);
        view.lay_out(&mut Fixed, (ROOM.0, Some(ROOM.1)));
        let tab = slot_middle(&view, Slot::TabBar, "bar.tab-1");
        click(&mut view, tab);
        assert_eq!(
            tab_background(&view, "bar.tab-1").as_deref(),
            Some("tab.active.bg")
        );
        assert_eq!(
            tab_background(&view, "bar.tab-0").as_deref(),
            Some("tab.inactive.bg")
        );
        // A new tab, chosen.
        let new = slot_middle(&view, Slot::TabBar, "bar.new");
        click(&mut view, new);
        assert_eq!(
            tab_background(&view, "bar.tab-3").as_deref(),
            Some("tab.active.bg")
        );
        assert_eq!(view.titlebar_areas().len(), 5);
        // The page's toggle, below the chrome, at its place in the window.
        let at = middle(&mut view, "anim.boxed");
        click(&mut view, at);
        let boxed = view.main.tree.by_key(&"anim.boxed".into()).unwrap();
        assert!(matches!(
            view.main.tree.kind(boxed),
            Some(lokyr_plugin_sdk::view::Kind::Toggle(toggle)) if !toggle.on
        ));
        // The pointer over a tab is over it.
        let at = slot_middle(&view, Slot::TabBar, "bar.tab-0");
        view.moved(at, &mut Fixed, Instant::now());
        view.lay_out(&mut Fixed, (ROOM.0, Some(ROOM.1)));
        assert_eq!(
            tab_background(&view, "bar.tab-0").as_deref(),
            Some("tab.hover.bg")
        );
    }

    #[test]
    fn the_tab_bar_goes_below_a_titled_titlebar_and_takes_the_top_in_full_screen() {
        let mut view = view();
        view.set_titlebar(40.0, 80.0);
        view.lay_out(&mut Fixed, (ROOM.0, Some(ROOM.1)));
        let toggle = slot_middle(&view, Slot::StatusBar, "status.titlebar");
        click(&mut view, toggle);
        assert_eq!(view.gallery.tab_bar_place(), TabBarPlace::BelowTitlebar);
        // The platform's titlebar of the title, 32 points high.
        view.set_titlebar(32.0, 80.0);
        view.lay_out(&mut Fixed, (ROOM.0, Some(ROOM.1)));
        let bar = view.region(slot_id(Slot::TabBar)).unwrap();
        assert_eq!((bar.x, bar.y, bar.width), (0.0, 32.0, ROOM.0));
        assert!(bar.height > 0.0);
        assert!(view.titlebar_areas().is_empty());
        assert_eq!(
            view.page().y,
            bar.y + bar.height + view.region(slot_id(Slot::PaneHeader)).unwrap().height
        );
        // A tab below the titlebar takes its clicks there.
        let tab = slot_middle(&view, Slot::TabBar, "bar.tab-2");
        click(&mut view, tab);
        assert_eq!(
            tab_background(&view, "bar.tab-2").as_deref(),
            Some("tab.active.bg")
        );
        // Back in the titlebar, then in full screen: no titlebar, no
        // buttons, the bar at the top of its view's height.
        let toggle = slot_middle(&view, Slot::StatusBar, "status.titlebar");
        click(&mut view, toggle);
        view.set_titlebar(0.0, 0.0);
        view.lay_out(&mut Fixed, (ROOM.0, Some(ROOM.1)));
        let bar = view.region(slot_id(Slot::TabBar)).unwrap();
        assert_eq!((bar.x, bar.y, bar.width), (0.0, 0.0, ROOM.0));
        assert!(bar.height > 0.0 && bar.height < 40.0, "{bar:?}");
        assert!(view.titlebar_areas().is_empty());
    }

    /// Whether `rect` of the display list is `frame`'s.
    fn is(rect: lokyr_render_core::Rect, frame: Frame) -> bool {
        (rect.x, rect.y, rect.width, rect.height)
            == (
                frame.x as i32,
                frame.y as i32,
                frame.width as u32,
                frame.height as u32,
            )
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn the_chrome_draws_in_its_regions_over_the_page() {
        use lokyr_font_coretext::CoreText;
        use lokyr_render_core::Shape;
        use lokyr_text_core::FontConfig;

        let mut view = view();
        view.set_titlebar(40.0, 80.0);
        view.lay_out(&mut Fixed, (ROOM.0, Some(ROOM.1)));
        let mut font = Font::new(CoreText::new(), FontConfig::default(), 1.0).unwrap();
        let tokens = Theme::builtin().variant(false).tokens.clone();
        let items = view.paint(
            &mut font,
            &tokens,
            ((0.0, 0.0), ROOM),
            (false, Instant::now()),
        );
        let shapes: Vec<&Shape> = items
            .iter()
            .filter_map(|item| match item {
                Item::Shapes(shapes) => Some(shapes.iter()),
                _ => None,
            })
            .flatten()
            .collect();
        // Each bar's background, of its region.
        for slot in [Slot::TabBar, Slot::StatusBar, Slot::PaneHeader] {
            let region = view.region(slot_id(slot)).unwrap();
            assert!(
                shapes.iter().any(|shape| is(shape.rect, region)),
                "{slot:?} {region:?}"
            );
        }
        // The page's, past the chrome, and the bars after it, over it.
        let page = shapes
            .iter()
            .position(|shape| shape.rect.y >= view.page().y as i32)
            .unwrap();
        let bar = view.region(slot_id(Slot::TabBar)).unwrap();
        let tab_bar = shapes.iter().position(|shape| is(shape.rect, bar)).unwrap();
        assert!(page < tab_bar);
    }

    #[test]
    fn the_page_s_pointer_areas_and_its_field_s_place_follow_it_below_the_chrome() {
        let mut view = view();
        view.set_titlebar(40.0, 80.0);
        view.lay_out(&mut Fixed, (ROOM.0, Some(ROOM.1)));
        let at = middle(&mut view, "i-beam");
        let inside = |rect: lokyr_render_core::Rect| {
            at.0 >= rect.x as f32
                && at.1 >= rect.y as f32
                && at.0 < (rect.x + rect.width as i32) as f32
                && at.1 < (rect.y + rect.height as i32) as f32
        };
        assert!(
            view.areas().iter().any(|area| area.shape
                == lokyr_plugin_sdk::view::PointerShape::Text
                && inside(area.rect)),
            "{at:?}"
        );
        let at = middle(&mut view, "name");
        click(&mut view, at);
        let area = view.text_area(&mut Fixed).unwrap();
        assert!(
            area.y <= at.1 && at.1 <= area.y + area.height && area.y >= view.page().y,
            "{area:?} {at:?}"
        );
    }

    #[test]
    fn the_strip_takes_the_pointer_over_the_header_and_a_tab_its_scroll() {
        let mut view = view();
        view.set_titlebar(40.0, 80.0);
        view.lay_out(&mut Fixed, (ROOM.0, Some(ROOM.1)));
        // The strip over the pane's header, which it covers.
        assert_eq!(view.target((10.0, 41.0)).0, slot_id(Slot::WindowStrip));
        assert_eq!(view.target((10.0, 50.0)).0, slot_id(Slot::PaneHeader));
        let tab = slot_middle(&view, Slot::TabBar, "bar.tab-1");
        view.scrolled(tab, (0.0, 3.0));
        assert!(
            view.gallery.scrolled().starts_with("`bar.tab-1`"),
            "{}",
            view.gallery.scrolled()
        );
    }

    #[test]
    fn a_panel_below_its_button_follows_the_page_below_the_chrome() {
        let mut view = view();
        view.set_titlebar(40.0, 80.0);
        view.lay_out(&mut Fixed, (ROOM.0, Some(ROOM.1)));
        let at = middle(&mut view, "open-below");
        click(&mut view, at);
        let button = view.main.tree.by_key(&"open-below".into()).unwrap();
        let frame = view.main.layout.frame(&view.main.tree, button).unwrap();
        let page = view.page();
        let (top, bottom) = (page.y + frame.y, page.y + frame.y + frame.height);
        let panel = view.panels.frame(BELOW).unwrap();
        assert!(
            panel.y == bottom || panel.y + panel.height == top,
            "{panel:?} {top} {bottom}"
        );
    }

    #[test]
    fn the_find_bar_keeps_to_the_page_and_the_strip_s_sweep_shows_over_it() {
        let mut view = view();
        view.set_titlebar(40.0, 80.0);
        view.lay_out(&mut Fixed, (ROOM.0, Some(ROOM.1)));
        let at = middle(&mut view, "open-find");
        click(&mut view, at);
        let find = view.panels.frame(FIND).unwrap();
        let page = view.page();
        assert!(find.y >= page.y + MARGIN, "{find:?} {page:?}");
        assert_eq!(find.x + find.width, page.x + page.width - MARGIN);
        // The strip's sweep, at the pane's top, once it spins.
        let tokens = Theme::builtin().variant(false).tokens.clone();
        let strip = |view: &View| {
            view.sweeps(&tokens, Instant::now())
                .into_iter()
                .filter(|sweep| sweep.rect.y == 40 && sweep.rect.height == 2)
                .count()
        };
        assert_eq!(strip(&view), 0);
        let spin = middle(&mut view, "spin");
        click(&mut view, spin);
        assert_eq!(strip(&view), 1);
        // The page's, below the chrome, as the page is.
        let sweeps = view.sweeps(&tokens, Instant::now());
        assert!(sweeps.len() > 1);
        assert!(sweeps.iter().all(|sweep| sweep.rect.y >= 40), "{sweeps:?}");
    }

    fn click(view: &mut View, at: (f32, f32)) {
        view.pressed(at, (Button::Primary, 1, false), &mut Fixed);
        view.released(at, (Button::Primary, 1));
        view.lay_out(&mut Fixed, (ROOM.0, Some(ROOM.1)));
    }

    fn focused_key(view: &View) -> Option<(u8, String)> {
        let (panel, node) = view.focused()?;
        Some((panel, view.tree(panel)?.key(node)?.0.clone()))
    }

    fn press(key: Key, text: &str) -> KeyEvent<'_> {
        KeyEvent {
            key,
            action: KeyAction::Press,
            mods: Mods::NONE,
            text,
            unmodified: text,
            base_layout: None,
        }
    }

    #[test]
    fn a_menu_opens_at_the_pointer_takes_the_focus_and_a_click_outside_closes_it_alone() {
        let mut view = view();
        let at = middle(&mut view, "open-menu");
        // Without the system's keyboard navigation, its items take no
        // focus, and Escape closes it all the same.
        click(&mut view, at);
        assert!(view.panels.is_open(MENU));
        assert_eq!(view.focused(), None);
        assert_eq!(
            view.key(&press(Key::Escape, ""), &mut Board, false),
            Keyed::Taken
        );
        assert!(!view.panels.is_open(MENU));
        assert_eq!(
            view.key(&press(Key::Escape, ""), &mut Board, false),
            Keyed::Left
        );
        view.focus.set_every_control(true);
        click(&mut view, at);
        assert!(view.panels.is_open(MENU));
        // At the pointer, upwards from it where the window has no room
        // below.
        let frame = view.panels.frame(MENU).unwrap();
        assert_eq!(frame.x, at.0.round());
        let below = frame.y == at.1.round();
        let above = frame.y + frame.height == at.1;
        assert!(below || above, "{frame:?} {at:?}");
        // The keyboard's focus in it, on its first item, which Tab keeps
        // within it.
        assert_eq!(focused_key(&view), Some((MENU, "menu.copy".to_owned())));
        view.key(&press(Key::Tab, ""), &mut Board, true);
        assert_eq!(focused_key(&view), Some((MENU, "menu.paste".to_owned())));
        // A click outside it, on the find bar's button: the menu closes, and
        // the click goes no further.
        let find = middle(&mut view, "open-find");
        click(&mut view, find);
        assert!(!view.panels.is_open(MENU));
        assert!(!view.panels.is_open(FIND));
        assert_eq!(view.focused(), None);
        // Chosen, an item closes it.
        click(&mut view, at);
        let frame = view.panels.frame(MENU).unwrap();
        let item = view.surface(MENU).unwrap();
        let split = item.tree.by_key(&"menu.split".into()).unwrap();
        let place = item.layout.frame(&item.tree, split).unwrap();
        click(
            &mut view,
            (frame.x + place.x + 4.0, frame.y + place.y + 4.0),
        );
        assert!(!view.panels.is_open(MENU));
    }

    #[test]
    fn a_press_on_a_panel_closes_those_over_it_and_a_menu_gives_the_focus_back() {
        let mut view = view();
        view.focus.set_every_control(true);
        let open = |view: &mut View, key: &str| {
            let at = middle(view, key);
            click(view, at);
        };
        // The find bar, then the panel below its button over it: a press
        // in the bar's field closes the panel, which a click outside
        // closes, and the field takes the press.
        open(&mut view, "open-find");
        open(&mut view, "open-below");
        assert!(view.panels.is_open(BELOW));
        let bar = view.panels.frame(FIND).unwrap();
        let surface = view.surface(FIND).unwrap();
        let field = surface.tree.by_key(&"find-bar.field".into()).unwrap();
        let place = surface.layout.frame(&surface.tree, field).unwrap();
        click(&mut view, (bar.x + place.x + 8.0, bar.y + place.y + 8.0));
        assert!(!view.panels.is_open(BELOW) && view.panels.is_open(FIND));
        assert_eq!(
            focused_key(&view),
            Some((FIND, "find-bar.field".to_owned()))
        );
        // A menu opened with the focus in the field, which its button
        // leaves there, gives it back to the field as Escape closes it.
        open(&mut view, "open-menu");
        assert_eq!(focused_key(&view), Some((MENU, "menu.copy".to_owned())));
        view.key(&press(Key::Escape, ""), &mut Board, true);
        assert!(!view.panels.is_open(MENU));
        assert_eq!(
            focused_key(&view),
            Some((FIND, "find-bar.field".to_owned()))
        );
        // And as an item chosen closes it.
        open(&mut view, "open-menu");
        let frame = view.panels.frame(MENU).unwrap();
        let menu = view.surface(MENU).unwrap();
        let split = menu.tree.by_key(&"menu.split".into()).unwrap();
        let place = menu.layout.frame(&menu.tree, split).unwrap();
        click(
            &mut view,
            (frame.x + place.x + 4.0, frame.y + place.y + 4.0),
        );
        assert!(!view.panels.is_open(MENU));
        assert_eq!(
            focused_key(&view),
            Some((FIND, "find-bar.field".to_owned()))
        );
    }

    #[test]
    fn a_click_on_a_tab_of_the_view_of_tabs_shows_its_page() {
        let mut view = view();
        // Into view, then the second tab's middle.
        middle(&mut view, "views");
        let tree = &view.main.tree;
        let node = tree.by_key(&"views".into()).unwrap();
        let segment = view.main.layout.segments(tree, node)[1];
        let page = view.page();
        let at = (
            page.x + segment.x + segment.width / 2.0,
            page.y + segment.y + segment.height / 2.0,
        );
        click(&mut view, at);
        let text = |view: &View, text: &str| {
            let tree = &view.main.tree;
            let page = tree.by_key(&"views.page".into()).unwrap();
            let mut found = false;
            let mut stack = vec![page];
            while let Some(id) = stack.pop() {
                found |=
                    tree.kind(id) == Some(&lokyr_plugin_sdk::view::Kind::Text(text.to_owned()));
                stack.extend(tree.children(id));
            }
            found
        };
        assert!(text(&view, "The appearance's page"));
        // The disabled one's, nothing.
        let tree = &view.main.tree;
        let node = tree.by_key(&"views".into()).unwrap();
        let disabled = view.main.layout.segments(tree, node)[3];
        let page = view.page();
        click(
            &mut view,
            (
                page.x + disabled.x + disabled.width / 2.0,
                page.y + disabled.y + disabled.height / 2.0,
            ),
        );
        assert!(text(&view, "The appearance's page"));
        // Its tabs of equal widths, as its toggle asks.
        let at = middle(&mut view, "views.fill-equally");
        click(&mut view, at);
        let tree = &view.main.tree;
        let node = tree.by_key(&"views".into()).unwrap();
        assert!(matches!(
            tree.kind(node),
            Some(lokyr_plugin_sdk::view::Kind::Tabs(tabs))
                if tabs.distribution == lokyr_plugin_sdk::view::Distribution::FillEqually
        ));
        let widths: Vec<f32> = view
            .main
            .layout
            .segments(tree, node)
            .iter()
            .map(|segment| segment.width)
            .collect();
        assert!(
            widths
                .windows(2)
                .all(|pair| (pair[0] - pair[1]).abs() < 0.01),
            "{widths:?}"
        );
    }

    #[test]
    fn a_transition_runs_to_its_end_and_no_frame_is_asked_for_after_it() {
        use std::time::Duration;
        let mut view = view();
        let start = Instant::now();
        view.animate(start, false);
        assert!(!view.animating(start));
        // The last tab moved first: the tabs slide, for 0.3 seconds.
        let at = middle(&mut view, "anim.shuffle");
        view.animate(start, false);
        click(&mut view, at);
        view.animate(start, false);
        assert!(view.animating(start));
        let tab = view.main.tree.by_key(&"anim.tab-3".into()).unwrap();
        let place = view.main.layout.frame(&view.main.tree, tab).unwrap();
        let shown = view.main.animations.offset(&"anim.tab-3".into(), start);
        assert!(shown.0 > 0.0, "{shown:?} {place:?}");
        assert!(view.animating(start + Duration::from_millis(299)));
        assert!(!view.animating(start + Duration::from_millis(300)));
        // The box hidden fades, for 0.2 seconds.
        let later = start + Duration::from_secs(1);
        let toggle = middle(&mut view, "anim.boxed");
        view.animate(later, false);
        click(&mut view, toggle);
        view.animate(later, false);
        let faded = view.main.animations.opacity(
            &"anim.box".into(),
            0.0,
            later + Duration::from_millis(100),
        );
        assert!(faded > 0.0 && faded < 1.0, "{faded}");
        assert!(!view.animating(later + Duration::from_millis(200)));
        // Where the system reduces motion, at once.
        let at = middle(&mut view, "anim.shuffle");
        let after = later + Duration::from_secs(1);
        view.animate(after, true);
        click(&mut view, at);
        view.animate(after, true);
        assert!(!view.animating(after));
        // In slow motion, the tabs slide for 3 seconds.
        let slow = after + Duration::from_secs(1);
        let toggle = middle(&mut view, "anim.slow");
        view.animate(slow, false);
        click(&mut view, toggle);
        view.animate(slow, false);
        click(&mut view, at);
        view.animate(slow, false);
        assert!(view.animating(slow + Duration::from_millis(2999)));
        assert!(!view.animating(slow + Duration::from_millis(3000)));
    }

    #[test]
    fn a_menu_of_menu_items_shows_as_the_platform_s_own_and_its_choice_comes_back() {
        let mut view = view();
        view.native_menus = true;
        let at = middle(&mut view, "open-menu");
        click(&mut view, at);
        // Drawn nowhere, of no focus: the platform's, at the pointer.
        assert!(!view.panels.is_open(MENU));
        assert_eq!(view.focused(), None);
        let (entries, place) = view.take_native().unwrap();
        assert_eq!(place, at);
        assert!(view.take_native().is_none());
        let titles: Vec<&str> = entries
            .iter()
            .filter_map(|entry| match entry {
                MenuEntry::Item { title, .. } | MenuEntry::Submenu { title, .. } => {
                    Some(title.as_str())
                }
                MenuEntry::Separator => None,
            })
            .collect();
        assert_eq!(
            titles,
            [
                "Copy",
                "Paste",
                "Hidden Files",
                "Split Right",
                "Open Recent"
            ]
        );
        // A toggle's choice switches it, ticked the next time; an item of a
        // submenu's chosen; another command, nothing.
        view.chosen("menu.hidden");
        let toggled = view.gallery.panel(MENU);
        let mut tree = Tree::new();
        tree.update(&toggled).unwrap();
        let hidden = tree.by_key(&"menu.hidden".into()).unwrap();
        assert!(matches!(
            tree.kind(hidden),
            Some(lokyr_plugin_sdk::view::Kind::Toggle(toggle)) if toggle.on
        ));
        view.chosen("menu.recent.lokyr");
        assert_eq!(view.gallery.last_panel(), "chose `menu.recent.lokyr`");
        click(&mut view, at);
        let (entries, _) = view.take_native().unwrap();
        assert!(entries.iter().any(|entry| matches!(
            entry,
            MenuEntry::Item { title, ticked: true, .. } if title == "Hidden Files"
        )));
        view.chosen("pane.copy");
        assert_eq!(view.gallery.last_panel(), "opened by `open-menu`");
        assert!(!view.panels.is_open(MENU));
    }

    #[test]
    fn escape_closes_the_find_bar_which_a_click_outside_leaves_and_its_field_takes_the_keys() {
        let mut view = view();
        let at = middle(&mut view, "open-find");
        click(&mut view, at);
        // In the window's top right corner, 12 points in.
        let frame = view.panels.frame(FIND).unwrap();
        assert_eq!(frame.y, 12.0);
        assert_eq!(frame.x + frame.width, ROOM.0 - 12.0);
        assert_eq!(
            focused_key(&view),
            Some((FIND, "find-bar.field".to_owned()))
        );
        // Its field takes the typing and the input method's composition,
        // whose rectangle is in the window's pixels.
        view.key(&press(Key::Char('a'), "a"), &mut Board, false);
        view.ime("にほ", 6..6, false);
        let field = view
            .surface(FIND)
            .unwrap()
            .fields
            .state(&"find-bar.field".into())
            .unwrap();
        assert_eq!(field.text(), "a");
        assert!(field.composition().is_some());
        let area = view.text_area(&mut Fixed).unwrap();
        assert!(area.x > frame.x && area.y > frame.y);
        // A click outside it keeps the composition, and the bar.
        let kept = view.pressed((20.0, 400.0), (Button::Primary, 1, false), &mut Fixed);
        view.released((20.0, 400.0), (Button::Primary, 1));
        assert!(kept);
        assert!(view.panels.is_open(FIND));
        let field = view
            .surface(FIND)
            .unwrap()
            .fields
            .state(&"find-bar.field".into())
            .unwrap();
        assert_eq!(field.text(), "aにほ");
        // A press in its field selects as the pointer moves, until Tab
        // takes the pointer back, as the window's loss of the focus does.
        let bar = view.surface(FIND).unwrap();
        let node = bar.tree.by_key(&"find-bar.field".into()).unwrap();
        let place = bar.layout.frame(&bar.tree, node).unwrap();
        let inside = (frame.x + place.x + 8.0, frame.y + place.y + 8.0);
        let selecting = |view: &View| view.surface(FIND).unwrap().fields.selecting();
        view.pressed(inside, (Button::Primary, 1, false), &mut Fixed);
        assert!(selecting(&view));
        view.key(&press(Key::Tab, ""), &mut Board, true);
        assert!(!selecting(&view));
        view.pressed(inside, (Button::Primary, 1, false), &mut Fixed);
        view.cancel();
        assert!(!selecting(&view));
        view.released(inside, (Button::Primary, 1));
        click(&mut view, inside);
        // Escape, its focus in it, closes it.
        assert_eq!(
            view.key(&press(Key::Escape, ""), &mut Board, false),
            Keyed::Taken
        );
        assert!(!view.panels.is_open(FIND));
        assert_eq!(view.focused(), None);
    }

    #[test]
    fn a_panel_s_tooltip_waits_shows_and_hides_as_the_content_s() {
        use std::time::Duration;
        let mut view = view();
        let at = middle(&mut view, "open-find");
        click(&mut view, at);
        let frame = view.panels.frame(FIND).unwrap();
        let bar = view.surface(FIND).unwrap();
        let case = bar.tree.by_key(&"find-bar.case".into()).unwrap();
        let place = bar.layout.frame(&bar.tree, case).unwrap();
        let over = (frame.x + place.x + 4.0, frame.y + place.y + 4.0);
        let start = Instant::now();
        view.moved(over, &mut Fixed, start);
        let deadline = view.deadline().unwrap();
        assert!(view.tick(deadline));
        let tip = view.surface(FIND).unwrap();
        assert_eq!(
            tip.tooltip.shown(&tip.tree).map(|(text, _)| text),
            Some("Match Case")
        );
        assert!(view.dismiss_tooltips());
        assert!(!view.tick(deadline + Duration::from_secs(5)));
    }

    #[test]
    fn a_panel_keeps_within_the_window_as_it_shrinks() {
        let mut view = view();
        let at = middle(&mut view, "open-find");
        click(&mut view, at);
        let width = view.panels.frame(FIND).unwrap().width;
        view.lay_out(&mut Fixed, (500.0, Some(300.0)));
        let frame = view.panels.frame(FIND).unwrap();
        assert_eq!(frame.x + frame.width, 500.0 - 12.0);
        // Narrower than it: laid out of the window's width, at its left.
        view.lay_out(&mut Fixed, (width - 40.0, Some(300.0)));
        let frame = view.panels.frame(FIND).unwrap();
        assert_eq!(frame.x, 0.0);
        assert!(frame.width <= width - 40.0);
        let laid = view.surface(FIND).unwrap().size().unwrap();
        assert!(laid.0 <= width - 40.0, "{laid:?}");
    }

    #[test]
    fn the_pointer_goes_through_a_note_and_a_panel_covers_the_sweeps_under_it() {
        let mut view = view();
        let at = middle(&mut view, "open-note");
        click(&mut view, at);
        assert!(view.panels.is_open(NOTE_PANEL));
        let note = view.panels.frame(NOTE_PANEL).unwrap();
        assert_eq!(note.x, 12.0);
        assert_eq!(note.y + note.height, ROOM.1 - 12.0);
        // It takes no focus, nor the pointer, nor a pointer's area.
        assert_eq!(view.focused(), None);
        let inside = (note.x + 4.0, note.y + 4.0);
        assert_eq!(view.panels.under(inside), None);
        assert_eq!(
            view.areas(),
            pointer::areas(&view.main.tree, &view.main.layout)
        );
        // The indeterminate bar's sweep shows, its section scrolled into
        // view; a menu over it leaves it out.
        let tokens = &Theme::builtin().light.tokens;
        middle(&mut view, "less");
        let before = view.sweeps(tokens, Instant::now());
        let sweep = *before
            .iter()
            .find(|sweep| matches!(sweep.shape, lokyr_ui_core::progress::SweepShape::Bar { .. }))
            .unwrap();
        let at = middle(&mut view, "open-menu");
        click(&mut view, at);
        if let Some((_, place)) = view.surfaces.get_mut(&MENU) {
            *place = Place::Pointer((sweep.visible.x as f32 + 4.0, sweep.visible.y as f32 - 4.0));
        }
        view.lay_out(&mut Fixed, (ROOM.0, Some(ROOM.1)));
        let after = view.sweeps(tokens, Instant::now());
        assert!(after.len() < before.len());
        assert!(!after.iter().any(|kept| kept.rect == sweep.rect));
    }

    /// A client's `action` of the node `target`, of `data`.
    fn asked(
        action: accesskit::Action,
        target: accesskit::NodeId,
        data: Option<accesskit::ActionData>,
    ) -> ActionRequest {
        ActionRequest {
            action,
            target_tree: accesskit::TreeId::ROOT,
            target_node: target,
            data,
        }
    }

    /// The id of the node of `key` of the surface `panel`, and its node in
    /// `tree`.
    fn of(
        view: &View,
        tree: &TreeUpdate,
        panel: u8,
        key: &str,
    ) -> (accesskit::NodeId, accesskit::Node) {
        let node = view
            .surface(panel)
            .unwrap()
            .tree
            .by_key(&key.into())
            .unwrap();
        let id = access::node_id(panel, node);
        let found = tree.nodes.iter().find(|(at, _)| *at == id).unwrap();
        (id, found.1.clone())
    }

    #[test]
    fn accessibility_reads_presses_and_fills_the_gallery_s_widgets() {
        use accesskit::{Action, ActionData, Role, Toggled};
        let mut view = view();
        let tree = view.accessibility("Gallery");
        assert_eq!(tree.focus, lokyr_ui_core::access::WINDOW);
        // The field, of its text; the toggle, of its state; the list, of
        // its label and its rows.
        let (name, field) = of(&view, &tree, MAIN, "name");
        assert_eq!(field.role(), Role::TextInput);
        assert!(field.value().unwrap().starts_with("Lokyr"));
        let (spin, toggle) = of(&view, &tree, MAIN, "spin");
        assert_eq!(toggle.toggled(), Some(Toggled::False));
        let (rows, list) = of(&view, &tree, MAIN, "rows");
        assert_eq!((list.role(), list.label()), (Role::ListBox, Some("Rows")));
        assert!(!list.children().is_empty());
        // A press switches the toggle, a value set fills the field, and a
        // focus moves the keyboard's to the list.
        view.access(&asked(Action::Click, spin, None));
        view.lay_out(&mut Fixed, (ROOM.0, Some(ROOM.1)));
        let value = Some(ActionData::Value("main".into()));
        view.access(&asked(Action::SetValue, name, value));
        view.access(&asked(Action::Focus, rows, None));
        let tree = view.accessibility("Gallery");
        assert_eq!(
            of(&view, &tree, MAIN, "spin").1.toggled(),
            Some(Toggled::True)
        );
        assert_eq!(of(&view, &tree, MAIN, "name").1.value(), Some("main"));
        assert_eq!(tree.focus, rows);
        assert!(
            tree.nodes
                .iter()
                .any(|(_, node)| node.label() == Some("events: focus of `rows`"))
        );
        // The find bar, a group of its label over the content, whose field
        // a value set fills too.
        view.gallery.event("open-find", UiEventKind::Click);
        view.render();
        view.lay_out(&mut Fixed, (ROOM.0, Some(ROOM.1)));
        let tree = view.accessibility("Gallery");
        let bar = view.surface(FIND).unwrap();
        let root = access::node_id(FIND, bar.tree.root().unwrap());
        let window = &tree.nodes[0].1;
        assert_eq!(window.children().last(), Some(&root));
        let group = &tree.nodes.iter().find(|(at, _)| *at == root).unwrap().1;
        assert_eq!((group.role(), group.label()), (Role::Group, Some("Find")));
        let (field, _) = of(&view, &tree, FIND, "find-bar.field");
        let value = Some(ActionData::Value("needle".into()));
        view.access(&asked(Action::SetValue, field, value));
        let tree = view.accessibility("Gallery");
        assert_eq!(
            of(&view, &tree, FIND, "find-bar.field").1.value(),
            Some("needle")
        );
        // Its toggles, of their tooltips.
        let (_, case) = of(&view, &tree, FIND, "find-bar.case");
        assert_eq!(case.label(), Some("Match Case"));
        // An id of no surface does nothing.
        view.access(&asked(Action::Click, lokyr_ui_core::access::WINDOW, None));
    }
}
