//! The hyperlink table: the targets of OSC 8 links, deduplicated and
//! reference-counted like styles, and the cells that are part of them.
//!
//! OSC 8 is described at
//! <https://gist.github.com/egmontkob/eb114294efbcd5adb1944c9f3cb5feda>.

use super::TableFull;
use super::arena::{Arena, Span};
use super::cell_map::CellMap;
use super::index::{Ids, Index, Slot, hash_bytes};

/// How many bytes a chunk of the arena holds.
const CHUNK: usize = 32;

/// How many chunks the arena has for each link it has room for.
const CHUNKS_PER_LINK: u16 = 4;

/// The target of a link, as OSC 8 gives it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hyperlink<'a> {
    /// The `id` parameter, which ties together the cells of one link across
    /// rows and redraws. Empty when the link has none.
    pub id: &'a [u8],
    /// The URI.
    pub uri: &'a [u8],
}

/// The id of a link in its page's hyperlink table.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct HyperlinkId(u16);

/// A link in the table.
#[derive(Clone, Copy, Debug, Default)]
struct Link {
    /// Its id, then its URI.
    bytes: Span,
    /// How many of the bytes are the id.
    id_len: u16,
    hash: u64,
    /// The cells that are part of the link, and the pens that hold it.
    refs: u32,
}

/// The links of a page, deduplicated and reference-counted, and the cells
/// that are part of them.
///
/// A link with the same id and URI as one in the table shares its
/// [`HyperlinkId`]. The terminal's pen holds a reference to the link it
/// writes, and each cell of the link holds one; the last one frees it.
/// The capacity is fixed when the table is created.
#[derive(Clone, Debug)]
pub struct HyperlinkTable {
    /// The links, by id. Id 0 is unused.
    links: Box<[Link]>,
    bytes: Arena<u8, CHUNK>,
    ids: Ids,
    index: Index,
    /// The link of each cell that is part of one.
    cells: CellMap<u16>,
}

impl HyperlinkTable {
    /// A table with room for `links` links, and for `cells` cells that are
    /// part of them.
    pub(crate) fn new(links: u16, cells: u16) -> Self {
        Self {
            links: vec![Link::default(); usize::from(links) + 1].into_boxed_slice(),
            bytes: Arena::new(links.saturating_mul(CHUNKS_PER_LINK)),
            ids: Ids::new(links),
            index: Index::new(links),
            cells: CellMap::new(cells),
        }
    }

    pub(crate) fn memory_for(links: u16, cells: u16) -> usize {
        (usize::from(links) + 1) * size_of::<Link>()
            + Arena::<u8, CHUNK>::memory_for(links.saturating_mul(CHUNKS_PER_LINK))
            + Ids::memory_for(links)
            + Index::memory_for(links)
            + CellMap::<u16>::memory_for(cells)
    }

    pub(crate) fn memory(&self) -> usize {
        size_of_val(&*self.links)
            + self.bytes.memory()
            + self.ids.memory()
            + self.index.memory()
            + self.cells.memory()
    }

    /// How many links the table can hold.
    pub fn capacity(&self) -> u16 {
        self.ids.capacity()
    }

    /// How many cells that are part of a link the table can hold.
    pub fn cell_capacity(&self) -> u16 {
        self.cells.capacity()
    }

    /// How many links the table holds.
    pub fn len(&self) -> usize {
        self.ids
            .ever_used()
            .filter(|&id| self.links[usize::from(id)].refs > 0)
            .count()
    }

