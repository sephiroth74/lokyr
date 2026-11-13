//! The accessories of a window's titlebar: the text at its right, as the
//! keys that bring it forward, a label of the titlebar's font, in the
//! secondary label's color, after AppKit's own; and the room at its left
//! for the content's small buttons, which the system's title leaves.

use objc2::rc::Retained;
use objc2::{MainThreadOnly, define_class, msg_send};
use objc2_app_kit::{
    NSAutoresizingMaskOptions, NSColor, NSFont, NSLayoutAttribute, NSResponder, NSTextField,
    NSTitlebarAccessoryViewController, NSView, NSWindow,
};
use objc2_foundation::{MainThreadMarker, NSObject, NSPoint, NSRect, NSSize, NSString};

/// The room right of the text, in points, as the titlebar leaves left of
/// the window's buttons.
const MARGIN: f64 = 10.0;

/// The height of the accessory's view before AppKit gives it the
/// titlebar's, in points, which the label is centred in.
const HEIGHT: f64 = 28.0;

/// A window's accessory of its titlebar's right, and its label.
pub(crate) struct TitleAccessory {
    controller: Retained<NSTitlebarAccessoryViewController>,
    label: Retained<NSTextField>,
    view: Retained<NSView>,
}

impl TitleAccessory {
    /// The accessory of `text`, added to `window`'s titlebar.
    pub(crate) fn add(window: &NSWindow, text: &str, mtm: MainThreadMarker) -> Self {
        let label = NSTextField::labelWithString(&NSString::from_str(text), mtm);
        label.setTextColor(Some(&NSColor::secondaryLabelColor()));
        label.setFont(Some(&NSFont::titleBarFontOfSize(0.0)));
        // Centred as AppKit makes the view the titlebar's height.
        label.setAutoresizingMask(
            NSAutoresizingMaskOptions::ViewMinYMargin | NSAutoresizingMaskOptions::ViewMaxYMargin,
        );
        let view = NSView::initWithFrame(
            NSView::alloc(mtm),
            NSRect::new(NSPoint::ZERO, NSSize::new(MARGIN, HEIGHT)),
        );
        view.addSubview(&label);
        let controller = NSTitlebarAccessoryViewController::new(mtm);
        controller.setView(&view);
        controller.setLayoutAttribute(NSLayoutAttribute::Trailing);
        let accessory = Self {
            controller,
            label,
            view,
        };
        accessory.lay_out();
        window.addTitlebarAccessoryViewController(&accessory.controller);
        accessory
    }

    /// Shows `text` in place of the text before.
    pub(crate) fn set_text(&self, text: &str) {
        self.label.setStringValue(&NSString::from_str(text));
        self.lay_out();
    }

    /// Takes the accessory out of its window's titlebar.
    pub(crate) fn remove(&self) {
        self.controller.removeFromParentViewController();
    }

    /// Sizes the label to its text, centred in the view's height, and the
    /// view to the label with its margin.
    fn lay_out(&self) {
        let size = self.label.fittingSize();
        let height = self.view.frame().size.height;
        self.label.setFrame(NSRect::new(
            NSPoint::new(0.0, ((height - size.height) / 2.0).round()),
            size,
        ));
        self.view
            .setFrameSize(NSSize::new(size.width + MARGIN, height));
    }
}

define_class!(
    // SAFETY: `NSView` has no requirement of its subclasses that this one
    // breaks, and the view does not implement `Drop`.
    #[unsafe(super(NSView, NSResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    pub(crate) struct TitleRoomView;

    impl TitleRoomView {
        // SAFETY: AppKit's signature: clicks go through, to the content's
        // view, which draws the small buttons under it.
        #[unsafe(method_id(hitTest:))]
        fn hit_test(&self, _point: NSPoint) -> Option<Retained<NSView>> {
            None
        }
    }
);

/// A window's accessory of its titlebar's left, right after the window's
/// buttons: an empty room of no click, as wide as the content's small
/// buttons there and the margin after the window's buttons. AppKit sets the
/// system's title past it, and, of a room at the left, centres the title in
/// the window, where macOS 27 would set it at the left, right after the
/// window's buttons; of a width of 0 too.
pub(crate) struct TitleRoom {
    controller: Retained<NSTitlebarAccessoryViewController>,
    view: Retained<TitleRoomView>,
}

impl TitleRoom {
    /// The room of `width` points, added to `window`'s titlebar.
    pub(crate) fn add(window: &NSWindow, width: f64, mtm: MainThreadMarker) -> Self {
        // SAFETY: `NSView`'s `initWithFrame:` has this signature.
        let view: Retained<TitleRoomView> = unsafe {
            msg_send![
                TitleRoomView::alloc(mtm),
                initWithFrame: NSRect::new(NSPoint::ZERO, NSSize::new(width.max(0.0), HEIGHT))
            ]
        };
        let controller = NSTitlebarAccessoryViewController::new(mtm);
        controller.setView(&view);
        controller.setLayoutAttribute(NSLayoutAttribute::Leading);
        window.addTitlebarAccessoryViewController(&controller);
        Self { controller, view }
    }

    /// Makes the room `width` points wide.
    pub(crate) fn set_width(&self, width: f64) {
        let height = self.view.frame().size.height;
        self.view.setFrameSize(NSSize::new(width.max(0.0), height));
    }

    /// Takes the room out of its window's titlebar.
    pub(crate) fn remove(&self) {
        self.controller.removeFromParentViewController();
    }
}
