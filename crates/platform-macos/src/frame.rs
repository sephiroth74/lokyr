//! Frame pacing: the display link of a window's view, which runs only while
//! the app asks for frames.

use std::cell::Cell;
use std::rc::Weak;

use bolt_platform_core::{WindowEvent, WindowId};
use objc2::rc::Retained;
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::NSView;
use objc2_foundation::{NSObject, NSObjectProtocol, NSRunLoop, NSRunLoopCommonModes};
use objc2_quartz_core::CADisplayLink;

use crate::shared::{Pending, Shared};

pub(crate) struct FrameTargetIvars {
    shared: Weak<Shared>,
    window: WindowId,
    /// Whether the app asked for a frame since the last refresh.
    requested: Cell<bool>,
    /// Whether a frame due at once is queued, which the requests that come
    /// before it take as theirs.
    queued: Cell<bool>,
}

define_class!(
    // SAFETY: `NSObject` has no requirement of its subclasses, and the
    // target does not implement `Drop`.
    #[unsafe(super = NSObject)]
    #[thread_kind = MainThreadOnly]
    #[ivars = FrameTargetIvars]
    pub(crate) struct FrameTarget;

    impl FrameTarget {
        // SAFETY: a display link's action, with its signature.
        #[unsafe(method(frame:))]
        fn frame(&self, link: &CADisplayLink) {
            let ivars = self.ivars();
            if !ivars.requested.replace(false) {
                // A refresh that no frame was asked for stops the link,
                // which then wakes nothing until the next request.
                link.setPaused(true);
                return;
            }
            if let Some(shared) = ivars.shared.upgrade() {
                shared.post(Pending::Window(ivars.window, WindowEvent::Frame));
            }
        }
    }

    // SAFETY: `NSObjectProtocol` has no requirement.
    unsafe impl NSObjectProtocol for FrameTarget {}
);

/// The display link of a window's view, which follows the display that the
/// view is on, at its refresh rate, in every common mode of the main run
/// loop, so that it runs in a live resize and while a menu is open.
pub(crate) struct Pacer {
    link: Retained<CADisplayLink>,
    target: Retained<FrameTarget>,
}

impl Pacer {
    /// A stopped display link of `view`, whose frames go to `window`.
    pub(crate) fn new(
        mtm: MainThreadMarker,
        view: &NSView,
        shared: Weak<Shared>,
        window: WindowId,
    ) -> Self {
        let target = FrameTarget::alloc(mtm).set_ivars(FrameTargetIvars {
            shared,
            window,
            requested: Cell::new(false),
            queued: Cell::new(false),
        });
        // SAFETY: `NSObject`'s `init` has this signature.
        let target: Retained<FrameTarget> = unsafe { msg_send![super(target), init] };
        // SAFETY: the target is a `FrameTarget`, whose `frame:` takes the
        // link, as a display link's action does.
        let link = unsafe { view.displayLinkWithTarget_selector(&target, sel!(frame:)) };
        link.setPaused(true);
        // SAFETY: the main run loop, and its common modes, by AppKit's
        // constant.
        unsafe { link.addToRunLoop_forMode(&NSRunLoop::mainRunLoop(), NSRunLoopCommonModes) };
        Self { link, target }
    }

    /// Asks for a frame. Returns whether it is due at once: the link had
    /// stopped, after a refresh without a frame, and starts again for the
    /// frames after this one.
    pub(crate) fn request(&self) -> bool {
        let ivars = self.target.ivars();
        if ivars.queued.get() {
            return false;
        }
        if self.link.isPaused() {
            self.link.setPaused(false);
            ivars.queued.set(true);
            true
        } else {
            ivars.requested.set(true);
            false
        }
    }

    /// The frame came: requests from now on are for the next.
    pub(crate) fn delivered(&self) {
        self.target.ivars().queued.set(false);
    }

    /// Takes the link off the run loop, for good.
    pub(crate) fn stop(&self) {
        self.link.invalidate();
    }
}
