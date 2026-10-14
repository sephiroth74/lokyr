//! Character sets: the sets of 94 characters that designations put in
//! G0–G3, and the shifts that put one of them in use.

/// A set of 94 characters, which gives the printable ASCII from `!` to `~`
/// other meanings.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Charset {
    /// ASCII, as it is.
    #[default]
    Ascii,
    /// DEC Special Graphics, whose lowercase letters draw lines and boxes.
    DecSpecialGraphics,
    /// The British set, whose `#` is `£`.
    British,
}

impl Charset {
    /// The set that a designation with `final_byte` names, if the terminal
    /// knows it.
    pub fn designated_by(final_byte: u8) -> Option<Self> {
        match final_byte {
            b'B' => Some(Self::Ascii),
            b'0' => Some(Self::DecSpecialGraphics),
            b'A' => Some(Self::British),
            _ => None,
        }
    }

    /// What the printable ASCII character `c` stands for in the set.
    pub fn map(self, c: char) -> char {
        match self {
            Self::Ascii => c,
            Self::British => match c {
                '#' => '£',
                _ => c,
            },
            // As the VT100 draws them (https://vt100.net/docs/vt100-ug/table3-9.html).
            Self::DecSpecialGraphics => match c {
                '_' => ' ',
                '`' => '◆',
                'a' => '▒',
                'b' => '\u{2409}',
                'c' => '\u{240C}',
                'd' => '\u{240D}',
                'e' => '\u{240A}',
                'f' => '°',
                'g' => '±',
                'h' => '\u{2424}',
                'i' => '\u{240B}',
                'j' => '┘',
                'k' => '┐',
                'l' => '┌',
                'm' => '└',
                'n' => '┼',
                'o' => '⎺',
                'p' => '⎻',
                'q' => '─',
                'r' => '⎼',
                's' => '⎽',
                't' => '├',
                'u' => '┤',
                'v' => '┴',
                'w' => '┬',
                'x' => '│',
                'y' => '≤',
                'z' => '≥',
                '{' => 'π',
                '|' => '≠',
                '}' => '£',
                '~' => '·',
                _ => c,
            },
        }
    }
}

/// One of the four slots for character sets, G0 to G3.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Shift {
    /// G0, which SI puts in use.
    #[default]
    G0,
    /// G1, which SO puts in use.
    G1,
    /// G2, which LS2 puts in use and SS2 uses for one character.
    G2,
    /// G3, which LS3 puts in use and SS3 uses for one character.
    G3,
}

/// The character sets of a terminal: those in G0–G3, the slot in use, and
/// a single shift that is pending, if any.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Charsets {
    sets: [Charset; 4],
    shift: Shift,
    single: Option<Shift>,
}

impl Charsets {
    /// The character set in `slot`.
    pub fn get(&self, slot: Shift) -> Charset {
        self.sets[slot as usize]
    }

    /// Puts `set` in `slot`.
    pub fn designate(&mut self, slot: Shift, set: Charset) {
        self.sets[slot as usize] = set;
    }

    /// The slot in use.
    pub fn shift(&self) -> Shift {
        self.shift
    }

    /// Puts the set of `slot` in use, as the locking shifts do.
    pub fn lock(&mut self, slot: Shift) {
        self.shift = slot;
    }

    /// The slot that the next character comes from instead, as SS2 and
    /// SS3 ask.
    pub fn single_shift(&self) -> Option<Shift> {
        self.single
    }

    /// Makes the next character come from the set of `slot`.
    pub fn set_single_shift(&mut self, slot: Shift) {
        self.single = Some(slot);
    }

    /// Whether printable ASCII prints as it is: the set in use is ASCII,
    /// and no single shift is pending.
    pub fn is_plain(&self) -> bool {
        self.single.is_none() && self.get(self.shift) == Charset::Ascii
    }

    /// What `c` prints as: the character it stands for in the set of a
    /// pending single shift, which it uses up, or else in the set in use.
    /// Only printable ASCII stands for something else.
    pub fn translate(&mut self, c: char) -> char {
        let slot = self.single.take().unwrap_or(self.shift);
        if matches!(c, ' '..='~') {
            self.get(slot).map(c)
        } else {
            c
        }
    }
}
