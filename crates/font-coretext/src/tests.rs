use bolt_text_core::{BitmapFormat, CellMetrics, FaceError, FontConfig, Name, resolve_faces};

use super::*;

/// The styles, weights and slants of `faces`.
fn styles(faces: &[FaceInfo]) -> Vec<(&str, f32, bool)> {
    faces
        .iter()
        .map(|face| (face.style.as_str(), face.weight, face.italic))
        .collect()
}

/// The regular face of the family `name`, as text-core chooses it.
fn regular(backend: &mut CoreText, name: &str) -> FaceId {
    let resolved = resolve_faces(backend, &config(name, None)).expect("the family has faces");
    assert_eq!(resolved.missing_family, None, "{name} is installed");
    resolved.faces.regular.id
}

fn config(family: &str, style: Option<&str>) -> FontConfig {
    FontConfig {
        family: Some(Name::try_from(family.to_owned()).unwrap()),
        style: style.map(|style| Name::try_from(style.to_owned()).unwrap()),
        ..FontConfig::default()
    }
}

#[test]
fn menlo_s_faces_are_named_in_english() {
    let faces = CoreText::new().family("Menlo").unwrap();
    assert_eq!(
        styles(&faces),
        [
            ("Regular", 400.0, false),
            ("Italic", 400.0, true),
            ("Bold", 700.0, false),
            ("Bold Italic", 700.0, true),
        ]
    );
}

#[test]
fn sf_mono_is_the_system_s_monospaced_family() {
    let mut backend = CoreText::new();
    let default = backend.default_family();
    let system = backend.family(&default).unwrap();
    assert_eq!(backend.family(SF_MONO), Some(system.clone()));
    assert_eq!(backend.family("sf mono"), Some(system.clone()));
    let found = styles(&system);
    for face in [
        ("Regular", 400.0, false),
        ("Semibold", 600.0, false),
        ("Bold", 700.0, false),
        ("Bold Italic", 700.0, true),
    ] {
        assert!(found.contains(&face), "{face:?} in {found:?}");
    }
}

#[test]
fn obliques_are_italic() {
    let faces = CoreText::new().family("Courier").unwrap();
    assert_eq!(
        styles(&faces),
        [
            ("Regular", 400.0, false),
            ("Oblique", 400.0, true),
            ("Bold", 700.0, false),
            ("Bold Oblique", 700.0, true),
        ]
    );
}

#[test]
fn a_family_that_is_not_installed_has_no_faces() {
    assert_eq!(CoreText::new().family("No Such Family Of Bolt's"), None);
}

#[test]
fn a_face_keeps_its_id() {
    let mut backend = CoreText::new();
    let first = backend.family("Menlo").unwrap();
    backend.family("Courier").unwrap();
    assert_eq!(backend.family("Menlo").unwrap(), first);
}

#[test]
fn a_style_group_s_name_takes_its_whole_family() {
    // The name that the four faces of a style group give their family, as
    // Terminal.app shows it; a family that this Mac may not have.
    let mut backend = CoreText::new();
    if backend.family("CaskaydiaCove Nerd Font Mono").is_none() {
        eprintln!("CaskaydiaCove Nerd Font Mono is not installed: the test checks nothing");
        return;
    }
    let faces = backend.family("CaskaydiaCove NFM").unwrap();
    let found = styles(&faces);
    assert!(found.contains(&("SemiBold", 600.0, false)), "{found:?}");
    assert!(found.contains(&("ExtraLight", 200.0, false)), "{found:?}");
}

/// The cells of `family`'s regular face at 11, 13 and 22 points, at scales
/// of 1 and 2: width, height and baseline.
fn cells(family: &str) -> Vec<(u32, u32, u32)> {
    let mut backend = CoreText::new();
    let face = regular(&mut backend, family);
    let mut cells = Vec::new();
    for points in [11.0_f32, 13.0, 22.0] {
        for scale in [1.0_f32, 2.0] {
            let cell = CellMetrics::new(
                &backend.metrics(face, points * scale),
                &FontConfig::default(),
            );
            cells.push((cell.width, cell.height, cell.baseline));
        }
    }
    cells
}

#[test]
fn menlo_s_cells() {
    assert_eq!(
        cells("Menlo"),
        [
            (7, 13, 10),
            (14, 26, 21),
            (8, 16, 13),
            (16, 31, 25),
            (14, 26, 21),
            (27, 52, 41),
        ]
    );
}

