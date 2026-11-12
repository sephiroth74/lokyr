//! The image of a drag, as of a tab dragged out of its bar: a window of no
//! frame and no shadow over every window, those in full screen too, which
//! the pointer goes through, of a layer that the renderer draws, clear
//! where it draws nothing, and which slides back as the drag ends where it
//! started.

use std::ffi::c_void;
use std::ptr::NonNull;
use std::time::Duration;

use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2_app_kit::{
    NSBackingStoreType, NSColor, NSPopUpMenuWindowLevel, NSWindow, NSWindowAnimationBehavior,
    NSWindowCollectionBehavior, NSWindowStyleMask,
};
use objc2_foundation::{NSPoint, NSRect};

use crate::layer::LayerView;
use crate::quick;

/// How long the image takes to slide back to where the drag started.
const SLIDE_BACK: Duration = Duration::from_millis(200);

/// The image of a drag, while it shows.
pub(crate) struct DragImage {
    window: Retained<NSWindow>,
    view: Retained<LayerView>,
}

impl DragImage {
    /// The image at `frame`, of AppKit's coordinates, in front of every
    /// window.
    pub(crate) fn new(mtm: MainThreadMarker, frame: NSRect) -> Self {
        // SAFETY: a new window, of a valid style, which
        // `setReleasedWhenClosed` below keeps from being released twice.
        let window = unsafe {
            NSWindow::initWithContentRect_styleMask_backing_defer(
                mtm.alloc(),
                frame,
                NSWindowStyleMask::Borderless,
                NSBackingStoreType::Buffered,
                false,
            )
        };
        // SAFETY: `Retained` owns the window, which closing must then not
        // release.
        unsafe { window.setReleasedWhenClosed(false) };
        window.setOpaque(false);
        window.setBackgroundColor(Some(&NSColor::clearColor()));
        // The image draws its own shadow, of its shape.
        window.setHasShadow(false);
        window.setIgnoresMouseEvents(true);
        window.setLevel(NSPopUpMenuWindowLevel);
        window.setCollectionBehavior(
            NSWindowCollectionBehavior::CanJoinAllSpaces
                | NSWindowCollectionBehavior::Transient
                | NSWindowCollectionBehavior::FullScreenAuxiliary
                | NSWindowCollectionBehavior::IgnoresCycle,
        );
        window.setExcludedFromWindowsMenu(true);
        window.setAnimationBehavior(NSWindowAnimationBehavior::None);
        let view = LayerView::filling(mtm, NSRect::new(NSPoint::ZERO, frame.size));
        window.setContentView(Some(&view));
        window.orderFrontRegardless();
        Self { window, view }
    }

    /// Moves the image to `frame`, of AppKit's coordinates, of its size.
    pub(crate) fn place(&self, frame: NSRect) {
        let now = self.window.frame();
        if now.size == frame.size {
            if now.origin != frame.origin {
                self.window.setFrameOrigin(frame.origin);
            }
        } else {
            self.window.setFrame_display(frame, false);
        }
    }

    /// The view of its layer, by its address, for a renderer: it lives as
    /// long as the image.
    pub(crate) fn address(&self) -> NonNull<c_void> {
        self.view.address()
    }

    /// Hides the image, at once, or sliding to `to`, of AppKit's
    /// coordinates, and fading out as it goes, at once where the system
    /// reduces motion; the window goes once it hid.
    pub(crate) fn hide(self, to: Option<NSRect>) {
        let Some(to) = to else {
            self.window.orderOut(None);
            return;
        };
        let ended = self.window.clone();
        quick::slide(
            &self.window,
            quick::duration(SLIDE_BACK),
            true,
            move |animator| {
                animator.setFrame_display(to, true);
                animator.setAlphaValue(0.0);
            },
            move || ended.orderOut(None),
        );
    }
}
