//! A view tree as a host shows it, a window's content or a panel's: its
//! tree and its layout, and what the pointer, the keyboard and the input
//! method do to it, its fields', lists', widgets' and scroll areas' state,
//! and its tooltip. The host keeps the keyboard's focus, which goes across
//! the surfaces of a window, the clipboard and the platform's composition,
//! which the surface tells it to end; the events go to the view's plugin.

use std::collections::BTreeSet;
use std::time::Instant;

use lokyr_plugin_sdk::host::TabId;
use lokyr_plugin_sdk::view::{Key, Kind, Node, UiEventKind};
use lokyr_render_core::{Item, Rect};
use lokyr_text_core::FontBackend;
use lokyr_vt_core::input::KeyEvent;

use crate::animation::Animations;
use crate::field::{Carets, Clipboard, Fields};
use crate::font::Font;
use crate::layout::{Frame, Layout, Measure, Room};
use crate::list::{self, Lists};
use crate::paint::{self, Shown};
use crate::pointer::{self, Button, NodeEvent, Pointer};
use crate::progress::{self, FillArea, SweepArea};
use crate::theme::Tokens;
use crate::tooltip::Tooltip;
use crate::tree::{Invalid, NodeId, Tree};
use crate::{tabs, widget};

/// A view tree as a host shows it, and its state.
pub struct Surface {
    /// Its tree.
    pub tree: Tree,
    /// Its layout.
    pub layout: Layout,
    /// The pointer over it.
    pub pointer: Pointer,
    /// Its fields.
    pub fields: Fields,
    /// Its lists.
    pub lists: Lists,
    /// Its tooltip.
    pub tooltip: Tooltip,
    /// Its nodes' transitions.
    pub animations: Animations,
    /// Whether the platform's window server draws the fills of its
    /// progresses that are not indeterminate, which its paint then leaves
    /// out, as [`Surface::fills`] gives them, so that a change of their
    /// value slides with no frame of the host's.
    pub fills_apart: bool,
    /// The progresses whose fills and moving parts its paint draws while
    /// the window server draws the others, as something drawn over them
    /// covers them, which the host finds.
    pub covered: BTreeSet<NodeId>,
    /// Whether the pointer changed what shows without an event, as the row
    /// of a menu's list that it hovers, which a frame shows.
    repaint: bool,
}

/// What a press of the pointer did to a surface.
#[derive(Debug, Default)]
pub struct Pressed {
    /// The events of its nodes.
    pub events: Vec<NodeEvent>,
    /// The nodes under the pointer, from the root, of which the host gives
    /// the keyboard's focus to the innermost that takes it.
    pub path: Vec<NodeId>,
    /// Whether it kept a field's composition as it stood, which the host
    /// tells the platform's input method to forget.
    pub composition_kept: bool,
}

impl Surface {
    /// An empty surface, at `scale` pixels a point.
    pub fn new(scale: f32) -> Self {
        Self {
            tree: Tree::new(),
            layout: Layout::new(scale),
            pointer: Pointer::new(),
            fields: Fields::new(),
            lists: Lists::new(),
            tooltip: Tooltip::new(),
            animations: Animations::new(),
            fills_apart: false,
            covered: BTreeSet::new(),
            repaint: false,
        }
    }

    /// Whether the pointer changed what shows without an event since the
    /// last time, as the row of a menu's list that it hovers, which a
    /// frame shows.
    pub fn take_repaint(&mut self) -> bool {
        std::mem::take(&mut self.repaint)
    }

    /// Takes `root`, its plugin's new render, diffed against the last; the
    /// last stays if the host refuses it.
    pub fn update(&mut self, root: &Node) -> Result<(), Invalid> {
        let changes = self.tree.update(root)?;
        self.layout.update(&self.tree, &changes);
        self.fields.updated(&self.tree);
        self.lists.updated(&self.tree);
        Ok(())
    }

