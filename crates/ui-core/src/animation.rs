//! The transitions of a view tree's nodes, which the host drives: a node of
//! a transition that a layout moves slides from where it showed to its new
//! place, and one whose opacity a render changes fades from the opacity
//! that it showed to its new one, over the transition's time and along its
//! curve; a change during a transition goes on from where it was. The host
//! draws a frame at each refresh of the display while one runs, and none
//! once they all end; where the system reduces motion, they end at once.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use lokyr_plugin_sdk::view::{Easing, Key, Transition};

use crate::layout::{Frame, Layout};
use crate::tree::{NodeId, Tree};

/// The transitions of a tree's nodes, by their keys.
#[derive(Debug, Default)]
pub struct Animations {
    nodes: HashMap<Key, Animated>,
}

/// A node of a transition, as it showed last and as it moves.
#[derive(Debug)]
struct Animated {
    /// Its place, in pixels, as its last layout put it.
    frame: Frame,
    /// Its opacity, as its last render gave it.
    opacity: f32,
    /// Its move, from where it showed, as it slides to its place.
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
    /// that moved starts to slide from where it showed, and each whose
    /// opacity changed to fade, unless `reduced`, as the system reduces
    /// motion, which ends them at once; the nodes gone, or of no
    /// transition, are forgotten.
    pub fn laid_out(&mut self, tree: &Tree, layout: &Layout, now: Instant, reduced: bool) {
        let mut seen = Vec::new();
        if let Some(root) = tree.root() {
            walk(tree, root, &mut seen);
        }
        let mut kept = HashMap::new();
        for (node, transition) in seen {
            // Of no scroll, which moves a node without a transition.
            let (Some(key), Some(frame), Some(style)) =
                (tree.key(node), layout.placed(tree, node), tree.style(node))
            else {
                continue;
            };
            let opacity = style.opacity.unwrap_or(1.0);
            let Some(mut animated) = self.nodes.remove(key) else {
                kept.insert(
                    key.clone(),
                    Animated {
                        frame,
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
            let (dx, dy) = (animated.frame.x - frame.x, animated.frame.y - frame.y);
            if (dx, dy) != (0.0, 0.0) {
                let (x, y) = animated.offset(now);
                animated.moving = (!reduced).then(|| run((x + dx, y + dy)));
            }
            if opacity != animated.opacity {
                let from = animated.shown_opacity(now);
                animated.fading = (!reduced).then(|| run((from, opacity)));
            }
            animated.frame = frame;
            animated.opacity = opacity;
            kept.insert(key.clone(), animated);
        }
        self.nodes = kept;
    }

    /// Whether a transition runs at `now`, for which the host draws a frame
    /// at each refresh of the display.
    pub fn running(&self, now: Instant) -> bool {
        self.nodes.values().any(|animated| {
            animated.moving.is_some_and(|run| !run.ended(now))
                || animated.fading.is_some_and(|run| !run.ended(now))
        })
    }

    /// How far the node of `key` shows from its place at `now`, in pixels,
    /// as it slides to it.
    pub fn offset(&self, key: &Key, now: Instant) -> (f32, f32) {
        self.nodes
            .get(key)
            .map_or((0.0, 0.0), |animated| animated.offset(now))
    }

    /// The opacity that the node of `key` shows at `now`, as it fades; its
    /// style's, `own`, without a fade.
    pub fn opacity(&self, key: &Key, own: f32, now: Instant) -> f32 {
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

/// The nodes of `id`'s subtree of a key and a transition, with it.
fn walk(tree: &Tree, id: NodeId, seen: &mut Vec<(NodeId, Transition)>) {
    if tree.key(id).is_some()
        && let Some(transition) = tree.style(id).and_then(|style| style.transition)
    {
        seen.push((id, transition));
    }
    for &child in tree.children(id) {
        walk(tree, child, seen);
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
