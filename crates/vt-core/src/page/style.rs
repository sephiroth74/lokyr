//! Styles, colors, and the style table of a page.

use super::TableFull;
use super::index::{Ids, Index, Slot};

/// A color in 8-bit RGB.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Rgb {
    /// Red.
    pub r: u8,
    /// Green.
    pub g: u8,
    /// Blue.
    pub b: u8,
}

impl Rgb {
    /// The color with these components.
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }
}

/// A color as programs choose it: the terminal's default, an entry of the
/// palette, or RGB.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Color {
    /// The default for where the color is used: the default foreground,
    /// background or underline color.
    #[default]
    Default,
    /// An entry of the 256-color palette.
    Palette(u8),
    /// An RGB color.
    Rgb(Rgb),
}

/// The kinds of underline, as SGR 4 and its sub-parameters choose them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Underline {
    /// No underline (SGR 24, or 4:0).
    #[default]
    None,
    /// A straight line (SGR 4, or 4:1).
    Single,
    /// Two lines (SGR 21, or 4:2).
    Double,
    /// A wavy line (4:3).
    Curly,
    /// A dotted line (4:4).
    Dotted,
    /// A dashed line (4:5).
    Dashed,
}

/// How text looks: what SGR sets.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Style {
    /// The text color.
    pub fg: Color,
    /// The background color.
    pub bg: Color,
    /// The underline color, which by default is the text color.
    pub underline_color: Color,
    /// The kind of underline.
    pub underline: Underline,
    /// Bold (SGR 1).
    pub bold: bool,
    /// Faint, or dim (SGR 2).
    pub faint: bool,
    /// Italic (SGR 3).
    pub italic: bool,
    /// Blinking (SGR 5 and 6).
    pub blink: bool,
    /// Foreground and background swapped (SGR 7).
    pub inverse: bool,
    /// Hidden (SGR 8).
    pub invisible: bool,
    /// Struck through (SGR 9).
    pub strikethrough: bool,
    /// Overlined (SGR 53).
    pub overline: bool,
}

impl Style {
    /// The style of text that SGR has not changed.
    pub const DEFAULT: Self = Self {
        fg: Color::Default,
        bg: Color::Default,
        underline_color: Color::Default,
        underline: Underline::None,
        bold: false,
        faint: false,
        italic: false,
        blink: false,
        inverse: false,
        invisible: false,
        strikethrough: false,
        overline: false,
    };

    /// Every field of the style, packed in 89 bits, for hashing.
    pub(crate) fn bits(&self) -> u128 {
        let color = |color: Color| -> u128 {
            match color {
                Color::Default => 0,
                Color::Palette(index) => 1 << 24 | u128::from(index),
                Color::Rgb(Rgb { r, g, b }) => {
                    2 << 24 | u128::from(r) << 16 | u128::from(g) << 8 | u128::from(b)
                }
            }
        };
        let flags = [
            self.bold,
            self.faint,
            self.italic,
            self.blink,
            self.inverse,
            self.invisible,
            self.strikethrough,
            self.overline,
        ]
        .iter()
        .enumerate()
        .fold(0, |flags, (bit, &on)| flags | u128::from(on) << bit);
        color(self.fg)
            | color(self.bg) << 26
            | color(self.underline_color) << 52
            | (self.underline as u128) << 78
            | flags << 81
    }
}

/// The id of a style in its page's style table.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct StyleId(u16);

impl StyleId {
    /// The default style, which every table has.
    pub const DEFAULT: Self = Self(0);

    pub(crate) const fn new(id: u16) -> Self {
        Self(id)
    }

    pub(crate) const fn get(self) -> u16 {
        self.0
    }
}

/// The styles of a page, deduplicated and reference-counted.
///
/// Cells hold a [`StyleId`] rather than a style. A cell that takes a style
/// adds a reference to it, and one that leaves it drops the reference; the
/// last one frees its id. The default style has id 0, which is always there
/// and is not counted, so unstyled text costs nothing.
///
/// The capacity is fixed when the table is created, and the table never
/// allocates after that.
#[derive(Clone, Debug)]
pub struct StyleTable {
    /// The styles, by id. Free ids hold the default style.
    styles: Box<[Style]>,
    /// How many references each style has; 0 for a free id.
    refs: Box<[u32]>,
    ids: Ids,
    /// From a style to its id. The default style is not in it.
    index: Index,
    /// How many styles the table holds, the default one aside.
    len: usize,
}

impl StyleTable {
    /// A table with room for `capacity` styles besides the default one.
    pub fn new(capacity: u16) -> Self {
        let ids = usize::from(capacity) + 1;
        Self {
            styles: vec![Style::DEFAULT; ids].into_boxed_slice(),
            refs: vec![0; ids].into_boxed_slice(),
            ids: Ids::new(capacity),
            index: Index::new(capacity),
            len: 0,
        }
    }

