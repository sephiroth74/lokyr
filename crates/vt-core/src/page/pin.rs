//! Pins: cells of a page list that the list keeps track of while their rows
//! move, for selections, marks and search results.

use std::ops::Range;

use super::PageId;

/// A cell of a [`PageList`](super::PageList): a column of a row of a page.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Position {
    /// The page.
    pub page: PageId,
    /// The row in the page.
    pub row: u16,
    /// The column.
    pub col: u16,
}

/// A pin of a [`PageList`](super::PageList), which follows its cell.
///
/// When its cell leaves the list, the pin is lost, and its id goes stale
/// rather than naming another pin: [`PageList::pin_position`] returns `None`
/// for it.
///
/// [`PageList::pin_position`]: super::PageList::pin_position
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PinId {
    index: u32,
    generation: u32,
}

/// Where a pin is: the node of its page in the list's slab, and its cell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Place {
    pub(super) node: u32,
    pub(super) row: u16,
    pub(super) col: u16,
}

/// No pin: the end of a row's chain of pins.
const NO_PIN: u32 = u32::MAX;

/// The node of a free slot's place.
const FREE: u32 = u32::MAX;

/// A slot of the slab.
#[derive(Debug)]
struct Slot {
    /// The pin's place, whose node is [`FREE`] while the slot is free.
    place: Place,
    /// How many pins the slot has held, so that ids go stale.
    generation: u32,
    /// The pins before and after this one in the chain of its row, while
    /// its node lists it.
    prev: u32,
    next: u32,
}

/// The pins of a list, in a slab. The list keeps, for each row of its
/// nodes that has pins, the first of them, and the pins of a row are
/// chained through their slots.
#[derive(Debug, Default)]
pub(super) struct Pins {
    slots: Vec<Slot>,
    /// The free slots.
    free: Vec<u32>,
}

impl Pins {
    /// A new pin at `place`. Its index goes in the list of its node.
    pub(super) fn add(&mut self, place: Place) -> PinId {
        let index = match self.free.pop() {
            Some(index) => {
                self.slots[index as usize].place = place;
                index
            }
            None => {
                self.slots.push(Slot {
                    place,
                    generation: 0,
                    prev: NO_PIN,
                    next: NO_PIN,
                });
                u32::try_from(self.slots.len() - 1).expect("fewer than 2^32 pins")
            }
        };
        PinId {
            index,
            generation: self.slots[index as usize].generation,
        }
    }

    /// Where the pin with `id` is, if it is not lost.
    pub(super) fn place(&self, id: PinId) -> Option<Place> {
        let slot = self.slots.get(id.index as usize)?;
        (slot.generation == id.generation && slot.place.node != FREE).then_some(slot.place)
    }

    /// The place of the pin at `index`, which the list of a node holds.
    pub(super) fn place_mut(&mut self, index: u32) -> &mut Place {
        let place = &mut self.slots[index as usize].place;
        debug_assert_ne!(place.node, FREE, "a node holds only pins in use");
        place
    }

    /// Loses the pin at `index`, whose node lets go of it: its slot is
    /// free again, and its id stale.
    pub(super) fn lose(&mut self, index: u32) {
        let slot = &mut self.slots[index as usize];
        slot.place.node = FREE;
        slot.generation = slot.generation.wrapping_add(1);
        self.free.push(index);
    }

    /// Where the pin at `index` is, if it is in use.
    pub(super) fn place_of(&self, index: u32) -> Option<Place> {
        let place = self.slots.get(index as usize)?.place;
        (place.node != FREE).then_some(place)
    }

    /// The index of the pin with `id`, if it is not lost.
    pub(super) fn index(&self, id: PinId) -> Option<u32> {
        self.place(id).map(|_| id.index)
    }

    /// How many pins are in use.
    pub(super) fn len(&self) -> usize {
        self.slots.len() - self.free.len()
    }

    /// Chains the pin at `index` first, before `next`, the chain's first
    /// pin until then, if any.
    fn link(&mut self, index: u32, next: Option<u32>) {
        let next = next.unwrap_or(NO_PIN);
        let slot = &mut self.slots[index as usize];
        (slot.prev, slot.next) = (NO_PIN, next);
        if next != NO_PIN {
            self.slots[next as usize].prev = index;
        }
    }

    /// Takes the pin at `index` out of its chain, and returns the pins
    /// before and after it, `NO_PIN` for none.
    fn unlink(&mut self, index: u32) -> (u32, u32) {
        let Slot { prev, next, .. } = self.slots[index as usize];
        if prev != NO_PIN {
            self.slots[prev as usize].next = next;
        }
        if next != NO_PIN {
            self.slots[next as usize].prev = prev;
        }
        (prev, next)
    }

    /// Gives each pin of the chain that starts at `first` row `row`.
    fn move_chain(&mut self, first: u32, row: u16) {
        let mut index = first;
        while index != NO_PIN {
            let slot = &mut self.slots[index as usize];
            debug_assert_ne!(slot.place.node, FREE, "a node holds only pins in use");
            slot.place.row = row;
            index = slot.next;
        }
    }

    /// The pins of the chain that starts at `first`, in order.
    fn chain(&self, first: u32) -> impl Iterator<Item = u32> + '_ {
        std::iter::successors(Some(first), |&index| {
            let next = self.slots[index as usize].next;
            (next != NO_PIN).then_some(next)
        })
    }
}

