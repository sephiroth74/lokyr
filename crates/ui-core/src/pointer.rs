//! The pointer over a view tree: the node under it, the topmost whose box
//! holds it within its ancestors' clips, as the tree draws a node over its
//! parent and over its siblings before it; what its moves and its buttons
//! do to the nodes of a key, which their plugins learn of; and its shape
//! over each node, as areas that the platform shows as the pointer moves,
//! without the app.
//!
//! The hover goes to every node of a key that the pointer is over, its
//! ancestors' with it, as the pointer over a tab's close is over the tab
//! too; a click to the innermost node of a key under the pointer, which the
//! button went down and up over.
//!
//! Places are in pixels from the root's top left corner, as the layout's
//! frames are.

use lokyr_plugin_sdk::view::{Key, PointerShape, UiEventKind};
use lokyr_render_core::Rect;

use crate::layout::{Frame, Layout};
use crate::paint::{inside_border, rect};
use crate::tree::{NodeId, Tree};

/// A button of the pointer, as the view tree's events tell them apart.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Button {
    /// The primary button, the left one.
    Primary,
    /// The secondary button, the right one, or the primary one with
    /// Control, as the platform's context menus take it.
    Secondary,
    /// The middle button.
    Middle,
}

impl Button {
    fn index(self) -> usize {
        match self {
            Self::Primary => 0,
            Self::Secondary => 1,
            Self::Middle => 2,
        }
    }
}

/// What the pointer did to a node of a key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NodeEvent {
    /// The node.
    pub node: NodeId,
    /// Its key, by which its plugin knows it.
    pub key: Key,
    /// What the pointer did.
    pub kind: UiEventKind,
}

/// An area of a tree whose pointer has a shape of its own, over the areas
/// after it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Area {
    /// Its pixels, from the root's top left corner.
    pub rect: Rect,
    /// The pointer's shape over it.
    pub shape: PointerShape,
}

/// The nodes under `at`, from the root to the topmost, which the pointer
/// there is over: none when no node's box holds it.
pub fn hit(tree: &Tree, layout: &Layout, at: (f32, f32)) -> Vec<NodeId> {
    let mut path = Vec::new();
    if let Some(root) = tree.root() {
        hit_node(tree, layout, root, (0.0, 0.0), None, at, &mut path);
    }
    path.reverse();
    path
}

/// Whether `id`, its frame from `origin`, or one of its children, holds
/// `at` within `clip`: its children first, the last over the others, then
/// its own box; the nodes hit pushed onto `path`, from the topmost.
fn hit_node(
    tree: &Tree,
    layout: &Layout,
    id: NodeId,
    origin: (f32, f32),
    clip: Option<Rect>,
    at: (f32, f32),
    path: &mut Vec<NodeId>,
) -> bool {
    // Nothing within a clip that `at` is outside of is hit.
    if clip.is_some_and(|clip| !contains(clip, at)) {
        return false;
    }
    let (Some(local), Some(style)) = (layout.local(id), tree.style(id)) else {
        return false;
    };
    let frame = Frame {
        x: origin.0 + local.x,
        y: origin.1 + local.y,
        ..local
    };
    let pixels = rect(frame);
    let inner = if style.clip {
        let inside = inside_border(pixels, style, layout.scale());
        Some(clip.map_or(inside, |clip| intersection(clip, inside)))
    } else {
        clip
    };
    for &child in tree.children(id).iter().rev() {
        if hit_node(tree, layout, child, (frame.x, frame.y), inner, at, path) {
            path.push(id);
            return true;
        }
    }
    if contains(pixels, at) {
        path.push(id);
        return true;
    }
    false
}

/// The pointer's shape over `path`, a hit's: of the topmost node that names
/// one, else the arrow.
pub fn shape(tree: &Tree, path: &[NodeId]) -> PointerShape {
    path.iter()
        .rev()
        .find_map(|&id| tree.style(id)?.pointer)
        .unwrap_or_default()
}

