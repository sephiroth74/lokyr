//! Alerts, on a window as a sheet, or on their own.

use std::rc::Weak;

use block2::RcBlock;
use lokyr_platform_core::{Alert, AlertId, AlertStyle};
use objc2::rc::Retained;
use objc2::runtime::{Bool, Sel};
use objc2::{ClassType, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{
    NSAlert, NSAlertFirstButtonReturn, NSAlertStyle, NSApplication, NSControl,
    NSControlStateValueOn, NSEvent, NSEventModifierFlags, NSModalResponse, NSResponder,
    NSTextField, NSView, NSWindow,
};
use objc2_foundation::{NSObject, NSPoint, NSRect, NSSize, NSString};

use crate::shared::{Pending, Shared};

/// The alert that shows `alert`.
pub(crate) fn new(mtm: MainThreadMarker, alert: &Alert) -> Retained<NSAlert> {
    let ns_alert = NSAlert::new(mtm);
    ns_alert.setMessageText(&NSString::from_str(&alert.message));
    ns_alert.setInformativeText(&NSString::from_str(&alert.detail));
    ns_alert.setAlertStyle(match alert.style {
        AlertStyle::Informational => NSAlertStyle::Informational,
        AlertStyle::Warning => NSAlertStyle::Warning,
        AlertStyle::Critical => NSAlertStyle::Critical,
    });
    for title in &alert.buttons {
        ns_alert.addButtonWithTitle(&NSString::from_str(title));
    }
    if let Some(title) = &alert.suppression {
        ns_alert.setShowsSuppressionButton(true);
        if let Some(checkbox) = ns_alert.suppressionButton() {
            checkbox.setTitle(&NSString::from_str(title));
        }
    }
    if let Some(text) = &alert.input {
        // SAFETY: `NSTextField`'s factory of a field of `text`, which makes
        // one of the class that it is sent to.
        let field: Retained<Field> =
            unsafe { msg_send![Field::class(), textFieldWithString: &*NSString::from_str(text)] };
        field.setFrame(NSRect::new(
            NSPoint::ZERO,
            NSSize::new(FIELD_WIDTH, FIELD_HEIGHT),
        ));
        ns_alert.setAccessoryView(Some(&field));
        // The field takes the keys as the alert opens.
        ns_alert.window().setInitialFirstResponder(Some(&field));
    }
    ns_alert
}

define_class!(
    /// An alert's field of text, which takes the keys of its editing, as
    /// ⌘V, while it is edited: AppKit's field editor takes them as actions
    /// sent to no target, which the main menu's items, whose target is
    /// theirs, never send, and the window asks its views about a key with
    /// Command before the main menu.
    // SAFETY: `NSTextField` has no requirement of its subclasses, and the
    // field does not implement `Drop`.
    #[unsafe(super(NSTextField, NSControl, NSView, NSResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    struct Field;

    impl Field {
        // SAFETY: `NSResponder`'s signature.
        #[unsafe(method(performKeyEquivalent:))]
        fn perform_key_equivalent(&self, event: &NSEvent) -> Bool {
            if let Some(action) = edit_action(event)
                && self.currentEditor().is_some()
            {
                let app = NSApplication::sharedApplication(self.mtm());
                // SAFETY: an action of the responder chain's, sent to the
                // first responder, the field's editor.
                return Bool::new(unsafe { app.sendAction_to_from(action, None, Some(self)) });
            }
            // SAFETY: the same method of the superclass.
            unsafe { msg_send![super(self), performKeyEquivalent: event] }
        }
    }
);

/// The action of editing that `event`, a key with Command, sends, as the
/// standard Edit menu's items have them: ⌘X, ⌘C, ⌘V, ⌘A, ⌘Z and ⇧⌘Z.
fn edit_action(event: &NSEvent) -> Option<Sel> {
    let held = event.modifierFlags()
        & (NSEventModifierFlags::Command
            | NSEventModifierFlags::Shift
            | NSEventModifierFlags::Option
            | NSEventModifierFlags::Control);
    let key = event
        .charactersIgnoringModifiers()?
        .to_string()
        .to_lowercase();
    let shifted = if held == NSEventModifierFlags::Command {
        false
    } else if held == NSEventModifierFlags::Command | NSEventModifierFlags::Shift {
        true
    } else {
        return None;
    };
    Some(match (key.as_str(), shifted) {
        ("x", false) => sel!(cut:),
        ("c", false) => sel!(copy:),
        ("v", false) => sel!(paste:),
        ("a", false) => sel!(selectAll:),
        ("z", false) => sel!(undo:),
        ("z", true) => sel!(redo:),
        _ => return None,
    })
}

/// The size of an alert's field of text, in points.
const FIELD_WIDTH: f64 = 300.0;
const FIELD_HEIGHT: f64 = 24.0;

/// The text of the field of `ns_alert`, once it closed: `None` for one that
/// shows none.
pub(crate) fn text(ns_alert: &NSAlert) -> Option<String> {
    let field = ns_alert.accessoryView()?.downcast::<NSTextField>().ok()?;
    Some(field.stringValue().to_string())
}

/// Whether the checkbox of `ns_alert`, once it closed, was ticked: `false`
/// for one that shows none.
pub(crate) fn suppressed(ns_alert: &NSAlert) -> bool {
    ns_alert.showsSuppressionButton()
        && ns_alert
            .suppressionButton()
            .is_some_and(|checkbox| checkbox.state() == NSControlStateValueOn)
}

/// Shows `ns_alert` as a sheet of `window`, whose answer comes to the
/// handler once it closes.
pub(crate) fn begin_sheet(
    ns_alert: &NSAlert,
    window: &NSWindow,
    shared: Weak<Shared>,
    alert: AlertId,
) {
    let completion = RcBlock::new(move |response: NSModalResponse| {
        if let Some(shared) = shared.upgrade() {
            shared.post(Pending::AlertResponse(alert, response));
        }
    });
    ns_alert.beginSheetModalForWindow_completionHandler(window, Some(&completion));
}

/// The index of the button that AppKit's `response` names, among `buttons`;
/// AppKit gives an alert without buttons an OK button. A response that
/// names no button, as when the alert is stopped, counts as the last one,
/// which cancels.
pub(crate) fn button(response: NSModalResponse, buttons: usize) -> usize {
    let last = buttons.max(1) - 1;
    usize::try_from(response - NSAlertFirstButtonReturn)
        .ok()
        .filter(|&button| button <= last)
        .unwrap_or(last)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_response_names_its_button() {
        assert_eq!(button(NSAlertFirstButtonReturn, 2), 0);
        assert_eq!(button(NSAlertFirstButtonReturn + 1, 2), 1);
        // Out of range, or stopped: the last button.
        assert_eq!(button(NSAlertFirstButtonReturn + 5, 2), 1);
        assert_eq!(button(-1000, 3), 2);
        // An alert without buttons has AppKit's OK.
        assert_eq!(button(NSAlertFirstButtonReturn, 0), 0);
    }
}