    /// How many styles the table can hold, the default one aside.
    pub fn capacity(&self) -> usize {
        usize::from(self.ids.capacity())
    }

    /// How many styles the table holds, the default one aside.
    pub fn len(&self) -> usize {
        self.len
    }

    /// Whether the table holds only the default style.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// The bytes that a table with room for `capacity` styles allocates.
    pub fn memory_for(capacity: u16) -> usize {
        (usize::from(capacity) + 1) * (size_of::<Style>() + size_of::<u32>())
            + Ids::memory_for(capacity)
            + Index::memory_for(capacity)
    }

    /// The bytes that the table allocates.
    pub fn memory(&self) -> usize {
        size_of_val(&*self.styles)
            + size_of_val(&*self.refs)
            + self.ids.memory()
            + self.index.memory()
    }

    /// Forgets every style but the default one, without allocating.
    pub fn clear(&mut self) {
        self.styles.fill(Style::DEFAULT);
        self.refs.fill(0);
        self.ids.clear();
        self.index.clear();
        self.len = 0;
    }

    /// A copy of the table with room for `capacity` styles, at least as many
    /// as it has room for now. Every style keeps its id and its references.
    pub fn grown(&self, capacity: u16) -> Self {
        let mut table = Self::new(capacity.max(self.ids.capacity()));
        let ids = self.styles.len();
        table.styles[..ids].copy_from_slice(&self.styles);
        table.refs[..ids].copy_from_slice(&self.refs);
        table.ids = self.ids.grown(table.ids.capacity());
        table.len = self.len;
        for id in self.index.ids() {
            if let Some(Slot::Empty(slot)) = table.find(&self.styles[usize::from(id)]) {
                table.index.insert(slot, id);
            }
        }
        table
    }

    /// The style with `id`.
    pub fn get(&self, id: StyleId) -> &Style {
        debug_assert!(self.is_live(id), "style {id:?} is not in the table");
        &self.styles[usize::from(id.get())]
    }

    /// How many references the style with `id` has. The default style has
    /// none, since it is not counted.
    pub fn references(&self, id: StyleId) -> u32 {
        self.refs[usize::from(id.get())]
    }

    /// Adds a reference to `style`, which joins the table if it is not
    /// there yet, and returns its id.
    pub fn acquire(&mut self, style: &Style) -> Result<StyleId, TableFull> {
        if *style == Style::DEFAULT {
            return Ok(StyleId::DEFAULT);
        }
        let slot = match self.find(style) {
            Some(Slot::Taken(slot)) => {
                let id = self.index.id(slot);
                self.refs[usize::from(id)] += 1;
                return Ok(StyleId::new(id));
            }
            Some(Slot::Empty(slot)) => slot,
            None => return Err(TableFull::Styles),
        };
        let id = self.ids.take().ok_or(TableFull::Styles)?;
        self.styles[usize::from(id)] = *style;
        self.refs[usize::from(id)] = 1;
        self.index.insert(slot, id);
        self.len += 1;
        Ok(StyleId::new(id))
    }

    /// Adds `n` references to the style with `id`, as when `n` cells take
    /// it at once.
    pub fn retain_many(&mut self, id: StyleId, n: u32) {
        if id != StyleId::DEFAULT {
            debug_assert!(self.is_live(id), "style {id:?} is not in the table");
            self.refs[usize::from(id.get())] += n;
        }
    }

    /// Whether `id` is in use for `style`: a check that costs less than
    /// finding the style, for an id that it is likely to have.
    pub fn holds(&self, id: StyleId, style: &Style) -> bool {
        let id = usize::from(id.get());
        self.refs.get(id).is_some_and(|&refs| refs > 0) && self.styles[id] == *style
    }

    /// Adds a reference to the style with `id`, as when a cell is copied.
    pub fn retain(&mut self, id: StyleId) {
        if id != StyleId::DEFAULT {
            debug_assert!(self.is_live(id), "style {id:?} is not in the table");
            self.refs[usize::from(id.get())] += 1;
        }
    }

    /// Drops a reference to the style with `id`, and frees the id with the
    /// last one.
    #[inline]
    pub fn release(&mut self, id: StyleId) {
        if id == StyleId::DEFAULT {
            return;
        }
        debug_assert!(self.is_live(id), "style {id:?} is not in the table");
        let refs = &mut self.refs[usize::from(id.get())];
        *refs = refs.saturating_sub(1);
        if *refs == 0 {
            self.free(id);
        }
    }

    /// Takes the style with `id`, which has no reference left, out of the
    /// table.
    fn free(&mut self, id: StyleId) {
        let style = self.styles[usize::from(id.get())];
        if let Some(Slot::Taken(slot)) = self.find(&style) {
            let styles = &self.styles;
            self.index.remove(slot, |id| hash(&styles[usize::from(id)]));
        }
        self.styles[usize::from(id.get())] = Style::DEFAULT;
        self.ids.give(id.get());
        self.len -= 1;
    }

