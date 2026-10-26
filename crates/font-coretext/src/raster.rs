//! Glyphs drawn into bitmaps by CoreText: coverage for the glyphs that the
//! text's color fills, color for the glyphs of their own colors.

use std::ptr::NonNull;

use bolt_text_core::{Bitmap, BitmapFormat, GlyphId, Synthesis};
use objc2_core_foundation::{
    CFNumber, CFString, CGFloat, CGPoint, CGRect, kCFPreferencesCurrentApplication,
};
use objc2_core_graphics::{
    CGBitmapContextCreate, CGColorSpace, CGContext, CGImageAlphaInfo, CGImageByteOrderInfo,
    CGTextDrawingMode, kCGColorSpaceSRGB,
};
use objc2_core_text::{CTFont, CTFontOrientation, CTFontSymbolicTraits};

/// The slant of a synthesized italic: 12°, as a shear of x by y.
pub(crate) const ITALIC_SHEAR: CGFloat = 0.212_556_56;

/// The pixels that a bitmap keeps around a glyph's outline, for the
/// coverage that antialiasing and font smoothing put past it.
const MARGIN: CGFloat = 1.0;

/// Whether macOS smooths fonts, as the user set it, thickening their stems:
/// the default is to, which `AppleFontSmoothing` set to 0 turns off.
pub(crate) fn system_smooths_fonts() -> bool {
    let key = CFString::from_static_str("AppleFontSmoothing");
    // SAFETY: Core Foundation's constant.
    let domain = unsafe { kCFPreferencesCurrentApplication };
    objc2_core_foundation::CFPreferencesCopyAppValue(&key, domain)
        .and_then(|value| value.downcast::<CFNumber>().ok())
        .and_then(|number| number.as_i64())
        .is_none_or(|value| value != 0)
}

/// The width of the stroke that draws a synthesized bold at `size` pixels,
/// which thickens each side of a stem by half of it.
fn bold_stroke(size: CGFloat) -> CGFloat {
    (size / 24.0).max(1.0)
}

/// `glyph` of `font`, drawn as `synthesis` says, into a bitmap: of coverage,
/// smoothed when `smoothing` is on, or of color, when the font's glyphs have
/// colors of their own.
pub(crate) fn rasterize(
    font: &CTFont,
    glyph: GlyphId,
    size: CGFloat,
    synthesis: Synthesis,
    smoothing: bool,
) -> Bitmap {
    // SAFETY: a font that CoreText made.
    let color = unsafe { font.symbolic_traits() }.contains(CTFontSymbolicTraits::TraitColorGlyphs);
    let format = if color {
        BitmapFormat::Rgba
    } else {
        BitmapFormat::Gray
    };
    let mut glyph = glyph.0;
    let mut bounds = CGRect::ZERO;
    // SAFETY: one glyph, and room for its one rectangle.
    unsafe {
        font.bounding_rects_for_glyphs(
            CTFontOrientation::Horizontal,
            NonNull::from(&mut glyph),
            &raw mut bounds,
            1,
        );
    }
    if bounds.size.width <= 0.0 || bounds.size.height <= 0.0 {
        return Bitmap::empty(format);
    }

    let stroke = if synthesis.bold && !color {
        bold_stroke(size)
    } else {
        0.0
    };
    let margin = MARGIN + stroke / 2.0;
    let left = (bounds.origin.x - margin).floor();
    let right = (bounds.origin.x + bounds.size.width + margin).ceil();
    let bottom = (bounds.origin.y - margin).floor();
    let top = (bounds.origin.y + bounds.size.height + margin).ceil();
    // A cast from a float saturates, and takes NaN to 0.
    let width = (right - left) as usize;
    let height = (top - bottom) as usize;
    let bytes = if color { 4 } else { 1 };
    let mut data = vec![0_u8; width * height * bytes];

    let (space, info) = if color {
        // SAFETY: Core Graphics' constant.
        let space = CGColorSpace::with_name(Some(unsafe { kCGColorSpaceSRGB }));
        (
            space,
            CGImageAlphaInfo::PremultipliedLast.0 | CGImageByteOrderInfo::Order32Big.0,
        )
    } else {
        (None, CGImageAlphaInfo::Only.0)
    };
    // SAFETY: `data` holds `height` rows of `width * bytes` bytes, which the
    // context draws into, and outlives it.
    let Some(context) = (unsafe {
        CGBitmapContextCreate(
            data.as_mut_ptr().cast(),
            width,
            height,
            8,
            width * bytes,
            space.as_deref(),
            info,
        )
    }) else {
        return Bitmap::empty(format);
    };
    let drawing = Some(&*context);
    CGContext::set_should_antialias(drawing, true);
    CGContext::set_allows_font_smoothing(drawing, true);
    CGContext::set_should_smooth_fonts(drawing, smoothing && !color);
    CGContext::set_gray_fill_color(drawing, 1.0, 1.0);
    if stroke > 0.0 {
        CGContext::set_text_drawing_mode(drawing, CGTextDrawingMode::FillStroke);
        CGContext::set_gray_stroke_color(drawing, 1.0, 1.0);
        CGContext::set_line_width(drawing, stroke);
    }
    let mut origin = CGPoint::new(-left, -bottom);
    // SAFETY: one glyph at one position, into the context, which is alive.
    unsafe {
        font.draw_glyphs(
            NonNull::from(&mut glyph),
            NonNull::from(&mut origin),
            1,
            &context,
        );
    }
    Bitmap {
        format,
        // Within the bounds of a glyph, which fit an `i32`.
        width: width as u32,
        height: height as u32,
        left: left as i32,
        top: top as i32,
        data,
    }
}
