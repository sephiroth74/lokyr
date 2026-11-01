//! A tab's layout: a binary tree of splits, whose leaves are its panes.
//!
//! A split shares its room between its two parts as its ratio says, or,
//! without one, as their panes along its direction weigh: so that panes
//! split alike take a part each, three of them a third, while a ratio that
//! the user gave, by a divider's drag, stays.

use super::PaneId;

/// How a split lays its two parts out.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Direction {
    /// Side by side: the first part on the left, the second on the right.
    Horizontal,
    /// One over the other: the first part above, the second below.
    Vertical,
}

/// An edge of a pane, which a resize moves.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Edge {
    /// Its left edge.
    Left,
    /// Its right edge.
    Right,
    /// Its top edge.
    Top,
    /// Its bottom edge.
    Bottom,
}

impl Edge {
    /// The direction of the splits whose divider is this edge.
    fn direction(self) -> Direction {
        match self {
            Self::Left | Self::Right => Direction::Horizontal,
            Self::Top | Self::Bottom => Direction::Vertical,
        }
    }

    /// The part of a split that a pane is in when the split's divider is
    /// this edge of it.
    fn side(self) -> Side {
        match self {
            Self::Right | Self::Bottom => Side::First,
            Self::Left | Self::Top => Side::Second,
        }
    }
}

/// A part of a split.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Side {
    /// The first: the left one, or the top one.
    First,
    /// The second: the right one, or the bottom one.
    Second,
}

/// The share of a split's room that its first part takes, in
/// ten-thousandths, from [`Ratio::MIN`] to [`Ratio::MAX`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Ratio(u16);

impl Ratio {
    /// The least share, so that no part of a split vanishes.
    pub const MIN: Self = Self(500);
    /// The most share.
    pub const MAX: Self = Self(9_500);
    /// Half and half.
    pub const HALF: Self = Self(5_000);

    /// The ratio of `share`, from 0 to 1, within [`Ratio::MIN`] and
    /// [`Ratio::MAX`].
    pub fn new(share: f64) -> Self {
        let parts = (share * 10_000.0).round();
        let parts = if parts.is_nan() { 5_000.0 } else { parts };
        Self(parts.clamp(f64::from(Self::MIN.0), f64::from(Self::MAX.0)) as u16)
    }

    /// The share, from 0 to 1.
    pub fn get(self) -> f64 {
        f64::from(self.0) / 10_000.0
    }
}

/// How a tab lays its panes out: a tree, whose leaves are panes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Layout {
    /// A pane, which takes the whole of its part.
    Leaf(PaneId),
    /// Two parts that share the room.
    Split {
        /// How the parts lie.
        direction: Direction,
        /// The share of the first part, as the user gave it; `None` shares
        /// the room by the parts' panes along the direction.
        ratio: Option<Ratio>,
        /// The left or top part.
        first: Box<Layout>,
        /// The right or bottom part.
        second: Box<Layout>,
    },
}

impl Layout {
    /// The panes of the layout, in its order: the first part's before the
    /// second's.
    pub fn panes(&self) -> Vec<PaneId> {
        let mut panes = Vec::new();
        self.collect(&mut panes);
        panes
    }

    fn collect(&self, panes: &mut Vec<PaneId>) {
        match self {
            Self::Leaf(pane) => panes.push(*pane),
            Self::Split { first, second, .. } => {
                first.collect(panes);
                second.collect(panes);
            }
        }
    }

    /// Whether `pane` is one of its leaves.
    pub fn contains(&self, pane: PaneId) -> bool {
        match self {
            Self::Leaf(leaf) => *leaf == pane,
            Self::Split { first, second, .. } => first.contains(pane) || second.contains(pane),
        }
    }

    /// The first pane of its order.
    pub fn first_pane(&self) -> PaneId {
        match self {
            Self::Leaf(pane) => *pane,
            Self::Split { first, .. } => first.first_pane(),
        }
    }

    /// The last pane of its order.
    pub fn last_pane(&self) -> PaneId {
        match self {
            Self::Leaf(pane) => *pane,
            Self::Split { second, .. } => second.last_pane(),
        }
    }

    /// How many panes lie across it along `direction`: one for a pane; the
    /// sum of its parts' for a split of that direction, whose parts lie
    /// along it; the most of its parts' for one of the other, whose parts
    /// lie across it.
    pub fn weight(&self, direction: Direction) -> u32 {
        match self {
            Self::Leaf(_) => 1,
            Self::Split {
                direction: own,
                first,
                second,
                ..
            } => {
                let (first, second) = (first.weight(direction), second.weight(direction));
                if *own == direction {
                    first + second
                } else {
                    first.max(second)
                }
            }
        }
    }

