//! The system's color panel, `NSColorPanel`: shown of a color, each color
//! that the user chooses in it, as it moves, and its closing told to the
//! app.

use std::cell::Cell;
use std::rc::Weak;

use lokyr_platform_core::{AppEvent, PanelColor};
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{NSColor, NSColorPanel, NSColorSpace, NSWindowWillCloseNotification};
use objc2_foundation::{NSNotification, NSNotificationCenter, NSObject, NSObjectProtocol};

use crate::shared::{Pending, Shared};

pub(crate) struct TargetIvars {
    shared: Weak<Shared>,
    /// Whether the panel's color is the app's, as it gives it, of no
    /// choice of the user's to tell.
    giving: Cell<bool>,
    /// Whether the app watches the panel, which it shows.
    watching: Cell<bool>,
}

define_class!(
    // SAFETY: `NSObject` has no requirement of its subclasses, and the
    // target does not implement `Drop`.
    #[unsafe(super = NSObject)]
    #[thread_kind = MainThreadOnly]
    #[ivars = TargetIvars]
    pub(crate) struct ColorTarget;

    impl ColorTarget {
        // SAFETY: an action's signature, of its sender.
        #[unsafe(method(colorChanged:))]
        fn color_changed(&self, _sender: Option<&AnyObject>) {
            let ivars = self.ivars();
            if ivars.giving.get() || !ivars.watching.get() {
                return;
            }
            let color = NSColorPanel::sharedColorPanel(self.mtm()).color();
            if let (Some(color), Some(shared)) = (panel_color(&color), ivars.shared.upgrade()) {
                shared.post(Pending::App(AppEvent::ColorPicked(color)));
            }
        }

        // SAFETY: a notification's observer's signature.
        #[unsafe(method(panelClosing:))]
        fn panel_closing(&self, _notification: &NSNotification) {
            let ivars = self.ivars();
            if !ivars.watching.replace(false) {
                return;
            }
            if let Some(shared) = ivars.shared.upgrade() {
                shared.post(Pending::App(AppEvent::ColorPanelClosed));
            }
        }
    }

    // SAFETY: `NSObjectProtocol` has no requirement.
    unsafe impl NSObjectProtocol for ColorTarget {}
);

impl ColorTarget {
    /// The target of the color panel's choices and closing, which tells
    /// them to the app of `shared`.
    pub(crate) fn new(mtm: MainThreadMarker, shared: Weak<Shared>) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(TargetIvars {
            shared,
            giving: Cell::new(false),
            watching: Cell::new(false),
        });
        // SAFETY: `NSObject`'s `init` has this signature.
        let target: Retained<Self> = unsafe { msg_send![super(this), init] };
        let panel = NSColorPanel::sharedColorPanel(mtm);
        // SAFETY: an observer of a selector that it answers, of the panel's
        // own notification; the target lives as long as the backend, which
        // the application's.
        unsafe {
            NSNotificationCenter::defaultCenter().addObserver_selector_name_object(
                &target,
                sel!(panelClosing:),
                Some(NSWindowWillCloseNotification),
                Some(&panel),
            );
        }
        target
    }

    /// Shows the panel of `color`, of its opacity's slider where `alpha`,
    /// in front, its choices told as they move.
    pub(crate) fn show(&self, color: PanelColor, alpha: bool) {
        let panel = NSColorPanel::sharedColorPanel(self.mtm());
        let ivars = self.ivars();
        panel.setShowsAlpha(alpha);
        panel.setContinuous(true);
        // SAFETY: the target answers the action's selector, and lives as
        // long as the backend.
        unsafe {
            panel.setTarget(Some(self));
            panel.setAction(Some(sel!(colorChanged:)));
        }
        let component = |value: u8| f64::from(value) / 255.0;
        let given = NSColor::colorWithSRGBRed_green_blue_alpha(
            component(color.red),
            component(color.green),
            component(color.blue),
            component(color.alpha),
        );
        ivars.giving.set(true);
        panel.setColor(&given);
        ivars.giving.set(false);
        ivars.watching.set(true);
        panel.orderFront(None);
    }

    /// Closes the panel, of no closing told.
    pub(crate) fn close(&self) {
        let ivars = self.ivars();
        ivars.watching.set(false);
        if !NSColorPanel::sharedColorPanelExists(self.mtm()) {
            return;
        }
        let panel = NSColorPanel::sharedColorPanel(self.mtm());
        // SAFETY: no target, nor action.
        unsafe {
            panel.setTarget(None);
            panel.setAction(None);
        }
        panel.orderOut(None);
    }
}

/// `color` in sRGB, of 8 bits a component; none of a color of no sRGB's
/// components, as a pattern.
fn panel_color(color: &NSColor) -> Option<PanelColor> {
    let srgb = color.colorUsingColorSpace(&NSColorSpace::sRGBColorSpace())?;
    let byte = |value: f64| (value.clamp(0.0, 1.0) * 255.0).round() as u8;
    Some(PanelColor {
        red: byte(srgb.redComponent()),
        green: byte(srgb.greenComponent()),
        blue: byte(srgb.blueComponent()),
        alpha: byte(srgb.alphaComponent()),
    })
}
