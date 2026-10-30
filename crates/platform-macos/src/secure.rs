//! Secure Keyboard Entry: the system's secure entry of the keyboard, which
//! keeps other apps from reading the keys that Lokyr takes, as at a
//! password's prompt. The system counts the calls that turn it on and off,
//! which stay balanced: one on at most, and off as the app ends.

use std::cell::Cell;

#[link(name = "Carbon", kind = "framework")]
unsafe extern "C" {
    /// Turns the secure entry on, once more: `noErr`, or an error.
    fn EnableSecureEventInput() -> i32;
    /// Turns it off, once.
    fn DisableSecureEventInput() -> i32;
}

/// The app's part of the secure entry: whether it turned it on.
#[derive(Debug, Default)]
pub(crate) struct SecureInput {
    on: Cell<bool>,
}

impl SecureInput {
    /// Turns the app's part on or off, if it is not already.
    pub(crate) fn set(&self, on: bool) {
        if self.on.get() == on {
            return;
        }
        // SAFETY: the system's calls, of no arguments, which a call of the
        // other kind balances; the app's part is one call at most.
        let status = unsafe {
            if on {
                EnableSecureEventInput()
            } else {
                DisableSecureEventInput()
            }
        };
        if status == 0 {
            self.on.set(on);
        } else {
            eprintln!(
                "lokyr: Secure Keyboard Entry cannot turn {}: {status}",
                if on { "on" } else { "off" }
            );
        }
    }
}

impl Drop for SecureInput {
    fn drop(&mut self) {
        self.set(false);
    }
}
