//! The metadata of a row.

use std::fmt;

/// What a row's metadata can say about it.
///
/// The flags about the row's cells say that some cell may need extra work,
/// so that the hot paths can skip the rows without it. They are reset when
/// the row is cleared, not when the last such cell goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum RowFlag {
    /// The text goes on to the next row: it wrapped at the right margin.
    SoftWrapped = 1 << 0,
    /// The row goes on from the previous one, which wrapped.
    Continuation = 1 << 1,
    /// A prompt starts on the row, as semantic prompts (OSC 133) mark it.
    PromptStart = 1 << 2,
    /// The row changed since it was last drawn.
    Dirty = 1 << 3,
    /// A cell may have a grapheme of several codepoints.
    Graphemes = 1 << 4,
    /// A cell may be part of a hyperlink.
    Hyperlinks = 1 << 5,
    /// A cell may show part of an image.
    Images = 1 << 6,
    /// A cell may have a style other than the default.
    Styles = 1 << 7,
}

/// Every flag, in the order of their bits.
const FLAGS: [RowFlag; 8] = [
    RowFlag::SoftWrapped,
    RowFlag::Continuation,
    RowFlag::PromptStart,
    RowFlag::Dirty,
    RowFlag::Graphemes,
    RowFlag::Hyperlinks,
    RowFlag::Images,
    RowFlag::Styles,
];

/// The metadata of a row, in one byte.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub struct Row(u8);

impl Row {
    /// Whether the row has `flag`.
    pub const fn has(self, flag: RowFlag) -> bool {
        self.0 & flag as u8 != 0
    }

    /// Sets or clears `flag`.
    pub fn set(&mut self, flag: RowFlag, on: bool) {
        if on {
            self.0 |= flag as u8;
        } else {
            self.0 &= !(flag as u8);
        }
    }
}

/// Shows the flags that are set.
impl fmt::Debug for Row {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_set()
            .entries(FLAGS.iter().filter(|&&flag| self.has(flag)))
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_flag_has_a_bit_of_its_own() {
        for (bit, flag) in FLAGS.into_iter().enumerate() {
            assert_eq!(flag as u8, 1 << bit);
            let mut row = Row::default();
            row.set(flag, true);
            assert_eq!(
                FLAGS.map(|other| row.has(other)),
                FLAGS.map(|other| other == flag)
            );
            row.set(flag, false);
            assert_eq!(row, Row::default());
        }
    }

    #[test]
    fn debug_shows_the_flags_set() {
        let mut row = Row::default();
        row.set(RowFlag::Dirty, true);
        row.set(RowFlag::SoftWrapped, true);
        assert_eq!(format!("{row:?}"), "{SoftWrapped, Dirty}");
    }
}
