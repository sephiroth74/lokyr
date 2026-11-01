//! CoreText font backend for macOS.
//!
//! Font discovery, fallback, shaping and rasterization, on CoreText: the
//! [`CoreText`] backend finds a family's faces with font descriptors,
//! measures them, and draws their glyphs.

#![cfg(target_os = "macos")]

use std::collections::HashMap;
use std::ffi::c_void;
use std::ptr::NonNull;
use std::time::Instant;

use lokyr_text_core::{
    Bitmap, FaceId, FaceInfo, FaceMetrics, Features, FontBackend, GlyphId, Raster, Rendering,
    SYMBOLS_FONT, ShapedGlyph, Synthesis,
};
use objc2_app_kit::{NSFont, NSFontWeightRegular};
use objc2_core_foundation::{
    CFArray, CFAttributedString, CFData, CFDictionary, CFNumber, CFRange, CFRetained, CFSet,
    CFString, CFType, CGAffineTransform, CGPoint, Type, kCFAllocatorNull, kCFTypeSetCallBacks,
};
use objc2_core_text::{
    CTFont, CTFontCollection, CTFontCollectionCopyOptions, CTFontDescriptor,
    CTFontManagerCreateFontDescriptorFromData, CTFontOrientation, CTFontSymbolicTraits,
    CTFontTableOptions, CTLine, CTRun, kCTFontAttributeName, kCTFontFamilyNameAttribute,
    kCTFontFeatureSettingsAttribute, kCTFontOpenTypeFeatureTag, kCTFontOpenTypeFeatureValue,
    kCTFontStyleNameKey, kCTFontWeightTrait, kCTFontWidthTrait,
};

/// The size at which the backend makes a face to read what does not depend
/// on its size: its style, slant and weight.
const PROBE_SIZE: f64 = 12.0;

/// The name of the system's monospaced family, which macOS does not install
/// under it: the configuration's `SF Mono` takes the system's, and the
/// default family is named so, rather than by its hidden name.
const SF_MONO: &str = "SF Mono";

/// The PostScript name of macOS's last resort, which the cascade gives for
/// a character that no other font has, as the private planes' first, and
/// which draws a box of the character's block.
const LAST_RESORT: &str = "LastResort";

/// The bits of the OS/2 table's selection that mark a face bold and oblique.
const FS_SELECTION_BOLD: u16 = 1 << 5;
const FS_SELECTION_OBLIQUE: u16 = 1 << 9;

/// The tags of the tables that the backend reads.
const OS2: u32 = u32::from_be_bytes(*b"OS/2");
const FVAR: u32 = u32::from_be_bytes(*b"fvar");
const GSUB: u32 = u32::from_be_bytes(*b"GSUB");
const MORX: u32 = u32::from_be_bytes(*b"morx");

