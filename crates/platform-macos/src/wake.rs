//! Waking the main thread: a source of its run loop, which other threads
//! signal.

use std::cell::RefCell;
use std::ffi::c_void;
use std::ptr;
use std::rc::{Rc, Weak};
use std::sync::Arc;

use bolt_platform_core::{AppEvent, Error, Waker};
use objc2_core_foundation::{
    CFRetained, CFRunLoop, CFRunLoopSource, CFRunLoopSourceContext, kCFRunLoopCommonModes,
};

use crate::shared::{Pending, Shared};

thread_local! {
    /// The application that the source wakes, on the main thread.
    static TARGET: RefCell<Weak<Shared>> = const { RefCell::new(Weak::new()) };
}

/// The source and the run loop that it is on.
struct Signal {
    source: CFRetained<CFRunLoopSource>,
    run_loop: CFRetained<CFRunLoop>,
}

// SAFETY: other threads only signal the source and wake its run loop, which
// Core Foundation makes for them: Apple's Threading Programming Guide signals
// a custom input source, and wakes its run loop, from any thread. The rest of
// the source's life is on the main thread.
unsafe impl Send for Signal {}
// SAFETY: as for `Send`.
unsafe impl Sync for Signal {}

/// The source of the main thread's run loop that a [`Waker`] signals, in
/// every common mode, so that it fires in a live resize and while a menu is
/// open. Dropping it takes it off the run loop.
pub(crate) struct WakeSource {
    signal: Arc<Signal>,
}

impl WakeSource {
    /// Adds the source to the main thread's run loop.
    pub(crate) fn install() -> Result<Self, Error> {
        let mut context = CFRunLoopSourceContext {
            version: 0,
            info: ptr::null_mut(),
            retain: None,
            release: None,
            copyDescription: None,
            equal: None,
            hash: None,
            schedule: None,
            cancel: None,
            perform: Some(perform),
        };
        // SAFETY: the context is valid for the call, which copies it.
        let source = unsafe { CFRunLoopSource::new(None, 0, &mut context) }.ok_or_else(|| {
            Error::Start("the run loop's wake source could not be made".to_owned())
        })?;
        let run_loop = CFRunLoop::main()
            .ok_or_else(|| Error::Start("the main thread has no run loop".to_owned()))?;
        // SAFETY: the mode is Core Foundation's constant.
        run_loop.add_source(Some(&source), unsafe { kCFRunLoopCommonModes });
        Ok(Self {
            signal: Arc::new(Signal { source, run_loop }),
        })
    }

    /// A waker that signals the source, from any thread.
    pub(crate) fn waker(&self) -> Waker {
        let signal = Arc::clone(&self.signal);
        Waker::new(move || {
            signal.source.signal();
            signal.run_loop.wake_up();
        })
    }

    /// Makes `shared` the application that the source wakes.
    pub(crate) fn set_target(&self, shared: &Rc<Shared>) {
        TARGET.with(|target| *target.borrow_mut() = Rc::downgrade(shared));
    }
}

impl Drop for WakeSource {
    fn drop(&mut self) {
        // A waker that outlives the application signals a source that does
        // nothing.
        self.signal.source.invalidate();
        TARGET.with(|target| *target.borrow_mut() = Weak::new());
    }
}

/// What the source does on the main thread once a waker signalled it: one
/// call for the signals that came since the last.
unsafe extern "C-unwind" fn perform(_info: *mut c_void) {
    let shared = TARGET.with(|target| target.borrow().upgrade());
    if let Some(shared) = shared {
        shared.post(Pending::App(AppEvent::Wake));
    }
}
