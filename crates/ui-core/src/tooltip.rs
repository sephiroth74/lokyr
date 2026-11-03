//! The tooltips: the text of a node's tooltip, which shows as the pointer
//! rests over the node for [`DELAY`], at the pointer's place, and hides as
//! it leaves the node. A press, a key or a scroll hides it until the
//! pointer leaves the node. Once one showed, the next shows at once, as the
//! pointer goes along a bar's buttons, for [`WARM`] after it hid, as
//! macOS's do.
//!
//! The host follows the pointer, and wakes at the time that the tooltip
//! waits for, by a timer of its own: no frame and no wakeup while no
//! tooltip waits.

use std::time::{Duration, Instant};

use lokyr_plugin_sdk::view::Key;

use crate::tree::{NodeId, Tree};

/// How long the pointer rests over a node before its tooltip shows.
pub const DELAY: Duration = Duration::from_millis(1000);

/// How long after a tooltip hid the next shows at once.
pub const WARM: Duration = Duration::from_millis(500);

/// The tooltip of the node that the pointer is over, if it has one, and
/// when the last one hid.
#[derive(Debug, Default)]
pub struct Tooltip {
    over: Option<Over>,
    hid: Option<Instant>,
}

/// The node of a tooltip that the pointer is over.
#[derive(Debug)]
struct Over {
    /// The node's key.
    key: Key,
    /// When the pointer came over it.
    since: Instant,
    /// Where the pointer is, or was as the tooltip showed, in pixels from
    /// the tree's corner.
    at: (f32, f32),
    /// Whether the tooltip shows.
    shown: bool,
    /// Whether a press, a key or a scroll hid it, until the pointer leaves
    /// the node.
    dismissed: bool,
}

impl Tooltip {
    /// No tooltip.
    pub fn new() -> Self {
        Self::default()
    }

    /// The pointer is at `at`, in pixels from the tree's corner, over the
    /// nodes `hovered` of `tree`, from the outermost, at `now`: the
    /// innermost of a tooltip is the one that it waits for, at once if
    /// one hid a moment ago. The time to call [`Tooltip::tick`] at, while
    /// it waits.
    pub fn hover(
        &mut self,
        tree: &Tree,
        hovered: impl DoubleEndedIterator<Item = NodeId>,
        at: (f32, f32),
        now: Instant,
    ) -> Option<Instant> {
        let target = hovered
            .rev()
            .find(|&node| tree.tooltip(node).is_some())
            .and_then(|node| tree.key(node));
        match (&mut self.over, target) {
            (Some(over), Some(key)) if over.key == *key => {
                // A tooltip that shows stays where it showed.
                if !over.shown {
                    over.at = at;
                }
            }
            (_, target) => {
                if self.over.as_ref().is_some_and(|over| over.shown) {
                    self.hid = Some(now);
                }
                let warm = self
                    .hid
                    .is_some_and(|hid| now.saturating_duration_since(hid) < WARM);
                self.over = target.map(|key| Over {
                    key: key.clone(),
                    since: now,
                    at,
                    shown: warm,
                    dismissed: false,
                });
            }
        }
        self.deadline()
    }

    /// The time that the tooltip waits for, if one waits.
    pub fn deadline(&self) -> Option<Instant> {
        self.over
            .as_ref()
            .filter(|over| !over.shown && !over.dismissed)
            .map(|over| over.since + DELAY)
    }

    /// Time passed, to `now`: whether a tooltip that waited shows now.
    pub fn tick(&mut self, now: Instant) -> bool {
        match &mut self.over {
            Some(over) if !over.shown && !over.dismissed && now >= over.since + DELAY => {
                over.shown = true;
                true
            }
            _ => false,
        }
    }

    /// A press, a key or a scroll: the tooltip hides, or does not show,
    /// until the pointer leaves its node, and the next waits its delay.
    /// Whether one showed.
    pub fn dismiss(&mut self) -> bool {
        self.hid = None;
        let Some(over) = &mut self.over else {
            return false;
        };
        let shown = over.shown && !over.dismissed;
        over.shown = false;
        over.dismissed = true;
        shown
    }

    /// The pointer left the tree, as it went out of its window, at `now`.
    pub fn left(&mut self, now: Instant) {
        if self
            .over
            .take()
            .is_some_and(|over| over.shown && !over.dismissed)
        {
            self.hid = Some(now);
        }
    }

    /// The tooltip that shows, of `tree`'s node, if it still has it: its
    /// text, and where the pointer was as it showed.
    pub fn shown<'t>(&self, tree: &'t Tree) -> Option<(&'t str, (f32, f32))> {
        let over = self.over.as_ref().filter(|over| over.shown)?;
        let node = tree.by_key(&over.key)?;
        Some((tree.tooltip(node)?, over.at))
    }
}

#[cfg(test)]
mod tests {
    use lokyr_plugin_sdk::view::Node;

    use super::*;

    /// A bar of two buttons of tooltips, `a` and `b`, in a box of none,
    /// `bar`, and a text.
    fn bar() -> (Tree, NodeId, NodeId, NodeId) {
        let mut tree = Tree::new();
        tree.update(&Node::container(vec![
            Node::container(vec![
                Node::icon_button("add").key("a").tooltip("New Tab"),
                Node::icon_button("close").key("b").tooltip("Close Tab"),
            ])
            .key("bar"),
            Node::text("plain"),
        ]))
        .unwrap();
        let bar = tree.children(tree.root().unwrap())[0];
        let (a, b) = (tree.children(bar)[0], tree.children(bar)[1]);
        (tree, bar, a, b)
    }

