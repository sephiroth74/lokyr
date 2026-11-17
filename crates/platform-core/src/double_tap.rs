//! A modifier's double tap: the modifier pressed and let go of alone twice
//! within the system's double-click interval, from any app, as a platform
//! tells the modifiers' changes, with no key nor click meanwhile.

use std::time::{Duration, Instant};

/// A modifier key, whose double tap the app may watch.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Modifier {
    /// Control.
    Control,
    /// Alt, Option on the Mac.
    Alt,
    /// Command on the Mac, Super elsewhere.
    Command,
    /// Shift.
    Shift,
}

/// The modifiers held, of either side, as a platform's event of their
/// change tells them; Caps Lock and the function key are none.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Held {
    /// Control.
    pub control: bool,
    /// Alt, or Option.
    pub alt: bool,
    /// Command, or Super.
    pub command: bool,
    /// Shift.
    pub shift: bool,
}

impl Held {
    /// None held.
    pub const NONE: Self = Self {
        control: false,
        alt: false,
        command: false,
        shift: false,
    };

    /// The modifier held alone, if one is.
    pub fn alone(self) -> Option<Modifier> {
        match (self.control, self.alt, self.command, self.shift) {
            (true, false, false, false) => Some(Modifier::Control),
            (false, true, false, false) => Some(Modifier::Alt),
            (false, false, true, false) => Some(Modifier::Command),
            (false, false, false, true) => Some(Modifier::Shift),
            _ => None,
        }
    }

    /// Whether none is held.
    pub fn is_empty(self) -> bool {
        self == Self::NONE
    }
}

/// Whether the app watches the double taps that it asked for, as the
/// platform answers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DoubleTaps {
    /// It watches none, as it asked for none.
    Off,
    /// It watches them.
    Watching,
    /// The system's leave to see the modifiers in other apps is not given:
    /// the platform asked for it, and watches once it is given.
    NotAllowed,
}

/// Finds the double taps in the changes of the modifiers held: a tap is a
/// modifier pressed alone, of none held before, and let go of within the
/// interval, with no key nor click meanwhile; a second tap of the same
/// modifier pressed within the interval of the first's press, of no key
/// nor click since the first ended, makes the double tap, as it ends.
#[derive(Clone, Debug)]
pub struct DoubleTap {
    interval: Duration,
    held: Held,
    state: State,
}

/// Where a double tap stands.
#[derive(Clone, Copy, Debug)]
enum State {
    /// None under way.
    Idle,
    /// `modifier` pressed alone at `at`; the second press, of the first
    /// pressed at `first`, if it is one.
    Pressed {
        modifier: Modifier,
        at: Instant,
        first: Option<Instant>,
    },
    /// `modifier` tapped once, pressed at `first` and let go of at
    /// `released`.
    Tapped {
        modifier: Modifier,
        first: Instant,
        released: Instant,
    },
}

impl DoubleTap {
    /// A finder of the double taps within `interval`, the system's
    /// double-click interval.
    pub const fn new(interval: Duration) -> Self {
        Self {
            interval,
            held: Held::NONE,
            state: State::Idle,
        }
    }

    /// Takes the system's double-click interval anew.
    pub fn set_interval(&mut self, interval: Duration) {
        self.interval = interval;
    }