    /// The share of its room that a split's first part takes: its ratio,
    /// or its parts' weights'; none for a pane.
    pub fn share(&self) -> Option<f64> {
        let Self::Split {
            direction,
            ratio,
            first,
            second,
        } = self
        else {
            return None;
        };
        Some(ratio.map_or_else(
            || {
                let (first, second) = (first.weight(*direction), second.weight(*direction));
                f64::from(first) / f64::from(first + second)
            },
            Ratio::get,
        ))
    }

    /// The path from the root to the leaf of `pane`, by the parts taken.
    pub fn path_of(&self, pane: PaneId) -> Option<Vec<Side>> {
        let mut path = Vec::new();
        self.find(pane, &mut path).then_some(path)
    }

    fn find(&self, pane: PaneId, path: &mut Vec<Side>) -> bool {
        match self {
            Self::Leaf(leaf) => *leaf == pane,
            Self::Split { first, second, .. } => {
                for (side, part) in [(Side::First, first), (Side::Second, second)] {
                    path.push(side);
                    if part.find(pane, path) {
                        return true;
                    }
                    path.pop();
                }
                false
            }
        }
    }

    /// The node at `path`, from the root.
    pub fn at(&self, path: &[Side]) -> Option<&Self> {
        let Some((side, rest)) = path.split_first() else {
            return Some(self);
        };
        match (self, side) {
            (Self::Split { first, .. }, Side::First) => first.at(rest),
            (Self::Split { second, .. }, Side::Second) => second.at(rest),
            (Self::Leaf(_), _) => None,
        }
    }

    fn at_mut(&mut self, path: &[Side]) -> Option<&mut Self> {
        let Some((side, rest)) = path.split_first() else {
            return Some(self);
        };
        match (self, side) {
            (Self::Split { first, .. }, Side::First) => first.at_mut(rest),
            (Self::Split { second, .. }, Side::Second) => second.at_mut(rest),
            (Self::Leaf(_), _) => None,
        }
    }

    /// Splits the leaf of `pane` along `direction`, with `new` in the part
    /// `side`, its ratio by the parts' weights; whether `pane` is a leaf of
    /// it, and `new` is not.
    pub fn split(&mut self, pane: PaneId, direction: Direction, new: PaneId, side: Side) -> bool {
        if self.contains(new) {
            return false;
        }
        let Some(path) = self.path_of(pane) else {
            return false;
        };
        let Some(leaf) = self.at_mut(&path) else {
            return false;
        };
        let (first, second) = match side {
            Side::First => (new, pane),
            Side::Second => (pane, new),
        };
        *leaf = Self::Split {
            direction,
            ratio: None,
            first: Box::new(Self::Leaf(first)),
            second: Box::new(Self::Leaf(second)),
        };
        true
    }

    /// Takes the leaf of `pane` away, its split's room to its sibling, which
    /// takes the split's place: the pane whose part now takes its room, as
    /// the sibling's nearest pane; `None` if `pane` is no leaf of it, or is
    /// its one pane, which a layout cannot lose.
    pub fn remove(&mut self, pane: PaneId) -> Option<PaneId> {
        let path = self.path_of(pane)?;
        let (side, parent) = path.split_last()?;
        let split = self.at_mut(parent)?;
        let Self::Split { first, second, .. } = split else {
            return None;
        };
        let sibling = match side {
            Side::First => std::mem::replace(second.as_mut(), Self::Leaf(pane)),
            Side::Second => std::mem::replace(first.as_mut(), Self::Leaf(pane)),
        };
        // The pane of the sibling next to the one that went.
        let nearest = match side {
            Side::First => sibling.first_pane(),
            Side::Second => sibling.last_pane(),
        };
        *split = sibling;
        Some(nearest)
    }

    /// Sets the ratio of the split at `path`; whether a split is there.
    pub fn set_ratio(&mut self, path: &[Side], to: Option<Ratio>) -> bool {
        match self.at_mut(path) {
            Some(Self::Split { ratio, .. }) => {
                *ratio = to;
                true
            }
            _ => false,
        }
    }

