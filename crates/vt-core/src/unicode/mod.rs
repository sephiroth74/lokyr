//! Unicode for the grid: grapheme clusters, and how many cells each one
//! takes.
//!
//! The properties come from tables of the Unicode Character Database,
//! which `cargo xtask ucd` generates for a pinned version of Unicode.

mod segment;
#[rustfmt::skip]
mod tables;
mod width;

pub use segment::Segmenter;
pub use tables::UNICODE_VERSION;
pub use width::{Mode, Tables, Widths};

/// How wide a character is in East Asian typography (UAX #11), as far as
/// the grid cares.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EastAsianWidth {
    /// Narrow, Halfwidth, Neutral: one cell.
    Narrow,
    /// Wide or Fullwidth: two cells.
    Wide,
    /// Ambiguous: one cell or two, depending on the context.
    Ambiguous,
}

/// The kind of a mark, from its General_Category.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mark {
    /// Mn, which combines with the character before it and takes no room.
    Nonspacing,
    /// Me, which encloses the character before it.
    Enclosing,
    /// Mc, which combines with the character before it and takes room.
    Spacing,
}

/// The Grapheme_Cluster_Break property of UAX #29.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(missing_docs)]
pub enum GraphemeBreak {
    Other,
    CR,
    LF,
    Control,
    Extend,
    ZWJ,
    RegionalIndicator,
    Prepend,
    SpacingMark,
    L,
    V,
    T,
    LV,
    LVT,
}

/// The Indic_Conjunct_Break property, which rule GB9c of UAX #29 uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(missing_docs)]
pub enum IndicConjunctBreak {
    None,
    Linker,
    Consonant,
    Extend,
}

/// The East_Asian_Width of `c` in the pinned version of Unicode.
pub fn east_asian_width(c: char) -> EastAsianWidth {
    find(tables::EAST_ASIAN_WIDTH, c).unwrap_or(EastAsianWidth::Narrow)
}

/// The East_Asian_Width of `c` in Unicode 8, for the programs that predate
/// Unicode 9, which made most emoji wide.
pub fn legacy_east_asian_width(c: char) -> EastAsianWidth {
    find(tables::LEGACY_EAST_ASIAN_WIDTH, c).unwrap_or(EastAsianWidth::Narrow)
}

/// The kind of mark that `c` is, if it is one.
pub fn mark(c: char) -> Option<Mark> {
    find(tables::MARKS, c)
}

/// Whether `c` is a format character (General_Category Cf), such as ZWJ.
pub fn is_format(c: char) -> bool {
    contains(tables::FORMAT, c)
}

/// Whether `c` has Default_Ignorable_Code_Point: a character that shows
/// nothing unless a font supports it, such as a variation selector.
pub fn is_default_ignorable(c: char) -> bool {
    contains(tables::DEFAULT_IGNORABLE, c)
}

/// Whether `c` has Emoji_Presentation: it shows as an emoji by default.
pub fn has_emoji_presentation(c: char) -> bool {
    contains(tables::EMOJI_PRESENTATION, c)
}

/// Whether `c` has Extended_Pictographic, which emoji ZWJ sequences use.
pub fn is_extended_pictographic(c: char) -> bool {
    contains(tables::EXTENDED_PICTOGRAPHIC, c)
}

/// The Grapheme_Cluster_Break of `c`.
pub fn grapheme_break(c: char) -> GraphemeBreak {
    find(tables::GRAPHEME_BREAK, c).unwrap_or(GraphemeBreak::Other)
}

/// The Indic_Conjunct_Break of `c`.
pub fn indic_conjunct_break(c: char) -> IndicConjunctBreak {
    find(tables::INDIC_CONJUNCT_BREAK, c).unwrap_or(IndicConjunctBreak::None)
}

/// The value of the range of `table` that holds `c`.
fn find<T: Copy>(table: &[(u32, u32, T)], c: char) -> Option<T> {
    let c = u32::from(c);
    let index = table
        .binary_search_by(|&(start, end, _)| {
            if end < c {
                std::cmp::Ordering::Less
            } else if start > c {
                std::cmp::Ordering::Greater
            } else {
                std::cmp::Ordering::Equal
            }
        })
        .ok()?;
    Some(table[index].2)
}

/// Whether a range of `table` holds `c`.
fn contains(table: &[(u32, u32)], c: char) -> bool {
    let c = u32::from(c);
    table
        .binary_search_by(|&(start, end)| {
            if end < c {
                std::cmp::Ordering::Less
            } else if start > c {
                std::cmp::Ordering::Greater
            } else {
                std::cmp::Ordering::Equal
            }
        })
        .is_ok()
}

#[cfg(test)]
mod tests;
