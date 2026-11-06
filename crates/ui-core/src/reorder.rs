//! The drags of reorderable nodes among their siblings, as a tab bar's tabs:
//! the node dragged follows the pointer along its parent's direction,
//! within the room of its reorderable siblings, over them; the others slide
//! out of its way as it passes their middles; a scroll area around them
//! scrolls as it goes past the area's edges. A drop slides it to its new
//! place, the others staying where they slid, until its plugin's render of
//! the new order, whose layout puts them there; a render that keeps the
//! order, or none within a moment, slides them back, as Escape does at
//! once.
//!
//! A node that carries a tab leaves its siblings as it is dragged out of
//! its bar, which close its room, and comes back as the pointer does; the
//! nodes that carry tabs in another surface, as the bar of another window,
//! open a room for it under the pointer, which a drop keeps until their
//! plugin's render of the tab among them.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use lokyr_plugin_sdk::view::{Direction, Easing, Key, Kind};

use crate::animation::ease;
use crate::layout::{self, Layout};
use crate::tree::{NodeId, Tree};

/// How long a node slides out of the way of the one dragged, or to its
/// place.
const SLIDE: Duration = Duration::from_millis(200);

/// How long a drop waits for its plugin's render of the new order before
/// its nodes slide back.
const WAIT: Duration = Duration::from_millis(500);

/// How fast a scroll area scrolls under the node dragged past its edge, in
/// points a second for each point that the node goes past it.
const SCROLL_RATE: f32 = 10.0;

/// The fastest that it scrolls, in points a second.
const FASTEST: f32 = 1500.0;

/// The reorderable nodes' drag, and their slides.
#[derive(Debug, Default)]
pub struct Reorder {
    lift: Option<Lift>,
    /// The nodes' slides, by their keys, along the axis of the last drag:
    /// a sibling's out of the way of the node dragged, the node's to its
    /// place, or each one's back to its place.
    slides: HashMap<Key, Slide>,
    /// Whether the last drag went across a row, rather than down a column.
    across: bool,
    /// Whether the system reduces motion, which ends the slides at once.
    reduced: bool,
    /// The node of the last drag, which shows over its siblings until its
    /// slide to its place ends.
    last: Option<Key>,
    /// The room that the nodes that carry tabs open for one dragged in
    /// from another surface.
    room: Option<Room>,
}

/// A room that a parent's nodes that carry tabs open for a node dragged in
/// from another surface, sliding aside from `index` on; or that a drop
/// keeps until their plugin's render.
#[derive(Debug)]
struct Room {
    parent: NodeId,
    /// Their places, of the last layout.
    slots: Vec<Slot>,
    /// Where the node would drop.
    index: usize,
    /// How far they slide, a node's size and the gap after it, in points.
    size: f32,
    /// Its drop, when and of which revision of the tree, waiting for the
    /// plugin's render.
    dropped: Option<(Instant, u64)>,
    /// The tree's revision whose layout `slots` are of.
    revision: u64,
}

/// A drag under way, or dropped and waiting for its plugin's render.
#[derive(Debug)]
struct Lift {
    /// The node dragged.
    key: Key,
    /// Its parent, a scroll area or not.
    parent: NodeId,
    /// Its reorderable siblings, itself among them, in their order, at
    /// their places from their parent's corner, of no scroll.
    slots: Vec<Slot>,
    /// Its index among them.
    from: usize,
    /// Where it would drop now.
    to: usize,
    /// Its start at the drag's start, in points from its parent's corner.
    grab: f32,
    /// How far the pointer moved along the axis since the drag's start, in
    /// points.
    moved: f32,
    /// How far the parent was scrolled at the drag's start, in points.
    scrolled: f32,
    /// How far it shows from its place, in points, under the pointer.
    shown: f32,
    /// When the parent last scrolled under it, as it goes past an edge.
    tick: Option<Instant>,
    /// Its drop, when and of which revision of the tree, waiting for its
    /// plugin's render.
    dropped: Option<(Instant, u64)>,
    /// The tree's revision whose layout `slots` are of.
    revision: u64,
    /// Whether it left its parent's room, as a node that carries a tab
    /// dragged out of its bar: it shows nowhere, and its siblings after it
    /// close its room.
    out: bool,
}

/// A reorderable node's place along the axis, in points.
#[derive(Clone, Debug, PartialEq)]
struct Slot {
    key: Key,
    start: f32,
    size: f32,
}

impl Slot {
    fn end(&self) -> f32 {
        self.start + self.size
    }

    fn middle(&self) -> f32 {
        self.start + self.size / 2.0
    }
}

/// A slide from `from` to `to`, in points, from `start`.
#[derive(Clone, Copy, Debug)]
struct Slide {
    from: f32,
    to: f32,
    start: Instant,
}

impl Slide {
    fn at(&self, now: Instant) -> f32 {
        let t = (now.saturating_duration_since(self.start).as_secs_f32() / SLIDE.as_secs_f32())
            .clamp(0.0, 1.0);
        self.from + (self.to - self.from) * ease(Easing::EaseOut, t)
    }

    fn ended(&self, now: Instant) -> bool {
        now.saturating_duration_since(self.start) >= SLIDE
    }
}

impl Reorder {
    /// No drag.
    pub fn new() -> Self {
        Self::default()
    }

    /// The drag of `node` started, a reorderable one of `tree`, of the last
    /// layout: whether it lifts, of a parent and a key.
    pub fn lift(&mut self, tree: &Tree, layout: &Layout, node: NodeId, now: Instant) -> bool {
        if self
            .lift
            .as_ref()
            .is_some_and(|lift| lift.dropped.is_some())
        {
            self.fold(tree, layout, now);
        }
        let (Some(key), Some(parent)) = (tree.key(node), tree.parent(node)) else {
            return false;
        };
        if !tree.reorderable(node) {
            return false;
        }
        let across = tree
            .layout(parent)
            .is_none_or(|layout| layout.direction == Direction::Row);
        let slots = slots(tree, layout, parent, across);
        let Some(from) = slots.iter().position(|slot| &slot.key == key) else {
            return false;
        };
        self.across = across;
        self.slides.clear();
        self.last = Some(key.clone());
        self.lift = Some(Lift {
            key: key.clone(),
            parent,
            grab: slots[from].start,
            slots,
            from,
            to: from,
            moved: 0.0,
            scrolled: layout.scrolled(parent) / layout.scale(),
            shown: 0.0,
            tick: None,
            dropped: None,
            revision: tree.revision(),
            out: false,
        });
        true
    }

