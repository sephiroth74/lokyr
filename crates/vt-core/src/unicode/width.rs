//! How many cells a character, or a grapheme cluster, takes.

use super::{
    EastAsianWidth, Mark, east_asian_width, is_default_ignorable, is_extended_pictographic,
    is_format, legacy_east_asian_width, mark,
};

/// How text splits into cells.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mode {
    /// A cell per grapheme cluster, as UAX #29 segments them, as wide as the
    /// whole cluster.
    #[default]
    Unicode,
    /// A cell per codepoint, as `wcwidth` counts them, for programs that
    /// compute widths themselves. A codepoint of no width joins the cell
    /// before it.
    Legacy,
}

/// Which width tables to follow.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tables {
    /// Those of the Unicode version of the tables.
    #[default]
    Current,
    /// Those of Unicode 8, for programs that predate Unicode 9, which gave
    /// most emoji two cells.
    Unicode8,
}

/// How the terminal counts the cells of text, which must match how the
/// programs count them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Widths {
    /// How text splits into cells.
    pub mode: Mode,
    /// Which width tables to follow.
    pub tables: Tables,
    /// Whether East Asian Ambiguous characters take two cells rather than
    /// one.
    pub ambiguous_wide: bool,
}

/// VARIATION SELECTOR-15, which asks for text presentation.
const TEXT_PRESENTATION: char = '\u{FE0E}';

/// VARIATION SELECTOR-16, which asks for emoji presentation.
const EMOJI_PRESENTATION: char = '\u{FE0F}';

/// ZERO WIDTH JOINER, which builds emoji sequences.
const ZWJ: char = '\u{200D}';

impl Widths {
    /// The cells that `c` takes on its own: 0, 1 or 2, as `wcwidth` counts
    /// them.
    ///
    /// Controls, marks that do not space, format characters, default
    /// ignorable characters, and the Hangul vowels and final consonants that
    /// combine into syllables take none, except SOFT HYPHEN, which takes
    /// one. Wide and fullwidth characters take two, and so do the ambiguous
    /// ones when they count as wide.
    pub fn char_width(self, c: char) -> u8 {
        let zero = matches!(c, '\0'..='\u{1F}' | '\u{7F}'..='\u{9F}' | '\u{1160}'..='\u{11FF}' | '\u{D7B0}'..='\u{D7FF}')
            || matches!(mark(c), Some(Mark::Nonspacing | Mark::Enclosing))
            || is_format(c)
            || is_default_ignorable(c);
        if c == '\u{AD}' {
            return 1;
        }
        if zero {
            return 0;
        }
        let east_asian = match self.tables {
            Tables::Current => east_asian_width(c),
            Tables::Unicode8 => legacy_east_asian_width(c),
        };
        match east_asian {
            EastAsianWidth::Wide => 2,
            EastAsianWidth::Ambiguous if self.ambiguous_wide => 2,
            EastAsianWidth::Ambiguous | EastAsianWidth::Narrow => 1,
        }
    }

    /// The cells that `cluster`, a grapheme cluster, takes.
    ///
    /// A cluster is as wide as its first codepoint, except where emoji
    /// presentation changes it: VS16 after a pictographic or a keycap base
    /// makes two cells, VS15 one; a pair of regional indicators, a flag,
    /// makes two; and so does an emoji ZWJ sequence. The tables of Unicode 8
    /// knew no emoji presentation, so with them a cluster is always as wide
    /// as its first codepoint.
    pub fn cluster_width(self, cluster: &[char]) -> u8 {
        let Some(&first) = cluster.first() else {
            return 0;
        };
        let width = self.char_width(first);
        if self.tables == Tables::Unicode8 {
            return width;
        }
        let emoji = is_extended_pictographic(first) || is_keycap_base(first);
        if emoji && cluster.contains(&EMOJI_PRESENTATION) {
            return 2;
        }
        if emoji && cluster.contains(&TEXT_PRESENTATION) {
            return 1;
        }
        if is_flag(first) && cluster.len() > 1 {
            return 2;
        }
        let joined = cluster
            .windows(2)
            .any(|pair| pair[0] == ZWJ && is_extended_pictographic(pair[1]));
        if is_extended_pictographic(first) && joined {
            return 2;
        }
        width
    }
}

/// The characters that a keycap sequence starts with: `#`, `*` and digits.
fn is_keycap_base(c: char) -> bool {
    matches!(c, '#' | '*' | '0'..='9')
}

fn is_flag(c: char) -> bool {
    matches!(c, '\u{1F1E6}'..='\u{1F1FF}')
}