    /// Gives its lengths of the titlebar and of the window's buttons
    /// `room`, the window's, for its next layout.
    pub fn set_room(&mut self, room: Room) {
        self.layout.set_room(&self.tree, room);
    }

    /// Lays the tree out in `width` and `height` pixels, either as its
    /// content's where it is `None`, its texts measured by `measure`, and
    /// its lists' rows that show: the `list-range` events of the items that
    /// they ask its plugin for.
    pub fn lay_out(
        &mut self,
        (width, height): (Option<f32>, Option<f32>),
        measure: &mut impl Measure,
    ) -> Vec<NodeEvent> {
        self.layout.compute(&self.tree, width, height, measure);
        self.lists.lay_out(&self.tree, &self.layout, measure)
    }

    /// Its last layout, at `now`: its nodes of transitions that it moved
    /// slide, those whose opacity changed fade, and those whose hover or
    /// press changed, unless the system reduces motion, `reduced`.
    pub fn animate(&mut self, now: Instant, reduced: bool) {
        self.animations
            .reorder
            .scroll(&self.tree, &mut self.layout, now);
        self.animations
            .laid_out(&self.tree, &self.layout, now, reduced);
        self.show_pointer(now);
    }

    /// Whether a transition runs at `now`, for which the host draws a frame
    /// at each refresh of the display.
    pub fn animating(&self, now: Instant) -> bool {
        self.animations.running(now)
    }

    /// Scrolls its fields to their carets, the focused one's, and the
    /// others to their start.
    pub fn follow(&mut self, focused: Option<NodeId>, carets: &mut dyn Carets) {
        self.fields
            .follow(&self.tree, &self.layout, focused, carets);
    }

    /// The tree or its layout changed under the pointer: the hover's events.
    pub fn laid_out(&mut self) -> Vec<NodeEvent> {
        self.pointer.laid_out(&self.tree, &self.layout)
    }

    /// The pointer moved to `at`, in pixels from the tree's corner, at
    /// `now`: a field's selection that a drag makes, the hover's and the
    /// drag's events, and when its tooltip waits for, if one does.
    pub fn moved(
        &mut self,
        at: (f32, f32),
        carets: &mut dyn Carets,
        now: Instant,
    ) -> (Vec<NodeEvent>, Option<Instant>) {
        if self.fields.selecting() {
            self.fields.drag((&self.tree, &self.layout), at, carets);
        }
        let events = self.pointer.moved(&self.tree, &self.layout, at);
        // The row of a menu's list under the pointer, hovered.
        let list = self
            .pointer
            .hovered()
            .rfind(|&node| matches!(self.tree.kind(node), Some(Kind::List(_))));
        self.repaint |= self
            .lists
            .hover((&self.tree, &self.layout), list.map(|node| (node, at)));
        for event in &events {
            match event.kind {
                UiEventKind::DragStart => {
                    self.animations
                        .reorder
                        .lift(&self.tree, &self.layout, event.node, now);
                }
                UiEventKind::DragMove { x, y } => {
                    self.animations.reorder.drag(&self.layout, (x, y), now);
                }
                _ => {}
            }
        }
        self.show_pointer(now);
        let deadline = self
            .tooltip
            .hover(&self.tree, self.pointer.hovered(), at, now);
        (events, deadline)
    }

    /// The hovers and the press that show, at `now`: the pointer's; or,
    /// as a node is dragged among its siblings, that node's alone, which
    /// the pointer shows over wherever the layout put the others, hovered
    /// and pressed as its press left it until the button goes up, then
    /// hovered until its plugin's render puts it at its place.
    fn show_pointer(&mut self, now: Instant) {
        let reorder = &self.animations.reorder;
        if let Some(key) = reorder.lifted().cloned() {
            let pressed = reorder.dragging();
            let path: Vec<Key> =
                std::iter::successors(self.tree.by_key(&key), |&node| self.tree.parent(node))
                    .filter_map(|node| self.tree.key(node).cloned())
                    .collect();
            self.animations.hovered(path.iter().rev(), now);
            self.animations.pressed(pressed.then_some(&key), now);
            return;
        }
        self.animations.hovered(self.pointer.hovered_keys(), now);
        self.animations.pressed(self.pointer.pressing_key(), now);
    }