    /// The pointer moved `offset` points from the drag's start, at `now`:
    /// the node follows it, and its siblings slide out of its way.
    pub fn drag(&mut self, layout: &Layout, (x, y): (f32, f32), now: Instant) {
        let across = self.across;
        let Some(lift) = self.lift.as_mut().filter(|lift| lift.dropped.is_none()) else {
            return;
        };
        lift.moved = if across { x } else { y };
        if !lift.out {
            self.follow(layout, now);
        }
    }

    /// The node dragged left its parent's room, `out`, as a node that
    /// carries a tab dragged out of its bar, or came back into it, at
    /// `now`: out, it shows nowhere, and its siblings after it slide into
    /// its room; back, it follows the pointer among them again. Whether it
    /// changed.
    pub fn carry_out(&mut self, out: bool, layout: &Layout, now: Instant) -> bool {
        let Some(lift) = self
            .lift
            .as_mut()
            .filter(|lift| lift.dropped.is_none() && lift.out != out)
        else {
            return false;
        };
        lift.out = out;
        if out {
            lift.shown = 0.0;
            lift.tick = None;
            self.close_lift(now);
        } else {
            self.follow(layout, now);
        }
        true
    }

    /// Whether the node dragged left its parent's room, its drag under way,
    /// or dropped there and waiting for its plugin's render.
    pub fn out(&self) -> bool {
        self.lift.as_ref().is_some_and(|lift| lift.out)
    }

    /// Whether the node of `key` shows nowhere, dragged out of its
    /// parent's room.
    pub fn hidden(&self, key: &Key) -> bool {
        self.lift
            .as_ref()
            .is_some_and(|lift| lift.out && &lift.key == key)
    }

    /// The siblings after the node dragged out slide into its room, at
    /// `now`.
    fn close_lift(&mut self, now: Instant) {
        let Some(lift) = &self.lift else {
            return;
        };
        let from = lift.from;
        let step = step(&lift.slots, from);
        let wanted: Vec<(Key, f32)> = lift
            .slots
            .iter()
            .enumerate()
            .filter(|&(at, _)| at != from)
            .map(|(at, slot)| (slot.key.clone(), if at > from { -step } else { 0.0 }))
            .collect();
        self.slide_to(wanted, now);
    }

    /// The nodes that carry tabs open a room under the pointer, at `at`, in
    /// pixels from the tree's corner, for one dragged in from another
    /// surface, at `now`, sliding aside from its place on: its index among
    /// them; `None` where the tree has none, or while a drop waits.
    pub fn open_room(
        &mut self,
        tree: &Tree,
        layout: &Layout,
        at: (f32, f32),
        now: Instant,
    ) -> Option<usize> {
        if self
            .room
            .as_ref()
            .is_some_and(|room| room.dropped.is_some())
        {
            return None;
        }
        let parent = carrying(tree)?;
        let across = tree
            .layout(parent)
            .is_none_or(|layout| layout.direction == Direction::Row);
        let slots = slots(tree, layout, parent, across);
        let first = slots.first()?;
        let size = first.size + slots.get(1).map_or(0.0, |next| next.start - first.end());
        let along = if across { at.0 } else { at.1 };
        let index = slots
            .iter()
            .filter(|slot| {
                tree.by_key(&slot.key)
                    .and_then(|id| layout.frame(tree, id))
                    .is_some_and(|frame| {
                        let (start, length) = if across {
                            (frame.x, frame.width)
                        } else {
                            (frame.y, frame.height)
                        };
                        start + length / 2.0 < along
                    })
            })
            .count();
        if self
            .room
            .as_ref()
            .is_some_and(|room| room.parent == parent && room.index == index)
        {
            return Some(index);
        }
        self.across = across;
        let wanted: Vec<(Key, f32)> = slots
            .iter()
            .enumerate()
            .map(|(at, slot)| (slot.key.clone(), if at >= index { size } else { 0.0 }))
            .collect();
        self.room = Some(Room {
            parent,
            slots,
            index,
            size,
            dropped: None,
            revision: tree.revision(),
        });
        self.slide_to(wanted, now);
        Some(index)
    }

    /// The room for a node dragged in closes, at `now`, as the pointer
    /// leaves, its nodes sliding back; whether one was open.
    pub fn close_room(&mut self, now: Instant) -> bool {
        let Some(room) = self.room.take_if(|room| room.dropped.is_none()) else {
            return false;
        };
        let wanted: Vec<(Key, f32)> = room
            .slots
            .iter()
            .map(|slot| (slot.key.clone(), 0.0))
            .collect();
        self.slide_to(wanted, now);
        true
    }

    /// The node dragged in dropped in the room, at `now`, of `tree`'s
    /// revision: the room stays until the plugin's render of the node
    /// among the others, or none within a moment; its index among them.
    pub fn drop_room(&mut self, revision: u64, now: Instant) -> Option<usize> {
        let room = self.room.as_mut().filter(|room| room.dropped.is_none())?;
        room.dropped = Some((now, revision));
        Some(room.index)
    }

    /// The index of the room for a node dragged in, while it is open.
    pub fn room(&self) -> Option<usize> {
        self.room
            .as_ref()
            .filter(|room| room.dropped.is_none())
            .map(|room| room.index)
    }

    /// Slides each node of `wanted`'s keys to its place there, at `now`,
    /// from where it shows, at once where the system reduces motion.
    fn slide_to(&mut self, wanted: Vec<(Key, f32)>, now: Instant) {
        for (key, wanted) in wanted {
            let shown = self.slides.get(&key).map_or(0.0, |slide| slide.at(now));
            match self.slides.get(&key) {
                Some(slide) if slide.to == wanted => {}
                None if wanted == 0.0 => {}
                _ => {
                    self.slides.insert(
                        key,
                        Slide {
                            from: if self.reduced { wanted } else { shown },
                            to: wanted,
                            start: now,
                        },
                    );
                }
            }
        }
    }