#[test]
fn sf_mono_s_cells() {
    // A system font, whose metrics macOS can change a little from one version
    // to the next: within a pixel of macOS 27's.
    let expected = [
        (7, 13, 11),
        (14, 26, 21),
        (9, 16, 13),
        (17, 31, 25),
        (14, 26, 21),
        (28, 52, 43),
    ];
    let found = cells(SF_MONO);
    for (found, expected) in found.iter().zip(expected) {
        let near = |a: u32, b: u32| a.abs_diff(b) <= 1;
        assert!(
            near(found.0, expected.0) && near(found.1, expected.1) && near(found.2, expected.2),
            "{found:?} against {expected:?}"
        );
    }
}

#[test]
fn a_face_measures_its_lines_about_the_baseline() {
    let mut backend = CoreText::new();
    let face = regular(&mut backend, "Menlo");
    let metrics = backend.metrics(face, 26.0);
    assert!(metrics.ascent > 0.0 && metrics.descent > 0.0, "{metrics:?}");
    assert!(metrics.underline_position < 0.0, "{metrics:?}");
    assert!(metrics.strikeout_position > 0.0, "{metrics:?}");
    assert!(
        metrics.underline_thickness > 0.0 && metrics.strikeout_thickness > 0.0,
        "{metrics:?}"
    );
    // Twice the size, twice the advance.
    let twice = backend.metrics(face, 52.0);
    assert!(
        (twice.advance - 2.0 * metrics.advance).abs() < 0.01,
        "{metrics:?} {twice:?}"
    );
    // A face that the backend never found measures nothing.
    assert_eq!(backend.metrics(FaceId(999), 26.0), FaceMetrics::default());
}

#[test]
fn a_face_has_the_glyphs_of_its_characters() {
    let mut backend = CoreText::new();
    let face = regular(&mut backend, "Menlo");
    assert!(backend.glyph(face, 'a').is_some());
    assert!(backend.glyph(face, '─').is_some());
    assert_ne!(backend.glyph(face, 'a'), backend.glyph(face, 'b'));
    // Menlo has no CJK, and no emoji past the BMP.
    assert_eq!(backend.glyph(face, '日'), None);
    assert_eq!(backend.glyph(face, '🦀'), None);
}

#[test]
fn a_missing_family_falls_back_to_sf_mono() {
    let mut backend = CoreText::new();
    let resolved = resolve_faces(
        &mut backend,
        &config("No Such Family Of Bolt's", Some("SemiBold")),
    )
    .unwrap();
    assert_eq!(resolved.family, backend.default_family());
    assert_eq!(
        resolved.missing_family.as_deref(),
        Some("No Such Family Of Bolt's")
    );
}

#[test]
fn a_missing_style_lists_the_family_s() {
    let error = resolve_faces(&mut CoreText::new(), &config("Menlo", Some("Black"))).unwrap_err();
    assert!(matches!(error, FaceError::MissingStyle { .. }));
    assert_eq!(
        error.to_string(),
        "the family `Menlo` has no style `Black`, which `style` names; it has Regular, Italic, Bold, Bold Italic"
    );
}

#[test]
fn a_style_of_sf_mono_is_found_whatever_its_case() {
    let mut backend = CoreText::new();
    let resolved = resolve_faces(&mut backend, &config(SF_MONO, Some("SemiBold"))).unwrap();
    let faces = backend.family(SF_MONO).unwrap();
    let regular = faces
        .iter()
        .find(|face| face.id == resolved.faces.regular.id)
        .unwrap();
    assert_eq!(regular.style, "Semibold");
    let bold = faces
        .iter()
        .find(|face| face.id == resolved.faces.bold.id)
        .unwrap();
    assert_eq!(bold.style, "Bold");
}

#[test]
fn coretext_s_weights_take_appkit_s_numbers() {
    assert_eq!(css_weight(-0.8), 100.0);
    assert_eq!(css_weight(0.0), 400.0);
    assert_eq!(css_weight(0.23), 500.0);
    assert_eq!(css_weight(0.3), 600.0);
    assert_eq!(css_weight(0.4), 700.0);
    assert_eq!(css_weight(0.62), 900.0);
    assert_eq!(css_weight(2.0), 1000.0);
    assert_eq!(css_weight(-0.2), 350.0);
}

