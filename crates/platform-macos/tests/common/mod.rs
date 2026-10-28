//! What the tests of the application share: a deadline, so that a test that
//! hangs fails rather than holds, and an alert to open.

// Each test takes what it needs of this module.
#![allow(dead_code)]

use std::process;
use std::thread;
use std::time::Duration;

use lokyr_platform_core::{Alert, AlertStyle, Waker};

/// Fails the test `name` if it has not ended within `seconds`.
pub fn deadline(name: &'static str, seconds: u64) {
    thread::spawn(move || {
        thread::sleep(Duration::from_secs(seconds));
        eprintln!("{name}: not done within {seconds} s");
        process::exit(1);
    });
}

/// Wakes the application from another thread after `millis`.
pub fn wake_after(waker: Waker, millis: u64) {
    thread::spawn(move || {
        thread::sleep(Duration::from_millis(millis));
        waker.wake();
    });
}

/// An alert of two buttons.
pub fn alert() -> Alert {
    Alert {
        style: AlertStyle::Warning,
        message: "A test's alert".to_owned(),
        detail: "It closes by itself.".to_owned(),
        buttons: vec!["OK".to_owned(), "Cancel".to_owned()],
        suppression: None,
    }
}
