//! The application's About panel: AppKit's standard one, of the bundle's
//! name, icon, version, build and copyright, with the app's credits.

use lokyr_platform_core::{AboutPanel, TargetKind};
use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2_app_kit::{
    NSAboutPanelOptionCredits, NSApplication, NSColor, NSFont, NSFontAttributeName,
    NSForegroundColorAttributeName, NSLinkAttributeName, NSMutableParagraphStyle,
    NSParagraphStyleAttributeName, NSTextAlignment,
};
use objc2_foundation::{NSAttributedString, NSDictionary, NSMutableAttributedString, NSString};

/// Opens the About panel of `about`'s credits, or brings it to the front.
pub(crate) fn show(mtm: MainThreadMarker, about: &AboutPanel) {
    let credits = credits(about);
    // SAFETY: AppKit's key of the panel's credits, which takes an
    // attributed string.
    let key = unsafe { NSAboutPanelOptionCredits };
    let options: Retained<NSDictionary<_, AnyObject>> =
        NSDictionary::from_retained_objects(&[key], &[credits.into()]);
    let app = NSApplication::sharedApplication(mtm);
    // SAFETY: the options' dictionary of the panel's keys, the credits an
    // attributed string as the key asks.
    unsafe { app.orderFrontStandardAboutPanelWithOptions(&options) };
}

/// The credits of `about`, a line each, centered in the system's small
/// font and the label's color, which follow the appearance; a line of a
/// link opens it at a click, as the panel's text opens its links, but a
/// program's, which would run.
fn credits(about: &AboutPanel) -> Retained<NSMutableAttributedString> {
    let credits = NSMutableAttributedString::new();
    let font = NSFont::systemFontOfSize(NSFont::smallSystemFontSize());
    let color = NSColor::labelColor();
    let paragraph = NSMutableParagraphStyle::new();
    paragraph.setAlignment(NSTextAlignment::Center);
    for (index, line) in about.credits.iter().enumerate() {
        let text = if index + 1 < about.credits.len() {
            format!("{}\n", line.text)
        } else {
            line.text.clone()
        };
        // SAFETY: AppKit's keys of a font, a color and a paragraph's style.
        let mut keys = unsafe {
            vec![
                NSFontAttributeName,
                NSForegroundColorAttributeName,
                NSParagraphStyleAttributeName,
            ]
        };
        let mut values: Vec<Retained<AnyObject>> = vec![
            font.clone().into(),
            color.clone().into(),
            paragraph.clone().into(),
        ];
        let link = line
            .link
            .as_ref()
            .filter(|target| target.kind != TargetKind::Program)
            .and_then(|target| crate::open::url_of(target).ok());
        if let Some(url) = link {
            // SAFETY: AppKit's key of a link, which takes a URL.
            keys.push(unsafe { NSLinkAttributeName });
            values.push(url.into());
        }
        let attributes = NSDictionary::from_retained_objects(&keys, &values);
        // SAFETY: the attributes of their keys' values, a font, a color, a
        // paragraph's style and a URL.
        let part = unsafe {
            NSAttributedString::new_with_attributes(&NSString::from_str(&text), &attributes)
        };
        credits.appendAttributedString(&part);
    }
    credits
}
