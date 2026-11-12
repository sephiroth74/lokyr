//! The transitions of a view tree's nodes, which the host drives: a node of
//! a transition that the layout of a new render moves slides from where it
//! showed to its new place, and one whose opacity a render changes fades
//! from the opacity that it showed to its new one, over the transition's
//! time and along its curve; a change during a transition goes on from
//! where it was. A node's place is its place in points from the nearest
//! node around it of a transition, which slides it with its own; and a
//! layout of no new render, as a resize's or another display's scale,
//! moves nodes without a slide. The host
//! draws a frame at each refresh of the display while one runs, and none
//! once they all end; where the system reduces motion, they end at once.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use lokyr_plugin_sdk::view::{Easing, Key, Transition};

use crate::layout::Layout;
use crate::reorder::Reorder;
use crate::tree::{NodeId, Tree};

/// The transitions of a tree's nodes, by their keys.
#[derive(Debug)]
pub struct Animations {
    nodes: HashMap<Key, Animated>,
    /// The tree's revision of the last layout.
    revision: Option<u64>,
    /// The pixels a point of the last layout.
    scale: f32,
    /// The nodes of a key whose hover shows, their own or their
    /// children's of their parent's, each of its node's transition, if it
    /// has one.
    hoverable: HashMap<Key, Option<Transition>>,
    /// Their hovers, as the pointer goes over them and away.
    hovers: HashMap<Key, Hovering>,
    /// The nodes of a key whose press shows, each of its transition.
    pressable: HashMap<Key, Option<Transition>>,
    /// Their presses, as the primary button goes down over them and up.
    presses: HashMap<Key, Hovering>,
    /// Whether the system reduces motion, as the last layout found it.
    reduced: bool,
    /// The drag of a reorderable node, and its slides.
    pub(crate) reorder: Reorder,
}

impl Default for Animations {
    fn default() -> Self {
        Self {
            nodes: HashMap::new(),
            revision: None,
            scale: 1.0,
            hoverable: HashMap::new(),
            hovers: HashMap::new(),
            pressable: HashMap::new(),
            presses: HashMap::new(),
            reduced: false,
            reorder: Reorder::new(),
        }
    }
}

/// A node's hover: whether the pointer is over it, and its fade there, from
/// how far it showed.
#[derive(Clone, Copy, Debug)]
struct Hovering {
    on: bool,
    fading: Option<Run<f32>>,
}

impl Hovering {
    /// How far its hover shows at `now`, from 0, none, to 1, all of it.
    fn amount(&self, now: Instant) -> f32 {
        let target = if self.on { 1.0 } else { 0.0 };
        match self.fading {
            Some(run) if !run.ended(now) => run.value + (target - run.value) * run.progress(now),
            _ => target,
        }
    }
}

/// A node of a transition, as it showed last and as it moves.
#[derive(Debug)]
struct Animated {
    /// Its place, in points from the nearest node around it of a
    /// transition, as its last layout put it.
    place: (f32, f32),
    /// Its opacity, as its last render gave it.
    opacity: f32,
    /// Its move, from where it showed, in points, as it slides to its
    /// place.
    moving: Option<Run<(f32, f32)>>,
    /// Its fade, from the opacity that it showed to its own.
    fading: Option<Run<(f32, f32)>>,
}

/// A transition under way: from what to what, from when, how long and
/// along which curve.
#[derive(Clone, Copy, Debug)]
struct Run<T> {
    value: T,
    start: Instant,
    duration: Duration,
    easing: Easing,
}

impl<T> Run<T> {
    /// How far it went at `now`, from 0 to 1, along its curve.
    fn progress(&self, now: Instant) -> f32 {
        let total = self.duration.as_secs_f32();
        if total <= 0.0 {
            return 1.0;
        }
        let t = (now.saturating_duration_since(self.start).as_secs_f32() / total).clamp(0.0, 1.0);
        ease(self.easing, t)
    }

    /// Whether it ended at `now`.
    fn ended(&self, now: Instant) -> bool {
        now.saturating_duration_since(self.start) >= self.duration
    }
}

