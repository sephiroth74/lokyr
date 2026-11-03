//! Text as the system draws it, as AppKit's labels do: the line that
//! CoreText lays out and Core Graphics draws, with the system's font
//! smoothing, against which the interface's text, which the GPU blends of
//! its glyphs' coverage, is measured.

use objc2_core_foundation::{
    CFAttributedString, CFBoolean, CFDictionary, CFString, CFType, CGFloat, CGPoint, CGRect, CGSize,
};
use objc2_core_graphics::{
    CGBitmapContextCreate, CGColorSpace, CGContext, CGImageAlphaInfo, CGImageByteOrderInfo,
    kCGColorSpaceSRGB,
};
use objc2_core_text::{
    CTFont, CTFontUIFontType, CTLine, kCTFontAttributeName,
    kCTForegroundColorFromContextAttributeName,
};

use crate::raster;

/// The points around the line, on each side.
const MARGIN: CGFloat = 4.0;

/// A line of text as the system draws it: its pixels, RGBA, 8 bits a
/// component in sRGB, premultiplied, a row after the other from the top.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SystemText {
    /// Its width, in pixels.
    pub width: u32,
    /// Its height, in pixels.
    pub height: u32,
    /// Its pixels.
    pub pixels: Vec<u8>,
}

/// `text` on one line, in the system's interface font of `points`, at
/// `scale` pixels a point, in `foreground` on `background`, sRGB's
/// components, as AppKit draws a label: of the system's font smoothing,
/// at positions of fractions of a pixel. `None` if Core Graphics makes no
/// bitmap of it.
pub fn system_text(
    text: &str,
    points: f64,
    scale: f64,
    foreground: [u8; 3],
    background: [u8; 3],
) -> Option<SystemText> {
    // SAFETY: the system's own font, of a size.
    let font = unsafe { CTFont::new_ui_font_for_language(CTFontUIFontType::System, points, None) }?;
    let string = CFString::from_str(text);
    // SAFETY: Core Text's constants.
    let (font_key, color_key) = unsafe {
        (
            kCTFontAttributeName,
            kCTForegroundColorFromContextAttributeName,
        )
    };
    let attributes = CFDictionary::<CFString, CFType>::from_slices(
        &[font_key, color_key],
        &[font.as_ref(), CFBoolean::new(true).as_ref()],
    );
    // SAFETY: a string and a dictionary of its attributes, which the call
    // copies.
    let attributed =
        unsafe { CFAttributedString::new(None, Some(&string), Some(attributes.as_opaque())) }?;
    // SAFETY: an attributed string, which the line copies.
    let line = unsafe { CTLine::with_attributed_string(&attributed) };
    let (mut ascent, mut descent) = (0.0, 0.0);
    // SAFETY: two values that the call writes, and no leading.
    let advance =
        unsafe { line.typographic_bounds(&raw mut ascent, &raw mut descent, std::ptr::null_mut()) };
    let points_wide = advance + 2.0 * MARGIN;
    let points_high = ascent + descent + 2.0 * MARGIN;
    let width = (points_wide * scale).ceil() as usize;
    let height = (points_high * scale).ceil() as usize;
    let mut pixels = vec![0_u8; width * height * 4];
    // SAFETY: Core Graphics' constant.
    let space = CGColorSpace::with_name(Some(unsafe { kCGColorSpaceSRGB }))?;
    // SAFETY: `pixels` holds `height` rows of `width` pixels of four bytes,
    // which the context draws into, and outlives it.
    let context = unsafe {
        CGBitmapContextCreate(
            pixels.as_mut_ptr().cast(),
            width,
            height,
            8,
            width * 4,
            Some(&space),
            CGImageAlphaInfo::PremultipliedLast.0 | CGImageByteOrderInfo::Order32Big.0,
        )
    }?;
    let drawing = Some(&*context);
    let component = |value: u8| CGFloat::from(value) / 255.0;
    CGContext::set_rgb_fill_color(
        drawing,
        component(background[0]),
        component(background[1]),
        component(background[2]),
        1.0,
    );
    CGContext::fill_rect(
        drawing,
        CGRect::new(
            CGPoint::ZERO,
            CGSize::new(width as CGFloat, height as CGFloat),
        ),
    );
    CGContext::scale_ctm(drawing, scale, scale);
    CGContext::set_should_antialias(drawing, true);
    CGContext::set_allows_font_smoothing(drawing, true);
    CGContext::set_should_smooth_fonts(drawing, raster::system_smooths_fonts());
    CGContext::set_allows_font_subpixel_positioning(drawing, true);
    CGContext::set_should_subpixel_position_fonts(drawing, true);
    CGContext::set_rgb_fill_color(
        drawing,
        component(foreground[0]),
        component(foreground[1]),
        component(foreground[2]),
        1.0,
    );
    CGContext::set_text_position(drawing, MARGIN, MARGIN + descent);
    // SAFETY: a line, into the context, which is alive.
    unsafe { line.draw(&context) };
    drop(context);
    Some(SystemText {
        width: width as u32,
        height: height as u32,
        pixels,
    })
}
