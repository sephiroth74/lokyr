//! The system's definitions of words, in its dictionary's panel, which
//! draws the word again where it is.

use lokyr_platform_core::Point;
use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2_app_kit::{NSFont, NSFontAttributeName, NSFontManager, NSFontTraitMask, NSView};
use objc2_foundation::{NSAttributedString, NSDictionary, NSPoint, NSString};

/// The weight that the font manager gives a family's regular face.
const REGULAR: isize = 5;

/// Shows the definition of `text` over `view`: the left end of its baseline
/// at `baseline`, from the view's top left corner, in `font`, a family and
/// its size in points, or in the system's font of a fixed pitch.
pub(crate) fn show(
    mtm: MainThreadMarker,
    view: &NSView,
    text: &str,
    baseline: Point,
    font: Option<(&str, f64)>,
) {
    let size = font.map_or(0.0, |(_, size)| size);
    let ns_font = font
        .and_then(|(family, size)| {
            NSFontManager::sharedFontManager(mtm).fontWithFamily_traits_weight_size(
                &NSString::from_str(family),
                NSFontTraitMask::empty(),
                REGULAR,
                size,
            )
        })
        .or_else(|| NSFont::userFixedPitchFontOfSize(size));
    let string = NSString::from_str(text);
    let shown: Retained<NSAttributedString> = match ns_font {
        Some(ns_font) => {
            // SAFETY: AppKit's key of a font, with a font.
            let key = unsafe { NSFontAttributeName };
            let attributes: Retained<NSDictionary<_, AnyObject>> =
                NSDictionary::from_retained_objects(
                    &[key],
                    &[Retained::into_super(Retained::into_super(ns_font))],
                );
            // SAFETY: the attributes are a font's, of its key.
            unsafe { NSAttributedString::new_with_attributes(&string, &attributes) }
        }
        None => NSAttributedString::from_nsstring(&string),
    };
    // The view's origin is its bottom left corner.
    let point = NSPoint::new(baseline.x, view.bounds().size.height - baseline.y);
    view.showDefinitionForAttributedString_atPoint(Some(&shown), point);
}
