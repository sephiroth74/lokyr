//! How many cells a character, or a grapheme cluster, takes.

use super::{EastAsianWidth, GraphemeBreak, Mark, Properties, properties};

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
    /// one, and the Hangul fillers, which take the cells of their East Asian
    /// width. Wide and fullwidth characters take two, and so do the
    /// ambiguous ones when they count as wide. In the unicode mode with the
    /// current tables, as `wcwidth` 0.9 counts them where Markus Kuhn's
    /// does not, a regional indicator takes two, alone as in a flag, and a
    /// prepended concatenation mark, a format character that shows over the
    /// digits after it, as the Arabic number signs, takes one.
    pub fn char_width(self, c: char) -> u8 {
        self.char_width_with(c, properties(c))
    }

    /// The cells that `c`, whose properties are `properties`, takes on its
    /// own, as [`Widths::char_width`] counts them, for a caller that has
    /// looked them up already.
    #[inline]
    pub fn char_width_with(self, c: char, properties: Properties) -> u8 {
        // The Hangul fillers are default ignorable, but `wcwidth` gives them
        // the cells that fonts draw them in.
        let filler = matches!(c, '\u{115F}' | '\u{3164}' | '\u{FFA0}');
        let zero = matches!(c, '\0'..='\u{1F}' | '\u{7F}'..='\u{9F}' | '\u{1160}'..='\u{11FF}' | '\u{D7B0}'..='\u{D7FF}')
            || matches!(properties.mark, Some(Mark::Nonspacing | Mark::Enclosing))
            || properties.format
            || (properties.default_ignorable && !filler);
        if c == '\u{AD}' {
            return 1;
        }
        if zero {
            return if properties.format {
                self.format_width(c)
            } else {
                0
            };
        }
        let east_asian = match self.tables {
            Tables::Current => properties.east_asian_width,
            Tables::Unicode8 => properties.legacy_east_asian_width,
        };
        match east_asian {
            EastAsianWidth::Wide => 2,
            EastAsianWidth::Ambiguous if self.ambiguous_wide => 2,
            // A regional indicator's East Asian width is neutral.
            EastAsianWidth::Ambiguous | EastAsianWidth::Narrow
                if properties.grapheme_break == GraphemeBreak::RegionalIndicator =>
            {
                self.regional_indicator_width()
            }
            EastAsianWidth::Ambiguous | EastAsianWidth::Narrow => 1,
        }
    }

    /// Whether the unicode mode counts with the current tables, which give
    /// some characters the cells of `wcwidth` 0.9 rather than Markus Kuhn's.
    fn counts_as_current(self) -> bool {
        self.mode == Mode::Unicode && self.tables == Tables::Current
    }

    /// The cells of `c`, a format character: one for a prepended
    /// concatenation mark, which shows, with the current tables of the
    /// unicode mode; none otherwise. Apart, out of the way of the text that
    /// most programs print.
    #[cold]
    #[inline(never)]
    fn format_width(self, c: char) -> u8 {
        u8::from(is_prepended_concatenation_mark(c) && self.counts_as_current())
    }

    /// The cells of a regional indicator: two with the current tables of the
    /// unicode mode, one otherwise.
    #[cold]
    #[inline(never)]
    fn regional_indicator_width(self) -> u8 {
        if self.counts_as_current() { 2 } else { 1 }
    }

    /// The cells that a grapheme cluster takes: `first`, its first
    /// codepoint, then `rest`, as a page keeps them.
    ///
    /// A cluster is as wide as its first codepoint that has a width, its
    /// base, which is its first codepoint unless a prepended character comes
    /// before it, except where emoji presentation changes it: VS16 after a
    /// pictographic or a keycap base makes two cells, VS15 one; a pair of
    /// regional indicators, a flag, makes two; and so does an emoji ZWJ
    /// sequence. Past emoji presentation, a codepoint after the base that
    /// takes cells of its own, [`Widths::spaces_in_cluster`], makes two: a
    /// vowel sign that spaces, as Bengali's া after ক, or a consonant that a
    /// virama joins, as in Devanagari's क्ष, as `wcwidth` 0.9 and the
    /// programs that count codepoint by codepoint give them. The tables of
    /// Unicode 8 knew no emoji presentation, so with them a cluster is always
    /// as wide as its first codepoint that has a width.
    pub fn cluster_width(self, first: char, rest: &[char]) -> u8 {
        // The base, the first codepoint that has a width, and what follows
        // it, which emoji presentation reads.
        let mut first_properties = properties(first);
        let mut width = self.char_width_with(first, first_properties);
        let (mut first, mut rest) = (first, rest);
        if width == 0 {
            let Some(at) = rest.iter().position(|&c| self.char_width(c) > 0) else {
                return 0;
            };
            (first, rest) = (rest[at], &rest[at + 1..]);
            first_properties = properties(first);
            width = self.char_width_with(first, first_properties);
        }
        if self.tables == Tables::Unicode8 {
            return width;
        }
        let pictographic = first_properties.extended_pictographic;
        let emoji = pictographic || is_keycap_base(first);
        if emoji && rest.contains(&EMOJI_PRESENTATION) {
            return 2;
        }
        if emoji && rest.contains(&TEXT_PRESENTATION) {
            return 1;
        }
        if is_flag(first) && rest.first().copied().is_some_and(is_flag) {
            return 2;
        }
        let joined = || {
            rest.windows(2)
                .any(|pair| pair[0] == ZWJ && properties(pair[1]).extended_pictographic)
        };
        if pictographic && joined() {
            return 2;
        }
        if width == 1
            && rest
                .iter()
                .any(|&c| self.spaces_in_cluster(c, properties(c)))
        {
            return 2;
        }
        width
    }

    /// Whether `c`, whose properties are `properties`, takes cells of its
    /// own after the base of a cluster, which then takes two: a codepoint
    /// that has a width, but an emoji modifier, which colours the emoji
    /// before it, and a prepended character, as `uucode` and Ghostty count
    /// them. Marks that do not space, joiners, variation selectors and the
    /// Hangul vowels and final consonants have no width.
    pub fn spaces_in_cluster(self, c: char, properties: Properties) -> bool {
        spaces_with_width(self.char_width_with(c, properties), c, properties)
    }

    /// The cells that a cluster takes once `c`, whose properties are
    /// `properties`, joins it, from `cells`, one or two, as a terminal that
    /// reads each codepoint as it comes counts them, as Ghostty does: a
    /// codepoint that takes cells of its own, [`Widths::spaces_in_cluster`],
    /// makes two; VS16 after a pictographic or a keycap base makes two, and
    /// VS15 after one makes one, `base` giving the cluster's base for them
    /// alone; anything else leaves it as many. The tables of Unicode 8 knew
    /// no emoji presentation, and leave it as many too. A cluster built a
    /// codepoint at a time takes the cells of [`Widths::cluster_width`] but
    /// where the room for a second cell lacked as it widened.
    pub fn joined_cells(
        self,
        cells: u8,
        c: char,
        properties: Properties,
        base: impl FnOnce() -> char,
    ) -> u8 {
        self.joined_cells_of(
            cells,
            c,
            properties,
            self.char_width_with(c, properties),
            base,
        )
    }

    /// [`Widths::joined_cells`], for a caller that has `width`, the cells
    /// that `c` takes on its own, as [`Widths::char_width_with`] gives them.
    #[inline]
    pub fn joined_cells_of(
        self,
        cells: u8,
        c: char,
        properties: Properties,
        width: u8,
        base: impl FnOnce() -> char,
    ) -> u8 {
        if self.tables == Tables::Unicode8 {
            return cells;
        }
        match c {
            EMOJI_PRESENTATION | TEXT_PRESENTATION => {
                let base = base();
                if is_emoji_base(base) {
                    if c == EMOJI_PRESENTATION { 2 } else { 1 }
                } else {
                    cells
                }
            }
            _ if spaces_with_width(width, c, properties) => 2,
            _ => cells,
        }
    }

    /// Whether `c`, whose properties are `properties`, joining a cluster
    /// that takes `cells` cells, one or two, as [`Widths::cluster_width`]
    /// gives them, leaves it as many: a wide cluster narrows only with VS15;
    /// a narrow one whose first codepoint has a width widens only with
    /// VS16, with a regional indicator that makes a flag, with a
    /// pictographic that a ZWJ joins, or with a codepoint that takes cells
    /// of its own in it.
    pub fn keeps_cells(self, cells: u8, c: char, properties: Properties) -> bool {
        self.keeps_cells_of(cells, c, properties, self.char_width_with(c, properties))
    }

    /// [`Widths::keeps_cells`], for a caller that has `width`, the cells
    /// that `c` takes on its own, as [`Widths::char_width_with`] gives them.
    #[inline]
    pub fn keeps_cells_of(self, cells: u8, c: char, properties: Properties, width: u8) -> bool {
        if cells == 2 {
            c != TEXT_PRESENTATION
        } else {
            c != EMOJI_PRESENTATION
                && properties.grapheme_break != GraphemeBreak::RegionalIndicator
                && !properties.extended_pictographic
                && !spaces_with_width(width, c, properties)
        }
    }
}