    fn is_live(&self, id: StyleId) -> bool {
        id == StyleId::DEFAULT
            || self
                .refs
                .get(usize::from(id.get()))
                .is_some_and(|&refs| refs > 0)
    }

    /// The slot of `style` in the index, or the empty slot where it would
    /// go.
    fn find(&self, style: &Style) -> Option<Slot> {
        self.index
            .find(hash(style), |id| self.styles[usize::from(id)] == *style)
    }
}

/// The hash of `style`: its packed bits, folded to 64.
fn hash(style: &Style) -> u64 {
    let bits = style.bits();
    (bits as u64) ^ ((bits >> 64) as u64).rotate_left(29)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use proptest::prelude::*;

    use super::*;

    fn bold(fg: u8) -> Style {
        Style {
            fg: Color::Palette(fg),
            bold: true,
            ..Style::DEFAULT
        }
    }

    #[test]
    fn the_default_style_is_always_there_and_never_counted() {
        let mut table = StyleTable::new(4);
        assert_eq!(table.acquire(&Style::DEFAULT), Ok(StyleId::DEFAULT));
        table.retain(StyleId::DEFAULT);
        table.release(StyleId::DEFAULT);
        table.release(StyleId::DEFAULT);
        assert_eq!(table.get(StyleId::DEFAULT), &Style::DEFAULT);
        assert_eq!(table.references(StyleId::DEFAULT), 0);
        assert!(table.is_empty());
        // Its fields pack to nothing, which a hash of cells takes for it.
        assert_eq!(Style::DEFAULT.bits(), 0);
    }

    #[test]
    fn equal_styles_share_an_id() {
        let mut table = StyleTable::new(4);
        let red = table.acquire(&bold(1)).unwrap();
        let green = table.acquire(&bold(2)).unwrap();
        assert_ne!(red, green);
        assert_eq!(table.acquire(&bold(1)), Ok(red));
        table.retain(red);
        assert_eq!(table.references(red), 3);
        assert_eq!(table.len(), 2);
        assert_eq!(table.get(red), &bold(1));
    }

    #[test]
    fn the_last_release_frees_the_style() {
        let mut table = StyleTable::new(4);
        let red = table.acquire(&bold(1)).unwrap();
        table.retain(red);
        table.release(red);
        assert_eq!(table.len(), 1);
        table.release(red);
        assert!(table.is_empty());
        // The freed id serves the next new style.
        assert_eq!(table.acquire(&bold(2)), Ok(red));
        assert_eq!(table.get(red), &bold(2));
    }

    #[test]
    fn a_full_table_still_shares_but_refuses_new_styles() {
        let mut table = StyleTable::new(3);
        let ids: Vec<StyleId> = (1..=3)
            .map(|fg| table.acquire(&bold(fg)).unwrap())
            .collect();
        assert_eq!(table.len(), table.capacity());
        assert_eq!(table.acquire(&bold(4)), Err(TableFull::Styles));
        assert_eq!(table.acquire(&bold(2)), Ok(ids[1]));
        assert_eq!(table.acquire(&Style::DEFAULT), Ok(StyleId::DEFAULT));
        // Freeing a style makes room.
        table.release(ids[0]);
        assert_eq!(table.acquire(&bold(4)), Ok(ids[0]));
    }

    #[test]
    fn a_table_without_capacity_holds_only_the_default_style() {
        let mut table = StyleTable::new(0);
        assert_eq!(table.acquire(&bold(1)), Err(TableFull::Styles));
        assert_eq!(table.acquire(&Style::DEFAULT), Ok(StyleId::DEFAULT));
    }

    #[test]
    fn a_grown_table_keeps_every_id_and_reference() {
        let mut table = StyleTable::new(3);
        let ids: Vec<StyleId> = (1..=3)
            .map(|fg| table.acquire(&bold(fg)).unwrap())
            .collect();
        table.retain(ids[2]);
        table.release(ids[0]);
        let mut grown = table.grown(8);
        assert_eq!(grown.capacity(), 8);
        assert_eq!(grown.len(), 2);
        for (fg, &id) in (1..).zip(&ids).skip(1) {
            assert_eq!(grown.get(id), &bold(fg));
            assert_eq!(
                grown.acquire(&bold(fg)),
                Ok(id),
                "the index knows the style"
            );
        }
        assert_eq!(grown.references(ids[2]), 3);
        // The freed id comes first, then the new room.
        assert_eq!(grown.acquire(&bold(9)), Ok(ids[0]));
        for fg in 10..15 {
            grown.acquire(&bold(fg)).unwrap();
        }
        assert_eq!(grown.acquire(&bold(15)), Err(TableFull::Styles));
    }

    #[test]
    fn memory_counts_what_the_table_allocates() {
        for capacity in [0, 1, 3, 255, 256, 1000, u16::MAX] {
            let table = StyleTable::new(capacity);
            assert_eq!(
                table.memory(),
                StyleTable::memory_for(capacity),
                "{capacity}"
            );
        }
    }

    #[test]
    fn every_field_tells_styles_apart() {
        let base = Style::DEFAULT;
        let rgb = Color::Rgb(Rgb::new(1, 2, 3));
        let variants = [
            Style {
                fg: Color::Palette(0),
                ..base
            },
            Style { fg: rgb, ..base },
            Style {
                bg: Color::Palette(0),
                ..base
            },
            Style { bg: rgb, ..base },
            Style {
                underline_color: rgb,
                ..base
            },
            Style {
                underline: Underline::Single,
                ..base
            },
            Style {
                underline: Underline::Double,
                ..base
            },
            Style {
                underline: Underline::Curly,
                ..base
            },
            Style {
                underline: Underline::Dotted,
                ..base
            },
            Style {
                underline: Underline::Dashed,
                ..base
            },
            Style { bold: true, ..base },
            Style {
                faint: true,
                ..base
            },
            Style {
                italic: true,
                ..base
            },
            Style {
                blink: true,
                ..base
            },
            Style {
                inverse: true,
                ..base
            },
            Style {
                invisible: true,
                ..base
            },
            Style {
                strikethrough: true,
                ..base
            },
            Style {
                overline: true,
                ..base
            },
        ];
        let mut table = StyleTable::new(32);
        let mut bits = Vec::new();
        for style in &variants {
            let id = table.acquire(style).unwrap();
            assert_eq!(table.get(id), style);
            bits.push(style.bits());
        }
        assert_eq!(table.len(), variants.len());
        bits.sort_unstable();
        bits.dedup();
        assert_eq!(
            bits.len(),
            variants.len(),
            "the packed bits tell them apart"
        );
    }

    /// A style from a small set, so that sequences of operations meet the
    /// same styles again.
    fn style() -> impl Strategy<Value = Style> {
        (0_u8..12, any::<bool>(), 0_u8..3).prop_map(|(fg, italic, underline)| Style {
            fg: Color::Palette(fg),
            italic,
            underline: [Underline::None, Underline::Curly, Underline::Dashed]
                [usize::from(underline)],
            ..Style::DEFAULT
        })
    }

    /// What a sequence of operations does: acquire a style, or release one
    /// of the references held so far.
    #[derive(Clone, Debug)]
    enum Op {
        Acquire(Style),
        Release(usize),
    }

    fn op() -> impl Strategy<Value = Op> {
        prop_oneof![
            3 => style().prop_map(Op::Acquire),
            2 => any::<usize>().prop_map(Op::Release),
        ]
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(1024))]

        #[test]
        fn the_table_matches_a_model(
            capacity in 0_u16..10,
            ops in prop::collection::vec(op(), 0..200),
        ) {
            let mut table = StyleTable::new(capacity);
            // The references held, and how many each live style has.
            let mut held: Vec<(Style, StyleId)> = Vec::new();
            let mut live: HashMap<Style, (StyleId, u32)> = HashMap::new();
            for op in ops {
                match op {
                    Op::Acquire(style) => match table.acquire(&style) {
                        Ok(id) if style == Style::DEFAULT => {
                            prop_assert_eq!(id, StyleId::DEFAULT);
                        }
                        Ok(id) => {
                            let entry = live.entry(style).or_insert((id, 0));
                            prop_assert_eq!(entry.0, id, "equal styles share an id");
                            entry.1 += 1;
                            held.push((style, id));
                        }
                        Err(full) => {
                            prop_assert_eq!(full, TableFull::Styles);
                            prop_assert!(!live.contains_key(&style));
                            prop_assert_eq!(live.len(), usize::from(capacity));
                        }
                    },
                    Op::Release(index) if !held.is_empty() => {
                        let (style, id) = held.swap_remove(index % held.len());
                        table.release(id);
                        let entry = live.get_mut(&style).expect("a held style is live");
                        entry.1 -= 1;
                        if entry.1 == 0 {
                            live.remove(&style);
                        }
                    }
                    Op::Release(_) => {}
                }
                prop_assert_eq!(table.len(), live.len());
                for (style, &(id, refs)) in &live {
                    prop_assert_eq!(table.get(id), style);
                    prop_assert_eq!(table.references(id), refs);
                }
                let mut ids: Vec<StyleId> = live.values().map(|&(id, _)| id).collect();
                ids.sort_unstable();
                ids.dedup();
                prop_assert_eq!(ids.len(), live.len(), "live styles have distinct ids");
            }
        }
    }
}