    /// Takes the change of the modifiers to `held` at `now`, of no key nor
    /// click of any app for `idle`, as the system tells it: gives the
    /// modifier tapped twice, as its second tap ends, after which a third
    /// tap starts anew.
    pub fn change(&mut self, held: Held, now: Instant, idle: Duration) -> Option<Modifier> {
        let was = std::mem::replace(&mut self.held, held);
        let within = |since: Instant| now.saturating_duration_since(since) <= self.interval;
        // Nothing of the user's since `since`, of no key nor click.
        let still = |since: Instant| idle >= now.saturating_duration_since(since);
        let mut tapped = None;
        self.state = match (was.alone(), held.alone(), self.state) {
            // A press of the modifier alone, of none held before: the
            // second of a tap, if it comes in time.
            (_, Some(modifier), state) if was.is_empty() => match state {
                State::Tapped {
                    modifier: first_of,
                    first,
                    released,
                } if first_of == modifier && within(first) && still(released) => State::Pressed {
                    modifier,
                    at: now,
                    first: Some(first),
                },
                _ => State::Pressed {
                    modifier,
                    at: now,
                    first: None,
                },
            },
            // The modifier let go of, of none held now: a tap, if it was
            // short and nothing came meanwhile.
            (
                Some(modifier),
                None,
                State::Pressed {
                    modifier: pressed,
                    at,
                    first,
                },
            ) if held.is_empty() && modifier == pressed && within(at) && still(at) => match first {
                Some(_) => {
                    tapped = Some(modifier);
                    State::Idle
                }
                None => State::Tapped {
                    modifier,
                    first: at,
                    released: now,
                },
            },
            // Another modifier with it, or a change that makes no tap.
            _ => State::Idle,
        };
        tapped
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const INTERVAL: Duration = Duration::from_millis(500);
    /// Long since the last key or click.
    const LONG: Duration = Duration::from_secs(60);

    fn ms(millis: u64) -> Duration {
        Duration::from_millis(millis)
    }

    const NONE: Held = Held {
        control: false,
        alt: false,
        command: false,
        shift: false,
    };
    const CONTROL: Held = Held {
        control: true,
        ..NONE
    };
    const COMMAND: Held = Held {
        command: true,
        ..NONE
    };
    const CONTROL_SHIFT: Held = Held {
        shift: true,
        ..CONTROL
    };

    /// The double taps of `changes`, each of its milliseconds from the
    /// start, the modifiers held then, and the milliseconds since the last
    /// key or click, in their order.
    fn taps(changes: &[(u64, Held, Duration)]) -> Vec<(u64, Modifier)> {
        let start = Instant::now();
        let mut finder = DoubleTap::new(INTERVAL);
        changes
            .iter()
            .filter_map(|&(at, held, idle)| {
                finder
                    .change(held, start + ms(at), idle)
                    .map(|modifier| (at, modifier))
            })
            .collect()
    }

    #[test]
    fn two_quick_taps_of_a_modifier_alone_make_a_double_tap_as_the_second_ends() {
        let quick = [
            (0, CONTROL, LONG),
            (80, NONE, LONG),
            (200, CONTROL, LONG),
            (280, NONE, LONG),
        ];
        assert_eq!(taps(&quick), [(280, Modifier::Control)]);
        // Of every modifier.
        let command =
            quick.map(|(at, held, idle)| (at, if held.is_empty() { NONE } else { COMMAND }, idle));
        assert_eq!(taps(&command), [(280, Modifier::Command)]);
        // The second press at the interval's end still counts.
        let late = [
            (0, CONTROL, LONG),
            (80, NONE, LONG),
            (500, CONTROL, LONG),
            (560, NONE, LONG),
        ];
        assert_eq!(taps(&late), [(560, Modifier::Control)]);
    }

    #[test]
    fn a_second_press_past_the_interval_starts_a_new_tap() {
        let slow = [
            (0, CONTROL, LONG),
            (80, NONE, LONG),
            (501, CONTROL, LONG),
            (580, NONE, LONG),
            // Which a third, in time, ends.
            (700, CONTROL, LONG),
            (760, NONE, LONG),
        ];
        assert_eq!(taps(&slow), [(760, Modifier::Control)]);
    }

    #[test]
    fn a_long_hold_is_no_tap() {
        let held = [
            (0, CONTROL, LONG),
            (600, NONE, LONG),
            (700, CONTROL, LONG),
            (760, NONE, LONG),
        ];
        assert!(taps(&held).is_empty());
        // Nor is a second one.
        let second = [
            (0, CONTROL, LONG),
            (80, NONE, LONG),
            (200, CONTROL, LONG),
            (800, NONE, LONG),
        ];
        assert!(taps(&second).is_empty());
    }

    #[test]
    fn a_key_or_a_click_while_held_or_between_the_taps_makes_none() {
        // Control-C twice: a key while it is held, 30 ms before the
        // modifier goes, each time.
        let shortcuts = [
            (0, CONTROL, LONG),
            (80, NONE, ms(30)),
            (200, CONTROL, ms(150)),
            (280, NONE, ms(30)),
        ];
        assert!(taps(&shortcuts).is_empty());
        // A key between the taps, 50 ms before the second press.
        let between = [
            (0, CONTROL, LONG),
            (80, NONE, LONG),
            (200, CONTROL, ms(50)),
            (280, NONE, ms(130)),
        ];
        assert!(taps(&between).is_empty());
        // A key before the first press counts not.
        let before = [
            (0, CONTROL, ms(10)),
            (80, NONE, ms(90)),
            (200, CONTROL, ms(210)),
            (280, NONE, ms(290)),
        ];
        assert_eq!(taps(&before), [(280, Modifier::Control)]);
    }

    #[test]
    fn another_modifier_with_it_or_between_makes_none() {
        // Control, then Shift with it, as a chord.
        let chord = [
            (0, CONTROL, LONG),
            (40, CONTROL_SHIFT, LONG),
            (60, CONTROL, LONG),
            (80, NONE, LONG),
            (200, CONTROL, LONG),
            (280, NONE, LONG),
        ];
        assert!(taps(&chord).is_empty());
        // A tap of Control, then one of Command.
        let two = [
            (0, CONTROL, LONG),
            (80, NONE, LONG),
            (200, COMMAND, LONG),
            (280, NONE, LONG),
        ];
        assert!(taps(&two).is_empty());
    }

    #[test]
    fn a_third_tap_starts_anew_and_a_fourth_makes_another() {
        let four = [
            (0, CONTROL, LONG),
            (60, NONE, LONG),
            (120, CONTROL, LONG),
            (180, NONE, LONG),
            (240, CONTROL, LONG),
            (300, NONE, LONG),
            (360, CONTROL, LONG),
            (420, NONE, LONG),
        ];
        assert_eq!(
            taps(&four),
            [(180, Modifier::Control), (420, Modifier::Control)]
        );
    }

    #[test]
    fn a_new_interval_holds_from_the_next_change() {
        let start = Instant::now();
        let mut finder = DoubleTap::new(INTERVAL);
        finder.set_interval(ms(200));
        assert_eq!(finder.change(CONTROL, start, LONG), None);
        assert_eq!(finder.change(NONE, start + ms(60), LONG), None);
        assert_eq!(finder.change(CONTROL, start + ms(300), LONG), None);
        assert_eq!(finder.change(NONE, start + ms(360), LONG), None);
    }
}
