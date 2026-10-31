use lokyr_text_core::{
    BitmapFormat, CellMetrics, FaceError, FontConfig, Name, Rendering, resolve_faces,
};

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
    assert_eq!(backend.default_family(), SF_MONO);
    let system = backend.family(&system_monospaced_family()).unwrap();
    assert!(system.iter().all(|face| !face.in_group));
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
    assert_eq!(CoreText::new().family("No Such Family Of Lokyr's"), None);
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
    // The name of the SemiBold faces' group, as Terminal.app shows the
    // SemiBold face.
    assert_eq!(
        regular_style(&mut backend, "CaskaydiaCove NFM SemiBold"),
        "SemiBold"
    );
}

/// The style of the regular face that text-core chooses in the family that
/// `name` names.
fn regular_style(backend: &mut CoreText, name: &str) -> String {
    let id = regular(backend, name);
    let faces = backend.family(name).unwrap();
    faces
        .into_iter()
        .find(|face| face.id == id)
        .map(|face| face.style)
        .unwrap()
}

#[test]
fn a_style_group_s_name_takes_its_face_as_regular() {
    // Names that the system's fonts give the faces of a weight.
    let mut backend = CoreText::new();
    let faces = backend.family("Avenir Next Demi Bold").unwrap();
    let group: Vec<&str> = faces
        .iter()
        .filter(|face| face.in_group)
        .map(|face| face.style.as_str())
        .collect();
    assert_eq!(group, ["Demi Bold", "Demi Bold Italic"]);
    let resolved = resolve_faces(&mut backend, &config("Avenir Next Demi Bold", None)).unwrap();
    let style = |id: FaceId| {
        faces
            .iter()
            .find(|face| face.id == id)
            .map(|face| face.style.as_str())
            .unwrap()
    };
    assert_eq!(style(resolved.faces.regular.id), "Demi Bold");
    assert_eq!(style(resolved.faces.bold.id), "Bold");
    assert_eq!(style(resolved.faces.italic.id), "Demi Bold Italic");
    assert_eq!(style(resolved.faces.bold_italic.id), "Bold Italic");
    assert_eq!(regular_style(&mut backend, "Hiragino Sans W3"), "W3");
    // The family's own name, in any case, names no group.
    assert_eq!(regular_style(&mut backend, "Avenir Next"), "Regular");
    assert_eq!(regular_style(&mut backend, "avenir next"), "Regular");
}

