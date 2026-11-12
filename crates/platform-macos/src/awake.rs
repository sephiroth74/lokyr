//! The power assertion that keeps the Mac awake: the system, or the system
//! and the display, kept from idle sleep, which `pmset -g assertions` lists
//! by its name. The app holds one at most, and the system releases it as
//! the app ends.

use std::cell::Cell;

use lokyr_platform_core::Awake;
use objc2_foundation::NSString;

#[link(name = "IOKit", kind = "framework")]
unsafe extern "C" {
    /// Creates a power assertion of `kind`, at `level`, named `name`: its
    /// id in `id`, and `kIOReturnSuccess`, or an error. The texts are
    /// `CFStringRef`s, which `NSString` is, toll-free.
    fn IOPMAssertionCreateWithName(
        kind: *const NSString,
        level: u32,
        name: *const NSString,
        id: *mut u32,
    ) -> i32;
    /// Releases the assertion `id`.
    fn IOPMAssertionRelease(id: u32) -> i32;
}

/// `kIOPMAssertionLevelOn`.
const ON: u32 = 255;

/// The assertion that keeps the system from idle sleep, the display free
/// to sleep.
const SYSTEM: &str = "PreventUserIdleSystemSleep";

/// The assertion that keeps the display, and the system with it, from idle
/// sleep.
const DISPLAY: &str = "PreventUserIdleDisplaySleep";

/// The app's power assertion: its id while it holds one, with what it
/// keeps awake and its name.
#[derive(Debug, Default)]
pub(crate) struct KeptAwake {
    held: Cell<Option<u32>>,
}

impl KeptAwake {
    /// Holds the assertion of `awake`, named `reason`, in place of the one
    /// before, or none of [`Awake::Off`].
    pub(crate) fn set(&self, awake: Awake, reason: &str) {
        if let Some(id) = self.held.take() {
            // SAFETY: an assertion that the app created, released once.
            let status = unsafe { IOPMAssertionRelease(id) };
            if status != 0 {
                eprintln!("lokyr: the power assertion cannot be released: {status}");
            }
        }
        let kind = match awake {
            Awake::Off => return,
            Awake::System => SYSTEM,
            Awake::Display => DISPLAY,
        };
        let (kind, name) = (NSString::from_str(kind), NSString::from_str(reason));
        let mut id = 0;
        // SAFETY: the system's call, of two texts that live through it and
        // of room for the id.
        let status = unsafe { IOPMAssertionCreateWithName(&*kind, ON, &*name, &raw mut id) };
        if status == 0 {
            self.held.set(Some(id));
        } else {
            eprintln!("lokyr: the Mac cannot be kept awake: {status}");
        }
    }

    /// Whether it holds an assertion.
    #[cfg(test)]
    pub(crate) fn holds(&self) -> bool {
        self.held.get().is_some()
    }
}

impl Drop for KeptAwake {
    fn drop(&mut self) {
        self.set(Awake::Off, "");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_assertion_is_held_replaced_and_released() {
        let kept = KeptAwake::default();
        assert!(!kept.holds());
        kept.set(Awake::System, "Lokyr: a test");
        assert!(kept.holds());
        let first = kept.held.get();
        // Another in its place, of an id of its own.
        kept.set(Awake::Display, "Lokyr: a test");
        assert!(kept.holds());
        assert_ne!(kept.held.get(), first);
        kept.set(Awake::Off, "");
        assert!(!kept.holds());
    }
}
