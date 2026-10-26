//! CoreText font backend for macOS.
//!
//! Font discovery, fallback, shaping and rasterization, on CoreText: the
//! [`CoreText`] backend finds a family's faces with font descriptors,
//! measures them, and draws their glyphs.

#![cfg(target_os = "macos")]

use std::collections::HashMap;
use std::ffi::c_void;
use std::ptr::NonNull;

use bolt_text_core::{
    Bitmap, FaceId, FaceInfo, FaceMetrics, Features, FontBackend, GlyphId, ShapedGlyph, Synthesis,
};
use objc2_app_kit::{NSFont, NSFontWeightRegular};
use objc2_core_foundation::{
    CFArray, CFDictionary, CFNumber, CFRange, CFRetained, CFSet, CFString, CFType,
    CGAffineTransform, Type, kCFTypeSetCallBacks,
};
use objc2_core_text::{
    CTFont, CTFontDescriptor, CTFontOrientation, CTFontSymbolicTraits, CTFontTableOptions,
    kCTFontFamilyNameAttribute, kCTFontStyleNameKey, kCTFontWeightTrait,
};

/// The size at which the backend makes a face to read what does not depend
/// on its size: its style, slant and weight.
const PROBE_SIZE: f64 = 12.0;

/// The name of the system's monospaced family, which macOS does not install
/// under it: the configuration's `SF Mono` takes the system's, and the
/// default family is named so, rather than by its hidden name.
const SF_MONO: &str = "SF Mono";

/// The tags of the tables that the backend reads.
const OS2: u32 = u32::from_be_bytes(*b"OS/2");
const FVAR: u32 = u32::from_be_bytes(*b"fvar");

/// The font backend of CoreText. The faces that it finds keep their ids as
/// long as it lives.
#[derive(Debug)]
pub struct CoreText {
    /// The faces found, by their ids.
    faces: Vec<CFRetained<CTFontDescriptor>>,
    /// The ids of the faces found, by their PostScript names.
    ids: HashMap<String, FaceId>,
    /// The faces made at a size, by their ids, the bits of the size, and
    /// whether they are slanted.
    fonts: HashMap<(FaceId, u64, bool), CFRetained<CTFont>>,
    /// Whether glyphs are drawn with macOS's font smoothing.
    smoothing: bool,
}

impl Default for CoreText {
    fn default() -> Self {
        Self::new()
    }
}

impl CoreText {
    /// A backend that has found no face yet, and smooths fonts as macOS
    /// does.
    pub fn new() -> Self {
        Self::with_font_smoothing(raster::system_smooths_fonts())
    }

    /// A backend that has found no face yet, and smooths fonts or not.
    pub fn with_font_smoothing(smoothing: bool) -> Self {
        Self {
            faces: Vec::new(),
            ids: HashMap::new(),
            fonts: HashMap::new(),
            smoothing,
        }
    }

    /// The face `face` at `size`, if the backend found it.
    fn font(&mut self, face: FaceId, size: f64) -> Option<CFRetained<CTFont>> {
        self.font_slanted(face, size, false)
    }

    /// The face `face` at `size`, slanted as a synthesized italic or not, if
    /// the backend found it.
    fn font_slanted(
        &mut self,
        face: FaceId,
        size: f64,
        slanted: bool,
    ) -> Option<CFRetained<CTFont>> {
        let key = (face, size.to_bits(), slanted);
        if let Some(font) = self.fonts.get(&key) {
            return Some(font.clone());
        }
        let descriptor = self.faces.get(usize::try_from(face.0).ok()?)?;
        let shear = CGAffineTransform {
            a: 1.0,
            b: 0.0,
            c: raster::ITALIC_SHEAR,
            d: 1.0,
            tx: 0.0,
            ty: 0.0,
        };
        let matrix = if slanted {
            &raw const shear
        } else {
            std::ptr::null()
        };
        // SAFETY: a descriptor that CoreText matched, at a size, with a
        // matrix that outlives the call or none.
        let font = unsafe { CTFont::with_font_descriptor(descriptor, size, matrix) };
        self.fonts.insert(key, font.clone());
        Some(font)
    }