    /// The node dropped where it is, at `now`, of `tree`'s revision: it
    /// slides to its new place, which its plugin's next render shows; its
    /// index among its reorderable siblings, if it changed.
    pub fn drop(&mut self, revision: u64, now: Instant) -> Option<usize> {
        let reduced = self.reduced;
        let lift = self.lift.as_mut().filter(|lift| lift.dropped.is_none())?;
        // Out of its parent's room, it waits for its plugin's render of its
        // move, its siblings closing its room meanwhile.
        if lift.out {
            lift.dropped = Some((now, revision));
            return None;
        }
        let (from, to) = (lift.from, lift.to);
        // At its own place, it slides back, of no render to wait for.
        if to == from {
            self.cancel(now);
            return None;
        }
        lift.dropped = Some((now, revision));
        let size = lift.slots[from].size;
        let place = match to {
            _ if to > from => lift.slots[to].end() - size,
            _ => lift.slots[to].start,
        } - lift.slots[from].start;
        let shown = lift.shown;
        self.slides.insert(
            lift.key.clone(),
            Slide {
                from: if reduced { place } else { shown },
                to: place,
                start: now,
            },
        );
        Some(to)
    }

    /// Escape, or the app taking the pointer back, at `now`: a drag under
    /// way ends, its nodes sliding back to their places; whether one did.
    pub fn cancel(&mut self, now: Instant) -> bool {
        let Some(lift) = self.lift.take_if(|lift| lift.dropped.is_none()) else {
            return false;
        };
        for slot in &lift.slots {
            let from = match self.slides.get(&slot.key) {
                _ if slot.key == lift.key => lift.shown,
                Some(slide) => slide.at(now),
                None => 0.0,
            };
            if from == 0.0 || self.reduced {
                self.slides.remove(&slot.key);
            } else {
                self.slides.insert(
                    slot.key.clone(),
                    Slide {
                        from,
                        to: 0.0,
                        start: now,
                    },
                );
            }
        }
        true
    }

    /// The key of the node dragged, its drag under way, or dropped and
    /// waiting for its plugin's render.
    pub fn lifted(&self) -> Option<&Key> {
        self.lift.as_ref().map(|lift| &lift.key)
    }

    /// Whether a drag is under way, not yet dropped.
    pub fn dragging(&self) -> bool {
        self.lift
            .as_ref()
            .is_some_and(|lift| lift.dropped.is_none())
    }

    /// The key of the node that shows over its siblings: the one dragged,
    /// or dropped and on its way to its place.
    pub fn raised(&self) -> Option<&Key> {
        self.lift.as_ref().map(|lift| &lift.key).or_else(|| {
            self.last
                .as_ref()
                .filter(|key| self.slides.contains_key(*key))
        })
    }

    /// How far the node of `key` shows from its place at `now`, in points,
    /// across and down.
    pub fn offset(&self, key: &Key, now: Instant) -> (f32, f32) {
        let along = match &self.lift {
            Some(lift) if &lift.key == key && lift.dropped.is_none() => lift.shown,
            _ => self.slides.get(key).map_or(0.0, |slide| slide.at(now)),
        };
        if self.across {
            (along, 0.0)
        } else {
            (0.0, along)
        }
    }

    /// Whether it moves at `now`, for which the host draws a frame at each
    /// refresh of the display: a slide, a scroll past an edge, or a drop
    /// that waits for its render.
    pub fn running(&self, now: Instant) -> bool {
        self.slides.values().any(|slide| !slide.ended(now))
            || self
                .lift
                .as_ref()
                .is_some_and(|lift| lift.tick.is_some() || lift.dropped.is_some())
            || self
                .room
                .as_ref()
                .is_some_and(|room| room.dropped.is_some())
    }

    /// At each frame, at `now`, of the last layout: the scroll area around
    /// the node dragged scrolls as the node goes past its edges, by how
    /// far, the node staying under the pointer.
    pub fn scroll(&mut self, tree: &Tree, layout: &mut Layout, now: Instant) {
        let across = self.across;
        let Some(lift) = self.lift.as_mut().filter(|lift| lift.dropped.is_none()) else {
            return;
        };
        if lift.out {
            lift.tick = None;
            return;
        }
        let parent = lift.parent;
        let scale = layout.scale();
        let (Some(area), true) = (
            layout.local(parent),
            matches!(tree.kind(parent), Some(Kind::Scroll))
                && layout::across(tree, parent) == across,
        ) else {
            lift.tick = None;
            return;
        };
        let view = if across { area.width } else { area.height } / scale;
        let scrolled = layout.scrolled(parent) / scale;
        let slot = &lift.slots[lift.from];
        let start = slot.start + lift.shown - scrolled;
        let past = if start < 0.0 {
            start
        } else {
            (start + slot.size - view).max(0.0)
        };
        if past == 0.0 {
            lift.tick = None;
            return;
        }
        let since = lift.tick.map_or(0.0, |tick| {
            now.saturating_duration_since(tick).as_secs_f32()
        });
        lift.tick = Some(now);
        let speed = (past.abs() * SCROLL_RATE).min(FASTEST) * past.signum();
        if !layout.scroll_by(tree, parent, speed * since * scale) && since > 0.0 {
            lift.tick = None;
        }
        self.follow(layout, now);
    }

    /// `tree` was laid out in `layout` at `now`, `reduced` as the system
    /// reduces motion: a drag under way takes the places of a new render; a
    /// drop takes its plugin's render of a new order, or none within a
    /// moment, its nodes sliding from where they showed to their places.
    /// The keys of the nodes that it slides so, whose own slides end.
    pub fn laid_out(
        &mut self,
        tree: &Tree,
        layout: &Layout,
        now: Instant,
        reduced: bool,
    ) -> Vec<Key> {
        self.reduced = reduced;
        self.slides
            .retain(|_, slide| !slide.ended(now) || slide.to != 0.0);
        let mut folded = self.room_laid_out(tree, layout, now);
        folded.extend(self.lift_laid_out(tree, layout, now));
        folded
    }