    /// Whether the table holds no link.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The target of the link with `id`.
    pub fn get(&self, id: HyperlinkId) -> Hyperlink<'_> {
        let link = self.link(id);
        let (id, uri) = self
            .bytes
            .get(link.bytes)
            .split_at(usize::from(link.id_len));
        Hyperlink { id, uri }
    }

    /// How many references the link with `id` has.
    pub fn references(&self, id: HyperlinkId) -> u32 {
        self.link(id).refs
    }

    /// Adds a reference to `link`, which joins the table if it is not there
    /// yet, and returns its id.
    pub fn acquire(&mut self, link: Hyperlink<'_>) -> Result<HyperlinkId, TableFull> {
        let id_len = u16::try_from(link.id.len()).map_err(|_| TableFull::Hyperlinks)?;
        let len =
            u16::try_from(link.id.len() + link.uri.len()).map_err(|_| TableFull::Hyperlinks)?;
        let hash = hash_link(link);
        let slot = match self.find(link, hash) {
            Some(Slot::Taken(slot)) => {
                let id = self.index.id(slot);
                self.links[usize::from(id)].refs += 1;
                return Ok(HyperlinkId(id));
            }
            Some(Slot::Empty(slot)) => slot,
            None => return Err(TableFull::Hyperlinks),
        };
        let bytes = self.bytes.alloc(len).ok_or(TableFull::Hyperlinks)?;
        let Some(id) = self.ids.take() else {
            self.bytes.free(bytes);
            return Err(TableFull::Hyperlinks);
        };
        let (id_bytes, uri_bytes) = self.bytes.get_mut(bytes).split_at_mut(usize::from(id_len));
        id_bytes.copy_from_slice(link.id);
        uri_bytes.copy_from_slice(link.uri);
        self.links[usize::from(id)] = Link {
            bytes,
            id_len,
            hash,
            refs: 1,
        };
        self.index.insert(slot, id);
        Ok(HyperlinkId(id))
    }

    /// Adds a reference to the link with `id`.
    pub fn retain(&mut self, id: HyperlinkId) {
        debug_assert!(self.link(id).refs > 0, "link {id:?} is not in the table");
        self.links[usize::from(id.0)].refs += 1;
    }

    /// Drops a reference to the link with `id`, and frees it with the last
    /// one.
    pub fn release(&mut self, id: HyperlinkId) {
        let link = self.link(id);
        debug_assert!(link.refs > 0, "link {id:?} is not in the table");
        if link.refs > 1 {
            self.links[usize::from(id.0)].refs -= 1;
            return;
        }
        if let Some(Slot::Taken(slot)) = self.find(self.get(id), link.hash) {
            let links = &self.links;
            self.index.remove(slot, |id| links[usize::from(id)].hash);
        }
        self.bytes.free(link.bytes);
        self.links[usize::from(id.0)] = Link::default();
        self.ids.give(id.0);
    }

    /// The link that `cell` is part of.
    pub(crate) fn cell(&self, cell: u32) -> Option<HyperlinkId> {
        self.cells.get(cell).map(HyperlinkId)
    }

    /// Makes `cell` part of the link with `id`, which gains a reference. The
    /// link the cell was part of loses one.
    pub(crate) fn set_cell(&mut self, cell: u32, id: HyperlinkId) -> Result<(), TableFull> {
        let old = self.cell(cell);
        self.cells
            .insert(cell, id.0)
            .map_err(|_| TableFull::LinkedCells)?;
        self.retain(id);
        if let Some(old) = old {
            self.release(old);
        }
        Ok(())
    }

    /// Moves the link of `from`, if any, to `to`, which has none. The link
    /// keeps its references.
    pub(crate) fn move_cell(&mut self, from: u32, to: u32) {
        if let Some(id) = self.cells.remove(from) {
            let moved = self.cells.insert(to, id);
            debug_assert!(moved.is_ok(), "an entry moves into the room it left");
        }
    }

    /// Makes `cell` part of no link.
    pub(crate) fn remove_cell(&mut self, cell: u32) {
        if let Some(id) = self.cells.remove(cell) {
            self.release(HyperlinkId(id));
        }
    }

    /// Forgets every link and linked cell, without allocating.
    pub(crate) fn clear(&mut self) {
        self.links.fill(Link::default());
        self.bytes.clear();
        self.ids.clear();
        self.index.clear();
        self.cells.clear();
    }

    /// The same links and cells, with room for `links` links and `cells`
    /// cells, at least as many as now.
    pub(crate) fn grown(&self, links: u16, cells: u16) -> Self {
        let links = links.max(self.capacity());
        let mut table = Self::new(links, cells.max(self.cell_capacity()));
        table.links[..self.links.len()].copy_from_slice(&self.links);
        table.bytes = self.bytes.grown(links.saturating_mul(CHUNKS_PER_LINK));
        table.ids = self.ids.grown(links);
        for id in self.index.ids() {
            let link = self.get(HyperlinkId(id));
            if let Some(Slot::Empty(slot)) = table.find(link, self.links[usize::from(id)].hash) {
                table.index.insert(slot, id);
            }
        }
        table.cells = self.cells.grown(table.cells.capacity());
        table
    }

    fn link(&self, id: HyperlinkId) -> Link {
        self.links[usize::from(id.0)]
    }

    fn find(&self, link: Hyperlink<'_>, hash: u64) -> Option<Slot> {
        self.index.find(hash, |id| {
            self.links[usize::from(id)].hash == hash && self.get(HyperlinkId(id)) == link
        })
    }
}