/// The areas of the pointer's shapes over `tree`, the first over those
/// after it, so that the pointer over each pixel takes the shape of the
/// node that [`hit`] finds there; as few as that leaves, a node within an
/// area of its own shape adding none. The pointer outside them takes the
/// shape of what is under the tree.
pub fn areas(tree: &Tree, layout: &Layout) -> Vec<Area> {
    let mut areas = Vec::new();
    if let Some(root) = tree.root() {
        areas_of(
            tree,
            layout,
            root,
            (0.0, 0.0),
            None,
            (PointerShape::default(), None),
            &mut areas,
        );
    }
    areas.reverse();
    areas
}

/// Adds the area of `id`, its frame from `origin`, within `clip`, of its
/// own shape or `inherited`, then its children's, to `areas`, in the order
/// that they draw: none where the area of `behind`, its nearest ancestor's
/// among them, holds its box and is of its shape, and no area after
/// `behind`'s overlaps it, as the pointer there then finds `behind`'s.
fn areas_of(
    tree: &Tree,
    layout: &Layout,
    id: NodeId,
    origin: (f32, f32),
    clip: Option<Rect>,
    (inherited, behind): (PointerShape, Option<usize>),
    areas: &mut Vec<Area>,
) {
    let (Some(local), Some(style)) = (layout.local(id), tree.style(id)) else {
        return;
    };
    let frame = Frame {
        x: origin.0 + local.x,
        y: origin.1 + local.y,
        ..local
    };
    let pixels = rect(frame);
    let shape = style.pointer.unwrap_or(inherited);
    let visible = clip.map_or(pixels, |clip| intersection(clip, pixels));
    let covered = behind.is_some_and(|index| {
        let area = areas[index];
        area.shape == shape
            && within(visible, area.rect)
            && areas[index + 1..]
                .iter()
                .all(|after| !overlap(after.rect, visible))
    });
    let behind = if covered || is_empty(visible) {
        behind
    } else {
        areas.push(Area {
            rect: visible,
            shape,
        });
        Some(areas.len() - 1)
    };
    let inner = if style.clip {
        let inside = inside_border(pixels, style, layout.scale());
        Some(clip.map_or(inside, |clip| intersection(clip, inside)))
    } else {
        clip
    };
    for &child in tree.children(id) {
        areas_of(
            tree,
            layout,
            child,
            (frame.x, frame.y),
            inner,
            (shape, behind),
            areas,
        );
    }
}

/// The pointer over a view tree, and what it did there: the nodes of a key
/// that it is over, and those that each button went down over.
#[derive(Debug, Default)]
pub struct Pointer {
    at: Option<(f32, f32)>,
    hovered: Vec<(NodeId, Key)>,
    pressed: [Option<NodeId>; 3],
    shape: PointerShape,
}

impl Pointer {
    /// A pointer over none of a tree's nodes.
    pub fn new() -> Self {
        Self::default()
    }

    /// The pointer moved to `at`: the nodes that it left, the innermost
    /// first, and those that it came over, the outermost first.
    pub fn moved(&mut self, tree: &Tree, layout: &Layout, at: (f32, f32)) -> Vec<NodeEvent> {
        self.at = Some(at);
        let path = hit(tree, layout, at);
        self.shape = shape(tree, &path);
        self.hover(tree, &path)
    }

    /// The pointer left the tree, as it went out of its window: it leaves
    /// every node that it was over.
    pub fn left(&mut self, tree: &Tree) -> Vec<NodeEvent> {
        self.at = None;
        self.shape = PointerShape::default();
        self.hover(tree, &[])
    }

    /// The tree changed, or its layout, under the pointer that did not move:
    /// the nodes that it is no longer over, and those that it is now over.
    /// A node taken away is left without an event.
    pub fn laid_out(&mut self, tree: &Tree, layout: &Layout) -> Vec<NodeEvent> {
        match self.at {
            Some(at) => self.moved(tree, layout, at),
            None => self.hover(tree, &[]),
        }
    }