/// The pixels of `bitmap` whose coverage, or alpha, passes `threshold`, as
/// the rows and columns that bound them, in the cell whose baseline is
/// `baseline`: left, top, right and bottom, the last two past the ink.
fn ink(bitmap: &Bitmap, baseline: u32, threshold: u8) -> Option<(i32, i32, i32, i32)> {
    let (bytes, channel) = match bitmap.format {
        BitmapFormat::Gray => (1, 0),
        BitmapFormat::Rgba => (4, 3),
    };
    let width = bitmap.width as usize;
    let mut bounds: Option<(i32, i32, i32, i32)> = None;
    for (row, pixels) in bitmap.data.chunks(width * bytes).enumerate() {
        for (column, pixel) in pixels.chunks(bytes).enumerate() {
            if pixel[channel] <= threshold {
                continue;
            }
            let x = bitmap.left + column as i32;
            let y = baseline as i32 - bitmap.top + row as i32;
            bounds = Some(match bounds {
                None => (x, y, x + 1, y + 1),
                Some((l, t, r, b)) => (l.min(x), t.min(y), r.max(x + 1), b.max(y + 1)),
            });
        }
    }
    bounds
}

/// The cell of Menlo's regular face at `pixels`.
fn menlo_cell(backend: &mut CoreText, pixels: f32) -> CellMetrics {
    let face = regular(backend, "Menlo");
    CellMetrics::new(&backend.metrics(face, pixels), &FontConfig::default())
}

fn glyph(
    backend: &mut CoreText,
    family: &str,
    character: char,
    pixels: f32,
    synthesis: Synthesis,
) -> Bitmap {
    let face = regular(backend, family);
    let glyph = backend
        .glyph(face, character)
        .expect("the face has the character");
    backend.rasterize(face, glyph, pixels, synthesis)
}

fn total(bitmap: &Bitmap) -> u64 {
    bitmap.data.iter().map(|&byte| u64::from(byte)).sum()
}

#[test]
fn ascii_keeps_within_its_cell() {
    let mut backend = CoreText::new();
    for pixels in [13.0, 26.0] {
        let cell = menlo_cell(&mut backend, pixels);
        for character in ['M', 'g', 'j', '_', '|', '@', 'W'] {
            let bitmap = glyph(
                &mut backend,
                "Menlo",
                character,
                pixels,
                Synthesis::default(),
            );
            assert_eq!(bitmap.format, BitmapFormat::Gray);
            assert_eq!(bitmap.data.len(), (bitmap.width * bitmap.height) as usize);
            // A glyph that meets its neighbours, as `_` does, can take a
            // pixel of theirs, across; never one of the rows above or
            // below.
            let (left, top, right, bottom) = ink(&bitmap, cell.baseline, 64).expect("ink");
            assert!(
                left >= -1
                    && top >= 0
                    && right <= cell.width as i32 + 1
                    && bottom <= cell.height as i32,
                "{character} at {pixels}: {:?} in {cell:?}",
                (left, top, right, bottom)
            );
        }
    }
}

#[test]
fn a_space_draws_nothing() {
    let mut backend = CoreText::new();
    let bitmap = glyph(&mut backend, "Menlo", ' ', 13.0, Synthesis::default());
    assert_eq!((bitmap.width, bitmap.height), (0, 0));
    assert!(bitmap.data.is_empty());
}

#[test]
fn cjk_keeps_within_two_cells() {
    let mut backend = CoreText::new();
    for pixels in [13.0, 26.0] {
        let cell = menlo_cell(&mut backend, pixels);
        for character in ['日', '本', '語', '國'] {
            let bitmap = glyph(
                &mut backend,
                "Hiragino Sans",
                character,
                pixels,
                Synthesis::default(),
            );
            let (left, top, right, bottom) = ink(&bitmap, cell.baseline, 64).expect("ink");
            assert!(
                left >= 0
                    && top >= 0
                    && right <= 2 * cell.width as i32
                    && bottom <= cell.height as i32,
                "{character} at {pixels}: {:?} in two of {cell:?}",
                (left, top, right, bottom)
            );
            // Wider than one cell: it takes the two.
            assert!(right - left > cell.width as i32, "{character}");
        }
    }
}

