//! The gallery's view as a window's host shows it: the gallery's content
//! and its floating panels, each a surface of its own, the panels over the
//! content, and the keyboard's focus across them; what the pointer, the
//! keyboard and the input method do to them, and the panels that a click
//! outside them or Escape closes. The window's own pieces, the platform's
//! timers, clipboard and composition, are the caller's.

use std::collections::BTreeMap;
use std::time::Instant;

use lokyr_plugin_sdk::view::{Key as NodeKey, UiEventKind};
use lokyr_render_core::Item;
use lokyr_text_core::FontBackend;
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

use crate::gallery::{BELOW, FIND, Gallery, MENU, PanelAsk};

/// The gallery's content, as a panel of the focus's.
pub const MAIN: u8 = 0;

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
    /// Where the pointer is, in pixels from the content's corner.
    at: (f32, f32),
    /// The window's room for the content, in pixels.
    room: (f32, f32),
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

    /// The surface of `panel`, the content's of `MAIN`.
    pub fn surface(&self, panel: u8) -> Option<&Surface> {
        match panel {
            MAIN => Some(&self.main),
            _ => self.surfaces.get(&panel).map(|(surface, _)| surface),
        }
    }

    fn surface_mut(&mut self, panel: u8) -> Option<&mut Surface> {
        match panel {
            MAIN => Some(&mut self.main),
            _ => self.surfaces.get_mut(&panel).map(|(surface, _)| surface),
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
        for (key, text) in self.gallery.take_texts() {
            self.main.fields.set_text(&NodeKey(key), &text);
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

    /// Closes `panel`, whose focus goes back to the pane.
    fn close(&mut self, panel: u8) {
        if self.panels.close(panel) || self.surfaces.contains_key(&panel) {
            self.surfaces.remove(&panel);
            self.focus.closed(panel);
            if self.captured == Some(panel) {
                self.captured = None;
            }
            if self.over == Some(panel) {
                self.over = None;
            }
            self.tell_focus();
        }
    }

    /// The anchor of `place`, in pixels of the content.
    fn anchor(&self, place: &Place) -> Anchor {
        match place {
            Place::Pointer(at) => Anchor::At(*at),
            Place::Below(key) => self
                .main
                .tree
                .by_key(key)
                .and_then(|node| self.main.layout.frame(&self.main.tree, node))
                .map_or(Anchor::At(self.at), Anchor::Below),
            Place::Corner(corner) => Anchor::Corner(
                Frame {
                    x: 0.0,
                    y: 0.0,
                    width: self.room.0,
                    height: self.room.1,
                },
                *corner,
                MARGIN * self.scale,
            ),
        }
    }

    /// The scale changed, to `scale` pixels a point.
    pub fn set_scale(&mut self, scale: f32) {
        self.scale = scale;
        self.main.layout.set_scale(&self.main.tree, scale);
        for (surface, _) in self.surfaces.values_mut() {
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
            let mut events = self.main.lay_out((Some(room.0), room.1), measure);
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

    /// The surface under `at`, in pixels of the content, the captured one
    /// while a button is down, and `at` from its corner.
    fn target(&self, at: (f32, f32)) -> (u8, (f32, f32)) {
        let panel = self
            .captured
            .or_else(|| self.panels.under(at))
            .unwrap_or(MAIN);
        (panel, self.local(panel, at))
    }

    /// `at`, in pixels of the content, from `panel`'s corner.
    fn local(&self, panel: u8, at: (f32, f32)) -> (f32, f32) {
        match self.panels.frame(panel) {
            Some(frame) if panel != MAIN => (at.0 - frame.x, at.1 - frame.y),
            _ => at,
        }
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
        if self.captured.is_none() && self.panels.under(at).is_none() {
            let outside = self.panels.pressed_outside(at);
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
        let panel = self.panels.under(at).unwrap_or(MAIN);
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
                for panel in std::iter::once(MAIN).chain(self.panels.order().collect::<Vec<_>>()) {
                    if let Some(surface) = self.surface_mut(panel) {
                        events.extend(surface.pointer.cancel(&surface.tree));
                    }
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
        let mut shown = self.main.tooltip.tick(now);
        for (surface, _) in self.surfaces.values_mut() {
            shown |= surface.tooltip.tick(now);
        }
        shown
    }

    /// The time that a tooltip waits for, the soonest, if one waits.
    pub fn deadline(&self) -> Option<Instant> {
        std::iter::once(&self.main)
            .chain(self.surfaces.values().map(|(surface, _)| surface))
            .filter_map(|surface| surface.tooltip.deadline())
            .min()
    }

    /// A key or the window's loss of its focus: the tooltips hide; whether
    /// one showed.
    pub fn dismiss_tooltips(&mut self) -> bool {
        let mut shown = self.main.tooltip.dismiss();
        for (surface, _) in self.surfaces.values_mut() {
            shown |= surface.tooltip.dismiss();
        }
        shown
    }

    /// The window lost the keyboard's focus: the pointer's drags cancel.
    pub fn cancel(&mut self) {
        let mut events = Vec::new();
        for panel in std::iter::once(MAIN).chain(self.panels.order().collect::<Vec<_>>()) {
            if let Some(surface) = self.surface_mut(panel) {
                events.extend(surface.pointer.cancel(&surface.tree));
            }
        }
        self.captured = None;
        self.dismiss_tooltips();
        self.apply(events);
    }

    /// The display list's items of the content and of the panels over it,
    /// from `origin` in the window's pixels, of `tokens`, in the window's
    /// `room` of pixels.
    pub fn paint<B: FontBackend>(
        &self,
        font: &mut Font<B>,
        tokens: &Tokens,
        (origin, room): ((f32, f32), (f32, f32)),
        dark: bool,
    ) -> Vec<Item> {
        let focused = self.focused();
        let focus_of = |panel: u8| focused.and_then(|(at, node)| (at == panel).then_some(node));
        let mut items = self
            .main
            .paint(font, tokens, (origin, room), (dark, focus_of(MAIN)));
        for panel in self.panels.order() {
            let (Some(frame), Some(surface)) = (self.panels.frame(panel), self.surface(panel))
            else {
                continue;
            };
            let at = (origin.0 + frame.x, origin.1 + frame.y);
            items.extend(surface.paint(font, tokens, (at, room), (dark, focus_of(panel))));
        }
        items
    }

    /// The areas of the pointer's shapes, in pixels of the content, the
    /// topmost panel's first, as the pointer goes through a tooltip.
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
        areas.extend(pointer::areas(&self.main.tree, &self.main.layout));
        areas
    }

    /// The sweeps of the indeterminate progresses, in pixels of the
    /// content, but those that a panel over them covers.
    pub fn sweeps(&self, tokens: &Tokens) -> Vec<SweepArea> {
        let mut sweeps: Vec<SweepArea> = self
            .main
            .sweeps(tokens)
            .into_iter()
            .filter(|sweep| !self.panels.covers(sweep.visible, None))
            .collect();
        for panel in self.panels.order() {
            let (Some(frame), Some(surface)) = (self.panels.frame(panel), self.surface(panel))
            else {
                continue;
            };
            let moved = |rect: lokyr_render_core::Rect| lokyr_render_core::Rect {
                x: rect.x + frame.x as i32,
                y: rect.y + frame.y as i32,
                ..rect
            };
            sweeps.extend(
                surface
                    .sweeps(tokens)
                    .into_iter()
                    .map(|sweep| SweepArea {
                        rect: moved(sweep.rect),
                        visible: moved(sweep.visible),
                        ..sweep
                    })
                    .filter(|sweep| !self.panels.covers(sweep.visible, Some(panel))),
            );
        }
        sweeps
    }

    /// The input method's rectangle of the field that has the focus, in
    /// pixels of the content.
    pub fn text_area(&self, carets: &mut dyn Carets) -> Option<Frame> {
        let (panel, node) = self.focused()?;
        let area = self.surface(panel)?.text_area(node, carets)?;
        let (x, y) = match self.panels.frame(panel) {
            Some(frame) if panel != MAIN => (frame.x, frame.y),
            _ => (0.0, 0.0),
        };
        Some(Frame {
            x: area.x + x,
            y: area.y + y,
            ..area
        })
    }
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
        fn carets(&mut self, text: &str) -> Vec<(usize, f32)> {
            let mut carets: Vec<(usize, f32)> = text
                .char_indices()
                .enumerate()
                .map(|(index, (at, _))| (at, index as f32 * 7.0))
                .collect();
            carets.push((text.len(), text.chars().count() as f32 * 7.0));
            carets
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
        (frame.x + frame.width / 2.0, frame.y + frame.height / 2.0)
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
        // Escape, its focus in it, closes it.
        assert_eq!(
            view.key(&press(Key::Escape, ""), &mut Board, false),
            Keyed::Taken
        );
        assert!(!view.panels.is_open(FIND));
        assert_eq!(view.focused(), None);
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
        let before = view.sweeps(tokens);
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
        let after = view.sweeps(tokens);
        assert!(after.len() < before.len());
        assert!(!after.iter().any(|kept| kept.rect == sweep.rect));
    }
}
