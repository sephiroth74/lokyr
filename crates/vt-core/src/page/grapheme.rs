//! The grapheme table: the codepoints after the first, for the cells whose
//! grapheme cluster has several.

use super::TableFull;
use super::arena::{Arena, Span};
use super::cell_map::CellMap;

/// How many codepoints a chunk of the arena holds. Most clusters add one or
/// two codepoints to the first, such as a combining mark or a variation
/// selector, and emoji ZWJ sequences add a few more.
const CHUNK: usize = 4;

/// The codepoints after the first of each cell that has several, keyed by
/// the cell's position in the page. The first codepoint stays in the cell.
#[derive(Clone, Debug)]
pub(crate) struct GraphemeTable {
    spans: CellMap<Span>,
    codepoints: Arena<char, CHUNK>,
}

impl GraphemeTable {
    /// A table with room for `capacity` cells, and a chunk of codepoints for
    /// each.
    pub(crate) fn new(capacity: u16) -> Self {
        Self {
            spans: CellMap::new(capacity),
            codepoints: Arena::new(capacity),
        }
    }

    pub(crate) fn memory_for(capacity: u16) -> usize {
        CellMap::<Span>::memory_for(capacity) + Arena::<char, CHUNK>::memory_for(capacity)
    }

    pub(crate) fn memory(&self) -> usize {
        self.spans.memory() + self.codepoints.memory()
    }

    pub(crate) fn capacity(&self) -> u16 {
        self.spans.capacity()
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.spans.len()
    }

    /// The codepoints of `cell` after its first: none for a cell without
    /// an entry.
    pub(crate) fn get(&self, cell: u32) -> &[char] {
        self.spans
            .get(cell)
            .map_or(&[], |span| self.codepoints.get(span))
    }

    /// Adds `c` to the codepoints of `cell`.
    pub(crate) fn push(&mut self, cell: u32, c: char) -> Result<(), TableFull> {
        let span = match self.spans.get(cell) {
            Some(span) => match self.codepoints.grow_in_place(span) {
                Some(grown) => grown,
                None => {
                    let len = span.len.checked_add(1).ok_or(TableFull::Graphemes)?;
                    self.codepoints
                        .relocate(span, len)
                        .ok_or(TableFull::Graphemes)?
                }
            },
            None => self.codepoints.alloc(1).ok_or(TableFull::Graphemes)?,
        };
        if self.spans.insert(cell, span).is_err() {
            // Only a new cell can find the map full, and its run is new too.
            self.codepoints.free(span);
            return Err(TableFull::Graphemes);
        }
        self.codepoints.get_mut(span)[usize::from(span.len) - 1] = c;
        Ok(())
    }

    /// Forgets the codepoints of `cell`.
    pub(crate) fn remove(&mut self, cell: u32) {
        if let Some(span) = self.spans.remove(cell) {
            self.codepoints.free(span);
        }
    }

    /// The same entries, with room for `capacity` cells, at least as many
    /// as now.
    pub(crate) fn grown(&self, capacity: u16) -> Self {
        let capacity = capacity.max(self.capacity());
        Self {
            spans: self.spans.grown(capacity),
            codepoints: self.codepoints.grown(capacity),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codepoints_follow_their_cell() {
        let mut table = GraphemeTable::new(4);
        assert_eq!(table.get(3), []);
        table.push(3, '\u{301}').unwrap();
        table.push(9, '\u{FE0F}').unwrap();
        table.push(3, '\u{302}').unwrap();
        assert_eq!(table.get(3), ['\u{301}', '\u{302}']);
        assert_eq!(table.get(9), ['\u{FE0F}']);
        assert_eq!(table.len(), 2);
        table.remove(3);
        assert_eq!(table.get(3), []);
        assert_eq!(table.len(), 1);
    }

    #[test]
    fn a_long_cluster_moves_to_room_enough() {
        let mut table = GraphemeTable::new(8);
        // A neighbour right after the first chunk, so the cluster must move
        // to grow.
        table.push(0, 'a').unwrap();
        table.push(1, 'b').unwrap();
        let marks: Vec<char> = ('\u{300}'..='\u{316}').collect();
        for &mark in &marks {
            table.push(0, mark).unwrap();
        }
        assert_eq!(table.get(0)[0], 'a');
        assert_eq!(&table.get(0)[1..], marks);
        assert_eq!(table.get(1), ['b']);
    }

    #[test]
    fn a_full_table_says_so_and_keeps_its_entries() {
        let mut table = GraphemeTable::new(2);
        table.push(0, 'a').unwrap();
        table.push(1, 'b').unwrap();
        assert_eq!(table.push(2, 'c'), Err(TableFull::Graphemes));
        for c in ['x', 'y', 'z'] {
            table.push(0, c).unwrap();
        }
        // Cell 0 fills its chunk; the arena has no other free chunk.
        assert_eq!(table.push(0, 'w'), Err(TableFull::Graphemes));
        assert_eq!(table.get(0), ['a', 'x', 'y', 'z']);
        let mut grown = table.grown(4);
        grown.push(0, 'w').unwrap();
        grown.push(2, 'c').unwrap();
        assert_eq!(grown.get(0), ['a', 'x', 'y', 'z', 'w']);
        assert_eq!(grown.get(1), ['b']);
        assert_eq!(grown.memory(), GraphemeTable::memory_for(4));
    }
}