    /// The key of the node dragged among its siblings, while the drag is
    /// under way.
    fn lifted(&self) -> Option<Key> {
        let reorder = &self.animations.reorder;
        reorder
            .dragging()
            .then(|| reorder.raised().cloned())
            .flatten()
    }

    /// The node dragged among its siblings, drawn over them, while the drag
    /// is under way.
    pub fn lifted_node(&self) -> Option<NodeId> {
        self.tree.by_key(&self.lifted()?)
    }

    /// The box of its tooltip, while it shows, in the window's pixels, of
    /// the surface at `origin` in a window of `room`, as its paint draws
    /// it.
    pub fn tooltip_rect<B: FontBackend>(
        &self,
        font: &mut Font<B>,
        (origin, room): ((f32, f32), (f32, f32)),
    ) -> Option<Rect> {
        let (text, at) = self.tooltip.shown(&self.tree)?;
        let at = (at.0 + origin.0, at.1 + origin.1);
        Some(paint::tooltip_rect(
            font,
            text,
            (at, room),
            self.layout.scale(),
        ))
    }

    /// The node dragged that carries a tab, its key and the tab, while its
    /// drag is under way.
    pub fn carried(&self) -> Option<(Key, TabId)> {
        let key = self.lifted()?;
        let tab = self.tree.carries(self.tree.by_key(&key)?)?;
        Some((key, tab))
    }

    /// The node dragged that carries a tab left its parent's room, `out`,
    /// or came back into it, at `now`, as the host finds the pointer out of
    /// its bar or back: out, it shows nowhere, its siblings closing its
    /// room; whether it changed.
    pub fn carry_out(&mut self, out: bool, now: Instant) -> bool {
        if self.carried().is_none() {
            return false;
        }
        let changed = self.animations.reorder.carry_out(out, &self.layout, now);
        self.show_pointer(now);
        changed
    }

    /// Whether the node dragged left its parent's room, its drag under way
    /// or dropped.
    pub fn carried_out(&self) -> bool {
        self.animations.reorder.out()
    }

    /// The nodes that carry tabs open a room under the pointer, at `at`, in
    /// pixels from the tree's corner, for a tab dragged in from another
    /// surface, at `now`: its index among them, or `None` without them.
    pub fn open_room(&mut self, at: (f32, f32), now: Instant) -> Option<usize> {
        self.animations
            .reorder
            .open_room(&self.tree, &self.layout, at, now)
    }

    /// The index of the room that its nodes opened for a tab dragged in,
    /// while it is open.
    pub fn room(&self) -> Option<usize> {
        self.animations.reorder.room()
    }

    /// The room for a tab dragged in closes, at `now`, its nodes sliding
    /// back; whether one was open.
    pub fn close_room(&mut self, now: Instant) -> bool {
        self.animations.reorder.close_room(now)
    }

    /// The tab dragged in dropped in its room, at `now`, which stays until
    /// its plugin's render of the tab there: its index among the others.
    pub fn drop_room(&mut self, now: Instant) -> Option<usize> {
        self.animations.reorder.drop_room(self.tree.revision(), now)
    }

    /// The pointer left the surface, at `now`, over another or out of the
    /// window: the leaves of the nodes that it was over.
    pub fn left(&mut self, now: Instant) -> Vec<NodeEvent> {
        self.tooltip.left(now);
        self.repaint |= self.lists.hover((&self.tree, &self.layout), None);
        let events = self.pointer.left(&self.tree);
        self.show_pointer(now);
        events
    }