/// The features of `GSUB` that substitute glyphs in context: the ligatures
/// and the contextual alternates, the required ones too, which change text
/// of one character a cluster.
const CONTEXTUAL: [&[u8; 4]; 6] = [b"liga", b"clig", b"dlig", b"rlig", b"calt", b"rclt"];

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
    /// The faces that shape text without ligatures, by their ids and the
    /// bits of the size.
    unjoined: HashMap<(FaceId, u64), CFRetained<CTFont>>,
    /// Whether each face substitutes glyphs in context, as it was found.
    substitutes: HashMap<FaceId, bool>,
    /// Whether glyphs are drawn with macOS's font smoothing.
    smoothing: bool,
    /// The face of the symbols' font, once loaded, if it loads.
    symbols: Option<Option<FaceId>>,
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
            unjoined: HashMap::new(),
            substitutes: HashMap::new(),
            smoothing,
            symbols: None,
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

    /// The face `face` at `size` that shapes text with `features`: without
    /// ligatures, its contextual alternates, standard and discretionary
    /// ligatures turned off, as some fonts put their ligatures in the first.
    fn shaping_font(
        &mut self,
        face: FaceId,
        size: f64,
        features: Features,
    ) -> Option<CFRetained<CTFont>> {
        let font = self.font(face, size)?;
        if features.ligatures {
            return Some(font);
        }
        if let Some(unjoined) = self.unjoined.get(&(face, size.to_bits())) {
            return Some(unjoined.clone());
        }
        // SAFETY: Core Text's constants.
        let (tag, value) = unsafe { (kCTFontOpenTypeFeatureTag, kCTFontOpenTypeFeatureValue) };
        let off = CFNumber::new_i32(0);
        let settings: Vec<CFRetained<CFDictionary<CFString, CFType>>> = ["calt", "liga", "dlig"]
            .into_iter()
            .map(|feature| {
                let name = CFString::from_str(feature);
                CFDictionary::<CFString, CFType>::from_slices(
                    &[tag, value],
                    &[name.as_ref(), off.as_ref()],
                )
            })
            .collect();
        let settings = CFArray::from_retained_objects(&settings);
        // SAFETY: Core Text's constant.
        let key = unsafe { kCTFontFeatureSettingsAttribute };
        let attributes =
            CFDictionary::<CFString, CFType>::from_slices(&[key], &[settings.as_ref()]);
        // SAFETY: a dictionary of descriptor attributes.
        let descriptor = unsafe { CTFontDescriptor::with_attributes(attributes.as_opaque()) };
        // SAFETY: a font that CoreText made, at its own size, without a
        // matrix.
        let unjoined =
            unsafe { font.copy_with_attributes(size, std::ptr::null(), Some(&descriptor)) };
        self.unjoined
            .insert((face, size.to_bits()), unjoined.clone());
        Some(unjoined)
    }

    /// The faces of `descriptors`, of a family, those of `group` marked as
    /// the style group's, and the one of the PostScript name `preferred` as
    /// the family's own.
    fn faces_of(
        &mut self,
        descriptors: &[CFRetained<CTFontDescriptor>],
        group: &[FaceId],
        preferred: Option<&str>,
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
                // SAFETY: a font that CoreText made.
                let name = unsafe { font.post_script_name() }.to_string();
                let style = style_name(&font);
                // SAFETY: Core Text's constant.
                let width = trait_value(&font, unsafe { kCTFontWidthTrait });
                FaceInfo {
                    id,
                    oblique: oblique(&font, &style),
                    style,
                    weight: weight(&font),
                    // SAFETY: a font that CoreText made.
                    italic: unsafe { font.symbolic_traits() }
                        .contains(CTFontSymbolicTraits::TraitItalic),
                    width: width as f32,
                    in_group: group.contains(&id),
                    preferred: preferred == Some(name.as_str()),
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
        let family = family_name(&descriptors[0]);
        let preferred = preferred_face(family.as_deref().unwrap_or(name));
        let preferred = preferred.as_deref();
        let whole = match family {
            Some(family) if family != name => matching(&family),
            _ => None,
        };
        let Some(whole) = whole else {
            return Some(self.faces_of(&descriptors, &[], preferred));
        };
        let mut group: Vec<FaceId> = self
            .faces_of(&descriptors, &[], None)
            .into_iter()
            .map(|face| face.id)
            .collect();
        // The whole family, under another name or in another case, is no
        // group of it.
        if group.len() == whole.len() {
            group.clear();
        }
        Some(self.faces_of(&whole, &group, preferred))
    }

    fn families(&mut self) -> Vec<String> {
        // SAFETY: a collection without options, and Core Text's constant,
        // an attribute that descriptors have.
        let names = unsafe {
            CTFontCollection::from_available_fonts(None).font_attribute(
                kCTFontFamilyNameAttribute,
                CTFontCollectionCopyOptions::Unique,
            )
        };
        // SAFETY: the values of an attribute, CFStrings, or kCFNull for a
        // descriptor without it.
        let names = unsafe { names.cast_unchecked::<CFType>() };
        names
            .iter()
            .filter_map(|name| name.downcast::<CFString>().ok())
            .map(|name| name.to_string())
            // macOS hides the families whose names start with a dot, as
            // the system's own.
            .filter(|name| !name.is_empty() && !name.starts_with('.'))
            .collect()
    }

    fn symbols(&mut self) -> Option<FaceId> {
        if let Some(known) = self.symbols {
            return known;
        }
        let loaded = symbols_descriptor().map(|descriptor| {
            // SAFETY: a descriptor that CoreText made of the font's data, at
            // a size, without a matrix.
            let font =
                unsafe { CTFont::with_font_descriptor(&descriptor, PROBE_SIZE, std::ptr::null()) };
            self.register(&descriptor, &font)
        });
        self.symbols = Some(loaded);
        loaded
    }

    fn last_resort(&mut self, face: FaceId) -> bool {
        self.font(face, PROBE_SIZE).is_some_and(|font| {
            // SAFETY: a font that CoreText made.
            unsafe { font.post_script_name() }.to_string() == LAST_RESORT
        })
    }

    fn monospaced(&mut self, face: FaceId) -> bool {
        self.font(face, PROBE_SIZE).is_some_and(|font| {
            // SAFETY: a font that CoreText made.
            unsafe { font.symbolic_traits() }.contains(CTFontSymbolicTraits::TraitMonoSpace)
        })
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
        face: FaceId,
        size: f32,
        text: &str,
        features: Features,
        glyphs: &mut Vec<ShapedGlyph>,
    ) {
        let Some(font) = self.shaping_font(face, f64::from(size), features) else {
            return;
        };
        let start = glyphs.len();
        let others = shape(&font, text, glyphs);
        // The faces of the runs that CoreText drew in another font, which
        // become faces of the backend's.
        let faces: Vec<FaceId> = others
            .iter()
            .map(|other| match other {
                Some(other) => {
                    // SAFETY: a font that CoreText made.
                    let descriptor = unsafe { other.font_descriptor() };
                    self.register(&descriptor, other)
                }
                None => face,
            })
            .collect();
        for glyph in &mut glyphs[start..] {
            glyph.face = faces[glyph.face.0 as usize];
        }
    }

    fn substitutes(&mut self, face: FaceId) -> bool {
        if let Some(&substitutes) = self.substitutes.get(&face) {
            return substitutes;
        }
        let substitutes = self.font(face, PROBE_SIZE).is_none_or(|font| {
            table(&font, MORX).is_some()
                || table(&font, GSUB).is_some_and(|gsub| gsub_substitutes(&gsub))
        });
        self.substitutes.insert(face, substitutes);
        substitutes
    }

    fn rasterize(
        &mut self,
        face: FaceId,
        glyph: GlyphId,
        size: f32,
        synthesis: Synthesis,
        rendering: Rendering,
    ) -> Bitmap {
        let size = f64::from(size);
        let Some(font) = self.font_slanted(face, size, synthesis.italic) else {
            return Bitmap::empty(lokyr_text_core::BitmapFormat::Gray);
        };
        raster::rasterize(&font, glyph, size, synthesis, self.smoothing, rendering)
    }

    /// Draws the glyphs on up to four threads, the caller's among them, a
    /// batch of eight glyphs or more, each thread taking the next glyph that
    /// none has taken; the fonts are made first, on the caller's.
    fn rasterize_many(&mut self, rasters: &[Raster], deadline: Instant) -> Vec<Option<Bitmap>> {
        let fonts: Vec<Option<raster::Shared>> = rasters
            .iter()
            .map(|raster| {
                self.font_slanted(raster.face, f64::from(raster.size), raster.synthesis.italic)
                    .map(raster::Shared)
            })
            .collect();
        raster::rasterize_many(rasters, &fonts, self.smoothing, deadline)
    }
}

