//! Incremental UTF-8 decoding, for the text of the ground state.

/// What invalid UTF-8 becomes: U+FFFD REPLACEMENT CHARACTER.
pub(crate) const REPLACEMENT: char = '\u{FFFD}';

/// Decodes UTF-8 one byte at a time, so that a character can be split
/// across reads.
///
/// Each maximal subpart of an invalid sequence becomes one U+FFFD, as the
/// Unicode Standard recommends (chapter 3, "U+FFFD Substitution of Maximal
/// Subparts"). The byte ranges of its table of well-formed sequences reject
/// overlong forms, surrogates and values past U+10FFFF at the first byte
/// that breaks them.
#[derive(Clone, Debug, Default)]
pub(crate) struct Utf8Decoder {
    /// The bits of the character decoded so far.
    code: u32,
    /// How many continuation bytes the character still needs: 0 when no
    /// character is in progress.
    needed: u8,
    /// The lowest value of the next continuation byte.
    lower: u8,
    /// The highest value of the next continuation byte.
    upper: u8,
}

/// What a byte did to the character in progress.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Step {
    /// The character needs more bytes.
    Partial,
    /// The byte completed the character.
    Done(char),
    /// The byte cannot continue the character, which is invalid and becomes
    /// U+FFFD. The byte itself is left to the caller.
    Invalid,
}

impl Utf8Decoder {
    /// Whether a character is in progress.
    pub(crate) fn is_partial(&self) -> bool {
        self.needed != 0
    }

    /// Starts a character with `byte`, which is 0x80 or above. Returns
    /// U+FFFD if `byte` cannot start one.
    pub(crate) fn start(&mut self, byte: u8) -> Option<char> {
        let (needed, lower, upper) = match byte {
            0xC2..=0xDF => (1, 0x80, 0xBF),
            0xE0 => (2, 0xA0, 0xBF),
            0xE1..=0xEC | 0xEE..=0xEF => (2, 0x80, 0xBF),
            0xED => (2, 0x80, 0x9F),
            0xF0 => (3, 0x90, 0xBF),
            0xF1..=0xF3 => (3, 0x80, 0xBF),
            0xF4 => (3, 0x80, 0x8F),
            _ => return Some(REPLACEMENT),
        };
        *self = Self {
            // The lead byte of a sequence of n bytes carries 7 - n bits.
            code: u32::from(byte & (0x7F >> (needed + 1))),
            needed,
            lower,
            upper,
        };
        None
    }

    /// Continues the character in progress with `byte`.
    pub(crate) fn resume(&mut self, byte: u8) -> Step {
        if !(self.lower..=self.upper).contains(&byte) {
            self.needed = 0;
            return Step::Invalid;
        }
        self.code = (self.code << 6) | u32::from(byte & 0x3F);
        self.needed -= 1;
        if self.needed > 0 {
            self.lower = 0x80;
            self.upper = 0xBF;
            return Step::Partial;
        }
        // The byte ranges let only scalar values through.
        Step::Done(char::from_u32(self.code).unwrap_or(REPLACEMENT))
    }
}

#[cfg(test)]
mod tests {
    use std::ops::RangeInclusive;

    use super::*;

    /// Decodes `bytes` as the parser does, with ASCII passing through.
    fn decode(bytes: &[u8]) -> String {
        let mut decoder = Utf8Decoder::default();
        let mut text = String::new();
        for &byte in bytes {
            if decoder.is_partial() {
                match decoder.resume(byte) {
                    Step::Partial => continue,
                    Step::Done(c) => {
                        text.push(c);
                        continue;
                    }
                    Step::Invalid => text.push(REPLACEMENT),
                }
            }
            if byte.is_ascii() {
                text.push(char::from(byte));
            } else if let Some(c) = decoder.start(byte) {
                text.push(c);
            }
        }
        if decoder.is_partial() {
            text.push(REPLACEMENT);
        }
        text
    }

    /// Decodes `bytes`, and checks the result against the standard
    /// library's lossy decoding, which substitutes maximal subparts too.
    fn check(bytes: &[u8]) -> String {
        let text = decode(bytes);
        assert_eq!(text, String::from_utf8_lossy(bytes), "{bytes:02x?}");
        text
    }