/// `t`, from 0 to 1, along `easing`'s curve.
pub fn ease(easing: Easing, t: f32) -> f32 {
    match easing {
        Easing::Linear => t,
        Easing::EaseIn => t * t,
        Easing::EaseOut => 1.0 - (1.0 - t) * (1.0 - t),
        Easing::EaseInOut => {
            if t < 0.5 {
                2.0 * t * t
            } else {
                1.0 - (-2.0 * t + 2.0).powi(2) / 2.0
            }
        }
    }
}

impl Animations {
    /// No transition.
    pub fn new() -> Self {
        Self::default()
    }

    /// `tree` was laid out in `layout`, at `now`: each node of a transition
    /// that moved since a new render starts to slide from where it showed,
    /// and each whose opacity changed to fade, unless `reduced`, as the
    /// system reduces motion, which ends them at once; a layout of the same
    /// render moves them without a slide; the nodes gone, or of no
    /// transition, are forgotten.
    pub fn laid_out(&mut self, tree: &Tree, layout: &Layout, now: Instant, reduced: bool) {
        let rendered = self.revision != Some(tree.revision());
        self.revision = Some(tree.revision());
        self.reduced = reduced;
        if rendered {
            self.hoverable.clear();
            self.pressable.clear();
            if let Some(root) = tree.root() {
                hoverable(tree, root, None, &mut self.hoverable);
                pressable(tree, root, &mut self.pressable);
            }
            let (hoverable, pressable) = (&self.hoverable, &self.pressable);
            self.hovers.retain(|key, _| hoverable.contains_key(key));
            self.presses.retain(|key, _| pressable.contains_key(key));
        }
        let scale = layout.scale();
        self.scale = scale;
        let mut seen = Vec::new();
        if let Some(root) = tree.root() {
            walk(tree, root, None, &mut seen);
        }
        // Of no scroll, which moves a node without a transition; in points,
        // which a display's scale does not change.
        let points = |id| {
            layout
                .placed(tree, id)
                .map(|frame| (frame.x / scale, frame.y / scale))
        };
        let mut kept = HashMap::new();
        for (node, transition, around) in seen {
            let (Some(key), Some(place), Some(style)) =
                (tree.key(node), points(node), tree.style(node))
            else {
                continue;
            };
            let base = around.and_then(points).unwrap_or((0.0, 0.0));
            let place = (place.0 - base.0, place.1 - base.1);
            let opacity = style.opacity.unwrap_or(1.0);
            let Some(mut animated) = self.nodes.remove(key) else {
                kept.insert(
                    key.clone(),
                    Animated {
                        place,
                        opacity,
                        moving: None,
                        fading: None,
                    },
                );
                continue;
            };
            let run = |value| Run {
                value,
                start: now,
                // To the microsecond, as an `f32` of seconds is not one.
                duration: Duration::from_micros((transition.duration.max(0.0) * 1e6).round() as u64),
                easing: transition.easing,
            };
            let (dx, dy) = (animated.place.0 - place.0, animated.place.1 - place.1);
            if rendered && (dx, dy) != (0.0, 0.0) {
                let (x, y) = animated.offset(now);
                animated.moving = (!reduced).then(|| run((x + dx, y + dy)));
            }
            if opacity != animated.opacity {
                let from = animated.shown_opacity(now);
                animated.fading = (!reduced).then(|| run((from, opacity)));
            }
            animated.place = place;
            animated.opacity = opacity;
            kept.insert(key.clone(), animated);
        }
        self.nodes = kept;
        // A drop's nodes slide of the drag's slides alone, from where they
        // showed.
        for key in self.reorder.laid_out(tree, layout, now, reduced) {
            if let Some(animated) = self.nodes.get_mut(&key) {
                animated.moving = None;
            }
        }
    }

    /// Whether a transition runs at `now`, for which the host draws a frame
    /// at each refresh of the display: a slide, a fade, or a hover's fade.
    pub fn running(&self, now: Instant) -> bool {
        self.nodes.values().any(|animated| {
            animated.moving.is_some_and(|run| !run.ended(now))
                || animated.fading.is_some_and(|run| !run.ended(now))
        }) || self
            .hovers
            .values()
            .chain(self.presses.values())
            .any(|hovering| hovering.fading.is_some_and(|run| !run.ended(now)))
            || self.reorder.running(now)
    }

