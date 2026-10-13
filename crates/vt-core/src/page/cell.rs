//! The cell: what one column of a row holds, packed in 64 bits.

use super::{Rgb, StyleId};

/// One column of a row, packed in 64 bits.
///
/// | Bits  | Field                                                   |
/// |-------|---------------------------------------------------------|
/// | 0–1   | Content tag                                             |
/// | 2–25  | Content: a codepoint, a palette index or an RGB color   |
/// | 26–41 | Style id                                                |
/// | 42–43 | Width                                                   |
/// | 44    | Protected                                               |
/// | 45    | Has a hyperlink                                         |
/// | 46–47 | Semantic content                                        |
/// | 48–63 | Reserved, always 0                                      |
///
/// What does not fit lives in tables of the page, keyed by the cell's
/// position: the other codepoints of a grapheme, and the hyperlink.
///
/// The fields are read and written by value, so that the cell stays one
/// word: `Cell::EMPTY.with_content(Content::Char('a'))`.
#[derive(Clone, Copy, Default, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct Cell(u64);

// A page is mostly cells, so their size is the size of the grid.
const _: () = assert!(size_of::<Cell>() == 8);

/// What a cell shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Content {
    /// A character. U+0000 means the cell is empty.
    Char(char),
    /// The first codepoint of a grapheme cluster, whose other codepoints live
    /// in the page's grapheme table.
    Grapheme(char),
    /// No character, only a background color from the palette, as an erase
    /// leaves with background color erase. It takes no style.
    BgPalette(u8),
    /// No character, only an RGB background color.
    BgRgb(Rgb),
}

/// How many columns a cell's character takes, and which of them the cell is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Width {
    /// A character that takes one column.
    #[default]
    Narrow,
    /// The first column of a character that takes two.
    Wide,
    /// The second column of a wide character, which holds nothing itself.
    SpacerTail,
    /// The last column of a row, left empty because the wide character that
    /// came next did not fit and went to the next row.
    SpacerHead,
}

/// What part of a shell's output a cell belongs to, as semantic prompts
/// (OSC 133) mark it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Semantic {
    /// The output of a command, and anything unmarked.
    #[default]
    Output,
    /// The prompt.
    Prompt,
    /// The command that the user typed.
    Input,
}

const TAG_SHIFT: u32 = 0;
const CONTENT_SHIFT: u32 = 2;
const STYLE_SHIFT: u32 = 26;
const WIDTH_SHIFT: u32 = 42;
const PROTECTED_SHIFT: u32 = 44;
const HYPERLINK_SHIFT: u32 = 45;
const SEMANTIC_SHIFT: u32 = 46;

const TAG_CHAR: u64 = 0;
const TAG_GRAPHEME: u64 = 1;
const TAG_BG_PALETTE: u64 = 2;
const TAG_BG_RGB: u64 = 3;

impl Cell {
    /// An empty cell: no character, in the default style, narrow, neither
    /// protected nor a link, and part of the output.
    pub const EMPTY: Self = Self(0);

    /// What the cell shows.
    pub const fn content(self) -> Content {
        let content = self.field(CONTENT_SHIFT, 24);
        // Only a valid character is ever stored as a codepoint.
        let codepoint = match char::from_u32(content as u32) {
            Some(c) => c,
            None => char::REPLACEMENT_CHARACTER,
        };
        match self.field(TAG_SHIFT, 2) {
            TAG_CHAR => Content::Char(codepoint),
            TAG_GRAPHEME => Content::Grapheme(codepoint),
            TAG_BG_PALETTE => Content::BgPalette(content as u8),
            _ => Content::BgRgb(Rgb::new(
                (content >> 16) as u8,
                (content >> 8) as u8,
                content as u8,
            )),
        }
    }