    #[test]
    fn every_scalar_value_decodes() {
        let text: String = (0..=0x10_FFFF).filter_map(char::from_u32).collect();
        assert_eq!(check(text.as_bytes()), text);
    }

    #[test]
    fn short_inputs_decode_as_the_standard_library_does() {
        // Every input of one or two bytes, then every lead byte with every
        // second byte: the second byte is where overlong forms, surrogates
        // and values past U+10FFFF stand out.
        for first in 0..=u8::MAX {
            check(&[first]);
            for second in 0..=u8::MAX {
                check(&[first, second]);
                if first >= 0xC0 {
                    check(&[first, second, 0x80, 0x80]);
                }
            }
        }
    }

    /// Every sequence of `len` bytes made of a byte from `lead`, a byte from
    /// `second`, then continuation bytes from either end of their range.
    fn sequences(lead: RangeInclusive<u8>, second: RangeInclusive<u8>, len: usize) -> Vec<Vec<u8>> {
        let mut all = Vec::new();
        for lead in lead {
            for second in second.clone() {
                for ends in 0..1_u32 << (len - 2) {
                    let mut bytes = vec![lead, second];
                    bytes.extend((0..len - 2).map(|i| [0x80, 0xBF][(ends >> i & 1) as usize]));
                    all.push(bytes);
                }
            }
        }
        all
    }

    #[test]
    fn forbidden_sequences_become_one_replacement_per_byte() {
        // Overlong forms, surrogates, and values past U+10FFFF. No character
        // starts with their first two bytes, so each byte is a maximal
        // subpart on its own.
        let forbidden = [
            sequences(0xC0..=0xC1, 0x80..=0xBF, 2),
            sequences(0xE0..=0xE0, 0x80..=0x9F, 3),
            sequences(0xF0..=0xF0, 0x80..=0x8F, 4),
            sequences(0xED..=0xED, 0xA0..=0xBF, 3),
            sequences(0xF4..=0xF4, 0x90..=0xBF, 4),
            sequences(0xF5..=0xF7, 0x80..=0xBF, 4),
        ];
        for bytes in forbidden.iter().flatten() {
            assert_eq!(check(bytes), "\u{FFFD}".repeat(bytes.len()), "{bytes:02x?}");
        }
    }

    #[test]
    fn a_character_cut_short_becomes_one_replacement() {
        // The first and last character of each lead byte whose second byte
        // has a range of its own, and of each length.
        let chars = [
            '\u{80}',
            '\u{7FF}',
            '\u{800}',
            '\u{FFF}',
            '\u{D000}',
            '\u{D7FF}',
            '\u{E000}',
            '\u{FFFF}',
            '\u{10000}',
            '\u{3FFFF}',
            '\u{40000}',
            '\u{100000}',
            '\u{10FFFF}',
        ];
        for c in chars {
            let bytes = c.to_string().into_bytes();
            for len in 1..bytes.len() {
                for next in ["x", "é", ""] {
                    let cut = [&bytes[..len], next.as_bytes()].concat();
                    assert_eq!(check(&cut), format!("\u{FFFD}{next}"), "{cut:02x?}");
                }
            }
        }
    }

    /// Markus Kuhn's UTF-8 decoder capability and stress test.
    const KUHN: &[u8] = include_bytes!("../../testdata/UTF-8-test.txt");

