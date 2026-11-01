//! Glyphs drawn into bitmaps by CoreText: coverage for the glyphs that the
//! text's color fills, color for the glyphs of their own colors.

use std::ptr::NonNull;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::Instant;

use lokyr_text_core::{Bitmap, BitmapFormat, GlyphId, Raster, Rendering, Synthesis};
use objc2_core_foundation::CFRetained;
use objc2_core_foundation::{CFString, CGFloat, CGPoint, CGRect, kCFPreferencesCurrentApplication};
use objc2_core_graphics::{
    CGBitmapContextCreate, CGColorSpace, CGContext, CGImageAlphaInfo, CGImageByteOrderInfo,
    CGLineJoin, CGTextDrawingMode, kCGColorSpaceSRGB,
};
use objc2_core_text::{CTFont, CTFontOrientation, CTFontSymbolicTraits};

/// The slant of a synthesized italic: 12°, as a shear of x by y.
pub(crate) const ITALIC_SHEAR: CGFloat = 0.212_556_56;

/// The pixels that a bitmap keeps around a glyph's outline, for the
/// coverage that antialiasing and font smoothing put past it.
const MARGIN: CGFloat = 1.0;

/// How many threads draw a batch of glyphs at most, the caller's among
/// them: past four, CoreGraphics, which draws a glyph's rows on threads of
/// its own and keeps a cache of their outlines, draws them little sooner,
/// 1 200 glyphs in 6 ms on twelve where four take 7.5 to 8.
const THREADS: usize = 4;

/// The fewest glyphs that take more threads than the caller's: a thread's
/// start costs about as much as a glyph.
const SHARED_FROM: usize = 8;

/// A font that the threads that draw a batch of glyphs share.
pub(crate) struct Shared(pub(crate) CFRetained<CTFont>);

// SAFETY: CoreText's font objects can be used by several threads at once,
// as its documentation says: "Font objects (CTFont, CTFontDescriptor, and
// associated objects) can be used simultaneously by multiple operations,
// work queues, or threads" (<https://developer.apple.com/documentation/coretext>).
// Each thread draws into a bitmap context of its own.
unsafe impl Send for Shared {}
// SAFETY: as for `Send`.
unsafe impl Sync for Shared {}

/// The glyphs of `rasters`, each in its font of `fonts`, drawn as
/// [`rasterize`] draws them, as many as there is time for until `deadline`:
/// on several threads, for a batch of a few, each taking the next glyph
/// that none has taken; `None` for those that none started by the deadline.
/// A glyph without its font is empty.
pub(crate) fn rasterize_many(
    rasters: &[Raster],
    fonts: &[Option<Shared>],
    smoothing: bool,
    deadline: Instant,
) -> Vec<Option<Bitmap>> {
    let next = AtomicUsize::new(0);
    let work = || {
        let mut drawn = Vec::new();
        while Instant::now() < deadline {
            let index = next.fetch_add(1, Ordering::Relaxed);
            let Some(raster) = rasters.get(index) else {
                break;
            };
            let bitmap = match &fonts[index] {
                Some(font) => rasterize(
                    &font.0,
                    raster.glyph,
                    CGFloat::from(raster.size),
                    raster.synthesis,
                    smoothing,
                    raster.rendering,
                ),
                None => Bitmap::empty(BitmapFormat::Gray),
            };
            drawn.push((index, bitmap));
        }
        drawn
    };
    let threads = if rasters.len() >= SHARED_FROM {
        THREADS
    } else {
        1
    };
    let mut bitmaps: Vec<Option<Bitmap>> = rasters.iter().map(|_| None).collect();
    thread::scope(|scope| {
        // A thread that cannot start leaves its share to the others.
        let helpers: Vec<_> = (1..threads)
            .filter_map(|_| {
                thread::Builder::new()
                    .name("glyphs".into())
                    .spawn_scoped(scope, work)
                    .ok()
            })
            .collect();
        let mut place = |drawn: Vec<(usize, Bitmap)>| {
            for (index, bitmap) in drawn {
                bitmaps[index] = Some(bitmap);
            }
        };
        place(work());
        for helper in helpers {
            // A thread that panicked leaves its glyphs for a later batch.
            if let Ok(drawn) = helper.join() {
                place(drawn);
            }
        }
    });
    bitmaps
}

/// Whether macOS smooths fonts, as the user set it, thickening their stems:
/// the default is to, which `AppleFontSmoothing` set to 0 turns off. The
/// value is read as Core Foundation's preferences read an integer, from the
/// application's domain or the global one, a string of digits too, as
/// `defaults write -g AppleFontSmoothing 0` writes one.
pub(crate) fn system_smooths_fonts() -> bool {
    let key = CFString::from_static_str("AppleFontSmoothing");
    // SAFETY: Core Foundation's constant.
    let domain = unsafe { kCFPreferencesCurrentApplication };
    let mut valid = 0;
    // SAFETY: a key, a domain, and a flag that the call writes.
    let value = unsafe {
        objc2_core_foundation::CFPreferencesGetAppIntegerValue(&key, domain, &raw mut valid)
    };
    smooths(valid != 0, value)
}

/// Whether fonts are smoothed for a value of `AppleFontSmoothing`, when the
/// preferences have a `valid` one: unless it is 0.
fn smooths(valid: bool, value: isize) -> bool {
    !valid || value != 0
}

/// The width of the stroke that draws a synthesized bold at `size` pixels,
/// which thickens each side of a stem by half of it.
fn bold_stroke(size: CGFloat) -> CGFloat {
    (size / 24.0).max(1.0)
}

/// `glyph` of `font`, drawn as `synthesis` says, into a bitmap: of coverage,
/// antialiased as `rendering` says and smoothed when `smoothing` is on but
/// for a thin glyph, or of color, when the font's glyphs have colors of
/// their own, which keep their antialiasing.
pub(crate) fn rasterize(
    font: &CTFont,
    glyph: GlyphId,
    size: CGFloat,
    synthesis: Synthesis,
    smoothing: bool,
    rendering: Rendering,
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
    let antialias = rendering.antialias || color;
    CGContext::set_should_antialias(drawing, antialias);
    CGContext::set_allows_font_smoothing(drawing, true);
    CGContext::set_should_smooth_fonts(
        drawing,
        smoothing && antialias && !color && !rendering.thin,
    );
    CGContext::set_gray_fill_color(drawing, 1.0, 1.0);
    if stroke > 0.0 {
        CGContext::set_text_drawing_mode(drawing, CGTextDrawingMode::FillStroke);
        CGContext::set_gray_stroke_color(drawing, 1.0, 1.0);
        CGContext::set_line_width(drawing, stroke);
        // Round joins, as the default miter's spikes at sharp corners, of
        // `M` or `^`, would leave the bitmap.
        CGContext::set_line_join(drawing, CGLineJoin::Round);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fonts_are_smoothed_unless_the_preference_is_0() {
        assert!(smooths(false, 0), "no preference");
        assert!(smooths(true, 1));
        assert!(smooths(true, 2));
        assert!(!smooths(true, 0));
    }
}
