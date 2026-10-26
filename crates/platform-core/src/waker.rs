//! Waking the main thread from another.

use std::fmt;
use std::sync::Arc;

/// Wakes the main thread from any thread, as a session's threads do when
/// its terminal changes: each wake brings an
/// [`AppEvent::Wake`](crate::AppEvent::Wake) to the main thread, and the
/// wakes that come before it is delivered bring one.
#[derive(Clone)]
pub struct Waker {
    wake: Arc<dyn Fn() + Send + Sync>,
}

impl Waker {
    /// A waker that calls `wake`, which the platform gives: on the Mac, a
    /// signal to a source of the main thread's run loop.
    pub fn new(wake: impl Fn() + Send + Sync + 'static) -> Self {
        Self {
            wake: Arc::new(wake),
        }
    }

    /// Wakes the main thread.
    pub fn wake(&self) {
        (self.wake)();
    }
}

impl fmt::Debug for Waker {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Waker").finish_non_exhaustive()
    }
}