    /// The pointer is over the nodes of `hovered`'s keys, at `now`: the
    /// hover of each node that shows one fades in or out, from where it
    /// showed, as its transition says, at once without one or where the
    /// system reduces motion.
    pub fn hovered<'a>(&mut self, hovered: impl IntoIterator<Item = &'a Key>, now: Instant) {
        let over: Vec<&Key> = hovered.into_iter().collect();
        follow(
            &self.hoverable,
            &mut self.hovers,
            &over,
            (now, self.reduced),
        );
    }

    /// The primary button is down over the node of `pressing`'s key, the
    /// pointer still over it, if it is, at `now`: the press of each node
    /// that shows one fades in or out, as [`Animations::hovered`] has a
    /// hover do.
    pub fn pressed(&mut self, pressing: Option<&Key>, now: Instant) {
        let over: Vec<&Key> = pressing.into_iter().collect();
        follow(
            &self.pressable,
            &mut self.presses,
            &over,
            (now, self.reduced),
        );
    }

    /// How far the press of the node of `key` shows at `now`, from 0, none,
    /// to 1, all of it.
    pub fn press(&self, key: &Key, now: Instant) -> f32 {
        self.presses
            .get(key)
            .map_or(0.0, |pressing| pressing.amount(now))
    }

    /// How far the hover of the node of `key` shows at `now`, from 0, none,
    /// to 1, all of it.
    pub fn hover(&self, key: &Key, now: Instant) -> f32 {
        self.hovers
            .get(key)
            .map_or(0.0, |hovering| hovering.amount(now))
    }

    /// How far the node of `key` shows from its place at `now`, in pixels
    /// of the last layout, as it slides to it, past the slides of the
    /// nodes around it, and as a drag moves it among its siblings.
    pub fn offset(&self, key: &Key, now: Instant) -> (f32, f32) {
        let (x, y) = self
            .nodes
            .get(key)
            .map_or((0.0, 0.0), |animated| animated.offset(now));
        let (across, down) = self.reorder.offset(key, now);
        ((x + across) * self.scale, (y + down) * self.scale)
    }

    /// The key of the node dragged among its siblings, or on its way to its
    /// place, which shows over them.
    pub fn raised(&self) -> Option<&Key> {
        self.reorder.raised()
    }

    /// The opacity that the node of `key` shows at `now`, as it fades; its
    /// style's, `own`, without a fade.
    pub fn opacity(&self, key: &Key, own: f32, now: Instant) -> f32 {
        // A node dragged out of its parent's room shows nowhere.
        if self.reorder.hidden(key) {
            return 0.0;
        }
        self.nodes
            .get(key)
            .map_or(own, |animated| animated.shown_opacity(now))
    }
}

impl Animated {
    fn offset(&self, now: Instant) -> (f32, f32) {
        match self.moving {
            Some(run) if !run.ended(now) => {
                let left = 1.0 - run.progress(now);
                (run.value.0 * left, run.value.1 * left)
            }
            _ => (0.0, 0.0),
        }
    }

    fn shown_opacity(&self, now: Instant) -> f32 {
        match self.fading {
            Some(run) if !run.ended(now) => {
                let (from, to) = run.value;
                from + (to - from) * run.progress(now)
            }
            _ => self.opacity,
        }
    }
}

/// Turns on the states of `states` of the keys of `on`, of those that
/// `shown` holds, each of its transition, and off the others, at `now`,
/// each fading from where it showed, at once without a transition or
/// where the system reduces motion, `reduced`.
fn follow(
    shown: &HashMap<Key, Option<Transition>>,
    states: &mut HashMap<Key, Hovering>,
    on: &[&Key],
    (now, reduced): (Instant, bool),
) {
    for (key, transition) in shown {
        let holds = on.contains(&key);
        let state = states.entry(key.clone()).or_insert(Hovering {
            on: false,
            fading: None,
        });
        if state.on == holds {
            continue;
        }
        let from = state.amount(now);
        state.on = holds;
        state.fading = transition.filter(|_| !reduced).map(|transition| Run {
            value: from,
            start: now,
            duration: Duration::from_micros((transition.duration.max(0.0) * 1e6).round() as u64),
            easing: transition.easing,
        });
    }
}

