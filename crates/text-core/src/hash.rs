//! A fast hasher for the keys of the text's caches, which the app makes
//! itself: the multiply-and-rotate hash of rustc's `FxHasher`, without its
//! resistance to keys chosen to collide, which these keys do not need.

use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};

/// A map whose keys hash with [`FastHasher`].
pub type FastMap<K, V> = HashMap<K, V, BuildHasherDefault<FastHasher>>;

/// The multiplier of `FxHasher`, from Firefox and rustc.
const SEED: u64 = 0x51_7c_c1_b7_27_22_0a_95;

/// A hasher of a word a step: rotate, mix in the word, multiply.
#[derive(Clone, Copy, Debug, Default)]
pub struct FastHasher {
    hash: u64,
}

impl FastHasher {
    fn add(&mut self, word: u64) {
        self.hash = (self.hash.rotate_left(5) ^ word).wrapping_mul(SEED);
    }
}

impl Hasher for FastHasher {
    fn write(&mut self, bytes: &[u8]) {
        let (words, rest) = bytes.as_chunks::<8>();
        for &word in words {
            self.add(u64::from_le_bytes(word));
        }
        if !rest.is_empty() {
            let mut last = [0; 8];
            last[..rest.len()].copy_from_slice(rest);
            self.add(u64::from_le_bytes(last));
        }
    }

    fn write_u8(&mut self, value: u8) {
        self.add(u64::from(value));
    }

    fn write_u16(&mut self, value: u16) {
        self.add(u64::from(value));
    }

    fn write_u32(&mut self, value: u32) {
        self.add(u64::from(value));
    }

    fn write_u64(&mut self, value: u64) {
        self.add(value);
    }

    fn write_usize(&mut self, value: usize) {
        self.add(value as u64);
    }

    fn finish(&self) -> u64 {
        self.hash
    }
}

#[cfg(test)]
mod tests {
    use std::hash::{BuildHasher, Hash};

    use super::*;

    fn hash(value: impl Hash) -> u64 {
        BuildHasherDefault::<FastHasher>::default().hash_one(value)
    }

    #[test]
    fn near_keys_hash_apart() {
        let hashes: std::collections::HashSet<u64> = (0..1000u32)
            .flat_map(|a| [hash((a, 0u16)), hash((a, 1u16))])
            .collect();
        assert_eq!(hashes.len(), 2000);
        assert_ne!(hash("ab"), hash("ba"));
        assert_ne!(hash([0u8; 9].as_slice()), hash([0u8; 8].as_slice()));
    }

    #[test]
    fn a_map_finds_its_keys() {
        let mut map = FastMap::default();
        for key in 0..500u32 {
            map.insert(key, key * 2);
        }
        assert!((0..500u32).all(|key| map[&key] == key * 2));
    }
}