    /// The cell with `content`.
    #[must_use]
    pub const fn with_content(self, content: Content) -> Self {
        let (tag, value) = match content {
            Content::Char(c) => (TAG_CHAR, c as u64),
            Content::Grapheme(c) => (TAG_GRAPHEME, c as u64),
            Content::BgPalette(index) => (TAG_BG_PALETTE, index as u64),
            Content::BgRgb(Rgb { r, g, b }) => {
                (TAG_BG_RGB, (r as u64) << 16 | (g as u64) << 8 | b as u64)
            }
        };
        self.with_field(TAG_SHIFT, 2, tag)
            .with_field(CONTENT_SHIFT, 24, value)
    }

    /// Whether the cell has no character and no background of its own.
    pub const fn is_empty(self) -> bool {
        self.field(TAG_SHIFT, 26) == 0
    }

    /// The style of the cell, in its page's style table.
    pub const fn style(self) -> StyleId {
        StyleId::new(self.field(STYLE_SHIFT, 16) as u16)
    }

    /// The cell with `style`.
    #[must_use]
    pub const fn with_style(self, style: StyleId) -> Self {
        self.with_field(STYLE_SHIFT, 16, style.get() as u64)
    }

    /// How many columns the cell's character takes, and which one this is.
    pub const fn width(self) -> Width {
        match self.field(WIDTH_SHIFT, 2) {
            0 => Width::Narrow,
            1 => Width::Wide,
            2 => Width::SpacerTail,
            _ => Width::SpacerHead,
        }
    }

    /// The cell with `width`.
    #[must_use]
    pub const fn with_width(self, width: Width) -> Self {
        self.with_field(WIDTH_SHIFT, 2, width as u64)
    }

    /// Whether the selective erases keep the cell (DECSCA).
    pub const fn is_protected(self) -> bool {
        self.field(PROTECTED_SHIFT, 1) != 0
    }

    /// The cell, protected or not.
    #[must_use]
    pub const fn with_protected(self, protected: bool) -> Self {
        self.with_field(PROTECTED_SHIFT, 1, protected as u64)
    }

    /// Whether the cell is part of a hyperlink (OSC 8), which the page's
    /// hyperlink table holds.
    pub const fn has_hyperlink(self) -> bool {
        self.field(HYPERLINK_SHIFT, 1) != 0
    }

    /// The cell, part of a hyperlink or not.
    #[must_use]
    pub const fn with_hyperlink(self, hyperlink: bool) -> Self {
        self.with_field(HYPERLINK_SHIFT, 1, hyperlink as u64)
    }

    /// What part of a shell's output the cell belongs to.
    pub const fn semantic(self) -> Semantic {
        // The fourth value is never stored.
        match self.field(SEMANTIC_SHIFT, 2) {
            1 => Semantic::Prompt,
            2 => Semantic::Input,
            _ => Semantic::Output,
        }
    }

    /// The cell with `semantic`.
    #[must_use]
    pub const fn with_semantic(self, semantic: Semantic) -> Self {
        self.with_field(SEMANTIC_SHIFT, 2, semantic as u64)
    }

    /// The `bits` bits from `shift` up.
    const fn field(self, shift: u32, bits: u32) -> u64 {
        (self.0 >> shift) & ((1 << bits) - 1)
    }

    /// The cell with `value` in the `bits` bits from `shift` up.
    const fn with_field(self, shift: u32, bits: u32, value: u64) -> Self {
        let mask = ((1 << bits) - 1) << shift;
        Self((self.0 & !mask) | ((value << shift) & mask))
    }
}

impl std::fmt::Debug for Cell {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Cell")
            .field("content", &self.content())
            .field("style", &self.style())
            .field("width", &self.width())
            .field("protected", &self.is_protected())
            .field("hyperlink", &self.has_hyperlink())
            .field("semantic", &self.semantic())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    fn content() -> impl Strategy<Value = Content> {
        prop_oneof![
            any::<char>().prop_map(Content::Char),
            any::<char>().prop_map(Content::Grapheme),
            any::<u8>().prop_map(Content::BgPalette),
            any::<(u8, u8, u8)>().prop_map(|(r, g, b)| Content::BgRgb(Rgb::new(r, g, b))),
        ]
    }

    fn width() -> impl Strategy<Value = Width> {
        prop::sample::select(
            &[
                Width::Narrow,
                Width::Wide,
                Width::SpacerTail,
                Width::SpacerHead,
            ][..],
        )
    }