#[test]
fn faces_that_tie_take_the_regular_one() {
    let mut backend = CoreText::new();
    // Hoefler Text's Ornaments, of the regular's weight, width and slant,
    // gives way to the face that CoreText gives for the family alone.
    let faces = backend.family("Hoefler Text").unwrap();
    let preferred: Vec<&str> = faces
        .iter()
        .filter(|face| face.preferred)
        .map(|face| face.style.as_str())
        .collect();
    assert_eq!(preferred, ["Regular"]);
    assert_eq!(regular_style(&mut backend, "Hoefler Text"), "Regular");
    // 3270 Nerd Font's fonts tell no width, which the styles' names do;
    // nothing is checked where it is not installed, as on CI.
    if backend.family("3270 Nerd Font").is_none() {
        eprintln!("3270 Nerd Font is not installed: the test checks no more");
        return;
    }
    assert_eq!(regular_style(&mut backend, "3270 Nerd Font"), "Regular");
    assert_eq!(
        regular_style(&mut backend, "3270 Nerd Font Mono"),
        "Regular"
    );
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
fn the_cascade_draws_what_a_face_lacks() {
    let mut backend = CoreText::new();
    let menlo = regular(&mut backend, "Menlo");
    // CJK, Arabic, Devanagari and an emoji, which Menlo lacks.
    for character in ['日', 'ع', 'क', '🦀'] {
        assert_eq!(backend.glyph(menlo, character), None, "{character}");
        let face = backend
            .fallback(menlo, &character.to_string())
            .unwrap_or_else(|| panic!("a face for {character}"));
        assert_ne!(face, menlo);
        assert!(backend.glyph(face, character).is_some(), "{character}");
    }
    // A character that Menlo draws needs no other face.
    assert_eq!(backend.fallback(menlo, "a"), None);
    // The emoji's face draws in color.
    let emoji = backend.fallback(menlo, "🦀").unwrap();
    let glyph = backend.glyph(emoji, '🦀').unwrap();
    let bitmap = backend.rasterize(
        emoji,
        glyph,
        26.0,
        Synthesis::default(),
        Rendering::default(),
    );
    assert_eq!(bitmap.format, BitmapFormat::Rgba);
    assert!(bitmap.width > 0);
    // VS16 asks for the emoji of a character that has a text one.
    let heart = backend.fallback(menlo, "\u{2764}\u{FE0F}").unwrap();
    assert_eq!(heart, emoji);
}

#[test]
fn a_missing_family_falls_back_to_sf_mono() {
    let mut backend = CoreText::new();
    let resolved = resolve_faces(
        &mut backend,
        &config("No Such Family Of Lokyr's", Some("SemiBold")),
    )
    .unwrap();
    assert_eq!(resolved.family, backend.default_family());
    assert_eq!(
        resolved.missing_family.as_deref(),
        Some("No Such Family Of Lokyr's")
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
fn a_missing_style_of_the_default_family_names_sf_mono() {
    let config = FontConfig {
        style: Some(Name::try_from("Black".to_owned()).unwrap()),
        ..FontConfig::default()
    };
    let error = resolve_faces(&mut CoreText::new(), &config).unwrap_err();
    let message = error.to_string();
    assert!(
        message.starts_with("the family `SF Mono` has no style `Black`"),
        "{message}"
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
    backend.rasterize(face, glyph, pixels, synthesis, Rendering::default())
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

/// The glyphs of `text` shaped in `family`'s first face, with ligatures or
/// not, as their ids and clusters.
fn shaped(backend: &mut CoreText, family: &str, text: &str, ligatures: bool) -> Vec<(u16, u32)> {
    let face = backend.family(family).expect("the family is installed")[0].id;
    let mut glyphs = Vec::new();
    backend.shape(face, 32.0, text, Features { ligatures }, &mut glyphs);
    glyphs
        .iter()
        .map(|glyph| (glyph.glyph.0, glyph.cluster))
        .collect()
}

#[test]
fn ligatures_join_glyphs_unless_they_are_off() {
    let mut backend = CoreText::new();
    // Hoefler Text, which macOS installs, joins `ffi` and `fi`, of
    // standard ligatures.
    let joined = shaped(&mut backend, "Hoefler Text", "office fi", true);
    let apart = shaped(&mut backend, "Hoefler Text", "office fi", false);
    assert_eq!(joined.len(), 6, "{joined:?}");
    assert_eq!(apart.len(), 9, "{apart:?}");
    // The ligature's cluster is the byte where it starts.
    assert_eq!(joined[1].1, 1);
    assert_eq!(
        apart
            .iter()
            .map(|&(_, cluster)| cluster)
            .collect::<Vec<_>>(),
        (0..9).collect::<Vec<_>>()
    );
}

#[test]
fn contextual_alternates_are_ligatures_too() {
    // Cascadia's ligatures are contextual alternates, a glyph a cell; it is
    // not installed on CI, where the test checks nothing.
    let mut backend = CoreText::new();
    if backend.family("CaskaydiaCove NFM").is_none() {
        eprintln!("CaskaydiaCove NFM is not installed: the test checks nothing");
        return;
    }
    let joined = shaped(&mut backend, "CaskaydiaCove NFM", "a->b", true);
    let apart = shaped(&mut backend, "CaskaydiaCove NFM", "a->b", false);
    assert_eq!(joined.len(), 4);
    assert_eq!(apart.len(), 4);
    assert_ne!(
        joined[1..3],
        apart[1..3],
        "the arrow's glyphs are its alternates"
    );
    assert_eq!(joined[0], apart[0]);
}

#[test]
fn an_emoji_sequence_shapes_into_one_glyph() {
    let mut backend = CoreText::new();
    for sequence in [
        "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}",
        "\u{1F1EE}\u{1F1F9}",
        "1\u{FE0F}\u{20E3}",
        "\u{1F44D}\u{1F3FD}",
    ] {
        let glyphs = shaped(&mut backend, "Apple Color Emoji", sequence, true);
        assert_eq!(glyphs.len(), 1, "{sequence:?}: {glyphs:?}");
    }
    // A combining mark joins its base, in a font that composes them.
    assert_eq!(shaped(&mut backend, "Menlo", "e\u{301}x", true).len(), 2);
}

#[test]
fn a_run_in_another_font_takes_that_font_s_face() {
    let mut backend = CoreText::new();
    let mono = regular(&mut backend, "SF Mono");
    if backend.glyph(mono, '\u{336}').is_some() {
        eprintln!("SF Mono has U+0336: the test checks nothing");
        return;
    }
    // CoreText draws the long stroke, which SF Mono lacks, with its base,
    // in a font of its cascade, which becomes a face of the backend's.
    let mut glyphs = Vec::new();
    let features = Features { ligatures: true };
    backend.shape(mono, 26.0, "s\u{336}x", features, &mut glyphs);
    let (stroke, after) = glyphs.split_at(glyphs.len() - 1);
    assert!(
        stroke.len() == 2
            && stroke
                .iter()
                .all(|glyph| glyph.face != mono && glyph.cluster < 3),
        "{glyphs:?}"
    );
    for glyph in stroke {
        let bitmap = backend.rasterize(
            glyph.face,
            glyph.glyph,
            26.0,
            Synthesis::default(),
            Rendering {
                antialias: true,
                thin: false,
            },
        );
        assert!(bitmap.width > 0, "{glyph:?}");
    }
    // The `x` after it is SF Mono's, where the line puts it, after the
    // other font's advance.
    assert_eq!((after[0].face, after[0].cluster), (mono, 3));
    assert!(after[0].x > 0.0);
    // Shaped again, the same face.
    let mut again = Vec::new();
    backend.shape(mono, 26.0, "s\u{336}x", features, &mut again);
    assert_eq!(again, glyphs);
}

#[test]
fn a_face_substitutes_in_context_by_its_tables() {
    let mut backend = CoreText::new();
    // SF Mono's `GSUB` has neither ligatures nor contextual alternates.
    let mono = regular(&mut backend, "SF Mono");
    assert!(!backend.substitutes(mono));
    // Hoefler Text joins `fi` through its `morx` table.
    let hoefler = backend
        .family("Hoefler Text")
        .unwrap()
        .into_iter()
        .find(|face| face.style == "Regular")
        .unwrap();
    assert!(backend.substitutes(hoefler.id));
    // A `GSUB` table of a header and a list of features.
    let gsub = |tags: &[&[u8; 4]]| {
        let mut table = vec![0, 1, 0, 0, 0, 0, 0, 10, 0, 0];
        table.extend_from_slice(&u16::try_from(tags.len()).unwrap().to_be_bytes());
        for tag in tags {
            table.extend_from_slice(*tag);
            table.extend_from_slice(&[0, 0]);
        }
        table
    };
    assert!(gsub_substitutes(&gsub(&[b"ccmp", b"calt"])));
    assert!(gsub_substitutes(&gsub(&[b"rclt"])));
    assert!(!gsub_substitutes(&gsub(&[b"ccmp", b"locl", b"ss01"])));
    assert!(!gsub_substitutes(&gsub(&[])));
    // A table too short to read is taken to substitute.
    assert!(gsub_substitutes(&[0, 1, 0]));
}

#[test]
fn a_face_marked_bold_of_the_regular_class_weighs_as_bold() {
    // An OS/2 table's weight class at 4 and selection at 62.
    let table = |class: u16, selection: u16| {
        let mut os2 = vec![0_u8; 64];
        os2[4..6].copy_from_slice(&class.to_be_bytes());
        os2[62..64].copy_from_slice(&selection.to_be_bytes());
        os2
    };
    let bold = 1 << 5;
    assert_eq!(os2_weight(&table(400, bold)), Some(700.0));
    assert_eq!(os2_weight(&table(400, 0)), Some(400.0));
    assert_eq!(os2_weight(&table(600, bold)), Some(600.0));
    assert_eq!(os2_weight(&table(800, bold)), Some(800.0));
    assert_eq!(os2_weight(&table(0, bold)), None);
    assert_eq!(os2_weight(&[0; 4]), None);
}

#[test]
fn the_system_s_faces_tell_their_widths_and_obliques() {
    let mut backend = CoreText::new();
    // Helvetica's Oblique faces, and Avenir Next Condensed's width.
    let helvetica = backend.family("Helvetica").unwrap();
    let oblique = helvetica
        .iter()
        .find(|face| face.style == "Oblique")
        .unwrap();
    assert!(oblique.italic && oblique.oblique);
    let regular = helvetica
        .iter()
        .find(|face| face.style == "Regular")
        .unwrap();
    assert!(!regular.oblique && regular.width == 0.0);
    let condensed = backend.family("Avenir Next Condensed").unwrap();
    assert!(
        condensed.iter().all(|face| face.width < 0.0),
        "{condensed:?}"
    );
}

#[test]
fn a_synthesized_bold_keeps_its_corners_within_its_bitmap() {
    // The default miter's spikes at the sharp corners of these reach past
    // the margin that the stroke takes, and the bitmap cuts them: round
    // joins leave its edges blank.
    let mut backend = CoreText::new();
    let face = regular(&mut backend, "Menlo");
    let bold = Synthesis {
        bold: true,
        italic: false,
    };
    for character in ['M', '^', 'W'] {
        let glyph = backend.glyph(face, character).unwrap();
        let bitmap = backend.rasterize(face, glyph, 96.0, bold, Rendering::default());
        let (width, height) = (bitmap.width as usize, bitmap.height as usize);
        let at = |x: usize, y: usize| bitmap.data[y * width + x];
        let edges = (0..width)
            .flat_map(|x| [at(x, 0), at(x, height - 1)])
            .chain((0..height).flat_map(|y| [at(0, y), at(width - 1, y)]));
        assert!(edges.into_iter().all(|pixel| pixel == 0), "{character}");
    }
}

/// The coverage of Menlo's `m` at 26 pixels, smoothed as macOS smooths
/// fonts, drawn as `rendering` says: its pixels.
fn menlo_m(rendering: Rendering) -> Vec<u8> {
    let mut backend = CoreText::with_font_smoothing(true);
    let face = regular(&mut backend, "Menlo");
    let glyph = backend.glyph(face, 'm').unwrap();
    backend
        .rasterize(face, glyph, 26.0, Synthesis::default(), rendering)
        .data
}

#[test]
fn without_antialiasing_each_pixel_is_covered_or_not() {
    let aliased = menlo_m(Rendering {
        antialias: false,
        thin: false,
    });
    assert!(aliased.contains(&255));
    assert!(aliased.iter().all(|&pixel| pixel == 0 || pixel == 255));
    let smooth = menlo_m(Rendering::default());
    assert!(smooth.iter().any(|&pixel| pixel != 0 && pixel != 255));
}

#[test]
fn a_thin_glyph_covers_less_than_a_smoothed_one() {
    let total = |pixels: Vec<u8>| pixels.iter().map(|&pixel| u32::from(pixel)).sum::<u32>();
    let smoothed = total(menlo_m(Rendering::default()));
    let thin = total(menlo_m(Rendering {
        antialias: true,
        thin: true,
    }));
    // Font smoothing adds about a third of coverage.
    assert!(thin * 5 < smoothed * 4, "thin {thin}, smoothed {smoothed}");
}

/// The glyphs of `text` in `face`, at 26 pixels, heavier, slanted or both
/// in turn.
fn rasters(backend: &mut CoreText, face: FaceId, text: &str) -> Vec<Raster> {
    text.chars()
        .filter_map(|character| backend.glyph(face, character))
        .enumerate()
        .map(|(index, glyph)| Raster {
            face,
            glyph,
            size: 26.0,
            synthesis: Synthesis {
                bold: index % 2 == 1,
                italic: index % 3 == 2,
            },
            rendering: Rendering::default(),
        })
        .collect()
}

#[test]
fn a_batch_draws_each_glyph_as_one_at_a_time_would() {
    let mut backend = CoreText::new();
    let face = regular(&mut backend, "Menlo");
    // Fewer glyphs than take threads of their own, and more.
    for text in ["abc", "The quick brown fox jumps over the lazy dog"] {
        let rasters = rasters(&mut backend, face, text);
        let deadline = Instant::now() + std::time::Duration::from_secs(60);
        let batch = backend.rasterize_many(&rasters, deadline);
        assert_eq!(batch.len(), rasters.len());
        for (raster, bitmap) in rasters.iter().zip(batch) {
            let alone = backend.rasterize(
                raster.face,
                raster.glyph,
                raster.size,
                raster.synthesis,
                raster.rendering,
            );
            assert_eq!(bitmap.as_ref(), Some(&alone), "{raster:?}");
        }
    }
}

#[test]
fn a_batch_past_its_deadline_draws_nothing() {
    let mut backend = CoreText::new();
    let face = regular(&mut backend, "Menlo");
    let rasters = rasters(&mut backend, face, "The quick brown fox");
    let batch = backend.rasterize_many(&rasters, Instant::now());
    assert!(batch.iter().all(Option::is_none), "{batch:?}");
}

#[test]
fn the_warm_up_leaves_the_cascade_to_the_backends() {
    warm_up("Menlo");
    warm_up("No Such Family");
    let mut backend = CoreText::new();
    let face = regular(&mut backend, "Menlo");
    let found = backend.fallback(face, "中").expect("a face of Chinese");
    assert!(backend.glyph(found, '中').is_some());
}

#[test]
fn the_list_of_fonts_names_each_style_as_the_config_takes_it() {
    use lokyr_text_core::catalog::{self, Filter};

    let mut backend = CoreText::new();
    for family in ["SF Mono", "Menlo"] {
        let filter = Filter {
            monospaced: true,
            family: Some(family),
        };
        let listed = catalog::list(&mut backend, Some(family), &filter);
        let [listed] = listed.as_slice() else {
            panic!("{family} alone, monospaced: {listed:?}");
        };
        assert_eq!(listed.name, family);
        assert!(listed.configured, "{family} is the config's");
        assert_eq!(listed.default, family == "SF Mono");
        assert!(listed.styles.contains(&"Regular".to_owned()), "{listed:?}");
        for style in &listed.styles {
            let resolved = resolve_faces(&mut backend, &config(&listed.name, Some(style)));
            assert!(resolved.is_ok(), "{family} {style}: {resolved:?}");
        }
    }
}

#[test]
fn the_system_s_families_leave_out_the_hidden_ones() {
    let families = CoreText::new().families();
    assert!(
        families.iter().any(|family| family == "Menlo"),
        "{families:?}"
    );
    assert!(!families.iter().any(|family| family.starts_with('.')));
}