    /// The faces of `descriptors`, of a family, those of `group` marked as
    /// the style group's.
    fn faces_of(
        &mut self,
        descriptors: &[CFRetained<CTFontDescriptor>],
        group: &[FaceId],
    ) -> Vec<FaceInfo> {
        descriptors
            .iter()
            .map(|descriptor| {
                // SAFETY: a descriptor that CoreText matched, at a size,
                // without a matrix.
                let font = unsafe {
                    CTFont::with_font_descriptor(descriptor, PROBE_SIZE, std::ptr::null())
                };
                let id = self.register(descriptor, &font);
                FaceInfo {
                    id,
                    style: style_name(&font),
                    weight: weight(&font),
                    // SAFETY: a font that CoreText made.
                    italic: unsafe { font.symbolic_traits() }
                        .contains(CTFontSymbolicTraits::TraitItalic),
                    in_group: group.contains(&id),
                }
            })
            .collect()
    }

    /// The id of the face of `descriptor`, which it registers the first time.
    fn register(&mut self, descriptor: &CTFontDescriptor, font: &CTFont) -> FaceId {
        // SAFETY: a font that CoreText made.
        let name = unsafe { font.post_script_name() }.to_string();
        if let Some(&id) = self.ids.get(&name) {
            return id;
        }
        let id = FaceId(u32::try_from(self.faces.len()).expect("fewer faces than an id counts"));
        self.faces.push(descriptor.retain());
        self.ids.insert(name, id);
        id
    }
}

impl FontBackend for CoreText {
    fn default_family(&mut self) -> String {
        SF_MONO.to_owned()
    }

    fn family(&mut self, name: &str) -> Option<Vec<FaceInfo>> {
        let descriptors = matching(name).or_else(|| {
            name.eq_ignore_ascii_case(SF_MONO)
                .then(|| matching(&system_monospaced_family()))
                .flatten()
        })?;
        // A name that some faces give their family, as the faces of a style
        // group do, takes the family that they belong to, whole, with the
        // group's faces marked.
        let whole = match family_name(&descriptors[0]) {
            Some(family) if family != name => matching(&family),
            _ => None,
        };
        let Some(whole) = whole else {
            return Some(self.faces_of(&descriptors, &[]));
        };
        let mut group: Vec<FaceId> = self
            .faces_of(&descriptors, &[])
            .into_iter()
            .map(|face| face.id)
            .collect();
        // The whole family, under another name or in another case, is no
        // group of it.
        if group.len() == whole.len() {
            group.clear();
        }
        Some(self.faces_of(&whole, &group))
    }

    fn metrics(&mut self, face: FaceId, size: f32) -> FaceMetrics {
        let Some(font) = self.font(face, f64::from(size)) else {
            return FaceMetrics::default();
        };
        // SAFETY: a font that CoreText made; the getters read it.
        let (ascent, descent, leading, underline, underline_thickness, x_height) = unsafe {
            (
                font.ascent(),
                font.descent(),
                font.leading(),
                font.underline_position(),
                font.underline_thickness(),
                font.x_height(),
            )
        };
        let (strikeout_top, strikeout_thickness) = strikeout(&font, f64::from(size)).unwrap_or((
            x_height / 2.0 + underline_thickness / 2.0,
            underline_thickness,
        ));
        FaceMetrics {
            ascent: ascent as f32,
            descent: descent as f32,
            leading: leading as f32,
            advance: widest_digit(&font) as f32,
            // CoreText gives the top of the underline, as the OpenType
            // `post` table has it.
            underline_position: (underline - underline_thickness / 2.0) as f32,
            underline_thickness: underline_thickness as f32,
            strikeout_position: (strikeout_top - strikeout_thickness / 2.0) as f32,
            strikeout_thickness: strikeout_thickness as f32,
        }
    }

    fn glyph(&mut self, face: FaceId, character: char) -> Option<GlyphId> {
        let font = self.font(face, PROBE_SIZE)?;
        glyph_of(&font, character)
    }

    fn fallback(&mut self, face: FaceId, cluster: &str) -> Option<FaceId> {
        let font = self.font(face, PROBE_SIZE)?;
        let string = CFString::from_str(cluster);
        let range = CFRange {
            location: 0,
            length: string.length(),
        };
        // SAFETY: a font that CoreText made, and a range of the string's
        // own length. CoreText gives the font of its cascade that draws the
        // most of the string, the font itself when no other does.
        let found = unsafe { font.for_string(&string, range) };
        // SAFETY: fonts that CoreText made.
        let (name, own) = unsafe { (found.post_script_name(), font.post_script_name()) };
        if name.to_string() == own.to_string() {
            return None;
        }
        // SAFETY: a font that CoreText made.
        let descriptor = unsafe { found.font_descriptor() };
        Some(self.register(&descriptor, &found))
    }

