//! The tests of kitty's benchmark, `kitten __benchmark__`, whose data
//! `lokyr-throughput` times, rebuilt as its version 0.49.2 writes them
//! (<https://github.com/kovidgoyal/kitty/blob/v0.49.2/tools/cmd/benchmark/main.go>):
//! to a TTY in raw mode, whose line feeds stay as they are, on the alternate
//! screen, each repetition of a test's data inside a synchronized update.
//! The data that is random in kitty's, of a seed of its own each run, comes
//! here of a fixed seed, the same bytes each time.

/// A test of kitty's benchmark: what the kitten writes before its first
/// repetition, and a repetition, its data and what follows it.
pub struct Test {
    /// kitty's name for it.
    pub name: &'static str,
    /// What the kitten writes first: the terminal's state, a cleared screen
    /// with the test's name, and the synchronized update's start.
    pub setup: Vec<u8>,
    /// A repetition: the data, then the cleared screen with the test's
    /// name, and the synchronized update's end and a new start.
    pub payload: Vec<u8>,
}

/// The tests that are rebuilt.
pub fn tests() -> Vec<Test> {
    vec![
        test("Unique multi-codepoint Unicode cells", unique_clusters()),
        test("CSI codes with few chars", csi()),
    ]
}

/// The kitten's writes around `data`, as it runs a test without
/// `--render`.
fn test(name: &'static str, data: Vec<u8>) -> Test {
    let title = format!("\x1b[m\x1b[H\x1b[2JRunning: {name}\r\n");
    // The kitten's state: the cursor saved, the alternate screen cleared,
    // autowrap on and the cursor hidden.
    let mut setup = b"\x1b7\x1b[?1049h\x1b[H\x1b[2J\x1b[?7h\x1b[?25l".to_vec();
    setup.extend_from_slice(title.as_bytes());
    setup.extend_from_slice(b"\x1b[?2026h");
    let mut payload = data;
    payload.extend_from_slice(title.as_bytes());
    payload.extend_from_slice(b"\x1b[?2026l\x1b[?2026h");
    Test {
        name,
        setup,
        payload,
    }
}

/// "Unique multi-codepoint Unicode cells": 256 Ki clusters, each an `a`
/// and three combining marks of the 112 from U+0300, the marks the digits
/// of the cluster's number, so that no two clusters are alike.
fn unique_clusters() -> Vec<u8> {
    const CELLS: u32 = 256 * 1024;
    const MARKS: u32 = 0x70;
    let mut out = String::with_capacity(CELLS as usize * 7);
    for cell in 0..CELLS {
        out.push('a');
        let mut q = cell;
        for _ in 0..3 {
            out.push(char::from_u32(0x300 + q % MARKS).expect("a combining mark"));
            q /= MARKS;
        }
    }
    out.into_bytes()
}

/// The printable characters of kitty's random text, and its controls.
const TEXT: &[u8] =
    b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ  `~!@#$%^&*()_+-=[]{}\\|;:'\",<.>/?\n\t";

/// "CSI codes with few chars": 1 MiB and 17 bytes at least of random
/// pieces, a tenth of them text and the rest sequences of SGR, of the
/// cursor's moves, of erasing, of modes and of REP.
fn csi() -> Vec<u8> {
    const SIZE: usize = 1024 * 1024 + 17;
    let mut random = Random(0x6b69_7474_795f_6373);
    let mut out = Vec::with_capacity(SIZE + 80);
    while out.len() < SIZE {
        match random.below(100) {
            0..10 => {
                let length = random.below(72) + 1;
                out.extend((0..length).map(|_| TEXT[random.below(TEXT.len())]));
            }
            10..30 => out.extend_from_slice(b"\x1b[m\x1b[?1h\x1b[H"),
            30..40 => out.extend_from_slice(b"\x1b[1;2;3;4:3;31m"),
            40..50 => out.extend_from_slice(b"\x1b[38:5:24;48:2:125:136:147m"),
            50..60 => out.extend_from_slice(b"\x1b[58;5;44;2m"),
            60..80 => out.extend_from_slice(b"\x1b[m\x1b[10A\x1b[3E\x1b[2K"),
            _ => out.extend_from_slice(b"\x1b[39m\x1b[10`a\x1b[100b\x1b[?1l"),
        }
    }
    out.extend_from_slice(b"\x1b[m");
    out
}

/// SplitMix64 (<https://prng.di.unimi.it/splitmix64.c>), of a fixed seed.
struct Random(u64);

impl Random {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// A number below `n`.
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_unique_clusters_are_kittys() {
        let data = unique_clusters();
        assert_eq!(data.len(), 256 * 1024 * 7);
        // The first cluster's marks are the digits of 0, the second's of 1.
        assert!(data.starts_with("a\u{300}\u{300}\u{300}a\u{301}\u{300}\u{300}".as_bytes()));
        let text = String::from_utf8(data).expect("UTF-8");
        let clusters: std::collections::HashSet<&str> =
            text.split('a').filter(|marks| !marks.is_empty()).collect();
        assert_eq!(clusters.len(), 256 * 1024, "no two alike");
    }

    #[test]
    fn the_csi_test_is_the_same_each_time_and_of_kittys_size() {
        let (one, two) = (csi(), csi());
        assert_eq!(one, two);
        assert!(one.len() >= 1024 * 1024 + 17 + 3);
        assert!(one.ends_with(b"\x1b[m"));
        // Every piece of kitty's comes.
        for piece in [
            &b"\x1b[m\x1b[?1h\x1b[H"[..],
            b"\x1b[1;2;3;4:3;31m",
            b"\x1b[38:5:24;48:2:125:136:147m",
            b"\x1b[58;5;44;2m",
            b"\x1b[m\x1b[10A\x1b[3E\x1b[2K",
            b"\x1b[39m\x1b[10`a\x1b[100b\x1b[?1l",
        ] {
            assert!(one.windows(piece.len()).any(|window| window == piece));
        }
    }
}
