//! The text at the right of a window's titlebar, as the keys that bring it
//! forward: a label of the titlebar's font, in the secondary label's color,
//! in an accessory of the titlebar, after AppKit's own.

use objc2::MainThreadOnly;
use objc2::rc::Retained;
use objc2_app_kit::{
    NSAutoresizingMaskOptions, NSColor, NSFont, NSLayoutAttribute, NSTextField,
    NSTitlebarAccessoryViewController, NSView, NSWindow,
};
use objc2_foundation::{MainThreadMarker, NSPoint, NSRect, NSSize, NSString};

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
