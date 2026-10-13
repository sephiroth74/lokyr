//! Grapheme cluster boundaries, as UAX #29 defines them
//! (<https://www.unicode.org/reports/tr29/>), found one codepoint at a time.

use super::{
    GraphemeBreak, IndicConjunctBreak, grapheme_break, indic_conjunct_break,
    is_extended_pictographic,
};

/// Finds where grapheme clusters start, from the codepoints given to it one
/// after the other, as text arrives.
///
/// It keeps what the rules need to remember of the codepoints before: the
/// last one, how many regional indicators came in a row, and whether an
/// emoji ZWJ sequence or an Indic conjunct is under way.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Segmenter {
    previous: Option<GraphemeBreak>,
    /// Whether an odd number of regional indicators came last.
    odd_flags: bool,
    emoji: Emoji,
    /// Whether an Indic linker came last, or only extenders after it, so
    /// that a consonant joins (rule GB9c).
    after_linker: bool,
}

/// How far an emoji ZWJ sequence has come (rule GB11).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Emoji {
    #[default]
    None,
    /// An Extended_Pictographic, then Extend characters.
    Pictographic,
    /// The same, then a ZWJ: a pictographic joins.
    Joiner,
}

impl Segmenter {
    /// A segmenter at the start of the text.
    pub fn new() -> Self {
        Self::default()
    }

    /// Forgets the codepoints before, as at the start of the text.
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Whether `c` starts a new grapheme cluster, rather than joining the
    /// cluster of the codepoint before it.
    pub fn starts_cluster(&mut self, c: char) -> bool {
        use GraphemeBreak::*;

        let class = grapheme_break(c);
        let conjunct = indic_conjunct_break(c);
        let pictographic = is_extended_pictographic(c);

        let starts = match self.previous {
            // GB1: the start of the text.
            None => true,
            Some(previous) => match (previous, class) {
                // GB3, then GB4 and GB5.
                (CR, LF) => false,
                (Control | CR | LF, _) | (_, Control | CR | LF) => true,
                // GB6, GB7, GB8: Hangul syllables.
                (L, L | V | LV | LVT) | (LV | V, V | T) | (LVT | T, T) => false,
                // GB9, GB9a, GB9b.
                (_, Extend | ZWJ | SpacingMark) | (Prepend, _) => false,
                // GB9c: a consonant after a linker and extenders.
                _ if conjunct == IndicConjunctBreak::Consonant && self.after_linker => false,
                // GB11: a pictographic after a pictographic, extenders and a ZWJ.
                _ if pictographic && self.emoji == Emoji::Joiner => false,
                // GB12 and GB13: regional indicators pair up.
                (RegionalIndicator, RegionalIndicator) => !self.odd_flags,
                // GB999.
                _ => true,
            },
        };

        self.odd_flags =
            class == RegionalIndicator && !(previous_is_flag(self.previous) && self.odd_flags);
        self.emoji = match (self.emoji, class) {
            _ if pictographic => Emoji::Pictographic,
            (Emoji::Pictographic, Extend) => Emoji::Pictographic,
            (Emoji::Pictographic, ZWJ) => Emoji::Joiner,
            _ => Emoji::None,
        };
        self.after_linker = conjunct == IndicConjunctBreak::Linker
            || (self.after_linker && conjunct == IndicConjunctBreak::Extend);
        self.previous = Some(class);
        starts
    }
}

fn previous_is_flag(previous: Option<GraphemeBreak>) -> bool {
    previous == Some(GraphemeBreak::RegionalIndicator)
}
