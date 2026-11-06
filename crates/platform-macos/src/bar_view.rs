//! The view that takes the presses over a window's titlebar for the
//! content's: the system's titlebar takes those that hit the content's view
//! itself, which never come to it, and drags the window from them, or does
//! nothing, as the content's view says; those that hit another view come to
//! it. This one, which the content's view gives as the view under the
//! pointer over the titlebar, of no drawing, hands them to the content's
//! view, which tells the content's parts from the window's, drags the
//! window from the window's itself and zooms it of a double click: its
//! presses, drags and releases, and its scrolls.

use objc2::rc::Retained;
use objc2::{MainThreadMarker, MainThreadOnly, define_class, msg_send};
use objc2_app_kit::{NSEvent, NSResponder, NSView, NSWindowOrderingMode};
use objc2_foundation::{NSObject, NSPoint, NSRect, NSSize};

define_class!(
    // SAFETY: `NSView` has no requirement of its subclasses that this one
    // breaks, and the view does not implement `Drop`.
    #[unsafe(super(NSView, NSResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    pub(crate) struct BarView;

    impl BarView {
        // SAFETY: AppKit's signature: a press over the view never moves the
        // window.
        #[unsafe(method(mouseDownCanMoveWindow))]
        fn mouse_down_can_move_window(&self) -> bool {
            false
        }

        // SAFETY: AppKit's signature: the content's view's answer.
        #[unsafe(method(acceptsFirstMouse:))]
        fn accepts_first_mouse(&self, event: Option<&NSEvent>) -> bool {
            // SAFETY: the content view's method, of this signature.
            self.content()
                .is_some_and(|content| unsafe { msg_send![&*content, acceptsFirstMouse: event] })
        }

        // SAFETY: AppKit's signatures, each of which takes an event, which
        // goes to the content's view.
        #[unsafe(method(mouseDown:))]
        fn mouse_down(&self, event: &NSEvent) {
            if let Some(content) = self.content() {
                // SAFETY: the content view's method, which takes an event.
                let () = unsafe { msg_send![&*content, mouseDown: event] };
            }
        }

        #[unsafe(method(mouseUp:))]
        fn mouse_up(&self, event: &NSEvent) {
            if let Some(content) = self.content() {
                // SAFETY: the content view's method, which takes an event.
                let () = unsafe { msg_send![&*content, mouseUp: event] };
            }
        }

        #[unsafe(method(mouseDragged:))]
        fn mouse_dragged(&self, event: &NSEvent) {
            if let Some(content) = self.content() {
                // SAFETY: the content view's method, which takes an event.
                let () = unsafe { msg_send![&*content, mouseDragged: event] };
            }
        }

        #[unsafe(method(rightMouseDown:))]
        fn right_mouse_down(&self, event: &NSEvent) {
            if let Some(content) = self.content() {
                // SAFETY: the content view's method, which takes an event.
                let () = unsafe { msg_send![&*content, rightMouseDown: event] };
            }
        }

        #[unsafe(method(rightMouseUp:))]
        fn right_mouse_up(&self, event: &NSEvent) {
            if let Some(content) = self.content() {
                // SAFETY: the content view's method, which takes an event.
                let () = unsafe { msg_send![&*content, rightMouseUp: event] };
            }
        }

        #[unsafe(method(rightMouseDragged:))]
        fn right_mouse_dragged(&self, event: &NSEvent) {
            if let Some(content) = self.content() {
                // SAFETY: the content view's method, which takes an event.
                let () = unsafe { msg_send![&*content, rightMouseDragged: event] };
            }
        }

        #[unsafe(method(otherMouseDown:))]
        fn other_mouse_down(&self, event: &NSEvent) {
            if let Some(content) = self.content() {
                // SAFETY: the content view's method, which takes an event.
                let () = unsafe { msg_send![&*content, otherMouseDown: event] };
            }
        }

        #[unsafe(method(otherMouseUp:))]
        fn other_mouse_up(&self, event: &NSEvent) {
            if let Some(content) = self.content() {
                // SAFETY: the content view's method, which takes an event.
                let () = unsafe { msg_send![&*content, otherMouseUp: event] };
            }
        }

        #[unsafe(method(otherMouseDragged:))]
        fn other_mouse_dragged(&self, event: &NSEvent) {
            if let Some(content) = self.content() {
                // SAFETY: the content view's method, which takes an event.
                let () = unsafe { msg_send![&*content, otherMouseDragged: event] };
            }
        }

        #[unsafe(method(scrollWheel:))]
        fn scroll_wheel(&self, event: &NSEvent) {
            if let Some(content) = self.content() {
                // SAFETY: the content view's method, which takes an event.
                let () = unsafe { msg_send![&*content, scrollWheel: event] };
            }
        }
    }
);

impl BarView {
    /// The content's view, which takes the view's events.
    fn content(&self) -> Option<Retained<NSView>> {
        // SAFETY: the view's superview is the content's, while it lives.
        unsafe { self.superview() }
    }

    /// The view, of the content view `over`, under it: a point of it in its
    /// corner, which no press needs to hit, as the content's view gives it
    /// as the view under the pointer.
    pub(crate) fn new(mtm: MainThreadMarker, over: &NSView) -> Retained<Self> {
        let frame = NSRect::new(NSPoint::ZERO, NSSize::new(1.0, 1.0));
        // SAFETY: `NSView`'s `initWithFrame:` has this signature.
        let view: Retained<Self> = unsafe { msg_send![Self::alloc(mtm), initWithFrame: frame] };
        over.addSubview_positioned_relativeTo(&view, NSWindowOrderingMode::Below, None);
        view
    }
}
