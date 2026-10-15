//! The flags of the kitty keyboard protocol, which programs push, pop, set
//! and query to choose how keys reach them:
//! <https://sw.kovidgoyal.net/kitty/keyboard-protocol/#progressive-enhancement>.

use super::Terminal;

/// The kitty keyboard protocol's enhancements that a program asked for.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct KeyboardFlags(u8);

impl KeyboardFlags {
    /// No enhancement: keys are sent as xterm sends them.
    pub const NONE: Self = Self(0);
    /// Keys that are ambiguous otherwise, such as Esc and Alt with a
    /// letter, are sent as escape codes.
    pub const DISAMBIGUATE: Self = Self(1);
    /// Repeats and releases are reported, not only presses.
    pub const REPORT_EVENTS: Self = Self(2);
    /// The shifted key and the key of the base layout come with the key.
    pub const REPORT_ALTERNATES: Self = Self(4);
    /// Every key is sent as an escape code, those that type text too.
    pub const REPORT_ALL_KEYS: Self = Self(8);
    /// The text that a key types comes with its escape code.
    pub const REPORT_TEXT: Self = Self(16);

    /// The flags that the protocol defines.
    const ALL: u8 = 0b1_1111;

    /// The flags whose bits are set in `bits`, those that the protocol
    /// does not define left out.
    pub fn from_bits(bits: u16) -> Self {
        // The mask keeps only the low five bits, so the value fits.
        Self((bits & u16::from(Self::ALL)) as u8)
    }

    /// The flags as the protocol numbers them.
    pub fn bits(self) -> u8 {
        self.0
    }

    /// Whether every flag of `other` is set.
    pub fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }
}

/// How many entries a screen's stack of flags holds.
const STACK: usize = 8;

/// A screen's stack of flags, of which the top entry is in force. The
/// bottom entry is always there, so that a program can set flags without
/// pushing; a push past the last entry drops the oldest.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct KeyboardStack {
    entries: [KeyboardFlags; STACK],
    top: usize,
}

impl KeyboardStack {
    /// The flags in force.
    pub(super) fn flags(&self) -> KeyboardFlags {
        self.entries[self.top]
    }

    fn push(&mut self, flags: KeyboardFlags) {
        if self.top == STACK - 1 {
            self.entries.copy_within(1.., 0);
        } else {
            self.top += 1;
        }
        self.entries[self.top] = flags;
    }

    /// Takes `n` entries off the stack. Taking the bottom one too resets
    /// all the flags.
    fn pop(&mut self, n: usize) {
        if n > self.top {
            *self = Self::default();
        } else {
            self.top -= n;
        }
    }

    /// Changes the flags in force: `mode` 1 replaces them with `flags`, 2
    /// sets those of `flags`, and 3 resets them. Other modes do nothing.
    fn set(&mut self, flags: KeyboardFlags, mode: u16) {
        let entry = &mut self.entries[self.top];
        entry.0 = match mode {
            1 => flags.0,
            2 => entry.0 | flags.0,
            3 => entry.0 & !flags.0,
            _ => return,
        };
    }
}

impl Terminal {
    /// The flags of the kitty keyboard protocol in force on the active
    /// screen.
    pub fn keyboard_flags(&self) -> KeyboardFlags {
        self.screen().keyboard.flags()
    }

    /// `CSI > flags u`: pushes `flags` onto the active screen's stack.
    pub(super) fn push_keyboard_flags(&mut self, flags: u16) {
        self.screen_mut()
            .keyboard
            .push(KeyboardFlags::from_bits(flags));
    }

    /// `CSI < n u`: pops `n` entries off the active screen's stack.
    pub(super) fn pop_keyboard_flags(&mut self, n: u16) {
        self.screen_mut().keyboard.pop(usize::from(n));
    }

    /// `CSI = flags ; mode u`: changes the flags in force.
    pub(super) fn set_keyboard_flags(&mut self, flags: u16, mode: u16) {
        self.screen_mut()
            .keyboard
            .set(KeyboardFlags::from_bits(flags), mode);
    }

    /// `CSI ? u`: reports the flags in force as `CSI ? flags u`.
    pub(super) fn report_keyboard_flags(&mut self) {
        let reply = format!("\x1b[?{}u", self.keyboard_flags().bits());
        self.reply(reply.as_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flags(bits: u16) -> KeyboardFlags {
        KeyboardFlags::from_bits(bits)
    }

    #[test]
    fn only_the_defined_flags_are_kept() {
        assert_eq!(flags(0xFF).bits(), 31);
        assert_eq!(flags(32), KeyboardFlags::NONE);
        let both = flags(9);
        assert!(both.contains(KeyboardFlags::DISAMBIGUATE));
        assert!(both.contains(KeyboardFlags::REPORT_ALL_KEYS));
        assert!(!both.contains(KeyboardFlags::REPORT_EVENTS));
        assert!(both.contains(KeyboardFlags::NONE));
    }

    #[test]
    fn pushes_and_pops_go_back_to_the_flags_before() {
        let mut stack = KeyboardStack::default();
        stack.set(flags(4), 1);
        stack.push(flags(1));
        stack.push(flags(3));
        assert_eq!(stack.flags(), flags(3));
        stack.pop(1);
        assert_eq!(stack.flags(), flags(1));
        stack.pop(1);
        assert_eq!(stack.flags(), flags(4), "the flags set before any push");
        stack.pop(1);
        assert_eq!(
            stack.flags(),
            KeyboardFlags::NONE,
            "an empty stack resets them"
        );
        stack.push(flags(2));
        stack.pop(5);
        assert_eq!(stack.flags(), KeyboardFlags::NONE);
    }

    #[test]
    fn a_full_stack_drops_its_oldest_entry() {
        let mut stack = KeyboardStack::default();
        for bits in 1..=10 {
            stack.push(flags(bits));
        }
        assert_eq!(stack.flags(), flags(10));
        stack.pop(STACK - 1);
        assert_eq!(
            stack.flags(),
            flags(3),
            "1 and 2 went, and the bottom entry"
        );
        stack.pop(1);
        assert_eq!(stack.flags(), KeyboardFlags::NONE);
    }

    #[test]
    fn set_replaces_adds_or_removes_flags() {
        let mut stack = KeyboardStack::default();
        stack.set(flags(5), 1);
        assert_eq!(stack.flags(), flags(5));
        stack.set(flags(10), 2);
        assert_eq!(stack.flags(), flags(15));
        stack.set(flags(6), 3);
        assert_eq!(stack.flags(), flags(9));
        stack.set(flags(6), 3);
        assert_eq!(stack.flags(), flags(9), "removing flags that are not set");
        stack.set(flags(31), 4);
        assert_eq!(stack.flags(), flags(9), "an unknown mode");
        stack.set(flags(2), 1);
        assert_eq!(stack.flags(), flags(2));
    }
}
