//! The app's timers, as Foundation's timers on the main run loop, in its
//! common modes, so that they fire during a live resize and while a menu
//! is open too.

use std::ptr::NonNull;
use std::rc::Weak;
use std::time::Instant;

use block2::RcBlock;
use lokyr_platform_core::TimerId;
use objc2::rc::Retained;
use objc2_foundation::{NSRunLoop, NSRunLoopCommonModes, NSTimer};

use crate::shared::{Pending, Shared};

/// The share of a timer's delay by which the system may fire it late, to
/// group its wakeups with others, up to a limit.
const TOLERANCE: f64 = 0.05;
const MAX_TOLERANCE: f64 = 0.05;

/// Starts a timer that tells `shared` that `timer` fired, once, at `at` or
/// a little after.
pub(crate) fn start(shared: Weak<Shared>, timer: TimerId, at: Instant) -> Retained<NSTimer> {
    let delay = at.saturating_duration_since(Instant::now()).as_secs_f64();
    let block = RcBlock::new(move |_: NonNull<NSTimer>| {
        if let Some(shared) = shared.upgrade() {
            shared.post(Pending::Timer(timer));
        }
    });
    // SAFETY: the block runs on the run loop that the timer is added to,
    // the main one, on the thread that made it, which is the only one that
    // touches what it holds.
    let ns_timer = unsafe { NSTimer::timerWithTimeInterval_repeats_block(delay, false, &block) };
    ns_timer.setTolerance((delay * TOLERANCE).min(MAX_TOLERANCE));
    // SAFETY: a timer that no run loop has yet, and Foundation's mode.
    unsafe { NSRunLoop::mainRunLoop().addTimer_forMode(&ns_timer, NSRunLoopCommonModes) };
    ns_timer
}