/// Whether `c`, which takes `width` cells on its own, takes cells of its
/// own after a cluster's base, as [`Widths::spaces_in_cluster`] tells.
#[inline]
fn spaces_with_width(width: u8, c: char, properties: Properties) -> bool {
    width > 0
        && !matches!(c, '\u{1F3FB}'..='\u{1F3FF}')
        && properties.grapheme_break != GraphemeBreak::Prepend
}

/// Whether emoji presentation reads `base`: a pictographic, or the base of
/// a keycap.
fn is_emoji_base(base: char) -> bool {
    properties(base).extended_pictographic || is_keycap_base(base)
}

/// The characters that a keycap sequence starts with: `#`, `*` and digits.
fn is_keycap_base(c: char) -> bool {
    matches!(c, '#' | '*' | '0'..='9')
}

/// The format characters that show, over the digits after them, which
/// `PropList.txt` of Unicode 18 lists as Prepended_Concatenation_Mark
/// (<https://www.unicode.org/Public/18.0.0/ucd/PropList.txt>).
fn is_prepended_concatenation_mark(c: char) -> bool {
    matches!(
        c,
        '\u{600}'..='\u{605}'
            | '\u{6DD}'
            | '\u{70F}'
            | '\u{890}'..='\u{891}'
            | '\u{8E2}'
            | '\u{110BD}'
            | '\u{110CD}'
    )
}

fn is_flag(c: char) -> bool {
    matches!(c, '\u{1F1E6}'..='\u{1F1FF}')
}
