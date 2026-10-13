//! Styles, colors, and the style table of a page.

use thiserror::Error;

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
    fn bits(&self) -> u128 {
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

/// Where a lookup in the index of a [`StyleTable`] ends.
#[derive(Clone, Copy, Debug)]
enum Slot {
    /// The slot that holds the style.
    Taken(usize),
    /// The empty slot where the style would go.
    Empty(usize),
}

/// The table has no room for another style.
#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
#[error("the style table is full")]
pub struct TableFull;

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
    /// An open-addressing hash index, from a style to its id. 0 is an empty
    /// slot, since the default style is not indexed.
    index: Box<[u16]>,
    /// Ids freed since they were first used, to use again.
    free: Vec<u16>,
    /// The lowest id that has never been used.
    unused: u16,
    /// How many styles the table holds, the default one aside.
    len: usize,
}

impl StyleTable {
    /// A table with room for `capacity` styles besides the default one.
    pub fn new(capacity: u16) -> Self {
        let slots = usize::from(capacity)
            .saturating_mul(2)
            .next_power_of_two()
            .max(2);
        let ids = usize::from(capacity) + 1;
        Self {
            styles: vec![Style::DEFAULT; ids].into_boxed_slice(),
            refs: vec![0; ids].into_boxed_slice(),
            index: vec![0; slots].into_boxed_slice(),
            free: Vec::with_capacity(usize::from(capacity)),
            unused: 1,
            len: 0,
        }
    }

    /// How many styles the table can hold, the default one aside.
    pub fn capacity(&self) -> usize {
        self.styles.len() - 1
    }

    /// How many styles the table holds, the default one aside.
    pub fn len(&self) -> usize {
        self.len
    }

    /// Whether the table holds only the default style.
    pub fn is_empty(&self) -> bool {
        self.len == 0
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
                let id = self.index[slot];
                self.refs[usize::from(id)] += 1;
                return Ok(StyleId::new(id));
            }
            Some(Slot::Empty(slot)) => slot,
            None => return Err(TableFull),
        };
        let id = if let Some(id) = self.free.pop() {
            id
        } else if usize::from(self.unused) < self.styles.len() {
            self.unused += 1;
            self.unused - 1
        } else {
            return Err(TableFull);
        };
        self.styles[usize::from(id)] = *style;
        self.refs[usize::from(id)] = 1;
        self.index[slot] = id;
        self.len += 1;
        Ok(StyleId::new(id))
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
    pub fn release(&mut self, id: StyleId) {
        if id == StyleId::DEFAULT {
            return;
        }
        debug_assert!(self.is_live(id), "style {id:?} is not in the table");
        let refs = &mut self.refs[usize::from(id.get())];
        *refs = refs.saturating_sub(1);
        if *refs > 0 {
            return;
        }
        let style = self.styles[usize::from(id.get())];
        if let Some(Slot::Taken(slot)) = self.find(&style) {
            self.remove_slot(slot);
        }
        self.styles[usize::from(id.get())] = Style::DEFAULT;
        self.free.push(id.get());
        self.len -= 1;
    }

    fn is_live(&self, id: StyleId) -> bool {
        id == StyleId::DEFAULT
            || self
                .refs
                .get(usize::from(id.get()))
                .is_some_and(|&refs| refs > 0)
    }

    /// The index slot where `style` belongs.
    fn home(&self, style: &Style) -> usize {
        // Fibonacci hashing: the top bits of the product are the best mixed.
        let bits = style.bits();
        let folded = (bits as u64) ^ ((bits >> 64) as u64).rotate_left(29);
        let hash = folded.wrapping_mul(0x9E37_79B9_7F4A_7C15);
        (hash >> (64 - self.index.len().trailing_zeros())) as usize
    }

    /// The slot of `style` in the index, or the empty slot where it would
    /// go.
    ///
    /// The index has twice as many slots as the table has ids, so a lookup
    /// always meets an empty slot. It still stops after a full turn, so that
    /// a broken index cannot hang the terminal.
    fn find(&self, style: &Style) -> Option<Slot> {
        let mask = self.index.len() - 1;
        let mut slot = self.home(style);
        for _ in 0..self.index.len() {
            match self.index[slot] {
                0 => return Some(Slot::Empty(slot)),
                id if self.styles[usize::from(id)] == *style => return Some(Slot::Taken(slot)),
                _ => slot = (slot + 1) & mask,
            }
        }
        debug_assert!(false, "the style index has no empty slot");
        None
    }

    /// Empties `slot`, and moves back the entries after it that it would
    /// hide from a lookup, so that the index needs no tombstones.
    fn remove_slot(&mut self, slot: usize) {
        let mask = self.index.len() - 1;
        let mut hole = slot;
        let mut next = slot;
        loop {
            next = (next + 1) & mask;
            let id = self.index[next];
            if id == 0 {
                break;
            }
            let home = self.home(&self.styles[usize::from(id)]);
            // The entry can fill the hole unless its home lies between the
            // hole and itself.
            if next.wrapping_sub(home) & mask >= next.wrapping_sub(hole) & mask {
                self.index[hole] = id;
                hole = next;
            }
        }
        self.index[hole] = 0;
    }
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
        assert_eq!(table.acquire(&bold(4)), Err(TableFull));
        assert_eq!(table.acquire(&bold(2)), Ok(ids[1]));
        assert_eq!(table.acquire(&Style::DEFAULT), Ok(StyleId::DEFAULT));
        // Freeing a style makes room.
        table.release(ids[0]);
        assert_eq!(table.acquire(&bold(4)), Ok(ids[0]));
    }

    #[test]
    fn a_table_without_capacity_holds_only_the_default_style() {
        let mut table = StyleTable::new(0);
        assert_eq!(table.acquire(&bold(1)), Err(TableFull));
        assert_eq!(table.acquire(&Style::DEFAULT), Ok(StyleId::DEFAULT));
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
                        Err(TableFull) => {
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