    /// `button` went down at `at`, in pixels from the tree's corner, of
    /// `clicks` in a row, Shift held or not: a field's composition kept as
    /// it stands; a field's caret placed, or a list's row selected, under
    /// the primary button; the pointer's events; the tooltip hidden.
    pub fn pressed(
        &mut self,
        at: (f32, f32),
        (button, clicks, shift): (Button, u32, bool),
        carets: &mut dyn Carets,
    ) -> Pressed {
        let mut pressed = Pressed::default();
        if let Some(node) = self.fields.composing(&self.tree) {
            pressed.events = self
                .fields
                .keep_composition(&self.tree, node)
                .unwrap_or_default();
            pressed.composition_kept = true;
        }
        pressed.path = pointer::hit(&self.tree, &self.layout, at);
        if button == Button::Primary
            && let Some(&node) = pressed.path.last()
        {
            match self.tree.kind(node) {
                Some(Kind::Field(_)) => self.fields.press(
                    (&self.tree, &self.layout),
                    node,
                    (at, clicks, shift),
                    carets,
                ),
                Some(Kind::List(_)) => pressed.events.extend(self.lists.press(
                    (&self.tree, &self.layout),
                    node,
                    (at, clicks),
                )),
                _ => {}
            }
        }
        pressed
            .events
            .extend(self.pointer.pressed(&self.tree, &self.layout, at, button));
        self.tooltip.dismiss();
        pressed
    }

    /// The app took the pointer back, at `now`, as the window lost the
    /// keyboard's focus or a key moved the focus: a drag under way is
    /// cancelled, a node dragged among its siblings sliding back, and a
    /// field's selection that a drag makes ends, as no button that is down
    /// clicks as it goes up.
    pub fn cancel(&mut self, now: Instant) -> Vec<NodeEvent> {
        self.fields.release();
        self.animations.reorder.cancel(now);
        let events = self.pointer.cancel(&self.tree);
        self.show_pointer(now);
        events
    }

    /// Escape, at `now`: the drag of a node among its siblings under way
    /// ends, the node sliding back to its place, which takes the key, the
    /// drag's cancel its event; `None` without one.
    pub fn escape(&mut self, now: Instant) -> Option<Vec<NodeEvent>> {
        self.lifted()?;
        Some(self.cancel(now))
    }

    /// `button` went up at `at`, of `clicks` in a row, at `now`: a field's
    /// selection ends; the pointer's events, a click's or a drag's end,
    /// which a reorder of the node dragged among its siblings comes before,
    /// if it dropped at another place.
    pub fn released(
        &mut self,
        at: (f32, f32),
        (button, clicks): (Button, u32),
        now: Instant,
    ) -> Vec<NodeEvent> {
        if button == Button::Primary {
            self.fields.release();
        }
        let dropped = match button {
            Button::Primary if self.lifted().is_some() => {
                self.animations.reorder.drop(self.tree.revision(), now)
            }
            _ => None,
        };
        let mut events = self
            .pointer
            .released(&self.tree, &self.layout, at, button, clicks);
        if let Some(index) = dropped
            && let Some(end) = events
                .iter()
                .position(|event| matches!(event.kind, UiEventKind::DragEnd { .. }))
        {
            let reorder = NodeEvent {
                kind: UiEventKind::Reorder { index },
                ..events[end].clone()
            };
            events.insert(end, reorder);
        }
        self.show_pointer(now);
        events
    }

    /// Whether a list or a scroll area under `at` would scroll, of the
    /// innermost under the pointer.
    pub fn scrolls_at(&self, at: (f32, f32)) -> bool {
        let path = pointer::hit(&self.tree, &self.layout, at);
        list::scrollable(&self.tree, &path).is_some()
    }

    /// A wheel or a trackpad scrolled at `at`, `delta` points across and
    /// towards the content above: the innermost list or scroll area under
    /// the pointer scrolls, and the hover follows; another node learns of
    /// the scroll. The tooltip hides.
    pub fn scrolled(&mut self, at: (f32, f32), delta: (f32, f32)) -> Vec<NodeEvent> {
        self.tooltip.dismiss();
        let path = pointer::hit(&self.tree, &self.layout, at);
        match list::scrollable(&self.tree, &path) {
            Some(node) => {
                list::scroll((&self.tree, &mut self.layout, &mut self.lists), node, delta);
                self.pointer.laid_out(&self.tree, &self.layout)
            }
            None => self.pointer.scrolled(&self.tree, &self.layout, at, delta),
        }
    }