/// The text that [`warm_up`] gives the system's cascade of fonts: Chinese,
/// Japanese and Korean, and an emoji.
const WARM_UP: [&str; 4] = ["中", "か", "한", "\u{1F600}\u{FE0F}"];

/// A sequence that [`warm_up`] shapes in the emoji's face: a family, of
/// emoji that ZWJs join.
const WARM_UP_SEQUENCE: &str = "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}";

/// Readies the system's cascade of fonts from the regular face of `family`,
/// and the faces that it gives the text of Chinese, Japanese, Korean and
/// emoji, as a thread of their own can before a frame shows them: the
/// cascade's first looks take over a millisecond each, and a face's first
/// glyph and first shaping tenths of one, where they take microseconds
/// after. What CoreText loads stays for the process, whose every backend
/// then finds it: about 4 MB of its memory.
pub fn warm_up(family: &str) {
    let mut backend = CoreText::new();
    let Some(face) = backend
        .family(family)
        .and_then(|faces| faces.iter().find(|face| !face.italic).map(|face| face.id))
    else {
        return;
    };
    for text in WARM_UP {
        let Some(found) = backend.fallback(face, text) else {
            continue;
        };
        let character = text.chars().next().unwrap_or_default();
        if let Some(glyph) = backend.glyph(found, character) {
            backend.rasterize(
                found,
                glyph,
                26.0,
                Synthesis::default(),
                Rendering::default(),
            );
        }
        if text.chars().nth(1).is_some() {
            let mut shaped = Vec::new();
            let features = Features { ligatures: false };
            backend.shape(found, 26.0, WARM_UP_SEQUENCE, features, &mut shaped);
        }
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

/// The PostScript name of the face that CoreText gives for the family
/// `name` alone.
fn preferred_face(name: &str) -> Option<String> {
    // SAFETY: Core Text's constant.
    let key = unsafe { kCTFontFamilyNameAttribute };
    let family = CFString::from_str(name);
    let attributes = CFDictionary::<CFString, CFType>::from_slices(&[key], &[family.as_ref()]);
    // SAFETY: a dictionary of descriptor attributes.
    let descriptor = unsafe { CTFontDescriptor::with_attributes(attributes.as_opaque()) };
    // SAFETY: a descriptor, at a size, without a matrix.
    let font = unsafe { CTFont::with_font_descriptor(&descriptor, PROBE_SIZE, std::ptr::null()) };
    // SAFETY: a font that CoreText made.
    let (found, face) = unsafe { (font.family_name(), font.post_script_name()) };
    // CoreText gives a font of another family for one that it lacks.
    (found.to_string() == name).then(|| face.to_string())
}

/// The descriptor of the symbols' font, of the bytes that the binary holds,
/// which CoreText reads where they are.
fn symbols_descriptor() -> Option<CFRetained<CTFontDescriptor>> {
    let length = isize::try_from(SYMBOLS_FONT.len()).ok()?;
    // SAFETY: bytes that live as long as the process, which no allocator
    // frees, as `kCFAllocatorNull` says.
    let data = unsafe {
        CFData::with_bytes_no_copy(None, SYMBOLS_FONT.as_ptr(), length, kCFAllocatorNull)
    }?;
    // SAFETY: the data of a font file.
    unsafe { CTFontManagerCreateFontDescriptorFromData(&data) }
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
    if !variable && let Some(weight) = table(font, OS2).and_then(|os2| os2_weight(&os2)) {
        return weight;
    }
    // SAFETY: Core Text's constant.
    css_weight(trait_value(font, unsafe { kCTFontWeightTrait }))
}

/// The weight that an OS/2 table gives: its weight class, from 1 to 1000;
/// or bold, 700, for a face that its selection marks bold with the class of
/// the regular weight or lighter, as some families give their bold faces
/// 400. A face marked bold of a class between, as a Demi Bold that a family
/// links to its regular one as bold, keeps its class.
fn os2_weight(os2: &[u8]) -> Option<f32> {
    let class = be_u16(os2, 4).filter(|class| (1..=1000).contains(class))?;
    let bold = be_u16(os2, 62).is_some_and(|selection| selection & FS_SELECTION_BOLD != 0);
    Some(if bold && class <= 400 {
        700.0
    } else {
        f32::from(class)
    })
}

/// Whether `font`, of the style `style`, is oblique, as its OS/2 table's
/// selection marks it, from the table's version 4, or as its style's name
/// says.
fn oblique(font: &CTFont, style: &str) -> bool {
    let marked = table(font, OS2).is_some_and(|os2| {
        be_u16(&os2, 0).is_some_and(|version| version >= 4)
            && be_u16(&os2, 62).is_some_and(|selection| selection & FS_SELECTION_OBLIQUE != 0)
    });
    let named = style.to_lowercase();
    marked || named.contains("oblique") || named.contains("slanted")
}

/// The value of the trait `key` of `font`, as CoreText gives it from -1 to
/// 1: 0 when it has none.
fn trait_value(font: &CTFont, key: &CFString) -> f64 {
    // SAFETY: a font that CoreText made. Its traits hold the value, when
    // they hold it, as a CFNumber, which lives as long as `traits`, within
    // the block.
    unsafe {
        let traits = font.traits();
        let value = traits.value(std::ptr::from_ref::<CFString>(key).cast());
        NonNull::new(value.cast_mut()).map_or(0.0, |number| {
            number.cast::<CFNumber>().as_ref().as_f64().unwrap_or(0.0)
        })
    }
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

/// Shapes `text` in `font`, through a line of CoreText's, and appends its
/// glyphs to `glyphs`, each at its place from the line's start; returns
/// the fonts of the runs: `None` for `font`'s own, and the font that
/// CoreText drew a run in otherwise, as it does for a cluster that `font`
/// lacks a character of. A glyph's face is the index of its run's font in
/// what this returns, which the caller turns into a face.
fn shape(
    font: &CTFont,
    text: &str,
    glyphs: &mut Vec<ShapedGlyph>,
) -> Vec<Option<CFRetained<CTFont>>> {
    let mut fonts = Vec::new();
    // The byte of each UTF-16 unit of `text`, which CoreText counts in.
    let mut bytes = Vec::with_capacity(text.len());
    for (at, character) in text.char_indices() {
        let at = u32::try_from(at).unwrap_or(u32::MAX);
        bytes.extend(std::iter::repeat_n(at, character.len_utf16()));
    }
    let string = CFString::from_str(text);
    // SAFETY: Core Text's constant.
    let key = unsafe { kCTFontAttributeName };
    let attributes = CFDictionary::<CFString, CFType>::from_slices(&[key], &[font.as_ref()]);
    // SAFETY: a string and a dictionary of its attributes, which the call
    // copies.
    let Some(attributed) =
        (unsafe { CFAttributedString::new(None, Some(&string), Some(attributes.as_opaque())) })
    else {
        return fonts;
    };
    // SAFETY: an attributed string, which the line copies.
    let line = unsafe { CTLine::with_attributed_string(&attributed) };
    // SAFETY: a line that CoreText made.
    let runs = unsafe { line.glyph_runs() };
    // SAFETY: the runs of a line are `CTRun`s.
    let runs = unsafe { runs.cast_unchecked::<CTRun>() };
    // SAFETY: a font that CoreText made.
    let own = unsafe { font.post_script_name() }.to_string();
    for run in runs.iter() {
        // SAFETY: a run that CoreText made.
        let count = usize::try_from(unsafe { run.glyph_count() }).unwrap_or(0);
        if count == 0 {
            continue;
        }
        let mut ids = vec![0_u16; count];
        let mut positions = vec![CGPoint::ZERO; count];
        let mut indices = vec![0_isize; count];
        // A range of no length is the run's whole.
        let whole = CFRange {
            location: 0,
            length: 0,
        };
        // SAFETY: each buffer holds the run's `count` items, which the
        // calls write.
        unsafe {
            run.glyphs(whole, NonNull::new_unchecked(ids.as_mut_ptr()));
            run.positions(whole, NonNull::new_unchecked(positions.as_mut_ptr()));
            run.string_indices(whole, NonNull::new_unchecked(indices.as_mut_ptr()));
        }
        let run_font = run_font(&run);
        let ours = run_font.as_ref().is_none_or(|other| {
            // SAFETY: a font that CoreText made.
            unsafe { other.post_script_name() }.to_string() == own
        });
        let index = u32::try_from(fonts.len()).unwrap_or(u32::MAX);
        fonts.push(if ours { None } else { run_font });
        for at in 0..count {
            let unit = usize::try_from(indices[at]).unwrap_or(0);
            glyphs.push(ShapedGlyph {
                face: FaceId(index),
                glyph: GlyphId(ids[at]),
                cluster: bytes.get(unit).copied().unwrap_or(0),
                // The positions are from the line's origin.
                x: positions[at].x as f32,
                y: positions[at].y as f32,
            });
        }
    }
    fonts
}

/// Whether a `GSUB` table has a feature that substitutes in context.
fn gsub_substitutes(gsub: &[u8]) -> bool {
    let Some(list) = be_u16(gsub, 6).map(usize::from) else {
        return true;
    };
    let count = be_u16(gsub, list).map_or(0, usize::from);
    (0..count).any(|feature| {
        let at = list + 2 + feature * 6;
        gsub.get(at..at + 4).is_some_and(|tag| {
            CONTEXTUAL
                .iter()
                .any(|contextual| tag == contextual.as_slice())
        })
    })
}

/// The font that `run` draws in.
fn run_font(run: &CTRun) -> Option<CFRetained<CTFont>> {
    // SAFETY: a run that CoreText made.
    let attributes = unsafe { run.attributes() };
    // SAFETY: a run's attributes are keyed by strings.
    let attributes = unsafe { attributes.cast_unchecked::<CFString, CFType>() };
    // SAFETY: Core Text's constant.
    let font = attributes.get(unsafe { kCTFontAttributeName })?;
    font.downcast::<CTFont>().ok()
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