    #[test]
    fn a_tooltip_shows_after_its_delay_and_hides_as_the_pointer_leaves() {
        let (tree, bar, a, _) = bar();
        let start = Instant::now();
        let mut tooltip = Tooltip::new();
        // Over the bar alone, of no tooltip: nothing waits.
        assert_eq!(
            tooltip.hover(&tree, [bar].into_iter(), (1.0, 1.0), start),
            None
        );
        // Over `a`: its delay, then its text where the pointer last was.
        let deadline = tooltip.hover(&tree, [bar, a].into_iter(), (5.0, 5.0), start);
        assert_eq!(deadline, Some(start + DELAY));
        let later = start + DELAY / 2;
        assert_eq!(
            tooltip.hover(&tree, [bar, a].into_iter(), (7.0, 6.0), later),
            Some(start + DELAY)
        );
        assert!(!tooltip.tick(later));
        assert_eq!(tooltip.shown(&tree), None);
        assert!(tooltip.tick(start + DELAY));
        assert!(!tooltip.tick(start + DELAY * 2));
        assert_eq!(tooltip.shown(&tree), Some(("New Tab", (7.0, 6.0))));
        // Moving over it, it stays where it showed, and nothing waits.
        assert_eq!(
            tooltip.hover(&tree, [bar, a].into_iter(), (9.0, 6.0), start + DELAY * 2),
            None
        );
        assert_eq!(tooltip.shown(&tree), Some(("New Tab", (7.0, 6.0))));
        // Out of it, it hides.
        tooltip.hover(&tree, [bar].into_iter(), (30.0, 6.0), start + DELAY * 3);
        assert_eq!(tooltip.shown(&tree), None);
    }

    #[test]
    fn the_next_shows_at_once_a_moment_after_one_hid() {
        let (tree, bar, a, b) = bar();
        let start = Instant::now();
        let mut tooltip = Tooltip::new();
        tooltip.hover(&tree, [bar, a].into_iter(), (5.0, 5.0), start);
        tooltip.tick(start + DELAY);
        // Straight to `b`, and through a gap to `a` again, in time: each at
        // once.
        let moved = start + DELAY + WARM / 2;
        assert_eq!(
            tooltip.hover(&tree, [bar, b].into_iter(), (25.0, 5.0), moved),
            None
        );
        assert_eq!(tooltip.shown(&tree), Some(("Close Tab", (25.0, 5.0))));
        tooltip.hover(&tree, [bar].into_iter(), (20.0, 5.0), moved);
        let back = moved + WARM / 2;
        tooltip.hover(&tree, [bar, a].into_iter(), (5.0, 5.0), back);
        assert_eq!(tooltip.shown(&tree), Some(("New Tab", (5.0, 5.0))));
        // Out of the tree, and back in time: at once.
        tooltip.left(back);
        let again = back + WARM / 2;
        assert_eq!(
            tooltip.hover(&tree, [bar, b].into_iter(), (25.0, 5.0), again),
            None
        );
        assert_eq!(tooltip.shown(&tree), Some(("Close Tab", (25.0, 5.0))));
        // A press, then the next in time: its delay, as a click ends the
        // run of tooltips.
        tooltip.dismiss();
        assert_eq!(
            tooltip.hover(&tree, [bar, a].into_iter(), (5.0, 5.0), again),
            Some(again + DELAY)
        );
        tooltip.tick(again + DELAY);
        let back = again + DELAY;
        // Out of the tree, and back past the moment: its delay again.
        tooltip.left(back);
        let late = back + WARM;
        assert_eq!(
            tooltip.hover(&tree, [bar, b].into_iter(), (25.0, 5.0), late),
            Some(late + DELAY)
        );
        assert_eq!(tooltip.shown(&tree), None);
    }

    #[test]
    fn a_press_hides_it_until_the_pointer_leaves_its_node() {
        let (mut tree, bar, a, b) = bar();
        let start = Instant::now();
        let mut tooltip = Tooltip::new();
        tooltip.hover(&tree, [bar, a].into_iter(), (5.0, 5.0), start);
        tooltip.tick(start + DELAY);
        assert!(tooltip.dismiss());
        assert!(!tooltip.dismiss());
        assert_eq!(tooltip.shown(&tree), None);
        // Over the node still, it waits for nothing.
        let after = start + DELAY * 3;
        assert_eq!(
            tooltip.hover(&tree, [bar, a].into_iter(), (6.0, 5.0), after),
            None
        );
        assert!(!tooltip.tick(after));
        // To the next, its delay, not at once.
        assert_eq!(
            tooltip.hover(&tree, [bar, b].into_iter(), (25.0, 5.0), after),
            Some(after + DELAY)
        );
        // A press before it shows keeps it from showing.
        tooltip.dismiss();
        assert_eq!(tooltip.deadline(), None);
        assert!(!tooltip.tick(after + DELAY));
        // A node whose tooltip goes, or that goes, shows none.
        tooltip.hover(&tree, [bar, a].into_iter(), (5.0, 5.0), after + DELAY);
        tooltip.tick(after + DELAY * 2);
        assert!(tooltip.shown(&tree).is_some());
        tree.update(&Node::container(vec![Node::text("plain")]))
            .unwrap();
        assert_eq!(tooltip.shown(&tree), None);
    }
}
