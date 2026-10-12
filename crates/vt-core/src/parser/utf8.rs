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

    #[test]
    fn valid_text_decodes() {
        let text = "aé€😀\u{10FFFF}\u{80}";
        assert_eq!(decode(text.as_bytes()), text);
    }

    #[test]
    fn invalid_bytes_become_one_replacement_each() {
        // A lone continuation byte, bytes that never start a character, and
        // a character cut short by ASCII.
        assert_eq!(
            decode(b"\x80a\xC0\xFFb\xE2\x82c"),
            "\u{FFFD}a\u{FFFD}\u{FFFD}b\u{FFFD}c"
        );
    }

    #[test]
    fn forbidden_values_break_at_their_second_byte() {
        // An overlong form, a surrogate and a value past U+10FFFF: the lead
        // byte is a maximal subpart on its own, and so is each byte after it.
        assert_eq!(decode(b"\xE0\x80\x80"), "\u{FFFD}".repeat(3));
        assert_eq!(decode(b"\xED\xA0\x80"), "\u{FFFD}".repeat(3));
        assert_eq!(decode(b"\xF4\x90\x80\x80"), "\u{FFFD}".repeat(4));
    }

    #[test]
    fn our_decoding_matches_the_standard_library() {
        let samples: [&[u8]; 5] = [
            b"\xF0\x9F\x98",
            b"\xF0\x9F\x98\x80\xF0",
            b"\xC2\xC2\xA9",
            b"\xEF\xBF\xBD\xEF\xBF",
            b"\xF5\x80\xBF\xF8",
        ];
        for bytes in samples {
            assert_eq!(decode(bytes), String::from_utf8_lossy(bytes), "{bytes:?}");
        }
    }
}
