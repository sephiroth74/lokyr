//! The families of fonts that the system has, each with its styles, by the
//! names that the configuration's `family` and `style` take them by.

use std::cmp::Ordering;
use std::io;

use crate::backend::{FaceId, FaceInfo, FontBackend};
use crate::faces::width;

/// A family of fonts that the system has.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Family {
    /// Its name, as `family` takes it.
    pub name: String,
    /// Its styles, as `style` takes them, each once: from the narrowest to
    /// the widest, and of a width from the lightest to the heaviest, the
    /// upright one before the italic.
    pub styles: Vec<String>,
    /// Whether the glyphs of each of its faces take one advance.
    pub monospaced: bool,
    /// Whether it is the family that the text takes when the configuration
    /// names none.
    pub default: bool,
    /// Whether it is the family that the configuration names.
    pub configured: bool,
}

/// Which families a list keeps: all of them by default.
#[derive(Clone, Copy, Debug, Default)]
pub struct Filter<'a> {
    /// The monospaced ones alone.
    pub monospaced: bool,
    /// The one of this name alone, as `family` takes it, its case aside.
    pub family: Option<&'a str>,
}

/// The families that `backend` finds, and the default one, which it may
/// have under another name, that `filter` keeps, in the order of their
/// names, their case aside; `configured`, the family that the configuration
/// names, is marked, as a name of some of its faces marks the family that
/// they are in.
pub fn list(
    backend: &mut dyn FontBackend,
    configured: Option<&str>,
    filter: &Filter<'_>,
) -> Vec<Family> {
    let default = backend.default_family();
    let mut names = backend.families();
    names.push(default.clone());
    names.sort_by(|a, b| by_name(a, b));
    names.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
    // A family named as it is needs no look at the others, which a name of
    // some faces of one does.
    if let Some(family) = filter.family
        && let Some(found) = names
            .iter()
            .position(|name| name.eq_ignore_ascii_case(family))
    {
        names = vec![names.swap_remove(found)];
    }
    let mut ids_of = |name: Option<&str>| -> Vec<FaceId> {
        name.and_then(|name| backend.family(name))
            .map(|faces| faces.iter().map(|face| face.id).collect())
            .unwrap_or_default()
    };
    let configured_ids = ids_of(configured);
    let wanted = ids_of(filter.family);

    let mut families = Vec::new();
    for name in names {
        let Some(mut faces) = backend.family(&name).filter(|faces| !faces.is_empty()) else {
            continue;
        };
        let of = |ids: &[FaceId]| faces.iter().any(|face| ids.contains(&face.id));
        if let Some(family) = filter.family
            && !(name.eq_ignore_ascii_case(family) || of(&wanted))
        {
            continue;
        }
        let monospaced = faces.iter().all(|face| backend.monospaced(face.id));
        if filter.monospaced && !monospaced {
            continue;
        }
        let configured = of(&configured_ids);
        faces.sort_by(by_look);
        let mut styles: Vec<String> = Vec::new();
        for face in faces {
            if !styles.contains(&face.style) {
                styles.push(face.style);
            }
        }
        families.push(Family {
            default: name.eq_ignore_ascii_case(&default),
            name,
            styles,
            monospaced,
            configured,
        });
    }
    families
}

/// Writes `families` to `out`, a line of each family's name, with what it
/// is in brackets; with `styles`, a line of each of its styles under it,
/// indented.
pub fn write(families: &[Family], styles: bool, out: &mut dyn io::Write) -> io::Result<()> {
    for family in families {
        let marks: Vec<&str> = [
            (family.monospaced, "monospaced"),
            (family.default, "the default"),
            (family.configured, "the config's"),
        ]
        .into_iter()
        .filter_map(|(marked, mark)| marked.then_some(mark))
        .collect();
        if marks.is_empty() {
            writeln!(out, "{}", family.name)?;
        } else {
            writeln!(out, "{}  ({})", family.name, marks.join(", "))?;
        }
        if styles {
            for style in &family.styles {
                writeln!(out, "  {style}")?;
            }
        }
    }
    Ok(())
}

/// Two names in the order of a list, their case aside, and then by it.
fn by_name(a: &str, b: &str) -> Ordering {
    a.to_lowercase()
        .cmp(&b.to_lowercase())
        .then_with(|| a.cmp(b))
}