    /// What `key` does to `node`, which has the keyboard's focus: a
    /// field's edit, a button's press or a toggle's switch, a view of
    /// tabs' choice, a list's selection; `None` for a key that none takes,
    /// which the app's bindings may take.
    pub fn key(
        &mut self,
        node: NodeId,
        key: &KeyEvent<'_>,
        clipboard: &mut dyn Clipboard,
    ) -> Option<Vec<NodeEvent>> {
        self.fields
            .key(&self.tree, node, key, clipboard)
            .or_else(|| widget::key(&self.tree, node, key))
            .or_else(|| tabs::key(&self.tree, node, key))
            .or_else(|| self.lists.key(&self.tree, node, key))
    }

    /// The input method composes `text` in the field `node`, of which it
    /// selects `selected`, in bytes; an empty `text` ends the composition:
    /// its events, `None` of a node that is no field.
    pub fn compose(
        &mut self,
        node: NodeId,
        text: &str,
        selected: std::ops::Range<usize>,
    ) -> Option<Vec<NodeEvent>> {
        self.fields.compose(&self.tree, node, text, selected)
    }

    /// The input method commits `text` into the field `node`: its events,
    /// `None` of a node that is no field.
    pub fn commit(&mut self, node: NodeId, text: &str) -> Option<Vec<NodeEvent>> {
        self.fields.commit(&self.tree, node, text)
    }

    /// The input method's rectangle in the field `node`, in pixels from the
    /// tree's corner: the composition's, or the caret's.
    pub fn text_area(&self, node: NodeId, carets: &mut dyn Carets) -> Option<Frame> {
        self.fields
            .text_area(&self.tree, &self.layout, node, carets)
    }

    /// The display list of the surface, from `origin` in the window's
    /// pixels, of `tokens`' colors, of the dark appearance or the light
    /// one, of the focus's ring around `focused`, of its transitions at
    /// `now`, of its plugin's `pictures`: its nodes, then its tooltip,
    /// within the window's `room`.
    pub fn paint<B: FontBackend>(
        &self,
        font: &mut Font<B>,
        tokens: &Tokens,
        (origin, room): ((f32, f32), (f32, f32)),
        (dark, focused, now): (bool, Option<NodeId>, Instant),
        pictures: &paint::Pictures,
    ) -> Vec<Item> {
        let mut items = paint::paint(
            &self.tree,
            &self.layout,
            font,
            tokens,
            origin,
            Shown {
                dark,
                focused,
                fields: &self.fields,
                // A drag among the siblings hovers and presses none of
                // them, wherever the layout put them under the pointer.
                hovered: self
                    .pointer
                    .hovered()
                    .next_back()
                    .filter(|_| self.lifted().is_none()),
                pressed: self.pointer.pressing().filter(|_| self.lifted().is_none()),
                lists: &self.lists,
                motion: Some((&self.animations, now)),
                fills: if self.fills_apart {
                    paint::Fills::Covered(&self.covered)
                } else {
                    paint::Fills::All
                },
                pictures,
            },
        );
        if let Some((text, at)) = self.tooltip.shown(&self.tree) {
            let at = (at.0 + origin.0, at.1 + origin.1);
            let scale = self.layout.scale();
            items.extend(paint::tooltip(
                font,
                tokens,
                text,
                (at, room),
                (scale, dark),
            ));
        }
        items
    }

    /// The sweeps of its indeterminate progresses, in pixels from the
    /// tree's corner, of their nodes' transitions at `now`.
    pub fn sweeps(&self, tokens: &Tokens, now: Instant) -> Vec<SweepArea> {
        progress::sweeps(
            &self.tree,
            &self.layout,
            tokens,
            Some((&self.animations, now)),
        )
    }

