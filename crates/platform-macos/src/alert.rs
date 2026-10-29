//! Alerts, on a window as a sheet, or on their own.

use std::rc::Weak;

use block2::RcBlock;
use lokyr_platform_core::{Alert, AlertId, AlertStyle};
use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2_app_kit::{
    NSAlert, NSAlertFirstButtonReturn, NSAlertStyle, NSControlStateValueOn, NSModalResponse,
    NSTextField, NSWindow,
};
use objc2_foundation::{NSPoint, NSRect, NSSize, NSString};

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
        let field = NSTextField::textFieldWithString(&NSString::from_str(text), mtm);
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
