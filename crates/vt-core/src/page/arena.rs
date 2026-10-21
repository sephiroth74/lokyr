//! A fixed arena for data of varying length, such as the codepoints of
//! graphemes and the bytes of hyperlinks.

/// Where a run of items lives in an [`Arena`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Span {
    /// The first chunk.
    pub(crate) chunk: u16,
    /// How many items, from the start of the chunk.
    pub(crate) len: u16,
}

/// Items in chunks of `CHUNK`, each run of items in a run of chunks.
///
/// A bitmap tells the free chunks from the used ones, and a run goes in the
/// first free run of chunks long enough. The search starts at the lowest
/// free chunk and skips the words of the bitmap that are full, so filling
/// the arena run after run costs linear time. The arena never grows.
#[derive(Clone, Debug)]
pub(crate) struct Arena<T, const CHUNK: usize> {
    items: Box<[T]>,
    /// One bit per chunk, set when it is in use.
    used: Box<[u64]>,
    chunks: u16,
    /// No chunk below it is free.
    first_free: u16,
}

impl<T: Copy + Default, const CHUNK: usize> Arena<T, CHUNK> {
    /// An arena of `chunks` chunks.
    pub(crate) fn new(chunks: u16) -> Self {
        Self {
            items: vec![T::default(); usize::from(chunks) * CHUNK].into_boxed_slice(),
            used: vec![0; usize::from(chunks).div_ceil(64)].into_boxed_slice(),
            chunks,
            first_free: 0,
        }
    }

    pub(crate) fn memory_for(chunks: u16) -> usize {
        usize::from(chunks) * CHUNK * size_of::<T>()
            + usize::from(chunks).div_ceil(64) * size_of::<u64>()
    }

    pub(crate) fn memory(&self) -> usize {
        size_of_val(&*self.items) + size_of_val(&*self.used)
    }

    /// How many chunks are in use.
    pub(crate) fn used_chunks(&self) -> usize {
        self.used
            .iter()
            .map(|word| word.count_ones() as usize)
            .sum()
    }

    /// Room for `len` items, which hold their default value. `None` if no free
    /// run of chunks is long enough.
    pub(crate) fn alloc(&mut self, len: u16) -> Option<Span> {
        let needed = Self::chunks_for(len);
        let mut run = 0;
        let mut chunk = self.first_free;
        while chunk < self.chunks {
            if chunk.is_multiple_of(64) && self.used[usize::from(chunk / 64)] == u64::MAX {
                run = 0;
                chunk = chunk.saturating_add(64);
                continue;
            }
            if self.is_used(chunk) {
                run = 0;
            } else {
                run += 1;
                if run == needed {
                    let start = chunk + 1 - needed;
                    self.mark(start, needed, true);
                    if start == self.first_free {
                        self.first_free = start + needed;
                    }
                    let span = Span { chunk: start, len };
                    self.get_mut(span).fill(T::default());
                    return Some(span);
                }
            }
            chunk += 1;
        }
        None
    }

    /// Gives back every chunk.
    pub(crate) fn clear(&mut self) {
        self.used.fill(0);
        self.first_free = 0;
    }

    /// Gives back the chunks of `span`.
    pub(crate) fn free(&mut self, span: Span) {
        self.mark(span.chunk, Self::chunks_for(span.len), false);
        self.first_free = self.first_free.min(span.chunk);
    }

    /// Makes `span` one item longer, in place if its last chunk has room or
    /// the chunk after it is free. `None` if it cannot grow in place.
    pub(crate) fn grow_in_place(&mut self, span: Span) -> Option<Span> {
        let len = span.len.checked_add(1)?;
        let (before, after) = (Self::chunks_for(span.len), Self::chunks_for(len));
        if after > before {
            let next = span.chunk + before;
            if next >= self.chunks || self.is_used(next) {
                return None;
            }
            self.mark(next, 1, true);
        }
        Some(Span { len, ..span })
    }

