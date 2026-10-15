//! Pins: cells of a page list that the list keeps track of while their rows
//! move, for selections, marks and search results.

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

/// A slot of the slab.
#[derive(Debug)]
struct Slot {
    /// `None` while the slot is free.
    place: Option<Place>,
    /// How many pins the slot has held, so that ids go stale.
    generation: u32,
}

/// The pins of a list, in a slab. The list keeps, for each of its nodes,
/// the indexes of the pins in it.
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
                self.slots[index as usize].place = Some(place);
                index
            }
            None => {
                self.slots.push(Slot {
                    place: Some(place),
                    generation: 0,
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
        if slot.generation == id.generation {
            slot.place
        } else {
            None
        }
    }

    /// The place of the pin at `index`, which the list of a node holds.
    pub(super) fn place_mut(&mut self, index: u32) -> &mut Place {
        self.slots[index as usize]
            .place
            .as_mut()
            .expect("a node holds only pins in use")
    }

    /// Loses the pin at `index`, whose node lets go of it: its slot is
    /// free again, and its id stale.
    pub(super) fn lose(&mut self, index: u32) {
        let slot = &mut self.slots[index as usize];
        slot.place = None;
        slot.generation = slot.generation.wrapping_add(1);
        self.free.push(index);
    }

    /// Where the pin at `index` is, if it is in use.
    pub(super) fn place_of(&self, index: u32) -> Option<Place> {
        self.slots.get(index as usize)?.place
    }

    /// The index of the pin with `id`, if it is not lost.
    pub(super) fn index(&self, id: PinId) -> Option<u32> {
        self.place(id).map(|_| id.index)
    }

    /// How many pins are in use.
    pub(super) fn len(&self) -> usize {
        self.slots.len() - self.free.len()
    }
}
