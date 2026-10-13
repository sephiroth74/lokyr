//! The machinery that the tables of a page share: ids that come and go, and
//! a hash index from keys to ids.

/// Hands out the ids from 1 to a capacity, and takes them back.
///
/// Id 0 stays out, so that a table can use it for "none".
#[derive(Clone, Debug)]
pub(crate) struct Ids {
    /// Ids given back, to hand out again first.
    free: Vec<u16>,
    /// The lowest id never handed out.
    unused: u16,
    capacity: u16,
}

impl Ids {
    pub(crate) fn new(capacity: u16) -> Self {
        Self {
            free: Vec::with_capacity(usize::from(capacity)),
            unused: 1,
            capacity,
        }
    }

    pub(crate) fn memory_for(capacity: u16) -> usize {
        usize::from(capacity) * size_of::<u16>()
    }

    pub(crate) fn memory(&self) -> usize {
        self.free.capacity() * size_of::<u16>()
    }

    pub(crate) fn capacity(&self) -> u16 {
        self.capacity
    }

    /// An id, or `None` if they are all in use.
    pub(crate) fn take(&mut self) -> Option<u16> {
        if let Some(id) = self.free.pop() {
            Some(id)
        } else if self.unused <= self.capacity {
            self.unused += 1;
            Some(self.unused - 1)
        } else {
            None
        }
    }

    /// Takes `id` back.
    pub(crate) fn give(&mut self, id: u16) {
        debug_assert!(id != 0 && id < self.unused, "id {id} was never handed out");
        self.free.push(id);
    }

    /// Every id handed out at some point, whether it is in use or not.
    pub(crate) fn ever_used(&self) -> std::ops::Range<u16> {
        1..self.unused
    }

    /// The same ids, with room for `capacity`.
    pub(crate) fn grown(&self, capacity: u16) -> Self {
        let mut ids = Self::new(capacity.max(self.capacity));
        ids.free.extend_from_slice(&self.free);
        ids.unused = self.unused;
        ids
    }
}

/// Where a lookup in an [`Index`] ends.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Slot {
    /// The slot that holds the key's id.
    Taken(usize),
    /// The empty slot where the key's id would go.
    Empty(usize),
}

/// An open-addressing hash index from keys to the ids of a table, whose
/// entries hold the keys.
///
/// It has twice as many slots as the table has ids, so a lookup always meets
/// an empty slot. Deleting moves back the ids that would be hidden, so it
/// needs no tombstones.
#[derive(Clone, Debug)]
pub(crate) struct Index {
    /// The ids, with 0 for an empty slot.
    slots: Box<[u16]>,
}

impl Index {
    /// An index for ids up to `capacity`.
    pub(crate) fn new(capacity: u16) -> Self {
        Self {
            slots: vec![0; Self::slots_for(capacity)].into_boxed_slice(),
        }
    }

    pub(crate) fn memory_for(capacity: u16) -> usize {
        Self::slots_for(capacity) * size_of::<u16>()
    }

    pub(crate) fn memory(&self) -> usize {
        size_of_val(&*self.slots)
    }

    fn slots_for(capacity: u16) -> usize {
        usize::from(capacity)
            .saturating_mul(2)
            .next_power_of_two()
            .max(2)
    }

    /// The id in `slot`.
    pub(crate) fn id(&self, slot: usize) -> u16 {
        self.slots[slot]
    }

    /// The ids in the index, in no particular order.
    pub(crate) fn ids(&self) -> impl Iterator<Item = u16> + '_ {
        self.slots.iter().copied().filter(|&id| id != 0)
    }

    /// The slot of the key with `hash`, which `is_key` recognizes from an
    /// id, or the empty slot where it would go.
    ///
    /// A lookup stops after a full turn of the slots, which never happens in
    /// a sound index, so that a broken one cannot hang the terminal.
    pub(crate) fn find(&self, hash: u64, mut is_key: impl FnMut(u16) -> bool) -> Option<Slot> {
        let mask = self.slots.len() - 1;
        let mut slot = self.home(hash);
        for _ in 0..self.slots.len() {
            match self.slots[slot] {
                0 => return Some(Slot::Empty(slot)),
                id if is_key(id) => return Some(Slot::Taken(slot)),
                _ => slot = (slot + 1) & mask,
            }
        }
        debug_assert!(false, "the index has no empty slot");
        None
    }

    /// Puts `id` in `slot`, an empty slot that [`Index::find`] returned.
    pub(crate) fn insert(&mut self, slot: usize, id: u16) {
        debug_assert_eq!(self.slots[slot], 0, "the slot is taken");
        self.slots[slot] = id;
    }

    /// Empties `slot`, and moves back the ids after it that it would hide
    /// from a lookup. `hash_of` gives the hash of an id's key.
    pub(crate) fn remove(&mut self, slot: usize, mut hash_of: impl FnMut(u16) -> u64) {
        let mask = self.slots.len() - 1;
        let mut hole = slot;
        let mut next = slot;
        loop {
            next = (next + 1) & mask;
            let id = self.slots[next];
            if id == 0 {
                break;
            }
            let home = self.home(hash_of(id));
            // The id can fill the hole unless its home lies between the hole
            // and itself.
            if next.wrapping_sub(home) & mask >= next.wrapping_sub(hole) & mask {
                self.slots[hole] = id;
                hole = next;
            }
        }
        self.slots[hole] = 0;
    }

    /// The slot where a key with `hash` belongs. Fibonacci hashing: the top
    /// bits of the product are the best mixed.
    fn home(&self, hash: u64) -> usize {
        let hash = hash.wrapping_mul(0x9E37_79B9_7F4A_7C15);
        (hash >> (64 - self.slots.len().trailing_zeros())) as usize
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_come_back_before_new_ones() {
        let mut ids = Ids::new(3);
        assert_eq!([ids.take(), ids.take()], [Some(1), Some(2)]);
        ids.give(1);
        assert_eq!(ids.take(), Some(1));
        assert_eq!(ids.take(), Some(3));
        assert_eq!(ids.take(), None);
        assert_eq!(ids.ever_used(), 1..4);
        let mut grown = ids.grown(5);
        assert_eq!(
            [grown.take(), grown.take(), grown.take()],
            [Some(4), Some(5), None]
        );
    }

    #[test]
    fn an_index_finds_its_keys_after_removals() {
        // Keys whose hashes all land in the same slot, the hardest case for
        // removal.
        let keys = [10_u64, 20, 30, 40, 50];
        let mut index = Index::new(4);
        let colliding = |_: u64| 7;
        for (id, &key) in (1..).zip(&keys) {
            let Some(Slot::Empty(slot)) =
                index.find(colliding(key), |found| keys[usize::from(found) - 1] == key)
            else {
                panic!("{key} is new");
            };
            index.insert(slot, id);
        }
        let Some(Slot::Taken(slot)) =
            index.find(colliding(20), |found| keys[usize::from(found) - 1] == 20)
        else {
            panic!("20 is there");
        };
        index.remove(slot, |id| colliding(keys[usize::from(id) - 1]));
        for &key in &keys {
            let found = index.find(colliding(key), |found| keys[usize::from(found) - 1] == key);
            assert_eq!(matches!(found, Some(Slot::Taken(_))), key != 20, "{key}");
        }
    }
}