    /// `button` went down at `at`: the hover's events, as the pointer came
    /// there, then the context menu's, of the secondary button.
    pub fn pressed(
        &mut self,
        tree: &Tree,
        layout: &Layout,
        at: (f32, f32),
        button: Button,
    ) -> Vec<NodeEvent> {
        let mut events = self.moved(tree, layout, at);
        let target = self.target();
        self.pressed[button.index()] = target.as_ref().map(|(id, _)| *id);
        if button == Button::Secondary
            && let Some((node, key)) = target
        {
            events.push(NodeEvent {
                node,
                key,
                kind: UiEventKind::ContextMenu,
            });
        }
        events
    }

    /// `button` went up at `at`, of `clicks` in a row, as the system's
    /// double click's interval counts them: the hover's events, then a
    /// click, if it went down over the same node, and the second click's
    /// double click.
    pub fn released(
        &mut self,
        tree: &Tree,
        layout: &Layout,
        at: (f32, f32),
        button: Button,
        clicks: u32,
    ) -> Vec<NodeEvent> {
        let mut events = self.moved(tree, layout, at);
        let pressed = self.pressed[button.index()].take();
        let Some((node, key)) = self.target().filter(|(id, _)| Some(*id) == pressed) else {
            return events;
        };
        let kinds: &[UiEventKind] = match button {
            Button::Primary if clicks == 2 => &[UiEventKind::Click, UiEventKind::DoubleClick],
            Button::Primary => &[UiEventKind::Click],
            Button::Middle => &[UiEventKind::MiddleClick],
            Button::Secondary => &[],
        };
        events.extend(kinds.iter().map(|&kind| NodeEvent {
            node,
            key: key.clone(),
            kind,
        }));
        events
    }

    /// The pointer's shape where it is: of the node under it, else the
    /// arrow.
    pub fn shape(&self) -> PointerShape {
        self.shape
    }

    /// The nodes of a key that the pointer is over, from the outermost.
    pub fn hovered(&self) -> impl Iterator<Item = NodeId> + '_ {
        self.hovered.iter().map(|(id, _)| *id)
    }

    /// The innermost node of a key that the pointer is over.
    fn target(&self) -> Option<(NodeId, Key)> {
        self.hovered.last().cloned()
    }

    /// Takes the nodes of a key on `path` as those that the pointer is over:
    /// a leave for each of those before that it is not over, if the tree
    /// still has it, from the innermost; an enter for each new one, from
    /// the outermost.
    fn hover(&mut self, tree: &Tree, path: &[NodeId]) -> Vec<NodeEvent> {
        let now: Vec<(NodeId, Key)> = path
            .iter()
            .filter_map(|&id| Some((id, tree.key(id)?.clone())))
            .collect();
        let mut events = Vec::new();
        for (node, key) in self.hovered.iter().rev() {
            if !now.iter().any(|(id, _)| id == node) && tree.key(*node) == Some(key) {
                events.push(NodeEvent {
                    node: *node,
                    key: key.clone(),
                    kind: UiEventKind::HoverLeave,
                });
            }
        }
        for (node, key) in &now {
            if !self.hovered.iter().any(|(id, _)| id == node) {
                events.push(NodeEvent {
                    node: *node,
                    key: key.clone(),
                    kind: UiEventKind::HoverEnter,
                });
            }
        }
        self.hovered = now;
        events
    }
}

/// Whether `rect` holds the point `at`, its left and top edges in, its
/// right and bottom ones out.
fn contains(rect: Rect, (x, y): (f32, f32)) -> bool {
    let (left, top) = (rect.x as f32, rect.y as f32);
    x >= left && y >= top && x < left + rect.width as f32 && y < top + rect.height as f32
}

/// Where `a` and `b` overlap: an empty rectangle where they do not.
fn intersection(a: Rect, b: Rect) -> Rect {
    let (left, top) = (a.x.max(b.x), a.y.max(b.y));
    let right = right(a).min(right(b));
    let bottom = bottom(a).min(bottom(b));
    Rect {
        x: left,
        y: top,
        width: u32::try_from(right - i64::from(left)).unwrap_or(0),
        height: u32::try_from(bottom - i64::from(top)).unwrap_or(0),
    }
}

