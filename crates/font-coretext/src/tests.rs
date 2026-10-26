use bolt_text_core::{CellMetrics, FaceError, FontConfig, Name, resolve_faces};

use super::*;

/// The styles, weights and slants of `faces`.
fn styles(faces: &[FaceInfo]) -> Vec<(&str, f32, bool)> {
    faces
        .iter()
        .map(|face| (face.style.as_str(), face.weight, face.italic))
        .collect()
}

/// The regular face of the family `name`.
fn regular(backend: &mut CoreText, name: &str) -> FaceId {
    let faces = backend.family(name).expect("the family is installed");
    faces
        .iter()
        .find(|face| face.style == "Regular")
        .expect("the family has a regular face")
        .id
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