/// The weights that a style's name tells, as CSS numbers them, by the words
/// of the name without case or spaces, those that contain another first.
const NAMED_WEIGHTS: [(&str, u16); 13] = [
    ("extralight", 200),
    ("ultralight", 200),
    ("semibold", 600),
    ("demibold", 600),
    ("extrabold", 800),
    ("ultrabold", 800),
    ("hairline", 100),
    ("thin", 100),
    ("light", 300),
    ("medium", 500),
    ("bold", 700),
    ("black", 900),
    ("heavy", 900),
];

/// The weight that the name of `style` tells: the regular one's without a
/// word of weight.
fn named_weight(style: &str) -> u16 {
    let style = style.to_lowercase().replace([' ', '-', '_'], "");
    NAMED_WEIGHTS
        .iter()
        .find(|(word, _)| style.contains(word))
        .map_or(400, |&(_, weight)| weight)
}

/// Two faces in the order of a family's list: by their widths, then their
/// weights, those that the fonts give and, for a tie, those that the names
/// tell, as some families give Thin and ExtraLight one weight; the upright
/// one before the italic, and the true italic before the oblique; and by
/// their styles' names.
fn by_look(a: &FaceInfo, b: &FaceInfo) -> Ordering {
    width(a)
        .total_cmp(&width(b))
        .then(a.weight.total_cmp(&b.weight))
        .then_with(|| named_weight(&a.style).cmp(&named_weight(&b.style)))
        .then(a.italic.cmp(&b.italic))
        .then(a.oblique.cmp(&b.oblique))
        .then_with(|| by_name(&a.style, &b.style))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::{Bitmap, BitmapFormat, FaceMetrics, Features, GlyphId, Rendering};
    use crate::backend::{ShapedGlyph, Synthesis};

    /// The faces of a family, their ids from `first` on: each of its style,
    /// weight, slant and width.
    fn faces(first: u32, faces: &[(&str, f32, bool, f32)]) -> Vec<FaceInfo> {
        (first..)
            .zip(faces)
            .map(|(id, &(style, weight, italic, width))| FaceInfo {
                id: FaceId(id),
                style: style.to_owned(),
                weight,
                italic,
                oblique: false,
                width,
                in_group: false,
                preferred: false,
            })
            .collect()
    }

    /// A system of three families and a hidden default one, whose fonts it
    /// neither measures nor draws: Menlo and the default monospaced, Times
    /// not; and a name of Iosevka's two heavy faces, as a style group's.
    struct System;

    impl System {
        fn faces(name: &str) -> Option<Vec<FaceInfo>> {
            match name.to_lowercase().as_str() {
                "menlo" => Some(faces(
                    0,
                    &[
                        ("Bold Italic", 700.0, true, 0.0),
                        ("Regular", 400.0, false, 0.0),
                        ("Italic", 400.0, true, 0.0),
                        ("Bold", 700.0, false, 0.0),
                    ],
                )),
                "times" => Some(faces(
                    10,
                    &[("Bold", 700.0, false, 0.0), ("Roman", 400.0, false, 0.0)],
                )),
                "iosevka" | "iosevka heavy" => Some(faces(
                    20,
                    &[
                        ("Heavy", 900.0, false, 0.0),
                        ("Condensed", 400.0, false, -0.2),
                        ("Regular", 400.0, false, 0.0),
                        ("Light", 300.0, false, 0.0),
                        ("Heavy Oblique", 900.0, true, 0.0),
                    ],
                )),
                "sf mono" => Some(faces(
                    30,
                    &[
                        ("Regular", 400.0, false, 0.0),
                        ("ExtraLight", 250.0, false, 0.0),
                        ("Thin", 250.0, false, 0.0),
                    ],
                )),
                "empty" => Some(Vec::new()),
                _ => None,
            }
        }
    }

    impl FontBackend for System {
        fn default_family(&mut self) -> String {
            "SF Mono".to_owned()
        }

        fn family(&mut self, name: &str) -> Option<Vec<FaceInfo>> {
            Self::faces(name)
        }

        fn families(&mut self) -> Vec<String> {
            ["Times", "menlo", "Iosevka", "Empty", "Gone"]
                .map(str::to_owned)
                .to_vec()
        }

        fn monospaced(&mut self, face: FaceId) -> bool {
            face.0 < 10 || face.0 >= 20
        }

        fn metrics(&mut self, _: FaceId, _: f32) -> FaceMetrics {
            FaceMetrics::default()
        }

        fn glyph(&mut self, _: FaceId, _: char) -> Option<GlyphId> {
            None
        }

        fn fallback(&mut self, _: FaceId, _: &str) -> Option<FaceId> {
            None
        }

        fn shape(&mut self, _: FaceId, _: f32, _: &str, _: Features, _: &mut Vec<ShapedGlyph>) {}

        fn rasterize(
            &mut self,
            _: FaceId,
            _: GlyphId,
            _: f32,
            _: Synthesis,
            _: Rendering,
        ) -> Bitmap {
            Bitmap::empty(BitmapFormat::Gray)
        }
    }

    fn names(families: &[Family]) -> Vec<&str> {
        families.iter().map(|family| family.name.as_str()).collect()
    }

    #[test]
    fn the_families_come_in_the_order_of_their_names_with_the_default_one() {
        let families = list(&mut System, None, &Filter::default());
        assert_eq!(names(&families), ["Iosevka", "menlo", "SF Mono", "Times"]);
        let default: Vec<bool> = families.iter().map(|family| family.default).collect();
        assert_eq!(default, [false, false, true, false]);
    }

    #[test]
    fn a_family_s_styles_go_by_width_weight_and_slant() {
        let families = list(&mut System, None, &Filter::default());
        assert_eq!(
            families[0].styles,
            ["Condensed", "Light", "Regular", "Heavy", "Heavy Oblique"]
        );
        assert_eq!(
            families[1].styles,
            ["Regular", "Italic", "Bold", "Bold Italic"]
        );
        assert_eq!(families[3].styles, ["Roman", "Bold"]);
        // Faces of one weight go as their names tell.
        assert_eq!(families[2].styles, ["Thin", "ExtraLight", "Regular"]);
    }

    #[test]
    fn the_monospaced_families_are_marked_and_kept_alone_on_ask() {
        let families = list(&mut System, None, &Filter::default());
        let monospaced: Vec<bool> = families.iter().map(|family| family.monospaced).collect();
        assert_eq!(monospaced, [true, true, true, false]);
        let filter = Filter {
            monospaced: true,
            family: None,
        };
        assert_eq!(
            names(&list(&mut System, None, &filter)),
            ["Iosevka", "menlo", "SF Mono"]
        );
    }

    #[test]
    fn the_config_s_family_is_marked_by_its_name_or_a_name_of_its_faces() {
        let configured = |name| -> Vec<String> {
            list(&mut System, Some(name), &Filter::default())
                .into_iter()
                .filter(|family| family.configured)
                .map(|family| family.name)
                .collect()
        };
        assert_eq!(configured("MENLO"), ["menlo"]);
        assert_eq!(configured("Iosevka Heavy"), ["Iosevka"]);
        assert!(configured("Gone").is_empty());
    }

    #[test]
    fn one_family_is_kept_by_its_name_or_a_name_of_its_faces() {
        let one = |name| {
            let filter = Filter {
                monospaced: false,
                family: Some(name),
            };
            names(&list(&mut System, None, &filter))
                .into_iter()
                .map(str::to_owned)
                .collect::<Vec<String>>()
        };
        assert_eq!(one("times"), ["Times"]);
        assert_eq!(one("Iosevka Heavy"), ["Iosevka"]);
        assert_eq!(one("sf mono"), ["SF Mono"]);
        assert!(one("Gone").is_empty());
    }

    #[test]
    fn the_list_is_written_a_line_a_family() {
        let families = list(&mut System, Some("Times"), &Filter::default());
        let mut out = Vec::new();
        write(&families, false, &mut out).unwrap();
        assert_eq!(
            String::from_utf8(out).unwrap(),
            "Iosevka  (monospaced)\n\
             menlo  (monospaced)\n\
             SF Mono  (monospaced, the default)\n\
             Times  (the config's)\n"
        );
    }

    #[test]
    fn the_list_is_written_a_line_a_family_and_a_style() {
        let families = list(&mut System, Some("Times"), &Filter::default());
        let mut out = Vec::new();
        write(&families, true, &mut out).unwrap();
        assert_eq!(
            String::from_utf8(out).unwrap(),
            "Iosevka  (monospaced)\n\
             \x20 Condensed\n\
             \x20 Light\n\
             \x20 Regular\n\
             \x20 Heavy\n\
             \x20 Heavy Oblique\n\
             menlo  (monospaced)\n\
             \x20 Regular\n\
             \x20 Italic\n\
             \x20 Bold\n\
             \x20 Bold Italic\n\
             SF Mono  (monospaced, the default)\n\
             \x20 Thin\n\
             \x20 ExtraLight\n\
             \x20 Regular\n\
             Times  (the config's)\n\
             \x20 Roman\n\
             \x20 Bold\n"
        );
    }
}