/// Whether `a` and `b` have a pixel in common.
fn overlap(a: Rect, b: Rect) -> bool {
    !is_empty(intersection(a, b))
}

/// Whether `inner` lies within `outer`, as an empty rectangle does.
fn within(inner: Rect, outer: Rect) -> bool {
    is_empty(inner)
        || (inner.x >= outer.x
            && inner.y >= outer.y
            && right(inner) <= right(outer)
            && bottom(inner) <= bottom(outer))
}

fn is_empty(rect: Rect) -> bool {
    rect.width == 0 || rect.height == 0
}

fn right(rect: Rect) -> i64 {
    i64::from(rect.x) + i64::from(rect.width)
}

fn bottom(rect: Rect) -> i64 {
    i64::from(rect.y) + i64::from(rect.height)
}

#[cfg(test)]
mod tests {
    use UiEventKind::{Click, ContextMenu, DoubleClick, HoverEnter, HoverLeave, MiddleClick};
    use lokyr_plugin_sdk::view::{self, Border, Length, Node, Style};

    use super::*;
    use crate::layout::Measure;

    /// Texts 6 points a character wide and 14 high, icons of 16.
    struct Fixed;

    impl Measure for Fixed {
        fn text(&mut self, text: &str) -> (f32, f32) {
            (6.0 * text.chars().count() as f32, 14.0)
        }

        fn icon(&mut self, _: &str) -> (f32, f32) {
            (16.0, 16.0)
        }
    }

    fn laid(root: &Node, scale: f32) -> (Tree, Layout) {
        let mut tree = Tree::new();
        let changes = tree.update(root).unwrap();
        let mut layout = Layout::new(scale);
        layout.update(&tree, &changes);
        layout.compute(&tree, None, None, &mut Fixed);
        (tree, layout)
    }

    fn sized(node: Node, width: Length, height: Length) -> Node {
        let layout = view::Layout {
            width,
            height,
            ..node.layout
        };
        node.layout(layout)
    }

    fn pointing(node: Node, shape: PointerShape) -> Node {
        let style = Style {
            pointer: Some(shape),
            ..node.style.clone()
        };
        node.style(style)
    }

    /// A tab of 100 points by 30: its title, without a key, and its close,
    /// of a key, under the hand.
    fn tab(key: &str, title: &str) -> Node {
        let tab = Node::container(vec![
            Node::text(title),
            Node::spacer(),
            Node::icon("close").key(format!("{key}.close").as_str()),
        ])
        .key(key);
        pointing(
            sized(tab, Length::Points(100.0), Length::Points(30.0)),
            PointerShape::PointingHand,
        )
    }

    /// A bar of two tabs, 200 points by 30.
    fn bar() -> Node {
        Node::container(vec![tab("one", "One"), tab("two", "Two")])
    }

    fn find(tree: &Tree, key: &str) -> NodeId {
        fn walk(tree: &Tree, id: NodeId, key: &str) -> Option<NodeId> {
            if tree.key(id).is_some_and(|found| found.0 == key) {
                return Some(id);
            }
            tree.children(id)
                .iter()
                .find_map(|&child| walk(tree, child, key))
        }
        walk(tree, tree.root().unwrap(), key).unwrap()
    }

    fn told(events: &[NodeEvent]) -> Vec<(&str, UiEventKind)> {
        events
            .iter()
            .map(|event| (event.key.0.as_str(), event.kind))
            .collect()
    }

    /// The keys of the nodes hit at `at`, from the root.
    fn keys(tree: &Tree, layout: &Layout, at: (f32, f32)) -> Vec<String> {
        hit(tree, layout, at)
            .iter()
            .map(|&id| tree.key(id).map_or("-".to_owned(), |key| key.0.clone()))
            .collect()
    }