    /// Moves the items of `span` to a new run of `len` items, at least as
    /// many, and gives back the old one. `None` if no free run is long
    /// enough, and `span` stays as it was.
    pub(crate) fn relocate(&mut self, span: Span, len: u16) -> Option<Span> {
        debug_assert!(len >= span.len, "a run only grows");
        let moved = self.alloc(len)?;
        let from = usize::from(span.chunk) * CHUNK;
        let to = usize::from(moved.chunk) * CHUNK;
        self.items
            .copy_within(from..from + usize::from(span.len), to);
        self.free(span);
        Some(moved)
    }

    pub(crate) fn get(&self, span: Span) -> &[T] {
        let start = usize::from(span.chunk) * CHUNK;
        &self.items[start..start + usize::from(span.len)]
    }

    pub(crate) fn get_mut(&mut self, span: Span) -> &mut [T] {
        let start = usize::from(span.chunk) * CHUNK;
        &mut self.items[start..start + usize::from(span.len)]
    }

    /// The same contents, in an arena of `chunks` chunks, at least as many
    /// as now. Every span stays valid.
    pub(crate) fn grown(&self, chunks: u16) -> Self {
        let mut arena = Self::new(chunks.max(self.chunks));
        arena.items[..self.items.len()].copy_from_slice(&self.items);
        arena.used[..self.used.len()].copy_from_slice(&self.used);
        arena.first_free = self.first_free;
        arena
    }

    fn chunks_for(len: u16) -> u16 {
        // An empty run still takes a chunk, so that it has a place.
        len.div_ceil(CHUNK as u16).max(1)
    }

    fn is_used(&self, chunk: u16) -> bool {
        self.used[usize::from(chunk / 64)] & (1 << (chunk % 64)) != 0
    }