/// Takes into `found` the nodes of `id`'s subtree of a key and a press of
/// their own, each of its transition.
fn pressable(tree: &Tree, id: NodeId, found: &mut HashMap<Key, Option<Transition>>) {
    if let (Some(key), Some(style)) = (tree.key(id), tree.style(id))
        && style.pressed.is_some()
    {
        found.insert(key.clone(), style.transition);
    }
    for &child in tree.children(id) {
        pressable(tree, child, found);
    }
}

/// Takes into `found` the nodes of `id`'s subtree whose hover shows, of
/// `around`, the nearest node of a key around `id`: those of a key and a
/// hover of their own, and those of a key around a node of its parent's
/// hover, each of its own transition.
fn hoverable(
    tree: &Tree,
    id: NodeId,
    around: Option<NodeId>,
    found: &mut HashMap<Key, Option<Transition>>,
) {
    let style = tree.style(id);
    let mut take = |node: NodeId| {
        if let Some(key) = tree.key(node) {
            let transition = tree.style(node).and_then(|style| style.transition);
            found.insert(key.clone(), transition);
        }
    };
    if style.is_some_and(|style| style.hover.is_some()) {
        take(id);
    }
    if let (Some(around), true) = (
        around,
        style.is_some_and(|style| style.parent_hover.is_some()),
    ) {
        take(around);
    }
    let inner = if tree.key(id).is_some() {
        Some(id)
    } else {
        around
    };
    for &child in tree.children(id) {
        hoverable(tree, child, inner, found);
    }
}

/// The nodes of `id`'s subtree of a key and a transition, with it and the
/// nearest node around each of them, from `around`.
fn walk(
    tree: &Tree,
    id: NodeId,
    around: Option<NodeId>,
    seen: &mut Vec<(NodeId, Transition, Option<NodeId>)>,
) {
    let mut inner = around;
    if tree.key(id).is_some()
        && let Some(transition) = tree.style(id).and_then(|style| style.transition)
    {
        seen.push((id, transition, around));
        inner = Some(id);
    }
    for &child in tree.children(id) {
        walk(tree, child, inner, seen);
    }
}

#[cfg(test)]
mod tests {
    use lokyr_plugin_sdk::view::{self, Direction, Length, Node, Style};

    use super::*;
    use crate::layout::Measure;

    struct Fixed;

    impl Measure for Fixed {
        fn text(&mut self, text: &str) -> (f32, f32) {
            (6.0 * text.chars().count() as f32, 14.0)
        }

        fn icon(&mut self, _: &str) -> (f32, f32) {
            (16.0, 16.0)
        }
    }

    const LINEAR: Transition = Transition {
        duration: 0.2,
        easing: Easing::Linear,
    };

    /// A row of boxes 20 points wide, of the keys of `order`, of a linear
    /// transition, the box `b` of `opacity`.
    fn row(order: &[&str], opacity: Option<f32>) -> Node {
        Node::container(
            order
                .iter()
                .map(|&key| {
                    Node::container(vec![])
                        .key(key)
                        .layout(view::Layout {
                            width: Length::Points(20.0),
                            height: Length::Points(10.0),
                            ..view::Layout::default()
                        })
                        .style(Style {
                            transition: Some(LINEAR),
                            opacity: if key == "b" { opacity } else { None },
                            ..Style::default()
                        })
                })
                .collect(),
        )
        .layout(view::Layout {
            direction: Direction::Row,
            ..view::Layout::default()
        })
    }

    fn laid(tree: &mut Tree, layout: &mut Layout, root: &Node) {
        let changes = tree.update(root).unwrap();
        layout.update(tree, &changes);
        layout.compute(tree, Some(200.0), None, &mut Fixed);
    }

