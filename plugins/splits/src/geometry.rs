//! Where a tab's panes lie, as its layout shares its room: each pane's
//! rectangle within the tab's, from 0 to 1 across and down, of each split's
//! ratio or its parts' panes; the pane beside another in a direction; and
//! the split whose divider is a pane's edge.

use lokyr_plugin_sdk::host::{Direction, Edge, Layout, PaneId};

/// How near two edges are to meet, of the tab's room.
pub(crate) const EPSILON: f64 = 1e-6;

/// A direction from a pane.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Towards {
    Left,
    Right,
    Up,
    Down,
}

impl Towards {
    /// The direction of `name`, as a command's argument names it.
    pub(crate) fn of(name: &str) -> Option<Self> {
        match name {
            "left" => Some(Self::Left),
            "right" => Some(Self::Right),
            "up" => Some(Self::Up),
            "down" => Some(Self::Down),
            _ => None,
        }
    }

    /// Its name, as a command's argument names it.
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Right => "right",
            Self::Up => "up",
            Self::Down => "down",
        }
    }
}

/// A rectangle of the tab's room, from 0 to 1 across and down.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Rect {
    pub(crate) x: f64,
    pub(crate) y: f64,
    pub(crate) width: f64,
    pub(crate) height: f64,
}

impl Rect {
    const WHOLE: Self = Self {
        x: 0.0,
        y: 0.0,
        width: 1.0,
        height: 1.0,
    };

    /// Its two parts, as a split of `direction` and `share` cuts it.
    fn cut(self, direction: Direction, share: f64) -> (Self, Self) {
        match direction {
            Direction::Horizontal => {
                let first = self.width * share;
                (
                    Self {
                        width: first,
                        ..self
                    },
                    Self {
                        x: self.x + first,
                        width: self.width - first,
                        ..self
                    },
                )
            }
            Direction::Vertical => {
                let first = self.height * share;
                (
                    Self {
                        height: first,
                        ..self
                    },
                    Self {
                        y: self.y + first,
                        height: self.height - first,
                        ..self
                    },
                )
            }
        }
    }
}

/// How many panes `layout` lays side by side along `direction`, its weight
/// in a split of that direction that shares by its panes.
fn weight(layout: &Layout, direction: Direction) -> u32 {
    match layout {
        Layout::Pane(_) => 1,
        Layout::Split {
            direction: own,
            first,
            second,
            ..
        } => {
            let (first, second) = (weight(first, direction), weight(second, direction));
            if *own == direction {
                first + second
            } else {
                first.max(second)
            }
        }
    }
}

/// The share of a split's room that its first part takes: its ratio, or
/// its parts' weights'.
fn share(direction: Direction, ratio: Option<f64>, first: &Layout, second: &Layout) -> f64 {
    ratio.unwrap_or_else(|| {
        let (first, second) = (weight(first, direction), weight(second, direction));
        f64::from(first) / f64::from(first + second)
    })
}

/// Each pane of `layout`, in its order, and its rectangle of the tab's
/// room.
pub(crate) fn rects(layout: &Layout) -> Vec<(PaneId, Rect)> {
    let mut rooms = Vec::new();
    lay(layout, Rect::WHOLE, &mut rooms);
    rooms
}

fn lay(layout: &Layout, room: Rect, rooms: &mut Vec<(PaneId, Rect)>) {
    match layout {
        Layout::Pane(pane) => rooms.push((*pane, room)),
        Layout::Split {
            direction,
            ratio,
            first,
            second,
        } => {
            let (one, other) = room.cut(*direction, share(*direction, *ratio, first, second));
            lay(first, one, rooms);
            lay(second, other, rooms);
        }
    }
}

/// The pane of `rooms` beside `pane` towards `towards`: of the panes whose
/// side faces the pane's side there, the nearest, of the most room shared
/// along that side, the first in the layout's order of a tie; none at the
/// tab's edge.
pub(crate) fn beside(rooms: &[(PaneId, Rect)], pane: PaneId, towards: Towards) -> Option<PaneId> {
    let &(_, from) = rooms.iter().find(|(id, _)| *id == pane)?;
    let along = |room: &Rect| match towards {
        Towards::Left | Towards::Right => (room.y, room.y + room.height),
        Towards::Up | Towards::Down => (room.x, room.x + room.width),
    };
    let (start, end) = along(&from);
    let mut best: Option<(f64, f64, PaneId)> = None;
    for &(id, room) in rooms {
        if id == pane {
            continue;
        }
        let gap = match towards {
            Towards::Left => from.x - (room.x + room.width),
            Towards::Right => room.x - (from.x + from.width),
            Towards::Up => from.y - (room.y + room.height),
            Towards::Down => room.y - (from.y + from.height),
        };
        if gap < -EPSILON {
            continue;
        }
        let (other_start, other_end) = along(&room);
        let shared = end.min(other_end) - start.max(other_start);
        if shared <= EPSILON {
            continue;
        }
        let better = best.is_none_or(|(best_gap, best_shared, _)| {
            gap < best_gap - EPSILON
                || (gap <= best_gap + EPSILON && shared > best_shared + EPSILON)
        });
        if better {
            best = Some((gap, shared, id));
        }
    }
    best.map(|(_, _, id)| id)
}

/// The rectangle of the split of `layout` whose divider is `pane`'s
/// `edge`: the nearest of its ancestors of that edge's direction with the
/// pane in the part that the edge bounds, as the host resizes it; none at
/// the tab's edge.
pub(crate) fn split_of(layout: &Layout, pane: PaneId, edge: Edge) -> Option<Rect> {
    let (direction, first) = match edge {
        Edge::Left => (Direction::Horizontal, false),
        Edge::Right => (Direction::Horizontal, true),
        Edge::Top => (Direction::Vertical, false),
        Edge::Bottom => (Direction::Vertical, true),
    };
    let mut found = None;
    let mut node = layout;
    let mut room = Rect::WHOLE;
    while let Layout::Split {
        direction: own,
        ratio,
        first: one,
        second: other,
    } = node
    {
        let in_first = contains(one, pane);
        if !in_first && !contains(other, pane) {
            return None;
        }
        if *own == direction && in_first == first {
            found = Some(room);
        }
        let (one_room, other_room) = room.cut(*own, share(*own, *ratio, one, other));
        (node, room) = if in_first {
            (one, one_room)
        } else {
            (other, other_room)
        };
    }
    found
}

/// Whether `layout` holds `pane`.
fn contains(layout: &Layout, pane: PaneId) -> bool {
    match layout {
        Layout::Pane(leaf) => *leaf == pane,
        Layout::Split { first, second, .. } => contains(first, pane) || contains(second, pane),
    }
}