    /// Moves the `edge` of `pane` by `by`, a share of the room of the split
    /// whose divider that edge is, the nearest of `pane`'s ancestors: right
    /// and down for a positive one. The split takes that ratio, within its
    /// bounds; whether `pane` has such an edge.
    pub fn resize(&mut self, pane: PaneId, edge: Edge, by: f64) -> bool {
        let Some(path) = self.path_of(pane) else {
            return false;
        };
        let (direction, side) = (edge.direction(), edge.side());
        // The nearest ancestor of that direction, with the pane in the part
        // that the edge bounds.
        for depth in (0..path.len()).rev() {
            let Some(split) = self.at(&path[..depth]) else {
                continue;
            };
            let Self::Split { direction: own, .. } = split else {
                continue;
            };
            if *own != direction || path[depth] != side {
                continue;
            }
            let share = split.share().unwrap_or(Ratio::HALF.get());
            return self.set_ratio(&path[..depth], Some(Ratio::new(share + by)));
        }
        false
    }

    /// Takes every ratio away, so that the panes share the room by their
    /// weights.
    pub fn equalize(&mut self) {
        if let Self::Split {
            ratio,
            first,
            second,
            ..
        } = self
        {
            *ratio = None;
            first.equalize();
            second.equalize();
        }
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    fn pane(id: u64) -> PaneId {
        PaneId::of(id)
    }

    fn leaf(id: u64) -> Layout {
        Layout::Leaf(pane(id))
    }

    #[test]
    fn panes_split_alike_take_a_part_each() {
        // A | B, then B | C: a third each.
        let mut layout = leaf(1);
        assert!(layout.split(pane(1), Direction::Horizontal, pane(2), Side::Second));
        assert!(layout.split(pane(2), Direction::Horizontal, pane(3), Side::Second));
        assert_eq!(layout.panes(), [pane(1), pane(2), pane(3)]);
        let root = layout.share().unwrap();
        let inner = layout.at(&[Side::Second]).unwrap().share().unwrap();
        assert!((root - 1.0 / 3.0).abs() < 1e-9, "{root}");
        assert!(((1.0 - root) * inner - 1.0 / 3.0).abs() < 1e-9, "{inner}");
        // A | (B over C): B and C lie across the root's direction, half each.
        let mut layout = leaf(1);
        layout.split(pane(1), Direction::Horizontal, pane(2), Side::Second);
        layout.split(pane(2), Direction::Vertical, pane(3), Side::Second);
        assert_eq!(layout.share(), Some(0.5));
        // A pane alone shares nothing.
        assert_eq!(leaf(1).share(), None);
    }

    #[test]
    fn a_pane_s_edge_moves_the_divider_of_its_nearest_split() {
        // (A | B) over C: B's left edge is the inner divider, its bottom the
        // root's.
        let mut layout = leaf(1);
        layout.split(pane(1), Direction::Vertical, pane(3), Side::Second);
        layout.split(pane(1), Direction::Horizontal, pane(2), Side::Second);
        assert!(layout.resize(pane(2), Edge::Left, -0.1));
        let inner = layout.at(&[Side::First]).unwrap().share().unwrap();
        assert!((inner - 0.4).abs() < 1e-9, "{inner}");
        assert!(layout.resize(pane(2), Edge::Bottom, 0.2));
        assert!((layout.share().unwrap() - 0.7).abs() < 1e-9);
        // B has no right edge to move, nor C a bottom one.
        assert!(!layout.resize(pane(2), Edge::Right, 0.1));
        assert!(!layout.resize(pane(3), Edge::Bottom, 0.1));
        // A divider goes no further than its bounds, and back by weights.
        assert!(layout.resize(pane(1), Edge::Right, 0.9));
        assert_eq!(
            layout.at(&[Side::First]).unwrap().share(),
            Some(Ratio::MAX.get())
        );
        layout.equalize();
        assert_eq!(layout.share(), Some(0.5));
        assert_eq!(layout.at(&[Side::First]).unwrap().share(), Some(0.5));
    }

    #[test]
    fn a_layout_keeps_its_one_pane() {
        let mut layout = leaf(1);
        assert_eq!(layout.remove(pane(1)), None);
        assert_eq!(layout, leaf(1));
        assert!(!layout.split(pane(1), Direction::Horizontal, pane(1), Side::First));
        assert!(!layout.split(pane(9), Direction::Horizontal, pane(2), Side::First));
    }

    #[derive(Clone, Debug)]
    enum Operation {
        Split {
            target: usize,
            vertical: bool,
            first: bool,
        },
        Remove {
            target: usize,
        },
        Ratio {
            target: usize,
            share: f64,
        },
        Resize {
            target: usize,
            edge: u8,
            by: f64,
        },
        Equalize,
    }

    fn operation() -> impl Strategy<Value = Operation> {
        prop_oneof![
            4 => (any::<usize>(), any::<bool>(), any::<bool>())
                .prop_map(|(target, vertical, first)| Operation::Split { target, vertical, first }),
            2 => any::<usize>().prop_map(|target| Operation::Remove { target }),
            1 => (any::<usize>(), -1.0..2.0f64).prop_map(|(target, share)| Operation::Ratio { target, share }),
            2 => (any::<usize>(), 0..4u8, -1.5..1.5f64)
                .prop_map(|(target, edge, by)| Operation::Resize { target, edge, by }),
            1 => Just(Operation::Equalize),
        ]
    }

    /// Every split's ratio, within its bounds, and its share, within 0 and 1.
    fn bounded(layout: &Layout) -> bool {
        match layout {
            Layout::Leaf(_) => true,
            Layout::Split {
                ratio,
                first,
                second,
                ..
            } => {
                let share = layout.share().unwrap();
                ratio.is_none_or(|ratio| (Ratio::MIN..=Ratio::MAX).contains(&ratio))
                    && share > 0.0
                    && share < 1.0
                    && bounded(first)
                    && bounded(second)
            }
        }
    }

    proptest! {
        #[test]
        fn the_tree_stays_whole_through_every_operation(
            operations in proptest::collection::vec(operation(), 1..60),
        ) {
            let mut layout = leaf(1);
            let mut expected = vec![pane(1)];
            let mut next = 2;
            for operation in operations {
                let before = layout.clone();
                let target = |index: usize| expected[index % expected.len()];
                match operation {
                    Operation::Split { target: index, vertical, first } => {
                        let at = target(index);
                        let direction = if vertical { Direction::Vertical } else { Direction::Horizontal };
                        let side = if first { Side::First } else { Side::Second };
                        prop_assert!(layout.split(at, direction, pane(next), side));
                        // The leaf, a split of the two now, of no ratio.
                        let path = before.path_of(at).unwrap();
                        let Some(Layout::Split { direction: own, ratio, .. }) = layout.at(&path) else {
                            return Err(TestCaseError::fail("no split"));
                        };
                        prop_assert_eq!((*own, *ratio), (direction, None));
                        expected.push(pane(next));
                        next += 1;
                    }
                    Operation::Remove { target: index } => {
                        let gone = target(index);
                        let removed = layout.remove(gone);
                        if expected.len() == 1 {
                            prop_assert_eq!(removed, None);
                            prop_assert_eq!(&layout, &before);
                            continue;
                        }
                        // Its split's place to its sibling, whole.
                        let path = before.path_of(gone).unwrap();
                        let (side, parent) = path.split_last().unwrap();
                        let mut sibling_path = parent.to_vec();
                        sibling_path.push(match side { Side::First => Side::Second, Side::Second => Side::First });
                        let sibling = before.at(&sibling_path).unwrap();
                        prop_assert_eq!(layout.at(parent).unwrap(), sibling);
                        prop_assert!(removed.is_some_and(|nearest| sibling.contains(nearest)));
                        expected.retain(|&pane| pane != gone);
                    }
                    Operation::Ratio { target: index, share } => {
                        let at = target(index);
                        let path = layout.path_of(at).unwrap();
                        if let Some((_, parent)) = path.split_last() {
                            prop_assert!(layout.set_ratio(parent, Some(Ratio::new(share))));
                        } else {
                            prop_assert!(!layout.set_ratio(&path, Some(Ratio::new(share))));
                        }
                    }
                    Operation::Resize { target: index, edge, by } => {
                        let edge = [Edge::Left, Edge::Right, Edge::Top, Edge::Bottom][usize::from(edge)];
                        layout.resize(target(index), edge, by);
                        prop_assert_eq!(layout.panes().len(), before.panes().len());
                    }
                    Operation::Equalize => layout.equalize(),
                }
                // Each pane in one leaf, and none else.
                let mut panes = layout.panes();
                let count = panes.len();
                panes.sort();
                panes.dedup();
                prop_assert_eq!(panes.len(), count);
                let mut wanted = expected.clone();
                wanted.sort();
                prop_assert_eq!(panes, wanted);
                prop_assert!(bounded(&layout));
            }
        }
    }
}