    /// The fills of its progresses that are not indeterminate, of
    /// `tokens`' colors, as its nodes' transitions show them at `now`,
    /// which the platform's window server draws where `fills_apart` says
    /// so.
    pub fn fills(&self, tokens: &Tokens, now: Instant) -> Vec<FillArea> {
        progress::fills(
            &self.tree,
            &self.layout,
            tokens,
            Some((&self.animations, now)),
        )
    }

    /// The size of the tree's root, in pixels, of the last layout.
    pub fn size(&self) -> Option<(f32, f32)> {
        let root = self.tree.root()?;
        let frame = self.layout.frame(&self.tree, root)?;
        Some((frame.width, frame.height))
    }
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    use lokyr_plugin_sdk::view::{self, Direction, Length, ListItem, UiEventKind};
    use lokyr_vt_core::input::{Key, KeyAction, Mods};

    use super::*;

    /// A font of the tests': a character 10 points wide, a text 14 high,
    /// an icon 16.
    struct Fixed;

    impl Measure for Fixed {
        fn text(&mut self, text: &str) -> (f32, f32) {
            (10.0 * text.chars().count() as f32, 14.0)
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
                .map(|(index, (at, _))| (at, index as f32 * 10.0))
                .collect();
            carets.push((text.len(), text.chars().count() as f32 * 10.0));
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

    /// A column of a field, a button, a list of 100 rows, and a scroll area
    /// of 10 boxes of 20 points in 40.
    fn view() -> Node {
        let mut list = Node::list(
            Node::text(""),
            100,
            (
                0,
                (0..100)
                    .map(|index| ListItem {
                        texts: vec![format!("{index}")],
                    })
                    .collect(),
            ),
        )
        .key("rows")
        .layout(view::Layout {
            height: Length::Points(40.0),
            ..view::Layout::default()
        });
        if let Kind::List(list) = &mut list.kind {
            list.row_height = Some(20.0);
        }
        let boxes = (0..10)
            .map(|index| {
                Node::container(vec![])
                    .key(format!("box-{index}").as_str())
                    .layout(view::Layout {
                        height: Length::Points(20.0),
                        ..view::Layout::default()
                    })
            })
            .collect();
        Node::container(vec![
            Node::field("Find").key("find"),
            Node::button("Go").key("go").tooltip("Go there"),
            list,
            Node::scroll(boxes).key("area").layout(view::Layout {
                direction: Direction::Column,
                height: Length::Points(40.0),
                ..view::Layout::default()
            }),
        ])
        .layout(view::Layout {
            direction: Direction::Column,
            ..view::Layout::default()
        })
    }

    fn surface() -> Surface {
        let mut surface = Surface::new(1.0);
        surface.update(&view()).unwrap();
        surface.lay_out((Some(200.0), None), &mut Fixed);
        surface
    }

    fn middle(surface: &Surface, key: &str) -> (f32, f32) {
        let node = surface.tree.by_key(&key.into()).unwrap();
        let frame = surface.layout.frame(&surface.tree, node).unwrap();
        (frame.x + frame.width / 2.0, frame.y + frame.height / 2.0)
    }

    fn kinds(events: &[NodeEvent]) -> Vec<(String, UiEventKind)> {
        events
            .iter()
            .filter(|event| {
                !matches!(
                    event.kind,
                    UiEventKind::HoverEnter | UiEventKind::HoverLeave
                )
            })
            .map(|event| (event.key.0.clone(), event.kind.clone()))
            .collect()
    }

    #[test]
    fn a_press_keeps_a_composition_selects_a_row_and_hides_the_tooltip() {
        let mut surface = surface();
        let now = Instant::now();
        // The pointer resting over the button: its tooltip shows.
        let (_, deadline) = surface.moved(middle(&surface, "go"), &mut Fixed, now);
        surface.tooltip.tick(deadline.unwrap());
        assert!(surface.tooltip.shown(&surface.tree).is_some());
        // A composition in the field, kept by a press on the list's second
        // row, which it selects; the tooltip hides.
        let find = surface.tree.by_key(&"find".into()).unwrap();
        surface.compose(find, "にほ", 6..6);
        let (x, y) = middle(&surface, "rows");
        let pressed = surface.pressed((x, y), (Button::Primary, 1, false), &mut Fixed);
        assert!(pressed.composition_kept);
        assert_eq!(
            kinds(&pressed.events),
            [
                (
                    "find".to_owned(),
                    UiEventKind::InputChanged {
                        text: "にほ".to_owned(),
                        revision: 1
                    }
                ),
                ("rows".to_owned(), UiEventKind::Select { item: 1 })
            ]
        );
        assert_eq!(
            surface.tree.key(*pressed.path.last().unwrap()).unwrap().0,
            "rows"
        );
        assert!(surface.tooltip.shown(&surface.tree).is_none());
        // A drag in the field selects until the button goes up.
        let find = surface.tree.by_key(&"find".into()).unwrap();
        let frame = surface.layout.frame(&surface.tree, find).unwrap();
        let start = (frame.x + 8.0, frame.y + 5.0);
        surface.pressed(start, (Button::Primary, 1, false), &mut Fixed);
        assert!(surface.fields.selecting());
        surface.released(start, (Button::Primary, 1), Instant::now());
        assert!(!surface.fields.selecting());
        // And until the app takes the pointer back, as Tab moves the focus.
        surface.pressed(start, (Button::Primary, 1, false), &mut Fixed);
        surface.cancel(Instant::now());
        assert!(!surface.fields.selecting());
        // Nothing to keep the next time; a click of the button.
        let at = middle(&surface, "go");
        let pressed = surface.pressed(at, (Button::Primary, 1, false), &mut Fixed);
        assert!(!pressed.composition_kept);
        assert_eq!(
            kinds(&surface.released(at, (Button::Primary, 1), Instant::now())),
            [("go".to_owned(), UiEventKind::Click)]
        );
    }

    #[test]
    fn a_scroll_scrolls_the_list_or_the_area_under_it_and_a_key_goes_where_it_is_taken() {
        let mut surface = surface();
        // Over the list: its rows, not an event.
        let events = surface.scrolled(middle(&surface, "rows"), (0.0, -30.0));
        assert!(kinds(&events).is_empty());
        let rows = surface.lists.state(&"rows".into()).unwrap();
        assert_eq!(rows.anchor(), (1, 10.0));
        // Over the area: its boxes.
        let area = surface.tree.by_key(&"area".into()).unwrap();
        surface.scrolled(middle(&surface, "area"), (0.0, -30.0));
        assert_eq!(surface.layout.scrolled(area), 30.0);
        // Over the button: its scroll's event.
        let events = surface.scrolled(middle(&surface, "go"), (0.0, -5.0));
        assert_eq!(
            kinds(&events),
            [("go".to_owned(), UiEventKind::Scroll { x: 0.0, y: -5.0 })]
        );
        // The keys: the field's typing, the button's Space, the list's
        // arrow, and none of another key.
        let press = |key, text| KeyEvent {
            key,
            action: KeyAction::Press,
            mods: Mods::NONE,
            text,
            unmodified: text,
            base_layout: None,
        };
        let node = |key: &str| surface.tree.by_key(&key.into()).unwrap();
        let (find, go, rows) = (node("find"), node("go"), node("rows"));
        let typed = surface
            .key(find, &press(Key::Char('a'), "a"), &mut Board)
            .unwrap();
        assert_eq!(
            kinds(&typed),
            [(
                "find".to_owned(),
                UiEventKind::InputChanged {
                    text: "a".to_owned(),
                    revision: 1
                }
            )]
        );
        let space = surface
            .key(go, &press(Key::Char(' '), " "), &mut Board)
            .unwrap();
        assert_eq!(kinds(&space), [("go".to_owned(), UiEventKind::Click)]);
        let down = surface
            .key(rows, &press(Key::Down, ""), &mut Board)
            .unwrap();
        assert_eq!(
            kinds(&down),
            [("rows".to_owned(), UiEventKind::Select { item: 0 })]
        );
        assert!(
            surface
                .key(go, &press(Key::Char('x'), "x"), &mut Board)
                .is_none()
        );
        // The input method's commit and its rectangle.
        let committed = surface.commit(find, "日").unwrap();
        assert_eq!(committed.len(), 1);
        assert!(surface.text_area(find, &mut Fixed).is_some());
        assert!(surface.text_area(go, &mut Fixed).is_none());
        assert_eq!(surface.size().map(|(width, _)| width), Some(200.0));
    }

    #[test]
    fn a_press_shows_while_the_button_is_down_and_the_pointer_over_its_node() {
        use std::time::Duration;

        use lokyr_plugin_sdk::view::{Hover, Style};

        let pressable = Node::container(vec![
            Node::container(vec![Node::text("tab")])
                .key("tab")
                .style(Style {
                    pressed: Some(Hover {
                        background: Some(view::Token::from("button.pressed.bg")),
                        ..Hover::default()
                    }),
                    ..Style::default()
                })
                .layout(view::Layout {
                    width: Length::Points(100.0),
                    ..view::Layout::default()
                }),
        ]);
        let mut surface = Surface::new(1.0);
        surface.update(&pressable).unwrap();
        surface.lay_out((Some(200.0), None), &mut Fixed);
        let start = Instant::now();
        surface.animate(start, false);
        let tab: view::Key = "tab".into();
        let press = |surface: &Surface| {
            surface
                .animations
                .press(&tab, start + Duration::from_secs(1))
        };
        let over = middle(&surface, "tab");
        surface.moved(over, &mut Fixed, start);
        surface.pressed(over, (Button::Primary, 1, false), &mut Fixed);
        surface.animate(start, false);
        assert_eq!(press(&surface), 1.0);
        // The pointer off its node, the button down: no press; back, again.
        surface.moved((150.0, over.1), &mut Fixed, start);
        assert_eq!(press(&surface), 0.0);
        surface.moved(over, &mut Fixed, start);
        assert_eq!(press(&surface), 1.0);
        // Up, none.
        surface.released(over, (Button::Primary, 1), start);
        surface.animate(start, false);
        assert_eq!(press(&surface), 0.0);
        // Nor of another button.
        surface.pressed(over, (Button::Secondary, 1, false), &mut Fixed);
        surface.animate(start, false);
        assert_eq!(press(&surface), 0.0);
    }

    #[test]
    fn a_sweep_fades_in_as_its_node_does() {
        use std::time::Duration;

        use lokyr_plugin_sdk::view::{Easing, Progress, ProgressState, Style, Transition};

        use crate::theme::Theme;

        let spinning = |opacity: f32| {
            Node::container(vec![
                Node::new(Kind::Progress(Progress {
                    state: ProgressState::Indeterminate,
                    ..Progress::default()
                }))
                .layout(view::Layout {
                    height: Length::Points(10.0),
                    ..view::Layout::default()
                }),
            ])
            .key("box")
            .style(Style {
                opacity: Some(opacity),
                transition: Some(Transition {
                    duration: 0.2,
                    easing: Easing::Linear,
                }),
                ..Style::default()
            })
        };
        let mut surface = Surface::new(1.0);
        let start = Instant::now();
        for opacity in [0.0, 1.0] {
            surface.update(&spinning(opacity)).unwrap();
            surface.lay_out((Some(200.0), None), &mut Fixed);
            surface.animate(start, false);
        }
        let tokens = &Theme::builtin().dark.tokens;
        let normal = tokens.get("progress.normal").unwrap();
        let half = surface.sweeps(tokens, start + Duration::from_millis(100));
        assert_eq!(half.len(), 1);
        assert!(
            (half[0].color.alpha - normal.alpha * 0.5).abs() < 1e-3,
            "{half:?}"
        );
    }
}
