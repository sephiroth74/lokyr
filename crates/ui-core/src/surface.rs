//! A view tree as a host shows it, a window's content or a panel's: its
//! tree and its layout, and what the pointer, the keyboard and the input
//! method do to it, its fields', lists', widgets' and scroll areas' state,
//! and its tooltip. The host keeps the keyboard's focus, which goes across
//! the surfaces of a window, the clipboard and the platform's composition,
//! which the surface tells it to end; the events go to the view's plugin.

use std::time::Instant;

use lokyr_plugin_sdk::view::{Kind, Node};
use lokyr_render_core::Item;
use lokyr_text_core::FontBackend;
use lokyr_vt_core::input::KeyEvent;

use crate::animation::Animations;
use crate::field::{Carets, Clipboard, Fields};
use crate::font::Font;
use crate::layout::{Frame, Layout, Measure};
use crate::list::{self, Lists};
use crate::paint::{self, Shown};
use crate::pointer::{self, Button, NodeEvent, Pointer};
use crate::progress::{self, SweepArea};
use crate::theme::Tokens;
use crate::tooltip::Tooltip;
use crate::tree::{Invalid, NodeId, Tree};
use crate::widget;

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
        }
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
    /// slide, and those whose opacity changed fade, unless the system
    /// reduces motion, `reduced`.
    pub fn animate(&mut self, now: Instant, reduced: bool) {
        self.animations
            .laid_out(&self.tree, &self.layout, now, reduced);
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
        let deadline = self
            .tooltip
            .hover(&self.tree, self.pointer.hovered(), at, now);
        (events, deadline)
    }

    /// The pointer left the surface, at `now`, over another or out of the
    /// window: the leaves of the nodes that it was over.
    pub fn left(&mut self, now: Instant) -> Vec<NodeEvent> {
        self.tooltip.left(now);
        self.pointer.left(&self.tree)
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

    /// The app took the pointer back, as the window lost the keyboard's
    /// focus or a key moved the focus: a drag under way is cancelled, and a
    /// field's selection that a drag makes ends, as no button that is down
    /// clicks as it goes up.
    pub fn cancel(&mut self) -> Vec<NodeEvent> {
        self.fields.release();
        self.pointer.cancel(&self.tree)
    }

    /// `button` went up at `at`, of `clicks` in a row: a field's selection
    /// ends; the pointer's events, a click's or a drag's end.
    pub fn released(&mut self, at: (f32, f32), (button, clicks): (Button, u32)) -> Vec<NodeEvent> {
        if button == Button::Primary {
            self.fields.release();
        }
        self.pointer
            .released(&self.tree, &self.layout, at, button, clicks)
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
                list::scroll(
                    (&self.tree, &mut self.layout, &mut self.lists),
                    node,
                    delta.1,
                );
                self.pointer.laid_out(&self.tree, &self.layout)
            }
            None => self.pointer.scrolled(&self.tree, &self.layout, at, delta),
        }
    }

    /// What `key` does to `node`, which has the keyboard's focus: a
    /// field's edit, a button's press or a toggle's switch, a list's
    /// selection; `None` for a key that none takes, which the app's
    /// bindings may take.
    pub fn key(
        &mut self,
        node: NodeId,
        key: &KeyEvent<'_>,
        clipboard: &mut dyn Clipboard,
    ) -> Option<Vec<NodeEvent>> {
        self.fields
            .key(&self.tree, node, key, clipboard)
            .or_else(|| widget::key(&self.tree, node, key))
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
    /// `now`: its nodes, then its tooltip, within the window's `room`.
    pub fn paint<B: FontBackend>(
        &self,
        font: &mut Font<B>,
        tokens: &Tokens,
        (origin, room): ((f32, f32), (f32, f32)),
        (dark, focused, now): (bool, Option<NodeId>, Instant),
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
                hovered: self.pointer.hovered().next_back(),
                pressed: self.pointer.pressing(),
                lists: &self.lists,
                motion: Some((&self.animations, now)),
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
    /// tree's corner.
    pub fn sweeps(&self, tokens: &Tokens) -> Vec<SweepArea> {
        progress::sweeps(&self.tree, &self.layout, tokens)
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
                        text: "にほ".to_owned()
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
        surface.released(start, (Button::Primary, 1));
        assert!(!surface.fields.selecting());
        // And until the app takes the pointer back, as Tab moves the focus.
        surface.pressed(start, (Button::Primary, 1, false), &mut Fixed);
        surface.cancel();
        assert!(!surface.fields.selecting());
        // Nothing to keep the next time; a click of the button.
        let at = middle(&surface, "go");
        let pressed = surface.pressed(at, (Button::Primary, 1, false), &mut Fixed);
        assert!(!pressed.composition_kept);
        assert_eq!(
            kinds(&surface.released(at, (Button::Primary, 1))),
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
                    text: "a".to_owned()
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
}