    #[test]
    fn a_node_that_a_layout_moves_slides_to_its_place_and_ends() {
        let mut tree = Tree::new();
        let mut layout = Layout::new(1.0);
        let mut animations = Animations::new();
        let start = Instant::now();
        laid(&mut tree, &mut layout, &row(&["a", "b", "c"], None));
        animations.laid_out(&tree, &layout, start, false);
        assert!(!animations.running(start));
        // `c` to the front: it slides from 40 left, the others 20 right.
        laid(&mut tree, &mut layout, &row(&["c", "a", "b"], None));
        animations.laid_out(&tree, &layout, start, false);
        let key = |key: &str| Key::from(key);
        assert!(animations.running(start));
        assert_eq!(animations.offset(&key("c"), start), (40.0, 0.0));
        assert_eq!(animations.offset(&key("a"), start), (-20.0, 0.0));
        let half = start + Duration::from_millis(100);
        assert_eq!(animations.offset(&key("c"), half), (20.0, 0.0));
        // Moved back halfway: from where it shows.
        laid(&mut tree, &mut layout, &row(&["a", "b", "c"], None));
        animations.laid_out(&tree, &layout, half, false);
        assert_eq!(animations.offset(&key("c"), half), (-20.0, 0.0));
        let end = half + Duration::from_millis(200);
        assert_eq!(animations.offset(&key("c"), end), (0.0, 0.0));
        assert!(!animations.running(end));
    }

    #[test]
    fn a_node_inside_a_sliding_one_slides_with_it_alone() {
        let sliding = |node: Node| {
            node.style(Style {
                transition: Some(LINEAR),
                ..Style::default()
            })
        };
        // A box of a transition, its child of one, past a room of `left`.
        let nested = |left: f32| {
            let child = sliding(Node::container(vec![]).key("child").layout(view::Layout {
                width: Length::Points(10.0),
                height: Length::Points(10.0),
                ..view::Layout::default()
            }));
            Node::container(vec![
                Node::container(vec![]).layout(view::Layout {
                    width: Length::Points(left),
                    ..view::Layout::default()
                }),
                sliding(Node::container(vec![child]).key("parent")),
            ])
            .layout(view::Layout {
                direction: Direction::Row,
                ..view::Layout::default()
            })
        };
        let mut tree = Tree::new();
        let mut layout = Layout::new(2.0);
        let mut animations = Animations::new();
        let start = Instant::now();
        for left in [0.0, 20.0] {
            laid(&mut tree, &mut layout, &nested(left));
            animations.laid_out(&tree, &layout, start, false);
        }
        // Halfway, the box 10 points, 20 pixels, short of its place, and
        // its child of no slide of its own, which the box's moves.
        let half = start + Duration::from_millis(100);
        assert_eq!(animations.offset(&Key::from("parent"), half), (-20.0, 0.0));
        assert_eq!(animations.offset(&Key::from("child"), half), (0.0, 0.0));
    }

    #[test]
    fn a_layout_of_the_same_render_slides_nothing() {
        let centred = |order: &[&str]| {
            let mut row = row(order, None);
            row.layout.justify = view::Justify::Center;
            row
        };
        let mut tree = Tree::new();
        let mut layout = Layout::new(1.0);
        let mut animations = Animations::new();
        let start = Instant::now();
        laid(&mut tree, &mut layout, &centred(&["a", "b", "c"]));
        animations.laid_out(&tree, &layout, start, false);
        // A resize, which moves the centred boxes, and another display's
        // scale: no slide.
        layout.compute(&tree, Some(300.0), None, &mut Fixed);
        animations.laid_out(&tree, &layout, start, false);
        assert!(!animations.running(start));
        layout.set_scale(&tree, 2.0);
        layout.compute(&tree, Some(600.0), None, &mut Fixed);
        animations.laid_out(&tree, &layout, start, false);
        assert!(!animations.running(start));
        // A new render slides from where the resize put them: `c` to the
        // front, 40 points, 80 pixels, from its place.
        let changes = tree.update(&centred(&["c", "a", "b"])).unwrap();
        layout.update(&tree, &changes);
        layout.compute(&tree, Some(600.0), None, &mut Fixed);
        animations.laid_out(&tree, &layout, start, false);
        assert_eq!(animations.offset(&Key::from("c"), start), (80.0, 0.0));
    }

