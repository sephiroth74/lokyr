//! Windows, and the delegate that tells what happens to them.

use std::rc::Weak;

use bolt_platform_core::{PixelSize, Size, WindowAttributes, WindowEvent, WindowId, WindowSize};
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send};
use objc2_app_kit::{
    NSBackingStoreType, NSWindow, NSWindowDelegate, NSWindowOcclusionState, NSWindowStyleMask,
};
use objc2_foundation::{
    NSNotification, NSObject, NSObjectProtocol, NSPoint, NSRect, NSSize, NSString,
};

use crate::shared::{Pending, Shared};

/// A window that the app opened.
pub(crate) struct Window {
    ns_window: Retained<NSWindow>,
    // The window holds its delegate weakly.
    _delegate: Retained<WindowDelegate>,
}

impl Window {
    /// A window of the content size and the title of `attributes`, centred,
    /// and not yet shown.
    pub(crate) fn new(
        mtm: MainThreadMarker,
        shared: Weak<Shared>,
        id: WindowId,
        attributes: &WindowAttributes,
    ) -> Self {
        let content = NSRect::new(
            NSPoint::ZERO,
            NSSize::new(attributes.size.width, attributes.size.height),
        );
        let style = NSWindowStyleMask::Titled
            | NSWindowStyleMask::Closable
            | NSWindowStyleMask::Miniaturizable
            | NSWindowStyleMask::Resizable;
        // SAFETY: a new window, of a valid style, which `setReleasedWhenClosed`
        // below keeps from being released twice.
        let ns_window = unsafe {
            NSWindow::initWithContentRect_styleMask_backing_defer(
                NSWindow::alloc(mtm),
                content,
                style,
                NSBackingStoreType::Buffered,
                false,
            )
        };
        // SAFETY: `Retained` owns the window, which closing must then not
        // release.
        unsafe { ns_window.setReleasedWhenClosed(false) };
        ns_window.setTitle(&NSString::from_str(&attributes.title));
        ns_window.center();
        let delegate = WindowDelegate::new(mtm, shared, id);
        ns_window.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
        Self {
            ns_window,
            _delegate: delegate,
        }
    }

    pub(crate) fn ns_window(&self) -> &NSWindow {
        &self.ns_window
    }

    /// Brings the window in front of the others, with the focus.
    pub(crate) fn show(&self) {
        self.ns_window.makeKeyAndOrderFront(None);
    }

    pub(crate) fn close(&self) {
        // What the window does as it closes, such as losing the focus, has
        // no window left to tell it to.
        self.ns_window.setDelegate(None);
        if let Some(sheet) = self.ns_window.attachedSheet() {
            self.ns_window.endSheet(&sheet);
        }
        self.ns_window.close();
    }

    pub(crate) fn set_title(&self, title: &str) {
        self.ns_window.setTitle(&NSString::from_str(title));
    }

    pub(crate) fn size(&self) -> WindowSize {
        size_of(&self.ns_window)
    }

    pub(crate) fn is_focused(&self) -> bool {
        self.ns_window.isKeyWindow()
    }
}

/// The size of a window's content, in points and in the pixels of its
/// backing store.
fn size_of(window: &NSWindow) -> WindowSize {
    let scale = window.backingScaleFactor();
    let Some(view) = window.contentView() else {
        let frame = window.contentRectForFrameRect(window.frame());
        return WindowSize::new(points(frame.size), scale);
    };
    let bounds = view.bounds();
    let backing = view.convertRectToBacking(bounds);
    WindowSize {
        points: points(bounds.size),
        pixels: PixelSize {
            // A cast from a float saturates, and takes NaN to 0.
            width: backing.size.width.round() as u32,
            height: backing.size.height.round() as u32,
        },
        scale,
    }
}

fn points(size: NSSize) -> Size {
    Size {
        width: size.width,
        height: size.height,
    }
}

pub(crate) struct WindowDelegateIvars {
    shared: Weak<Shared>,
    id: WindowId,
}

define_class!(
    // SAFETY: `NSObject` has no requirement of its subclasses, and the
    // delegate does not implement `Drop`.
    #[unsafe(super = NSObject)]
    #[thread_kind = MainThreadOnly]
    #[ivars = WindowDelegateIvars]
    pub(crate) struct WindowDelegate;

    // SAFETY: `NSObjectProtocol` has no requirement.
    unsafe impl NSObjectProtocol for WindowDelegate {}

    // SAFETY: `NSWindowDelegate` has no requirement; each method has
    // AppKit's signature.
    unsafe impl NSWindowDelegate for WindowDelegate {
        #[unsafe(method(windowShouldClose:))]
        fn window_should_close(&self, _sender: &NSWindow) -> bool {
            // The window stays open until the app closes it.
            self.post(WindowEvent::CloseRequested);
            false
        }

        #[unsafe(method(windowDidResize:))]
        fn window_did_resize(&self, notification: &NSNotification) {
            if let Some(window) = window_of(notification) {
                self.post(WindowEvent::Resized(size_of(&window)));
            }
        }

        #[unsafe(method(windowDidChangeBackingProperties:))]
        fn window_did_change_backing_properties(&self, notification: &NSNotification) {
            if let Some(window) = window_of(notification) {
                self.post(WindowEvent::ScaleChanged(size_of(&window)));
            }
        }

        #[unsafe(method(windowDidBecomeKey:))]
        fn window_did_become_key(&self, _notification: &NSNotification) {
            self.post(WindowEvent::Focused(true));
        }

        #[unsafe(method(windowDidResignKey:))]
        fn window_did_resign_key(&self, _notification: &NSNotification) {
            self.post(WindowEvent::Focused(false));
        }

        #[unsafe(method(windowDidChangeOcclusionState:))]
        fn window_did_change_occlusion_state(&self, notification: &NSNotification) {
            if let Some(window) = window_of(notification) {
                let visible = window
                    .occlusionState()
                    .contains(NSWindowOcclusionState::Visible);
                self.post(WindowEvent::Occluded(!visible));
            }
        }
    }
);

impl WindowDelegate {
    fn new(mtm: MainThreadMarker, shared: Weak<Shared>, id: WindowId) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(WindowDelegateIvars { shared, id });
        // SAFETY: `NSObject`'s `init` has this signature.
        unsafe { msg_send![super(this), init] }
    }

    fn post(&self, event: WindowEvent) {
        let ivars = self.ivars();
        if let Some(shared) = ivars.shared.upgrade() {
            shared.post(Pending::Window(ivars.id, event));
        }
    }
}

/// The window that a window's notification is about.
fn window_of(notification: &NSNotification) -> Option<Retained<NSWindow>> {
    notification.object()?.downcast::<NSWindow>().ok()
}