    fn semantic() -> impl Strategy<Value = Semantic> {
        prop::sample::select(&[Semantic::Output, Semantic::Prompt, Semantic::Input][..])
    }

    /// Every field of a cell.
    type Fields = (Content, StyleId, Width, bool, bool, Semantic);

    fn fields() -> impl Strategy<Value = Fields> {
        (
            content(),
            any::<u16>().prop_map(StyleId::new),
            width(),
            any::<bool>(),
            any::<bool>(),
            semantic(),
        )
    }

    fn read(cell: Cell) -> Fields {
        (
            cell.content(),
            cell.style(),
            cell.width(),
            cell.is_protected(),
            cell.has_hyperlink(),
            cell.semantic(),
        )
    }

    /// `cell` with every field of `fields`, written in the order of `order`.
    fn write(cell: Cell, fields: Fields, order: &[usize]) -> Cell {
        let (content, style, width, protected, hyperlink, semantic) = fields;
        order.iter().fold(cell, |cell, &field| match field {
            0 => cell.with_content(content),
            1 => cell.with_style(style),
            2 => cell.with_width(width),
            3 => cell.with_protected(protected),
            4 => cell.with_hyperlink(hyperlink),
            _ => cell.with_semantic(semantic),
        })
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(4096))]

        #[test]
        fn every_field_round_trips(
            before in fields(),
            after in fields(),
            order in Just(vec![0, 1, 2, 3, 4, 5]).prop_shuffle(),
        ) {
            // Write over a cell that already holds other values, so that a
            // field that leaks into another one shows.
            let cell = write(Cell::EMPTY, before, &order);
            prop_assert_eq!(read(cell), before);
            let cell = write(cell, after, &order);
            prop_assert_eq!(read(cell), after);
            prop_assert_eq!(cell.0 >> 48, 0, "the reserved bits stay 0");
        }
    }

    #[test]
    fn the_empty_cell_is_all_defaults() {
        assert_eq!(
            read(Cell::EMPTY),
            (
                Content::Char('\0'),
                StyleId::DEFAULT,
                Width::Narrow,
                false,
                false,
                Semantic::Output,
            )
        );
        assert!(Cell::EMPTY.is_empty());
        assert_eq!(Cell::default(), Cell::EMPTY);
    }

    #[test]
    fn only_a_cell_without_content_is_empty() {
        let styled = Cell::EMPTY.with_style(StyleId::new(3)).with_protected(true);
        assert!(styled.is_empty());
        for content in [
            Content::Char(' '),
            Content::Grapheme('\0'),
            Content::BgPalette(0),
            Content::BgRgb(Rgb::new(0, 0, 0)),
        ] {
            assert!(!styled.with_content(content).is_empty(), "{content:?}");
        }
    }

    #[test]
    fn the_extremes_of_each_field_fit() {
        let cell = Cell::EMPTY
            .with_content(Content::Grapheme(char::MAX))
            .with_style(StyleId::new(u16::MAX))
            .with_width(Width::SpacerHead)
            .with_protected(true)
            .with_hyperlink(true)
            .with_semantic(Semantic::Input);
        // The layout of the table in the documentation of `Cell`.
        let layout = 1 | 0x10_FFFF << 2 | 0xFFFF << 26 | 3 << 42 | 1 << 44 | 1 << 45 | 2 << 46;
        assert_eq!(cell.0, layout);
        let cell = cell.with_content(Content::BgRgb(Rgb::new(0xFF, 0xFF, 0xFF)));
        assert_eq!(cell.content(), Content::BgRgb(Rgb::new(0xFF, 0xFF, 0xFF)));
    }

    #[test]
    fn debug_shows_the_fields() {
        let cell = Cell::EMPTY.with_content(Content::Char('a'));
        assert_eq!(
            format!("{cell:?}"),
            "Cell { content: Char('a'), style: StyleId(0), width: Narrow, \
             protected: false, hyperlink: false, semantic: Output }"
        );
    }
}
