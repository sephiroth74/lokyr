//! The scan for runs of printable ASCII, the bytes that print as they are.

/// The bytes that the scan looks at in one step.
const BLOCK: usize = 16;

/// How many bytes at the start of `bytes` are printable ASCII
/// (`0x20..=0x7E`).
///
/// It tests 16 bytes at a time, as two 64-bit words, with arithmetic that
/// marks the bytes outside the range and so also finds the first of them.
/// Plain arithmetic, rather than code that the compiler may or may not
/// vectorize, keeps its speed from depending on the optimizer.
pub(super) fn printable_len(bytes: &[u8]) -> usize {
    let (blocks, rest) = bytes.as_chunks::<BLOCK>();
    let mut len = 0;
    for block in blocks {
        let (words, _) = block.as_chunks::<8>();
        let low = outside(u64::from_le_bytes(words[0]));
        let high = outside(u64::from_le_bytes(words[1]));
        if low | high != 0 {
            let first = if low != 0 {
                low.trailing_zeros()
            } else {
                64 + high.trailing_zeros()
            };
            return len + (first / 8) as usize;
        }
        len += BLOCK;
    }
    len + bytewise(rest)
}

/// The high bit of the bytes of `word` outside the range, in little-endian
/// order, exact up to the first of them. A byte past the first can be
/// marked as well, by a borrow or a carry from it, which does not matter.
fn outside(word: u64) -> u64 {
    const ONES: u64 = 0x0101_0101_0101_0101;
    const HIGH: u64 = 0x8080_8080_8080_8080;
    // Below 0x20, the subtraction sets the high bit that the byte lacks.
    let below = word.wrapping_sub(ONES * 0x20) & !word & HIGH;
    // 0x7F reaches 0x80 when 1 is added; the bytes above have the bit.
    let above = (word.wrapping_add(ONES) | word) & HIGH;
    below | above
}

fn is_printable(byte: u8) -> bool {
    byte.wrapping_sub(0x20) < 0x5F
}

/// The same scan, a byte at a time.
fn bytewise(bytes: &[u8]) -> usize {
    bytes
        .iter()
        .position(|&byte| !is_printable(byte))
        .unwrap_or(bytes.len())
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    #[test]
    fn the_bytewise_scan_stops_at_the_first_byte_outside_the_range() {
        assert_eq!(bytewise(b""), 0);
        assert_eq!(bytewise(b" ~"), 2);
        assert_eq!(bytewise(b"ab\x1bc"), 2);
        assert_eq!(bytewise(b"\x7f"), 0);
        assert_eq!(bytewise(b"a\x1f"), 1);
        assert_eq!(bytewise("a\u{E9}".as_bytes()), 1);
    }

    #[test]
    fn the_scan_stops_where_the_bytewise_one_does() {
        let edges = [0x00, 0x0A, 0x1B, 0x1F, 0x20, 0x7E, 0x7F, 0x80, 0xC3, 0xFF];
        for len in 0..=3 * BLOCK + 3 {
            let mut bytes = vec![b'x'; len];
            assert_eq!(printable_len(&bytes), len, "{len} printable bytes");
            for at in 0..len {
                for edge in edges {
                    bytes[at] = edge;
                    assert_eq!(
                        printable_len(&bytes),
                        bytewise(&bytes),
                        "{edge:#x} at {at} of {len}"
                    );
                }
                bytes[at] = b'x';
            }
        }
    }

    #[test]
    fn the_first_byte_outside_wins_over_what_follows_it() {
        // Two bytes side by side, within a word, across two words and across
        // two blocks, where a borrow or a carry from the first could mark
        // the second.
        for at in [3, 7, 15] {
            for first in 0..=u8::MAX {
                for second in 0..=u8::MAX {
                    let mut bytes = [b'x'; 2 * BLOCK];
                    bytes[at] = first;
                    bytes[at + 1] = second;
                    assert_eq!(
                        printable_len(&bytes),
                        bytewise(&bytes),
                        "{first:#x} {second:#x} at {at}"
                    );
                }
            }
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(1024))]

        #[test]
        fn the_scan_agrees_with_the_bytewise_one(
            bytes in prop::collection::vec(
                prop_oneof![8 => 0x20_u8..=0x7E, 1 => any::<u8>()],
                0..200,
            ),
            start in 0_usize..BLOCK,
        ) {
            // From any offset: the blocks need no alignment.
            let bytes = &bytes[start.min(bytes.len())..];
            prop_assert_eq!(printable_len(bytes), bytewise(bytes));
        }
    }
}
