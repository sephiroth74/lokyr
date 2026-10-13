//! A map from the cells of a page to values, for what only some cells have.

use super::index::{Ids, Index, Slot};

/// Values for some cells of a page, keyed by the cell's position in the
/// page: its row times the width, plus its column.
///
/// The capacity is fixed when the map is created.
#[derive(Clone, Debug)]
pub(crate) struct CellMap<V> {
    /// The cell of each entry, by id. Id 0 is unused.
    cells: Box<[u32]>,
    /// The value of each entry, by id.
    values: Box<[V]>,
    ids: Ids,
    index: Index,
    len: usize,
}

impl<V: Copy + Default> CellMap<V> {
    pub(crate) fn new(capacity: u16) -> Self {
        let entries = usize::from(capacity) + 1;
        Self {
            cells: vec![0; entries].into_boxed_slice(),
            values: vec![V::default(); entries].into_boxed_slice(),
            ids: Ids::new(capacity),
            index: Index::new(capacity),
            len: 0,
        }
    }

    pub(crate) fn memory_for(capacity: u16) -> usize {
        (usize::from(capacity) + 1) * (size_of::<u32>() + size_of::<V>())
            + Ids::memory_for(capacity)
            + Index::memory_for(capacity)
    }

    pub(crate) fn memory(&self) -> usize {
        size_of_val(&*self.cells)
            + size_of_val(&*self.values)
            + self.ids.memory()
            + self.index.memory()
    }

    pub(crate) fn capacity(&self) -> u16 {
        self.ids.capacity()
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.len
    }

    pub(crate) fn get(&self, cell: u32) -> Option<V> {
        match self.find(cell)? {
            Slot::Taken(slot) => Some(self.values[usize::from(self.index.id(slot))]),
            Slot::Empty(_) => None,
        }
    }

    /// Sets the value of `cell`. `Err` with the value back if the map is
    /// full and the cell has no entry yet.
    pub(crate) fn insert(&mut self, cell: u32, value: V) -> Result<(), V> {
        let slot = match self.find(cell) {
            Some(Slot::Taken(slot)) => {
                self.values[usize::from(self.index.id(slot))] = value;
                return Ok(());
            }
            Some(Slot::Empty(slot)) => slot,
            None => return Err(value),
        };
        let Some(id) = self.ids.take() else {
            return Err(value);
        };
        self.cells[usize::from(id)] = cell;
        self.values[usize::from(id)] = value;
        self.index.insert(slot, id);
        self.len += 1;
        Ok(())
    }

    /// Removes the entry of `cell`, and returns its value.
    pub(crate) fn remove(&mut self, cell: u32) -> Option<V> {
        let Slot::Taken(slot) = self.find(cell)? else {
            return None;
        };
        let id = self.index.id(slot);
        let cells = &self.cells;
        self.index
            .remove(slot, |id| u64::from(cells[usize::from(id)]));
        self.ids.give(id);
        self.len -= 1;
        Some(self.values[usize::from(id)])
    }

    /// The same entries, with room for `capacity`, at least as many as now.
    pub(crate) fn grown(&self, capacity: u16) -> Self {
        let mut map = Self::new(capacity.max(self.capacity()));
        for id in self.ids.ever_used() {
            let id = usize::from(id);
            map.cells[id] = self.cells[id];
            map.values[id] = self.values[id];
        }
        map.ids = self.ids.grown(map.capacity());
        map.len = self.len;
        // Only the entries in use are in the index.
        for id in self.index.ids() {
            if let Some(Slot::Empty(slot)) = map.find(self.cells[usize::from(id)]) {
                map.index.insert(slot, id);
            }
        }
        map
    }

    fn find(&self, cell: u32) -> Option<Slot> {
        self.index
            .find(u64::from(cell), |id| self.cells[usize::from(id)] == cell)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use proptest::prelude::*;

    use super::*;

    #[test]
    fn values_follow_their_cells() {
        let mut map = CellMap::<u16>::new(3);
        map.insert(7, 70).unwrap();
        map.insert(1000, 10).unwrap();
        assert_eq!(map.get(7), Some(70));
        assert_eq!(map.get(8), None);
        map.insert(7, 71).unwrap();
        assert_eq!((map.get(7), map.get(1000)), (Some(71), Some(10)));
        assert_eq!(map.len(), 2);
        assert_eq!(map.remove(7), Some(71));
        assert_eq!(map.remove(7), None);
        assert_eq!(map.len(), 1);
    }

    #[test]
    fn a_full_map_refuses_new_cells_only() {
        let mut map = CellMap::<u16>::new(2);
        map.insert(1, 1).unwrap();
        map.insert(2, 2).unwrap();
        assert_eq!(map.insert(3, 3), Err(3));
        assert_eq!(map.insert(2, 20), Ok(()));
        map.remove(1);
        assert_eq!(map.insert(3, 3), Ok(()));
    }

    #[test]
    fn a_grown_map_keeps_its_entries() {
        let mut map = CellMap::<u16>::new(3);
        for cell in [5, 6, 7] {
            map.insert(cell, cell as u16 * 10).unwrap();
        }
        map.remove(6);
        let mut grown = map.grown(10);
        assert_eq!(grown.capacity(), 10);
        assert_eq!(
            (grown.get(5), grown.get(6), grown.get(7)),
            (Some(50), None, Some(70))
        );
        for cell in 100..108 {
            grown.insert(cell, 1).unwrap();
        }
        assert_eq!(grown.insert(200, 1), Err(1));
        assert_eq!(grown.memory(), CellMap::<u16>::memory_for(10));
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(512))]

        #[test]
        fn the_map_matches_a_model(
            capacity in 0_u16..12,
            ops in prop::collection::vec((0_u32..24, any::<Option<u16>>()), 0..200),
        ) {
            let mut map = CellMap::<u16>::new(capacity);
            let mut model: HashMap<u32, u16> = HashMap::new();
            for (cell, value) in ops {
                match value {
                    Some(value) => match map.insert(cell, value) {
                        Ok(()) => {
                            model.insert(cell, value);
                        }
                        Err(_) => {
                            prop_assert!(!model.contains_key(&cell));
                            prop_assert_eq!(model.len(), usize::from(capacity));
                        }
                    },
                    None => prop_assert_eq!(map.remove(cell), model.remove(&cell)),
                }
                prop_assert_eq!(map.len(), model.len());
                for cell in 0..24 {
                    prop_assert_eq!(map.get(cell), model.get(&cell).copied());
                }
            }
        }
    }
}