    #[test]
    fn a_click_goes_to_the_innermost_node_of_a_key_pressed_and_released_over_it() {
        let (tree, layout) = laid(&bar(), 1.0);
        let mut pointer = Pointer::new();
        let mut click = |at, button, clicks| {
            let mut events = pointer.pressed(&tree, &layout, at, button);
            events.extend(pointer.released(&tree, &layout, at, button, clicks));
            told(&events)
                .into_iter()
                .filter(|(_, kind)| !matches!(kind, HoverEnter | HoverLeave))
                .map(|(key, kind)| (key.to_owned(), kind))
                .collect::<Vec<_>>()
        };
        let title = (10.0, 15.0);
        let close = (92.0, 15.0);
        assert_eq!(
            click(title, Button::Primary, 1),
            [("one".to_owned(), Click)]
        );
        // The close's click is the close's alone, not its tab's.
        assert_eq!(
            click(close, Button::Primary, 1),
            [("one.close".to_owned(), Click)]
        );
        assert_eq!(
            click((110.0, 15.0), Button::Primary, 2),
            [("two".to_owned(), Click), ("two".to_owned(), DoubleClick)]
        );
        assert_eq!(
            click(title, Button::Primary, 3),
            [("one".to_owned(), Click)]
        );
        assert_eq!(
            click(title, Button::Middle, 1),
            [("one".to_owned(), MiddleClick)]
        );
        // The context menu's comes as the button goes down, and nothing as
        // it goes up.
        assert_eq!(
            click(close, Button::Secondary, 1),
            [("one.close".to_owned(), ContextMenu)]
        );
        // Outside the tabs, nothing of a key.
        assert_eq!(click((150.0, 40.0), Button::Primary, 1), []);

        // Down over the close and up over its tab is no click; nor is a
        // release without a press.
        let mut pointer = Pointer::new();
        pointer.pressed(&tree, &layout, close, Button::Primary);
        let events = pointer.released(&tree, &layout, title, Button::Primary, 1);
        assert_eq!(told(&events), [("one.close", HoverLeave)]);
        let events = pointer.released(&tree, &layout, title, Button::Primary, 1);
        assert_eq!(told(&events), []);
        // A button's press is its own: the middle one's release does not
        // end the primary's.
        pointer.pressed(&tree, &layout, title, Button::Primary);
        assert_eq!(
            told(&pointer.released(&tree, &layout, title, Button::Middle, 1)),
            []
        );
        assert_eq!(
            told(&pointer.released(&tree, &layout, title, Button::Primary, 1)),
            [("one", Click)]
        );
    }

    #[test]
    fn the_places_are_of_the_layout_s_pixels_at_its_scale() {
        let (tree, layout) = laid(&bar(), 2.0);
        assert_eq!(keys(&tree, &layout, (20.0, 30.0)), ["-", "one", "-"]);
        assert_eq!(
            keys(&tree, &layout, (184.0, 30.0)),
            ["-", "one", "one.close"]
        );
        assert_eq!(keys(&tree, &layout, (220.0, 30.0)), ["-", "two", "-"]);
        assert_eq!(keys(&tree, &layout, (220.0, 60.0)), Vec::<String>::new());
    }