/// The hash of a link's id and URI, which tells apart where one ends and
/// the other starts.
fn hash_link(link: Hyperlink<'_>) -> u64 {
    hash_bytes(link.id) ^ hash_bytes(link.uri).rotate_left(31) ^ link.id.len() as u64
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use proptest::prelude::*;

    use super::*;

    fn link<'a>(id: &'a str, uri: &'a str) -> Hyperlink<'a> {
        Hyperlink {
            id: id.as_bytes(),
            uri: uri.as_bytes(),
        }
    }

    #[test]
    fn equal_links_share_an_id() {
        let mut table = HyperlinkTable::new(4, 8);
        let a = table.acquire(link("1", "https://example.com")).unwrap();
        let b = table.acquire(link("2", "https://example.com")).unwrap();
        let c = table.acquire(link("", "https://example.com")).unwrap();
        assert_eq!(table.acquire(link("1", "https://example.com")), Ok(a));
        assert_ne!(a, b);
        assert_ne!(b, c);
        assert_eq!(table.get(a), link("1", "https://example.com"));
        assert_eq!(table.get(c), link("", "https://example.com"));
        assert_eq!(table.references(a), 2);
    }

    #[test]
    fn the_id_and_the_uri_do_not_run_together() {
        let mut table = HyperlinkTable::new(4, 8);
        let a = table.acquire(link("ab", "c")).unwrap();
        let b = table.acquire(link("a", "bc")).unwrap();
        assert_ne!(a, b);
        assert_eq!(table.get(b), link("a", "bc"));
    }

    #[test]
    fn the_last_release_frees_the_link_and_its_bytes() {
        let mut table = HyperlinkTable::new(1, 8);
        let long = "https://example.com/".repeat(6);
        let a = table.acquire(link("", &long)).unwrap();
        assert_eq!(table.acquire(link("", "x")), Err(TableFull::Hyperlinks));
        table.release(a);
        // The id and the bytes serve the next link.
        let b = table.acquire(link("", &long)).unwrap();
        assert_eq!(table.get(b), link("", &long));
    }

    #[test]
    fn a_link_too_long_for_the_arena_says_so() {
        let mut table = HyperlinkTable::new(2, 8);
        let long = "x".repeat(CHUNK * usize::from(2 * CHUNKS_PER_LINK) + 1);
        assert_eq!(table.acquire(link("", &long)), Err(TableFull::Hyperlinks));
        assert!(table.acquire(link("", "fits")).is_ok());
    }

    #[test]
    fn cells_hold_references_to_their_link() {
        let mut table = HyperlinkTable::new(4, 2);
        let a = table.acquire(link("", "a")).unwrap();
        let b = table.acquire(link("", "b")).unwrap();
        table.set_cell(10, a).unwrap();
        table.set_cell(11, a).unwrap();
        assert_eq!(table.references(a), 3);
        assert_eq!(table.set_cell(12, a), Err(TableFull::LinkedCells));
        // A cell that changes links moves its reference.
        table.set_cell(11, b).unwrap();
        assert_eq!((table.references(a), table.references(b)), (2, 2));
        assert_eq!(table.cell(11), Some(b));
        table.remove_cell(10);
        table.remove_cell(10);
        assert_eq!(table.references(a), 1);
        assert_eq!(table.cell(10), None);
    }

    #[test]
    fn a_grown_table_keeps_its_links_and_cells() {
        let mut table = HyperlinkTable::new(2, 2);
        let a = table.acquire(link("1", "a")).unwrap();
        let b = table.acquire(link("", "b")).unwrap();
        table.set_cell(0, a).unwrap();
        table.set_cell(1, b).unwrap();
        let mut grown = table.grown(8, 16);
        assert_eq!((grown.capacity(), grown.cell_capacity()), (8, 16));
        assert_eq!(grown.get(a), link("1", "a"));
        assert_eq!(
            grown.acquire(link("1", "a")),
            Ok(a),
            "the index knows the link"
        );
        assert_eq!((grown.cell(0), grown.cell(1)), (Some(a), Some(b)));
        assert_eq!(grown.references(b), 2);
        for n in 0..6 {
            grown.acquire(link("", &n.to_string())).unwrap();
        }
        assert_eq!(grown.memory(), HyperlinkTable::memory_for(8, 16));
    }

    /// What a sequence of operations does: a pen acquires a link or lets one
    /// go, and a cell joins the link of a pen or leaves its link.
    #[derive(Clone, Debug)]
    enum Op {
        Acquire(u8),
        Release(usize),
        Link(u32, usize),
        Unlink(u32),
    }

    fn op() -> impl Strategy<Value = Op> {
        prop_oneof![
            3 => (0_u8..8).prop_map(Op::Acquire),
            2 => any::<usize>().prop_map(Op::Release),
            3 => (0_u32..12, any::<usize>()).prop_map(|(cell, pen)| Op::Link(cell, pen)),
            1 => (0_u32..12).prop_map(Op::Unlink),
        ]
    }

    /// Link `n` of a small set, with ids and URIs of varying lengths.
    fn target(n: u8) -> (String, String) {
        let id = if n.is_multiple_of(2) {
            String::new()
        } else {
            n.to_string()
        };
        (
            id,
            format!("https://example.com/{}", "x".repeat(usize::from(n) * 9)),
        )
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(512))]

        #[test]
        fn the_table_matches_a_model(
            links in 0_u16..6,
            cells in 0_u16..8,
            ops in prop::collection::vec(op(), 0..150),
        ) {
            let mut table = HyperlinkTable::new(links, cells);
            // The links that pens hold, and the link of each linked cell.
            let mut pens: Vec<(u8, HyperlinkId)> = Vec::new();
            let mut linked: HashMap<u32, (u8, HyperlinkId)> = HashMap::new();
            let mut ids: HashMap<u8, HyperlinkId> = HashMap::new();
            for op in ops {
                match op {
                    Op::Acquire(n) => {
                        let (id, uri) = target(n);
                        match table.acquire(link(&id, &uri)) {
                            Ok(found) => {
                                if let Some(&known) = ids.get(&n) {
                                    prop_assert_eq!(found, known, "equal links share an id");
                                }
                                ids.insert(n, found);
                                pens.push((n, found));
                            }
                            Err(full) => {
                                prop_assert_eq!(full, TableFull::Hyperlinks);
                                prop_assert!(!ids.contains_key(&n), "a link in the table is found");
                            }
                        }
                    }
                    Op::Release(pen) if !pens.is_empty() => {
                        let (_, id) = pens.swap_remove(pen % pens.len());
                        table.release(id);
                    }
                    Op::Link(cell, pen) if !pens.is_empty() => {
                        let (n, id) = pens[pen % pens.len()];
                        match table.set_cell(cell, id) {
                            Ok(()) => {
                                linked.insert(cell, (n, id));
                            }
                            Err(full) => {
                                prop_assert_eq!(full, TableFull::LinkedCells);
                                prop_assert!(!linked.contains_key(&cell));
                                prop_assert_eq!(linked.len(), usize::from(cells));
                            }
                        }
                    }
                    Op::Unlink(cell) => {
                        table.remove_cell(cell);
                        linked.remove(&cell);
                    }
                    Op::Release(_) | Op::Link(..) => {}
                }
                // A link lives while a pen or a cell holds it.
                let mut refs: HashMap<HyperlinkId, (u8, u32)> = HashMap::new();
                for &(n, id) in pens.iter().chain(linked.values()) {
                    refs.entry(id).or_insert((n, 0)).1 += 1;
                }
                ids.retain(|_, id| refs.contains_key(id));
                for (&id, &(n, count)) in &refs {
                    let (link_id, uri) = target(n);
                    prop_assert_eq!(table.get(id), link(&link_id, &uri));
                    prop_assert_eq!(table.references(id), count);
                }
                for cell in 0..12 {
                    prop_assert_eq!(table.cell(cell), linked.get(&cell).map(|&(_, id)| id));
                }
            }
        }
    }
}