    fn shape(
        &mut self,
        _face: FaceId,
        _size: f32,
        _text: &str,
        _features: Features,
        _glyphs: &mut Vec<ShapedGlyph>,
    ) {
        unimplemented!("shaping with CoreText is not implemented yet")
    }

    fn rasterize(
        &mut self,
        face: FaceId,
        glyph: GlyphId,
        size: f32,
        synthesis: Synthesis,
    ) -> Bitmap {
        let size = f64::from(size);
        let Some(font) = self.font_slanted(face, size, synthesis.italic) else {
            return Bitmap::empty(bolt_text_core::BitmapFormat::Gray);
        };
        raster::rasterize(&font, glyph, size, synthesis, self.smoothing)
    }
}

/// The family of the system's monospaced font, SF Mono, which AppKit gives
/// under a name of its own.
fn system_monospaced_family() -> String {
    // SAFETY: AppKit's constant.
    let regular = unsafe { NSFontWeightRegular };
    let font = NSFont::monospacedSystemFontOfSize_weight(13.0, regular);
    font.familyName()
        .map_or_else(|| "Menlo".to_owned(), |name| name.to_string())
}

/// The descriptors of the faces whose family is `name`, if there are any.
fn matching(name: &str) -> Option<Vec<CFRetained<CTFontDescriptor>>> {
    // SAFETY: Core Text's constant.
    let key = unsafe { kCTFontFamilyNameAttribute };
    let family = CFString::from_str(name);
    let attributes = CFDictionary::<CFString, CFType>::from_slices(&[key], &[family.as_ref()]);
    let mut keys = [std::ptr::from_ref::<CFString>(key).cast::<c_void>()];
    // SAFETY: a set of one CFString, which the callbacks retain.
    let mandatory = unsafe { CFSet::new(None, keys.as_mut_ptr(), 1, &kCFTypeSetCallBacks) }?;
    // SAFETY: a dictionary of descriptor attributes.
    let descriptor = unsafe { CTFontDescriptor::with_attributes(attributes.as_opaque()) };
    // SAFETY: a set of attribute names.
    let found: CFRetained<CFArray> =
        unsafe { descriptor.matching_font_descriptors(Some(&mandatory)) }?;
    // SAFETY: CoreText's matches are descriptors.
    let found = unsafe { found.cast_unchecked::<CTFontDescriptor>() };
    let found = found.to_vec();
    (!found.is_empty()).then_some(found)
}

/// The name of the family that `descriptor` gives as its face's.
fn family_name(descriptor: &CTFontDescriptor) -> Option<String> {
    // SAFETY: Core Text's constant, and an attribute that a descriptor has.
    let value = unsafe { descriptor.attribute(kCTFontFamilyNameAttribute) }?;
    value
        .downcast::<CFString>()
        .ok()
        .map(|name| name.to_string())
}

/// The style of `font`, by its name in the font's own tables, which is not
/// the one that macOS shows in the user's language.
fn style_name(font: &CTFont) -> String {
    // SAFETY: Core Text's constant, and a name that a font has.
    unsafe { font.name(kCTFontStyleNameKey) }.map_or_else(String::new, |name| name.to_string())
}

/// The weight of `font`, as CSS numbers it: the OS/2 table's for a font of
/// one instance, and otherwise CoreText's, from -1 to 1, by AppKit's
/// weights, as a variable font's instances share one OS/2 table.
fn weight(font: &CTFont) -> f32 {
    let variable = table(font, FVAR).is_some();
    if !variable
        && let Some(os2) = table(font, OS2)
        && let Some(class) = be_u16(&os2, 4)
        && (1..=1000).contains(&class)
    {
        return f32::from(class);
    }
    // SAFETY: a font that CoreText made. Its traits hold the weight, when
    // they hold it, as a CFNumber, which lives as long as `traits`, within
    // the block.
    let trait_weight = unsafe {
        let traits = font.traits();
        let value = traits.value(std::ptr::from_ref::<CFString>(kCTFontWeightTrait).cast());
        NonNull::new(value.cast_mut()).map_or(0.0, |number| {
            number.cast::<CFNumber>().as_ref().as_f64().unwrap_or(0.0)
        })
    };
    css_weight(trait_weight)
}