    #[test]
    fn the_hover_enters_and_leaves_each_node_of_a_key_its_ancestors_with_it() {
        let (tree, layout) = laid(&bar(), 1.0);
        let mut pointer = Pointer::new();
        let events = pointer.moved(&tree, &layout, (10.0, 15.0));
        assert_eq!(told(&events), [("one", HoverEnter)]);
        assert_eq!(pointer.shape(), PointerShape::PointingHand);
        assert_eq!(told(&pointer.moved(&tree, &layout, (12.0, 15.0))), []);
        // Over the close, still over its tab.
        let events = pointer.moved(&tree, &layout, (92.0, 15.0));
        assert_eq!(told(&events), [("one.close", HoverEnter)]);
        assert_eq!(
            pointer.hovered().collect::<Vec<_>>(),
            [find(&tree, "one"), find(&tree, "one.close")]
        );
        // To the other tab: the close left first, then its tab.
        let events = pointer.moved(&tree, &layout, (110.0, 15.0));
        assert_eq!(
            told(&events),
            [
                ("one.close", HoverLeave),
                ("one", HoverLeave),
                ("two", HoverEnter)
            ]
        );
        // Into the close straight from outside: its tab first.
        let events = pointer.moved(&tree, &layout, (150.0, 50.0));
        assert_eq!(told(&events), [("two", HoverLeave)]);
        assert_eq!(pointer.shape(), PointerShape::Arrow);
        let events = pointer.moved(&tree, &layout, (192.0, 15.0));
        assert_eq!(
            told(&events),
            [("two", HoverEnter), ("two.close", HoverEnter)]
        );
        let events = pointer.left(&tree);
        assert_eq!(
            told(&events),
            [("two.close", HoverLeave), ("two", HoverLeave)]
        );
        assert_eq!(pointer.shape(), PointerShape::Arrow);
    }

    fn relaid(tree: &mut Tree, layout: &mut Layout, root: &Node) {
        let changes = tree.update(root).unwrap();
        layout.update(tree, &changes);
        layout.compute(tree, None, None, &mut Fixed);
    }

    #[test]
    fn a_tree_changed_under_the_still_pointer_tells_what_it_is_over_now() {
        let (mut tree, mut layout) = laid(&bar(), 1.0);
        let mut pointer = Pointer::new();
        let at = (50.0, 15.0);
        pointer.moved(&tree, &layout, at);
        pointer.pressed(&tree, &layout, at, Button::Primary);
        // The first tab closes, and the second takes its place under the
        // pointer: the first, taken away, is left without an event.
        relaid(
            &mut tree,
            &mut layout,
            &Node::container(vec![tab("two", "Two")]),
        );
        assert_eq!(
            told(&pointer.laid_out(&tree, &layout)),
            [("two", HoverEnter)]
        );
        // The press was over the first, which no release clicks.
        let events = pointer.released(&tree, &layout, at, Button::Primary, 1);
        assert_eq!(told(&events), []);
        // A tab opens before it, under the pointer.
        relaid(&mut tree, &mut layout, &bar());
        assert_eq!(
            told(&pointer.laid_out(&tree, &layout)),
            [("two", HoverLeave), ("one", HoverEnter)]
        );
        relaid(&mut tree, &mut layout, &Node::container(vec![]));
        assert_eq!(told(&pointer.laid_out(&tree, &layout)), []);
        assert_eq!(pointer.hovered().count(), 0);
        // Out of the window, a change tells nothing.
        relaid(&mut tree, &mut layout, &bar());
        pointer.left(&tree);
        assert_eq!(told(&pointer.laid_out(&tree, &layout)), []);
    }

    /// A column of 100 points: `a`, a row 20 high, of `a1`, 50 by 60,
    /// which overflows it, and `b`, 60 by 20, which draws over `a1`'s
    /// overflow; `a` clips if `clip`, inside a border of 2 points.
    fn overlapping(clip: bool) -> Node {
        let a1 = sized(
            Node::container(vec![]).key("a1"),
            Length::Points(50.0),
            Length::Points(60.0),
        );
        let a = sized(
            Node::container(vec![a1]).key("a"),
            Length::Auto,
            Length::Points(20.0),
        )
        .style(Style {
            border: Some(Border {
                color: "panel.border".into(),
                width: 2.0,
            }),
            clip,
            ..Style::default()
        });
        let b = sized(
            Node::container(vec![]).key("b"),
            Length::Points(60.0),
            Length::Points(20.0),
        );
        let column = Node::container(vec![a, b]).layout(view::Layout {
            direction: view::Direction::Column,
            width: Length::Points(100.0),
            ..view::Layout::default()
        });
        column.key("column")
    }