#[test]
fn an_emoji_keeps_its_colors() {
    let mut backend = CoreText::new();
    let cell = menlo_cell(&mut backend, 26.0);
    let bitmap = glyph(
        &mut backend,
        "Apple Color Emoji",
        '🦀',
        26.0,
        Synthesis::default(),
    );
    assert_eq!(bitmap.format, BitmapFormat::Rgba);
    assert_eq!(
        bitmap.data.len(),
        (bitmap.width * bitmap.height * 4) as usize
    );
    let pixels: Vec<&[u8]> = bitmap.data.chunks(4).collect();
    // Premultiplied: no channel past its alpha.
    assert!(
        pixels
            .iter()
            .all(|p| p[0] <= p[3] && p[1] <= p[3] && p[2] <= p[3])
    );
    // The crab is red: opaque pixels much redder than green.
    let red = pixels
        .iter()
        .filter(|p| p[3] > 200 && u32::from(p[0]) > u32::from(p[1]) + 80)
        .count();
    assert!(red > 20, "{red} red pixels");
    // Its width takes two cells, within a pixel either way.
    let (left, _, right, _) = ink(&bitmap, cell.baseline, 64).expect("ink");
    assert!(
        right - left <= 2 * cell.width as i32 + 2,
        "{:?} in two of {cell:?}",
        (left, right)
    );
}

#[test]
fn smoothing_thickens_the_strokes() {
    let smooth = glyph(
        &mut CoreText::with_font_smoothing(true),
        "Menlo",
        'm',
        13.0,
        Synthesis::default(),
    );
    let plain = glyph(
        &mut CoreText::with_font_smoothing(false),
        "Menlo",
        'm',
        13.0,
        Synthesis::default(),
    );
    assert!(
        total(&smooth) > total(&plain) * 11 / 10,
        "{} {}",
        total(&smooth),
        total(&plain)
    );
}

/// How far right the ink of `bitmap`'s top rows is from that of its bottom
/// rows, in pixels: the lean of a stem.
fn lean(bitmap: &Bitmap) -> f32 {
    let width = bitmap.width as usize;
    let centre = |rows: &[u8]| {
        let (sum, weight) = rows
            .chunks(width)
            .flat_map(|row| row.iter().enumerate())
            .fold((0.0, 0.0), |(sum, weight), (column, &value)| {
                (
                    sum + column as f32 * f32::from(value),
                    weight + f32::from(value),
                )
            });
        sum / weight
    };
    let quarter = (bitmap.height as usize / 4) * width;
    centre(&bitmap.data[..quarter]) - centre(&bitmap.data[bitmap.data.len() - quarter..])
}

#[test]
fn synthesis_draws_a_heavier_and_a_slanted_face() {
    let mut backend = CoreText::with_font_smoothing(false);
    let regular = glyph(&mut backend, "Menlo", 'I', 26.0, Synthesis::default());
    let bold = glyph(
        &mut backend,
        "Menlo",
        'I',
        26.0,
        Synthesis {
            bold: true,
            italic: false,
        },
    );
    let italic = glyph(
        &mut backend,
        "Menlo",
        'I',
        26.0,
        Synthesis {
            bold: false,
            italic: true,
        },
    );
    assert!(
        total(&bold) > total(&regular) * 6 / 5,
        "{} {}",
        total(&bold),
        total(&regular)
    );
    // Upright, the stem stands; slanted by 12°, its top is a fifth of its
    // height to the right of its bottom.
    assert!(lean(&regular).abs() < 1.0, "{}", lean(&regular));
    assert!(lean(&italic) > 2.5, "{}", lean(&italic));
    // The slanted glyph keeps within its bitmap: nothing at its edges.
    let width = italic.width as usize;
    for row in italic.data.chunks(width) {
        assert!(row[0] == 0 && row[width - 1] == 0);
    }
}

#[test]
fn twice_the_scale_twice_the_glyph() {
    let mut backend = CoreText::new();
    let small = glyph(&mut backend, "Menlo", 'M', 13.0, Synthesis::default());
    let large = glyph(&mut backend, "Menlo", 'M', 26.0, Synthesis::default());
    let near = |a: u32, b: u32| a.abs_diff(b) <= 3;
    assert!(near(large.width, 2 * small.width) && near(large.height, 2 * small.height));
}