    /// How many U+FFFD each case of the stress test decodes to. Cases that
    /// are not listed have none.
    ///
    /// The file suggests one U+FFFD per malformed sequence, or one per byte.
    /// Substituting maximal subparts gives one per byte to what no character
    /// starts with (lone bytes, the old 5- and 6-byte forms, overlong forms,
    /// surrogates and values past U+10FFFF), and one to a character cut
    /// short. In 2.3.3, U+FFFD is the valid character itself.
    const KUHN_REPLACEMENTS: &[(&str, usize)] = &[
        ("2.1.5", 5),
        ("2.1.6", 6),
        ("2.2.4", 4),
        ("2.2.5", 5),
        ("2.2.6", 6),
        ("2.3.3", 1),
        ("2.3.5", 4),
        ("3.1.1", 1),
        ("3.1.2", 1),
        ("3.1.3", 2),
        ("3.1.4", 3),
        ("3.1.5", 4),
        ("3.1.6", 5),
        ("3.1.7", 6),
        ("3.1.8", 7),
        ("3.1.9", 64),
        ("3.2.1", 32),
        ("3.2.2", 16),
        ("3.2.3", 8),
        ("3.2.4", 4),
        ("3.2.5", 2),
        ("3.3.1", 1),
        ("3.3.2", 2),
        ("3.3.3", 3),
        ("3.3.4", 4),
        ("3.3.5", 5),
        ("3.3.6", 1),
        ("3.3.7", 1),
        ("3.3.8", 3),
        ("3.3.9", 4),
        ("3.3.10", 5),
        ("3.4", 29),
        ("3.5.1", 1),
        ("3.5.2", 1),
        ("3.5.3", 4),
        ("4.1.1", 2),
        ("4.1.2", 3),
        ("4.1.3", 4),
        ("4.1.4", 5),
        ("4.1.5", 6),
        ("4.2.1", 2),
        ("4.2.2", 3),
        ("4.2.3", 4),
        ("4.2.4", 5),
        ("4.2.5", 6),
        ("4.3.1", 2),
        ("4.3.2", 3),
        ("4.3.3", 4),
        ("4.3.4", 5),
        ("4.3.5", 6),
        ("5.1.1", 3),
        ("5.1.2", 3),
        ("5.1.3", 3),
        ("5.1.4", 3),
        ("5.1.5", 3),
        ("5.1.6", 3),
        ("5.1.7", 3),
        ("5.2.1", 6),
        ("5.2.2", 6),
        ("5.2.3", 6),
        ("5.2.4", 6),
        ("5.2.5", 6),
        ("5.2.6", 6),
        ("5.2.7", 6),
        ("5.2.8", 6),
    ];

    /// The number that opens a line of the stress test, as in `3.1.9`.
    fn case_of(line: &str) -> Option<&str> {
        let first = line.split_whitespace().next()?;
        first
            .starts_with(|c: char| c.is_ascii_digit())
            .then_some(first)
    }

    #[test]
    fn kuhn_stress_test_gives_replacements_where_expected() {
        let text = check(KUHN);
        let lines: Vec<&str> = text.split('\n').collect();

        // Decoding resynchronizes after every invalid sequence, so every line
        // survives, up to the last one.
        assert_eq!(lines.len(), KUHN.split(|&byte| byte == b'\n').count());
        assert!(lines[lines.len() - 2].starts_with("THE END"));

        let mut found: Vec<(&str, usize)> = Vec::new();
        let mut case = "";
        for line in &lines {
            case = case_of(line).unwrap_or(case);
            let count = line.matches(REPLACEMENT).count();
            match found.last_mut() {
                _ if count == 0 => {}
                Some((last, total)) if *last == case => *total += count,
                _ => found.push((case, count)),
            }
        }
        assert_eq!(found, KUHN_REPLACEMENTS);

        // The valid characters decode to themselves.
        // "kosme" in Greek, with the oxia accent of U+1F79.
        assert!(text.contains("\"\u{3BA}\u{1F79}\u{3C3}\u{3BC}\u{3B5}\""));
        let valid = [
            ("2.1.1", '\0'),
            ("2.1.2", '\u{80}'),
            ("2.1.3", '\u{800}'),
            ("2.1.4", '\u{10000}'),
            ("2.2.1", '\u{7F}'),
            ("2.2.2", '\u{7FF}'),
            ("2.2.3", '\u{FFFF}'),
            ("2.3.1", '\u{D7FF}'),
            ("2.3.2", '\u{E000}'),
            ("2.3.3", '\u{FFFD}'),
            ("2.3.4", '\u{10FFFF}'),
            ("5.3.1", '\u{FFFE}'),
            ("5.3.2", '\u{FFFF}'),
        ];
        for (case, c) in valid {
            let line = lines
                .iter()
                .find(|line| case_of(line) == Some(case))
                .expect("every case is in the file");
            assert!(line.contains(&format!("\"{c}\"")), "{case}");
        }
    }
}