    #[test]
    fn the_topmost_node_within_its_clips_is_hit() {
        let (tree, layout) = laid(&overlapping(false), 1.0);
        assert_eq!(keys(&tree, &layout, (10.0, 10.0)), ["column", "a", "a1"]);
        // `b` draws over `a1`'s overflow.
        assert_eq!(keys(&tree, &layout, (10.0, 30.0)), ["column", "b"]);
        assert_eq!(keys(&tree, &layout, (80.0, 30.0)), ["column"]);
        // `a1`'s overflow out of every box is hit.
        assert_eq!(keys(&tree, &layout, (10.0, 50.0)), ["column", "a", "a1"]);
        assert_eq!(keys(&tree, &layout, (60.0, 50.0)), Vec::<String>::new());
        // Where `a` clips, inside its border.
        let (tree, layout) = laid(&overlapping(true), 1.0);
        assert_eq!(keys(&tree, &layout, (10.0, 10.0)), ["column", "a", "a1"]);
        assert_eq!(keys(&tree, &layout, (1.0, 1.0)), ["column", "a"]);
        assert_eq!(keys(&tree, &layout, (10.0, 19.0)), ["column", "a"]);
        assert_eq!(keys(&tree, &layout, (10.0, 30.0)), ["column", "b"]);
        assert_eq!(keys(&tree, &layout, (10.0, 50.0)), Vec::<String>::new());
        // The events follow.
        let mut pointer = Pointer::new();
        pointer.pressed(&tree, &layout, (10.0, 19.0), Button::Primary);
        let events = pointer.released(&tree, &layout, (10.0, 19.0), Button::Primary, 1);
        assert_eq!(told(&events), [("a", Click)]);
    }

    /// The shape that the first of `areas` over `at` gives, if one is.
    fn shape_over(areas: &[Area], at: (f32, f32)) -> Option<PointerShape> {
        areas
            .iter()
            .find(|area| contains(area.rect, at))
            .map(|area| area.shape)
    }

    /// Whether the areas give each pixel of `extent` the shape of the node
    /// hit there, and none where none is.
    fn areas_agree(tree: &Tree, layout: &Layout, extent: (u32, u32)) {
        let areas = areas(tree, layout);
        for y in 0..extent.1 {
            for x in 0..extent.0 {
                let at = (x as f32 + 0.5, y as f32 + 0.5);
                let path = hit(tree, layout, at);
                let expected = (!path.is_empty()).then(|| shape(tree, &path));
                assert_eq!(shape_over(&areas, at), expected, "at {at:?}");
            }
        }
    }

    #[test]
    fn the_areas_give_each_pixel_the_shape_of_the_node_hit_there() {
        // A tab's title and close within its area: one area a tab.
        let (tree, layout) = laid(&bar(), 1.0);
        assert_eq!(areas(&tree, &layout).len(), 3);
        areas_agree(&tree, &layout, (220, 40));
        let (tree, layout) = laid(&bar(), 2.0);
        areas_agree(&tree, &layout, (440, 80));
        // `a` of the hand, `a1` of its shape over `b`'s arrow, `b` over
        // `a1`'s overflow, of the text's I-beam; `b`'s child of the hand.
        for clip in [false, true] {
            let mut root = overlapping(clip);
            root.children[0] = pointing(root.children[0].clone(), PointerShape::PointingHand);
            root.children[1] = pointing(root.children[1].clone(), PointerShape::Text);
            root.children[1].children = vec![pointing(
                sized(
                    Node::container(vec![]),
                    Length::Points(10.0),
                    Length::Points(10.0),
                ),
                PointerShape::PointingHand,
            )];
            let (tree, layout) = laid(&root, 1.0);
            areas_agree(&tree, &layout, (110, 70));
            // `b` without a shape of its own: the arrow of the column, over
            // `a1`'s hand.
            root.children[1].style.pointer = None;
            let (tree, layout) = laid(&root, 1.0);
            areas_agree(&tree, &layout, (110, 70));
        }
    }
}