    /// The room for a node dragged in takes the places of a new render; a
    /// drop takes the plugin's render of the node among the others, or
    /// none within a moment, the others sliding from where they showed to
    /// their places: their keys.
    fn room_laid_out(&mut self, tree: &Tree, layout: &Layout, now: Instant) -> Vec<Key> {
        let Some(room) = &self.room else {
            return Vec::new();
        };
        if tree.revision() == room.revision && room.dropped.is_none() {
            return Vec::new();
        }
        let across = self.across;
        let rendered = if tree.kind(room.parent).is_some() {
            slots(tree, layout, room.parent, across)
        } else {
            Vec::new()
        };
        match room.dropped {
            None => {
                // A new render of the bar under the room: its places anew.
                let (index, size) = (room.index, room.size);
                let wanted: Vec<(Key, f32)> = rendered
                    .iter()
                    .enumerate()
                    .map(|(at, slot)| (slot.key.clone(), if at >= index { size } else { 0.0 }))
                    .collect();
                if let Some(room) = self.room.as_mut() {
                    room.slots = rendered;
                    room.revision = tree.revision();
                }
                self.slide_to(wanted, now);
                Vec::new()
            }
            Some((since, revision)) => {
                let joined = revision != tree.revision()
                    && rendered
                        .iter()
                        .map(|slot| &slot.key)
                        .ne(room.slots.iter().map(|slot| &slot.key));
                if !joined && now.saturating_duration_since(since) < WAIT {
                    return Vec::new();
                }
                let Some(room) = self.room.take() else {
                    return Vec::new();
                };
                self.fold_slots(&room.slots, &rendered, now)
            }
        }
    }

    /// The drag under way takes the places of a new render; a drop takes
    /// its plugin's render of a new order, or none within a moment: the
    /// keys of the nodes that slide from where they showed.
    fn lift_laid_out(&mut self, tree: &Tree, layout: &Layout, now: Instant) -> Vec<Key> {
        let Some(lift) = &self.lift else {
            return Vec::new();
        };
        // Its node gone, as a tab that its program closed, the others slide
        // from where they show to their places.
        if tree.by_key(&lift.key).is_none() {
            return self.fold(tree, layout, now);
        }
        let across = self.across;
        match lift.dropped {
            None if lift.revision != tree.revision() => {
                let slots = slots(tree, layout, lift.parent, across);
                let Some(lift) = self.lift.as_mut() else {
                    return Vec::new();
                };
                lift.revision = tree.revision();
                match slots.iter().position(|slot| slot.key == lift.key) {
                    Some(from) => {
                        lift.slots = slots;
                        lift.from = from;
                        if lift.out {
                            self.close_lift(now);
                        } else {
                            self.follow(layout, now);
                        }
                    }
                    None => self.lift = None,
                }
                Vec::new()
            }
            Some((since, revision)) => {
                let order: Vec<&Key> = lift.slots.iter().map(|slot| &slot.key).collect();
                let rendered = slots(tree, layout, lift.parent, across);
                let reordered =
                    revision != tree.revision() && rendered.iter().map(|slot| &slot.key).ne(order);
                if reordered || now.saturating_duration_since(since) >= WAIT {
                    self.fold(tree, layout, now)
                } else {
                    Vec::new()
                }
            }
            None => Vec::new(),
        }
    }

    /// The drop's nodes slide from where they show to their places of the
    /// last layout, at `now`: their keys.
    fn fold(&mut self, tree: &Tree, layout: &Layout, now: Instant) -> Vec<Key> {
        let Some(lift) = self.lift.take() else {
            return Vec::new();
        };
        let rendered = slots(tree, layout, lift.parent, self.across);
        self.fold_slots(&lift.slots, &rendered, now)
    }

    /// The nodes of `slots`, of an earlier layout, slide from where they
    /// show at `now` to their places of `rendered`, of the last: their
    /// keys; those of no place there go.
    fn fold_slots(&mut self, slots: &[Slot], rendered: &[Slot], now: Instant) -> Vec<Key> {
        let mut folded = Vec::new();
        for slot in slots {
            let shown = slot.start
                + self
                    .slides
                    .get(&slot.key)
                    .map_or(0.0, |slide| slide.at(now));
            let Some(now_at) = rendered.iter().find(|at| at.key == slot.key) else {
                self.slides.remove(&slot.key);
                continue;
            };
            let left = shown - now_at.start;
            if left.abs() < 0.5 || self.reduced {
                self.slides.remove(&slot.key);
            } else {
                self.slides.insert(
                    slot.key.clone(),
                    Slide {
                        from: left,
                        to: 0.0,
                        start: now,
                    },
                );
            }
            folded.push(slot.key.clone());
        }
        folded
    }

    /// The node dragged under the pointer, within its siblings' room, at
    /// `now`: where it would drop, and its siblings' slides out of its way.
    fn follow(&mut self, layout: &Layout, now: Instant) {
        let Some(lift) = self.lift.as_mut() else {
            return;
        };
        let scrolled = layout.scrolled(lift.parent) / layout.scale();
        let (first, last) = match (lift.slots.first(), lift.slots.last()) {
            (Some(first), Some(last)) => (first.start, last.end()),
            _ => return,
        };
        let from = lift.from;
        let size = lift.slots[from].size;
        let start = (lift.grab + lift.moved + scrolled - lift.scrolled).clamp(first, last - size);
        lift.shown = start - lift.slots[from].start;
        // Past a sibling's middle, or at its edge, as at the end of the
        // room, which a node larger than the sibling reaches before the
        // middle.
        let (middle, end) = (start + size / 2.0, start + size);
        let after = (from + 1..lift.slots.len())
            .take_while(|&at| {
                let slot = &lift.slots[at];
                middle > slot.middle() || end >= slot.end()
            })
            .last();
        let before = (0..from)
            .rev()
            .take_while(|&at| {
                let slot = &lift.slots[at];
                middle < slot.middle() || start <= slot.start
            })
            .last();
        lift.to = after.or(before).unwrap_or(from);
        // The room that it leaves, of the gap after it, or before it at
        // the end.
        let gap = match (lift.slots.get(from + 1), from.checked_sub(1)) {
            (Some(next), _) => next.start - lift.slots[from].end(),
            (None, Some(before)) => lift.slots[from].start - lift.slots[before].end(),
            (None, None) => 0.0,
        };
        let step = size + gap;
        let to = lift.to;
        let wanted: Vec<(Key, f32)> = lift
            .slots
            .iter()
            .enumerate()
            .filter(|&(at, _)| at != from)
            .map(|(at, slot)| {
                let wanted = match at {
                    _ if from < at && at <= to => -step,
                    _ if to <= at && at < from => step,
                    _ => 0.0,
                };
                (slot.key.clone(), wanted)
            })
            .collect();
        self.slide_to(wanted, now);
    }
}