/// The pins of a node, by row: the first pin of each row, whose chain
/// holds the others, so that a row's pins are found and moved without going
/// through the page's others, and a pin is let go of at once. A row's first
/// pin takes 4 bytes for each row up to it, while the node has pins.
#[derive(Debug, Default)]
pub(super) struct RowPins {
    /// The first pin of each row, [`NO_PIN`] for none.
    first: Vec<u32>,
    /// How many rows have pins.
    rows: usize,
}

impl RowPins {
    /// Lists the pin at `index`, in row `row`, first in its chain.
    pub(super) fn push(&mut self, pins: &mut Pins, row: u16, index: u32) {
        let row = usize::from(row);
        if self.first.len() <= row {
            self.first.resize(row + 1, NO_PIN);
        }
        let next = std::mem::replace(&mut self.first[row], index);
        if next == NO_PIN {
            self.rows += 1;
        }
        pins.link(index, (next != NO_PIN).then_some(next));
    }

    /// Lets go of the pin at `index`, which the node lists in row `row`.
    pub(super) fn remove(&mut self, pins: &mut Pins, row: u16, index: u32) {
        let (prev, next) = pins.unlink(index);
        if prev == NO_PIN {
            let row = usize::from(row);
            debug_assert_eq!(self.first.get(row), Some(&index), "the first of its row");
            self.first[row] = next;
            if next == NO_PIN {
                self.emptied();
            }
        }
    }

    /// Counts a row that no longer has pins, and lets go of the rows'
    /// memory once none has.
    fn emptied(&mut self) {
        self.rows -= 1;
        if self.rows == 0 {
            self.first = Vec::new();
        }
    }

    /// Rotates the rows of `rows` left by `n`, as a scroll region moves
    /// them up, or right by `n` if `down`, as it moves them down, and gives
    /// each of their pins its new row in `pins`. The `n` rows that leave
    /// the region at one edge, to come back in at the other, have no pins.
    pub(super) fn rotate(&mut self, pins: &mut Pins, rows: Range<u16>, n: u16, down: bool) {
        if self.rows == 0 {
            return;
        }
        let end = usize::from(rows.end);
        if self.first.len() < end {
            self.first.resize(end, NO_PIN);
        }
        let range = usize::from(rows.start)..end;
        if down {
            self.first[range].rotate_right(usize::from(n));
        } else {
            self.first[range].rotate_left(usize::from(n));
        }
        for row in rows {
            let first = self.first[usize::from(row)];
            if first != NO_PIN {
                pins.move_chain(first, row);
            }
        }
    }

    /// The pins of row `row`, which the node no longer lists.
    pub(super) fn take_row(&mut self, pins: &Pins, row: u16) -> Vec<u32> {
        let Some(first) = self.first.get_mut(usize::from(row)) else {
            return Vec::new();
        };
        let first = std::mem::replace(first, NO_PIN);
        if first == NO_PIN {
            return Vec::new();
        }
        self.emptied();
        pins.chain(first).collect()
    }

    /// The rows that have pins, in order, with their pins, which the node
    /// no longer lists.
    pub(super) fn take(&mut self, pins: &Pins) -> Vec<(u16, Vec<u32>)> {
        let rows = std::mem::take(self);
        rows.firsts()
            .map(|(row, first)| (row, pins.chain(first).collect()))
            .collect()
    }

    /// Every pin, row by row, which the node no longer lists.
    pub(super) fn take_all(&mut self, pins: &Pins) -> Vec<u32> {
        let rows = std::mem::take(self);
        rows.firsts()
            .flat_map(|(_, first)| pins.chain(first))
            .collect()
    }

    /// The rows that have pins, in order, with the first of each.
    fn firsts(&self) -> impl Iterator<Item = (u16, u32)> + '_ {
        // A page has fewer than 2^16 rows.
        self.first
            .iter()
            .enumerate()
            .filter(|&(_, &first)| first != NO_PIN)
            .map(|(row, &first)| (row as u16, first))
    }

    /// Every pin, with its row, row by row.
    #[cfg(test)]
    pub(super) fn iter<'a>(&'a self, pins: &'a Pins) -> impl Iterator<Item = (u16, u32)> + 'a {
        self.firsts()
            .flat_map(move |(row, first)| pins.chain(first).map(move |pin| (row, pin)))
    }

    /// Lets go of every pin.
    pub(super) fn clear(&mut self) {
        *self = Self::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_let_go_of_their_memory_once_no_pin_is_left() {
        let mut pins = Pins::default();
        let mut rows = RowPins::default();
        let mut pin = |row| {
            let id = pins.add(Place {
                node: 0,
                row,
                col: 0,
            });
            pins.index(id).unwrap()
        };
        let (a, b, c) = (pin(3), pin(7), pin(7));
        rows.push(&mut pins, 3, a);
        rows.push(&mut pins, 7, b);
        rows.push(&mut pins, 7, c);
        rows.remove(&mut pins, 7, c);
        assert_eq!(rows.take_row(&pins, 7), [b]);
        assert!(rows.first.capacity() > 0, "a row has pins");
        assert_eq!(rows.take_row(&pins, 3), [a]);
        assert_eq!(rows.first.capacity(), 0);
        // The same, as the last pin is let go of.
        rows.push(&mut pins, 5, a);
        rows.remove(&mut pins, 5, a);
        assert_eq!(rows.first.capacity(), 0);
    }
}