    #[test]
    fn an_opacity_that_a_render_changes_fades_and_reduced_motion_ends_at_once() {
        let mut tree = Tree::new();
        let mut layout = Layout::new(1.0);
        let mut animations = Animations::new();
        let start = Instant::now();
        let key = Key::from("b");
        laid(&mut tree, &mut layout, &row(&["a", "b"], Some(0.0)));
        animations.laid_out(&tree, &layout, start, false);
        assert_eq!(animations.opacity(&key, 0.0, start), 0.0);
        laid(&mut tree, &mut layout, &row(&["a", "b"], Some(1.0)));
        animations.laid_out(&tree, &layout, start, false);
        let quarter = start + Duration::from_millis(50);
        assert_eq!(animations.opacity(&key, 1.0, quarter), 0.25);
        assert!(animations.running(quarter));
        // Changed back a quarter in: from the quarter that it shows.
        laid(&mut tree, &mut layout, &row(&["a", "b"], Some(0.0)));
        animations.laid_out(&tree, &layout, quarter, false);
        assert_eq!(animations.opacity(&key, 0.0, quarter), 0.25);
        let end = quarter + Duration::from_millis(200);
        assert_eq!(animations.opacity(&key, 0.0, end), 0.0);
        laid(&mut tree, &mut layout, &row(&["a", "b"], Some(1.0)));
        animations.laid_out(&tree, &layout, end, true);
        assert_eq!(animations.opacity(&key, 1.0, end), 1.0);
        assert!(!animations.running(end));
        // Of reduced motion, at once.
        laid(&mut tree, &mut layout, &row(&["b", "a"], Some(0.5)));
        animations.laid_out(&tree, &layout, end, true);
        assert!(!animations.running(end));
        assert_eq!(animations.offset(&key, end), (0.0, 0.0));
        assert_eq!(animations.opacity(&key, 0.5, end), 0.5);
        // A node of no transition, or gone, is forgotten: its own.
        laid(&mut tree, &mut layout, &Node::container(vec![]));
        animations.laid_out(&tree, &layout, end, false);
        assert_eq!(animations.opacity(&key, 0.7, end), 0.7);
    }

    #[test]
    fn a_scroll_moves_no_node_of_a_transition() {
        let area = |order: &[&str]| {
            Node::container(vec![
                Node::scroll(vec![row(order, None)])
                    .key("area")
                    .layout(view::Layout {
                        direction: Direction::Column,
                        height: Length::Points(5.0),
                        ..view::Layout::default()
                    }),
            ])
            .layout(view::Layout {
                direction: Direction::Column,
                ..view::Layout::default()
            })
        };
        let mut tree = Tree::new();
        let mut layout = Layout::new(1.0);
        let mut animations = Animations::new();
        let start = Instant::now();
        laid(&mut tree, &mut layout, &area(&["a", "b"]));
        animations.laid_out(&tree, &layout, start, false);
        let scroll = tree.children(tree.root().unwrap())[0];
        assert!(layout.scroll_by(&tree, scroll, 4.0));
        animations.laid_out(&tree, &layout, start, false);
        assert!(!animations.running(start));
    }

    #[test]
    fn the_curves_go_from_0_to_1() {
        for easing in [
            Easing::Linear,
            Easing::EaseIn,
            Easing::EaseOut,
            Easing::EaseInOut,
        ] {
            assert_eq!(ease(easing, 0.0), 0.0);
            assert_eq!(ease(easing, 1.0), 1.0);
        }
        assert_eq!(ease(Easing::EaseIn, 0.5), 0.25);
        assert_eq!(ease(Easing::EaseOut, 0.5), 0.75);
        assert_eq!(ease(Easing::EaseInOut, 0.25), 0.125);
        assert_eq!(ease(Easing::EaseInOut, 0.75), 0.875);
    }
}