/// How far the siblings after the node at `from` of `slots` slide to take
/// its room: its size and the gap after it, or before it at the end.
fn step(slots: &[Slot], from: usize) -> f32 {
    let size = slots.get(from).map_or(0.0, |slot| slot.size);
    let gap = match (slots.get(from + 1), from.checked_sub(1)) {
        (Some(next), _) => next.start - slots[from].end(),
        (None, Some(before)) => slots[from].start - slots[before].end(),
        (None, None) => 0.0,
    };
    size + gap
}

/// The first node of `tree`, from its root in its order, whose children
/// carry tabs, as a tab bar's row of tabs.
fn carrying(tree: &Tree) -> Option<NodeId> {
    let mut left: Vec<NodeId> = tree.root().into_iter().collect();
    while let Some(id) = left.pop() {
        let children = tree.children(id);
        if children.iter().any(|&child| tree.carries(child).is_some()) {
            return Some(id);
        }
        left.extend(children.iter().rev().copied());
    }
    None
}

/// The reorderable children of `parent` of a key, in their order, at their
/// places along the axis, `across` or down, of `layout`, in points.
fn slots(tree: &Tree, layout: &Layout, parent: NodeId, across: bool) -> Vec<Slot> {
    let scale = layout.scale();
    tree.children(parent)
        .iter()
        .filter(|&&child| tree.reorderable(child))
        .filter_map(|&child| {
            let key = tree.key(child)?.clone();
            let frame = layout.local(child)?;
            let (start, size) = if across {
                (frame.x, frame.width)
            } else {
                (frame.y, frame.height)
            };
            Some(Slot {
                key,
                start: start / scale,
                size: size / scale,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    use lokyr_plugin_sdk::host::TabId;
    use lokyr_plugin_sdk::view::{self, Length, Node, Transition, UiEventKind};

    use super::*;
    use crate::field::Carets;
    use crate::layout::Measure;
    use crate::pointer::Button;
    use crate::surface::Surface;

    /// A font of the tests': a character 10 points wide.
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
        fn carets(&mut self, _: &str) -> Rc<[(usize, f32)]> {
            Rc::new([(0, 0.0)])
        }
    }

    /// A node of `key`, `size` points along `direction` and 20 across,
    /// reorderable, of a transition of its own.
    fn tab(key: &str, size: f32, direction: Direction) -> Node {
        let (width, height) = match direction {
            Direction::Row => (size, 20.0),
            Direction::Column => (200.0, size),
        };
        Node::container(Vec::new())
            .key(key)
            .reorderable()
            .layout(view::Layout {
                width: Length::Points(width),
                height: Length::Points(height),
                ..view::Layout::default()
            })
            .style(view::Style {
                transition: Some(Transition {
                    duration: 0.3,
                    easing: Easing::EaseOut,
                }),
                ..view::Style::default()
            })
    }

    /// A bar of the tabs of `order`, 100 points each and 10 apart along
    /// `direction`, then a button of a new tab, not reorderable.
    fn bar(order: &[&str], direction: Direction) -> Node {
        let mut children: Vec<Node> = order.iter().map(|key| tab(key, 100.0, direction)).collect();
        children.push(Node::button("+").key("new"));
        Node::container(children).layout(view::Layout {
            direction,
            gap: 10.0,
            ..view::Layout::default()
        })
    }

    fn shown(root: &Node, now: Instant) -> Surface {
        let mut surface = Surface::new(1.0);
        surface.update(root).expect("a valid tree");
        surface.lay_out((Some(600.0), Some(400.0)), &mut Fixed);
        surface.animate(now, false);
        surface
    }

    fn offset(surface: &Surface, key: &str, now: Instant) -> (f32, f32) {
        surface.animations.offset(&Key::from(key), now)
    }

    /// The events of the node of `key`, of a drag or a reorder, and its
    /// clicks.
    fn dragged(events: &[crate::pointer::NodeEvent], key: &str) -> Vec<UiEventKind> {
        events
            .iter()
            .filter(|event| event.key.0 == key)
            .map(|event| event.kind.clone())
            .filter(|kind| !matches!(kind, UiEventKind::HoverEnter | UiEventKind::HoverLeave))
            .collect()
    }

    const LATER: Duration = Duration::from_millis(300);

    #[test]
    fn a_node_dragged_past_a_sibling_s_middle_takes_its_place_and_a_render_keeps_it_there() {
        let start = Instant::now();
        let later = start + LATER;
        let mut surface = shown(&bar(&["a", "b", "c"], Direction::Row), start);
        surface.pressed((50.0, 10.0), (Button::Primary, 1, false), &mut Fixed);
        let (events, _) = surface.moved((170.0, 10.0), &mut Fixed, start);
        assert_eq!(
            dragged(&events, "a"),
            [
                UiEventKind::DragStart,
                UiEventKind::DragMove { x: 120.0, y: 0.0 }
            ]
        );
        // Under the pointer, past the middle of the second, which slides out
        // of its way; the third stays.
        assert_eq!(offset(&surface, "a", start), (120.0, 0.0));
        assert_eq!(offset(&surface, "b", later), (-110.0, 0.0));
        assert_eq!(offset(&surface, "c", later), (0.0, 0.0));
        assert_eq!(surface.animations.raised(), Some(&Key::from("a")));
        // No hover nor press of the others, wherever the layout put them.
        assert!(surface.animations.hover(&Key::from("b"), later) == 0.0);
        let events = surface.released((170.0, 10.0), (Button::Primary, 1), later);
        assert_eq!(
            dragged(&events, "a"),
            [
                UiEventKind::Reorder { index: 1 },
                UiEventKind::DragEnd { x: 120.0, y: 0.0 }
            ]
        );
        // Its plugin's render of the new order: the second at its place
        // already, of no slide of its own transition, the first sliding on
        // from where it showed to its place.
        surface
            .update(&bar(&["b", "a", "c"], Direction::Row))
            .expect("a valid tree");
        surface.lay_out((Some(600.0), Some(400.0)), &mut Fixed);
        surface.animate(later, false);
        assert_eq!(offset(&surface, "b", later), (0.0, 0.0));
        assert_eq!(offset(&surface, "a", later), (10.0, 0.0));
        let ended = later + SLIDE;
        assert_eq!(offset(&surface, "a", ended), (0.0, 0.0));
        assert!(!surface.animating(ended));
        // Over the others until then, as the frame's layout forgets it.
        assert_eq!(surface.animations.raised(), Some(&Key::from("a")));
        surface.animate(ended, false);
        assert_eq!(surface.animations.raised(), None);
    }

    #[test]
    fn the_node_dragged_keeps_its_press_and_hover_through_the_moves_and_the_frames() {
        let start = Instant::now();
        let at = |ms| start + Duration::from_millis(ms);
        // Tabs of a hover and a press of their own.
        let mut root = bar(&["a", "b", "c"], Direction::Row);
        for tab in &mut root.children {
            tab.style.hover = Some(view::Hover::default());
            tab.style.pressed = Some(view::Hover::default());
        }
        let mut surface = shown(&root, start);
        let (a, b) = (Key::from("a"), Key::from("b"));
        surface.moved((50.0, 10.0), &mut Fixed, start);
        surface.pressed((50.0, 10.0), (Button::Primary, 1, false), &mut Fixed);
        surface.animate(start, false);
        // Over the second's place, then on, a frame after each move.
        for (x, ms) in [(170.0, 200), (175.0, 250), (180.0, 300)] {
            surface.moved((x, 10.0), &mut Fixed, at(ms));
            surface.animate(at(ms), false);
        }
        // Pressed and hovered since the press, the second neither.
        let steady = at(400);
        assert_eq!(surface.animations.press(&a, steady), 1.0);
        assert_eq!(surface.animations.hover(&a, steady), 1.0);
        assert_eq!(surface.animations.hover(&b, steady), 0.0);
        assert_eq!(surface.animations.press(&b, steady), 0.0);
        // Its release ends its press; the others stay unhovered until the
        // render.
        surface.released((180.0, 10.0), (Button::Primary, 1), steady);
        surface.animate(steady, false);
        assert_eq!(surface.animations.hover(&b, at(800)), 0.0);
        assert_eq!(surface.animations.hover(&a, at(800)), 1.0);
        assert_eq!(surface.animations.press(&a, at(800)), 0.0);
    }

    #[test]
    fn escape_puts_the_node_back_and_its_release_clicks_nothing() {
        let start = Instant::now();
        let later = start + LATER;
        let mut surface = shown(&bar(&["a", "b", "c"], Direction::Row), start);
        assert!(surface.escape(start).is_none());
        surface.pressed((50.0, 10.0), (Button::Primary, 1, false), &mut Fixed);
        surface.moved((170.0, 10.0), &mut Fixed, start);
        let events = surface.escape(later).expect("a drag under way");
        assert_eq!(dragged(&events, "a"), [UiEventKind::DragCancel]);
        // From where they showed, back to their places.
        assert_eq!(offset(&surface, "a", later), (120.0, 0.0));
        assert_eq!(offset(&surface, "b", later), (-110.0, 0.0));
        let ended = later + SLIDE;
        assert_eq!(offset(&surface, "a", ended), (0.0, 0.0));
        assert_eq!(offset(&surface, "b", ended), (0.0, 0.0));
        let events = surface.released((170.0, 10.0), (Button::Primary, 1), ended);
        assert!(dragged(&events, "a").is_empty(), "{events:?}");
        assert!(dragged(&events, "b").is_empty(), "{events:?}");
    }

    #[test]
    fn a_drop_that_its_plugin_does_not_render_slides_back() {
        let start = Instant::now();
        let later = start + LATER;
        let mut surface = shown(&bar(&["a", "b", "c"], Direction::Row), start);
        surface.pressed((50.0, 10.0), (Button::Primary, 1, false), &mut Fixed);
        surface.moved((170.0, 10.0), &mut Fixed, start);
        surface.released((170.0, 10.0), (Button::Primary, 1), later);
        // At its new place, waiting for the render.
        let waiting = later + SLIDE;
        surface.animate(waiting, false);
        assert_eq!(offset(&surface, "a", waiting), (110.0, 0.0));
        assert_eq!(offset(&surface, "b", waiting), (-110.0, 0.0));
        assert!(surface.animating(waiting));
        // A render of the same order waits too.
        surface
            .update(&bar(&["a", "b", "c"], Direction::Row))
            .expect("a valid tree");
        surface.animate(waiting, false);
        assert_eq!(offset(&surface, "a", waiting), (110.0, 0.0));
        let given_up = later + WAIT;
        surface.animate(given_up, false);
        assert_eq!(offset(&surface, "a", given_up), (110.0, 0.0));
        assert_eq!(offset(&surface, "b", given_up), (-110.0, 0.0));
        let back = given_up + SLIDE;
        assert_eq!(offset(&surface, "a", back), (0.0, 0.0));
        assert_eq!(offset(&surface, "b", back), (0.0, 0.0));
        assert!(!surface.animating(back));
    }

    #[test]
    fn a_node_dragged_down_a_column_stays_within_its_siblings_room() {
        let start = Instant::now();
        let later = start + LATER;
        let mut surface = shown(&bar(&["a", "b", "c"], Direction::Column), start);
        // The third, 220 points down, dragged far up: no higher than the
        // first's place, the two others sliding down.
        surface.pressed((50.0, 270.0), (Button::Primary, 1, false), &mut Fixed);
        surface.moved((50.0, -300.0), &mut Fixed, start);
        assert_eq!(offset(&surface, "c", start), (0.0, -220.0));
        assert_eq!(offset(&surface, "a", later), (0.0, 110.0));
        assert_eq!(offset(&surface, "b", later), (0.0, 110.0));
        let events = surface.released((50.0, -300.0), (Button::Primary, 1), later);
        assert_eq!(
            dragged(&events, "c"),
            [
                UiEventKind::Reorder { index: 0 },
                UiEventKind::DragEnd { x: 0.0, y: -570.0 }
            ]
        );
        // Dragged back where it was, no reorder.
        let mut surface = shown(&bar(&["a", "b", "c"], Direction::Column), start);
        surface.pressed((50.0, 270.0), (Button::Primary, 1, false), &mut Fixed);
        surface.moved((50.0, 200.0), &mut Fixed, start);
        surface.moved((50.0, 280.0), &mut Fixed, start);
        assert_eq!(offset(&surface, "c", start), (0.0, 0.0));
        let events = surface.released((50.0, 280.0), (Button::Primary, 1), later);
        assert_eq!(
            dragged(&events, "c"),
            [UiEventKind::DragEnd { x: 0.0, y: 10.0 }]
        );
        // Of no render to wait for, at its place once its slide ends.
        assert!(!surface.animating(later + SLIDE));
    }

    #[test]
    fn a_scroll_area_scrolls_under_a_node_dragged_past_its_edge() {
        let start = Instant::now();
        let tabs = ["a", "b", "c", "d", "e"]
            .iter()
            .map(|key| tab(key, 100.0, Direction::Row))
            .collect();
        let root = Node::container(vec![Node::scroll(tabs).key("track").layout(view::Layout {
            width: Length::Points(250.0),
            height: Length::Points(20.0),
            ..view::Layout::default()
        })]);
        let mut surface = shown(&root, start);
        let track = surface.tree.by_key(&Key::from("track")).expect("the track");
        surface.pressed((50.0, 10.0), (Button::Primary, 1, false), &mut Fixed);
        // Its end 40 points past the area's.
        surface.moved((240.0, 10.0), &mut Fixed, start);
        surface.animate(start, false);
        assert!(surface.animating(start));
        let tenth = start + Duration::from_millis(100);
        surface.animate(tenth, false);
        // 400 points a second, a tenth of a second; the node stays under
        // the pointer.
        assert!((surface.layout.scrolled(track) - 40.0).abs() < 0.01);
        let (across, _) = offset(&surface, "a", tenth);
        assert!((across - 230.0).abs() < 0.01, "{across}");
        // Back within the area, the scroll stops.
        surface.moved((150.0, 10.0), &mut Fixed, tenth);
        surface.animate(tenth, false);
        let after = tenth + Duration::from_millis(100);
        surface.animate(after, false);
        assert!((surface.layout.scrolled(track) - 40.0).abs() < 0.01);
    }

    /// A bar of the tabs of `order`, as [`bar`]'s, each carrying the tab of
    /// its place in `order`, from 1.
    fn carrying_bar(order: &[&str]) -> Node {
        let mut root = bar(order, Direction::Row);
        for (at, tab) in root.children.iter_mut().take(order.len()).enumerate() {
            tab.carries = Some(TabId(at as u64 + 1));
        }
        root
    }

    fn opacity(surface: &Surface, key: &str, now: Instant) -> f32 {
        surface.animations.opacity(&Key::from(key), 1.0, now)
    }

    #[test]
    fn a_node_that_carries_a_tab_leaves_its_bar_and_its_siblings_close_its_room() {
        let start = Instant::now();
        let later = start + LATER;
        let mut surface = shown(&carrying_bar(&["a", "b", "c"]), start);
        surface.pressed((50.0, 10.0), (Button::Primary, 1, false), &mut Fixed);
        surface.moved((60.0, 80.0), &mut Fixed, start);
        assert_eq!(
            surface.carried(),
            Some((Key::from("a"), TabId(1))),
            "the tab that it carries"
        );
        assert!(surface.carry_out(true, start));
        assert!(!surface.carry_out(true, start), "out already");
        assert!(surface.carried_out());
        // Hidden, the others sliding into its room, by its width and the
        // gap after it.
        assert_eq!(opacity(&surface, "a", later), 0.0);
        assert_eq!(offset(&surface, "b", later), (-110.0, 0.0));
        assert_eq!(offset(&surface, "c", later), (-110.0, 0.0));
        // Its moves out of the bar move nothing.
        surface.moved((300.0, 90.0), &mut Fixed, later);
        assert_eq!(offset(&surface, "b", later), (-110.0, 0.0));
        // Back, it follows the pointer among them again, past the second's
        // middle, which goes back to its place before it.
        assert!(surface.carry_out(false, later));
        surface.moved((170.0, 10.0), &mut Fixed, later);
        let back = later + LATER;
        assert_eq!(opacity(&surface, "a", back), 1.0);
        assert_eq!(offset(&surface, "a", back), (120.0, 0.0));
        assert_eq!(offset(&surface, "b", back), (-110.0, 0.0));
        assert_eq!(offset(&surface, "c", back), (0.0, 0.0));
    }

    #[test]
    fn a_node_dropped_out_of_its_bar_waits_for_its_plugin_s_render_without_it() {
        let start = Instant::now();
        let later = start + LATER;
        let mut surface = shown(&carrying_bar(&["a", "b", "c"]), start);
        surface.pressed((50.0, 10.0), (Button::Primary, 1, false), &mut Fixed);
        surface.moved((60.0, 80.0), &mut Fixed, start);
        surface.carry_out(true, start);
        let events = surface.released((60.0, 80.0), (Button::Primary, 1), later);
        // No reorder, its end alone, which the host's drop comes before.
        assert_eq!(
            dragged(&events, "a"),
            [UiEventKind::DragEnd { x: 10.0, y: 70.0 }]
        );
        assert!(surface.carried_out());
        assert_eq!(surface.carried(), None, "no drag under way");
        assert_eq!(opacity(&surface, "a", later), 0.0);
        assert!(surface.animating(later), "a drop waits for its render");
        // Its plugin's render of its move: the others where they slid, of
        // no slide of their own transitions.
        surface
            .update(&carrying_bar(&["b", "c"]))
            .expect("a valid tree");
        surface.lay_out((Some(600.0), Some(400.0)), &mut Fixed);
        surface.animate(later, false);
        assert_eq!(offset(&surface, "b", later), (0.0, 0.0));
        assert_eq!(offset(&surface, "c", later), (0.0, 0.0));
        assert!(!surface.carried_out());
        assert!(!surface.animating(later + SLIDE));
    }

    #[test]
    fn a_node_dropped_out_of_its_bar_comes_back_without_its_plugin_s_render() {
        let start = Instant::now();
        let mut surface = shown(&carrying_bar(&["a", "b", "c"]), start);
        surface.pressed((50.0, 10.0), (Button::Primary, 1, false), &mut Fixed);
        surface.moved((60.0, 80.0), &mut Fixed, start);
        surface.carry_out(true, start);
        let later = start + LATER;
        surface.released((60.0, 80.0), (Button::Primary, 1), later);
        // No render within the wait: the others slide back, and it shows
        // at its place.
        let waited = later + WAIT;
        surface.animate(waited, false);
        assert!(!surface.carried_out());
        assert_eq!(opacity(&surface, "a", waited), 1.0);
        assert_eq!(offset(&surface, "b", waited), (-110.0, 0.0));
        assert_eq!(offset(&surface, "b", waited + SLIDE), (0.0, 0.0));
    }

    #[test]
    fn escape_puts_a_node_dragged_out_of_its_bar_back() {
        let start = Instant::now();
        let mut surface = shown(&carrying_bar(&["a", "b", "c"]), start);
        surface.pressed((50.0, 10.0), (Button::Primary, 1, false), &mut Fixed);
        surface.moved((60.0, 80.0), &mut Fixed, start);
        surface.carry_out(true, start);
        let later = start + LATER;
        let events = surface.escape(later).expect("a drag to cancel");
        assert_eq!(dragged(&events, "a"), [UiEventKind::DragCancel]);
        assert!(!surface.carried_out());
        assert_eq!(opacity(&surface, "a", later), 1.0);
        assert_eq!(offset(&surface, "b", later + SLIDE), (0.0, 0.0));
    }

    #[test]
    fn a_node_that_carries_no_tab_stays_in_its_bar() {
        let start = Instant::now();
        let mut surface = shown(&bar(&["a", "b", "c"], Direction::Row), start);
        surface.pressed((50.0, 10.0), (Button::Primary, 1, false), &mut Fixed);
        surface.moved((60.0, 80.0), &mut Fixed, start);
        assert_eq!(surface.carried(), None);
        assert!(!surface.carry_out(true, start));
        assert!(!surface.carried_out());
    }

    #[test]
    fn the_nodes_that_carry_tabs_open_a_room_under_the_pointer_for_one_dragged_in() {
        let start = Instant::now();
        let later = start + LATER;
        let mut surface = shown(&carrying_bar(&["a", "b", "c"]), start);
        // Past the first's middle, before the second's: the room after the
        // first, of a tab's width and the gap.
        assert_eq!(surface.open_room((80.0, 10.0), start), Some(1));
        assert_eq!(offset(&surface, "a", later), (0.0, 0.0));
        assert_eq!(offset(&surface, "b", later), (110.0, 0.0));
        assert_eq!(offset(&surface, "c", later), (110.0, 0.0));
        // On past the second's middle; then before the first's.
        assert_eq!(surface.open_room((200.0, 10.0), later), Some(2));
        let after = later + LATER;
        assert_eq!(offset(&surface, "b", after), (0.0, 0.0));
        assert_eq!(offset(&surface, "c", after), (110.0, 0.0));
        assert_eq!(surface.open_room((10.0, 10.0), after), Some(0));
        let first = after + LATER;
        assert_eq!(offset(&surface, "a", first), (110.0, 0.0));
        // Out again, they slide back.
        assert!(surface.close_room(first));
        assert!(!surface.close_room(first), "closed already");
        assert_eq!(offset(&surface, "a", first + SLIDE), (0.0, 0.0));
        // A bar of no tab that carries one opens none.
        let mut plain = shown(&bar(&["a", "b"], Direction::Row), start);
        assert_eq!(plain.open_room((80.0, 10.0), start), None);
    }

    #[test]
    fn a_tab_dropped_in_a_room_keeps_it_until_its_plugin_s_render_of_it_there() {
        let start = Instant::now();
        let later = start + LATER;
        let mut surface = shown(&carrying_bar(&["a", "b"]), start);
        surface.open_room((80.0, 10.0), start);
        assert_eq!(surface.drop_room(later), Some(1));
        assert_eq!(surface.drop_room(later), None, "dropped already");
        assert!(surface.animating(later), "a drop waits for its render");
        assert_eq!(offset(&surface, "b", later), (110.0, 0.0));
        // The render of the tab between them: the second at its place, of
        // no slide of its own transition.
        surface
            .update(&carrying_bar(&["a", "x", "b"]))
            .expect("a valid tree");
        surface.lay_out((Some(600.0), Some(400.0)), &mut Fixed);
        surface.animate(later, false);
        assert_eq!(offset(&surface, "b", later), (0.0, 0.0));
        assert_eq!(offset(&surface, "x", later), (0.0, 0.0));
        assert!(!surface.animating(later + SLIDE));
        // Without a render, the room closes after the wait.
        let mut waiting = shown(&carrying_bar(&["a", "b"]), start);
        waiting.open_room((80.0, 10.0), start);
        waiting.drop_room(later);
        let waited = later + WAIT;
        waiting.animate(waited, false);
        assert!(!waiting.animating(waited + SLIDE));
        assert_eq!(offset(&waiting, "b", waited + SLIDE), (0.0, 0.0));
    }
}