/// A weight of CoreText's, from -1 to 1, as CSS numbers it, by the weights
/// of AppKit's fonts between them.
fn css_weight(weight: f64) -> f32 {
    const WEIGHTS: [(f64, f64); 11] = [
        (-1.0, 1.0),
        (-0.8, 100.0),
        (-0.6, 200.0),
        (-0.4, 300.0),
        (0.0, 400.0),
        (0.23, 500.0),
        (0.3, 600.0),
        (0.4, 700.0),
        (0.56, 800.0),
        (0.62, 900.0),
        (1.0, 1000.0),
    ];
    let weight = weight.clamp(-1.0, 1.0);
    let css = WEIGHTS
        .windows(2)
        .find(|pair| weight <= pair[1].0)
        .map_or(1000.0, |pair| {
            let ((low, low_css), (high, high_css)) = (pair[0], pair[1]);
            low_css + (weight - low) / (high - low) * (high_css - low_css)
        });
    css as f32
}

/// The advance of `font`'s widest digit.
fn widest_digit(font: &CTFont) -> f64 {
    let mut characters: Vec<u16> = "0123456789".encode_utf16().collect();
    let mut glyphs = vec![0_u16; characters.len()];
    let mut advances = vec![objc2_core_foundation::CGSize::ZERO; characters.len()];
    let count = isize::try_from(characters.len()).expect("ten digits");
    // SAFETY: the buffers hold `count` items each, which the calls read and
    // write.
    unsafe {
        font.glyphs_for_characters(
            NonNull::new_unchecked(characters.as_mut_ptr()),
            NonNull::new_unchecked(glyphs.as_mut_ptr()),
            count,
        );
        font.advances_for_glyphs(
            CTFontOrientation::Horizontal,
            NonNull::new_unchecked(glyphs.as_mut_ptr()),
            advances.as_mut_ptr(),
            count,
        );
    }
    advances
        .iter()
        .map(|advance| advance.width)
        .fold(0.0, f64::max)
}

/// The glyph of `character` in `font`, if it has one.
fn glyph_of(font: &CTFont, character: char) -> Option<GlyphId> {
    let mut units = [0_u16; 2];
    let units = character.encode_utf16(&mut units);
    let mut glyphs = [0_u16; 2];
    let count = isize::try_from(units.len()).expect("one or two units");
    // SAFETY: the buffers hold `count` items each, which the call reads and
    // writes.
    let found = unsafe {
        font.glyphs_for_characters(
            NonNull::new_unchecked(units.as_mut_ptr()),
            NonNull::new_unchecked(glyphs.as_mut_ptr()),
            count,
        )
    };
    (found && glyphs[0] != 0).then_some(GlyphId(glyphs[0]))
}

/// The top of the strikeout, from the baseline, and its thickness, at `size`,
/// from `font`'s OS/2 table.
fn strikeout(font: &CTFont, size: f64) -> Option<(f64, f64)> {
    let os2 = table(font, OS2)?;
    let thickness = be_i16(&os2, 26)?;
    let top = be_i16(&os2, 28)?;
    // SAFETY: a font that CoreText made.
    let units = f64::from(unsafe { font.units_per_em() });
    if thickness <= 0 || units <= 0.0 {
        return None;
    }
    Some((
        f64::from(top) * size / units,
        f64::from(thickness) * size / units,
    ))
}

/// The bytes of `font`'s table `tag`, if it has it.
fn table(font: &CTFont, tag: u32) -> Option<Vec<u8>> {
    // SAFETY: a font that CoreText made, and a table's tag.
    let data = unsafe { font.table(tag, CTFontTableOptions::NoOptions) }?;
    Some(data.to_vec())
}

fn be_u16(bytes: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_be_bytes([*bytes.get(at)?, *bytes.get(at + 1)?]))
}

fn be_i16(bytes: &[u8], at: usize) -> Option<i16> {
    Some(i16::from_be_bytes([*bytes.get(at)?, *bytes.get(at + 1)?]))
}

mod raster;
#[cfg(test)]
mod tests;