    fn mark(&mut self, start: u16, count: u16, used: bool) {
        for chunk in start..start + count {
            let (word, bit) = (usize::from(chunk / 64), 1 << (chunk % 64));
            debug_assert_ne!(
                self.used[word] & bit != 0,
                used,
                "chunk {chunk} is already so"
            );
            if used {
                self.used[word] |= bit;
            } else {
                self.used[word] &= !bit;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    type Bytes = Arena<u8, 4>;

    #[test]
    fn runs_take_whole_chunks() {
        let mut arena = Bytes::new(4);
        let a = arena.alloc(3).unwrap();
        let b = arena.alloc(5).unwrap();
        assert_eq!((a.chunk, b.chunk), (0, 1));
        arena.get_mut(a).copy_from_slice(b"abc");
        arena.get_mut(b).copy_from_slice(b"defgh");
        assert_eq!(arena.get(a), b"abc");
        assert_eq!(arena.get(b), b"defgh");
        assert_eq!(arena.alloc(5), None, "only one chunk is left");
        assert!(arena.alloc(4).is_some());
    }

    #[test]
    fn freed_chunks_serve_again() {
        let mut arena = Bytes::new(3);
        let a = arena.alloc(4).unwrap();
        let _b = arena.alloc(4).unwrap();
        let c = arena.alloc(4).unwrap();
        arena.free(a);
        arena.free(c);
        // Two free chunks, but not next to each other.
        assert_eq!(arena.alloc(8), None);
        let d = arena.alloc(2).unwrap();
        assert_eq!(d.chunk, 0);
        assert_eq!(arena.get(d), [0, 0], "new room holds the default");
    }

    #[test]
    fn a_run_grows_in_place_while_it_can() {
        let mut arena = Bytes::new(3);
        let mut a = arena.alloc(3).unwrap();
        a = arena.grow_in_place(a).unwrap();
        assert_eq!(a.len, 4);
        // The next chunk is free, so the run takes it.
        a = arena.grow_in_place(a).unwrap();
        assert_eq!(a.len, 5);
        let b = arena.alloc(1).unwrap();
        assert_eq!(b.chunk, 2);
        for _ in 0..3 {
            a = arena.grow_in_place(a).unwrap();
        }
        assert_eq!(arena.grow_in_place(a), None, "the next chunk is taken");
    }

    #[test]
    fn a_run_can_move_to_a_longer_one() {
        let mut arena = Bytes::new(4);
        let a = arena.alloc(4).unwrap();
        arena.get_mut(a).copy_from_slice(b"abcd");
        let _b = arena.alloc(1).unwrap();
        let moved = arena.relocate(a, 7).unwrap();
        assert_eq!(moved.chunk, 2);
        assert_eq!(&arena.get(moved)[..4], b"abcd");
        assert_eq!(arena.alloc(4).unwrap().chunk, 0, "the old run is free");
        assert_eq!(arena.relocate(moved, 20), None);
    }

    #[test]
    fn a_grown_arena_keeps_every_span() {
        let mut arena = Bytes::new(2);
        let a = arena.alloc(6).unwrap();
        arena.get_mut(a).copy_from_slice(b"abcdef");
        let mut grown = arena.grown(70);
        assert_eq!(grown.get(a), b"abcdef");
        assert_eq!(grown.alloc(4).unwrap().chunk, 2);
        assert!(
            grown.alloc(4 * 67).is_some(),
            "the new chunks span bitmap words"
        );
        assert_eq!(grown.memory(), Bytes::memory_for(70));
    }

    /// What a sequence of operations does to the arena.
    #[derive(Clone, Debug)]
    enum Op {
        Alloc(u16),
        Free(usize),
        Grow(usize),
    }

    fn op() -> impl Strategy<Value = Op> {
        prop_oneof![
            3 => (1_u16..40).prop_map(Op::Alloc),
            2 => any::<usize>().prop_map(Op::Free),
            2 => any::<usize>().prop_map(Op::Grow),
        ]
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(512))]

        #[test]
        fn the_arena_matches_a_model(
            chunks in 0_u16..200,
            ops in prop::collection::vec(op(), 0..150),
        ) {
            let mut arena = Bytes::new(chunks);
            // The live runs, with what they hold.
            let mut runs: Vec<(Span, Vec<u8>)> = Vec::new();
            let mut next_byte = 0_u8;
            let mut fill = |len: usize| -> Vec<u8> {
                (0..len).map(|_| { next_byte = next_byte.wrapping_add(1); next_byte }).collect()
            };
            for op in ops {
                match op {
                    Op::Alloc(len) => match arena.alloc(len) {
                        Some(span) => {
                            let bytes = fill(usize::from(len));
                            arena.get_mut(span).copy_from_slice(&bytes);
                            runs.push((span, bytes));
                        }
                        None => {
                            // No free run of chunks is long enough.
                            let mut used = vec![false; usize::from(chunks)];
                            for (span, _) in &runs {
                                let start = usize::from(span.chunk);
                                used[start..start + usize::from(Bytes::chunks_for(span.len))].fill(true);
                            }
                            let longest = used
                                .split(|&chunk| chunk)
                                .map(<[bool]>::len)
                                .max()
                                .unwrap_or(0);
                            prop_assert!(longest < usize::from(Bytes::chunks_for(len)));
                        }
                    },
                    Op::Free(index) if !runs.is_empty() => {
                        let (span, _) = runs.swap_remove(index % runs.len());
                        arena.free(span);
                    }
                    Op::Grow(index) if !runs.is_empty() => {
                        let index = index % runs.len();
                        let (span, mut bytes) = runs[index].clone();
                        let grown = arena
                            .grow_in_place(span)
                            .or_else(|| arena.relocate(span, span.len + 1));
                        if let Some(grown) = grown {
                            bytes.extend(fill(1));
                            *arena.get_mut(grown).last_mut().unwrap() = bytes[bytes.len() - 1];
                            runs[index] = (grown, bytes);
                        }
                    }
                    Op::Free(_) | Op::Grow(_) => {}
                }
                // Runs never overlap, and keep what they hold.
                let mut owner = vec![None; usize::from(chunks)];
                for (n, (span, bytes)) in runs.iter().enumerate() {
                    let start = usize::from(span.chunk);
                    let end = start + usize::from(Bytes::chunks_for(span.len));
                    for chunk in &mut owner[start..end] {
                        prop_assert_eq!(*chunk, None, "a chunk is in two runs");
                        *chunk = Some(n);
                    }
                    prop_assert_eq!(arena.get(*span), &bytes[..]);
                }
            }
        }
    }
}
