//! The family that the terminal's text takes, and the faces of it that its
//! four styles take, as the configuration names them or the family's weights
//! choose them.

use crate::backend::{FaceId, FaceInfo, FontBackend, Synthesis};
use crate::config::{FontConfig, Name};

/// A face that a style of text takes, and what is drawn heavier or slanted
/// when the family lacks the face.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Face {
    /// The face.
    pub id: FaceId,
    /// What is made of it.
    pub synthesis: Synthesis,
}

impl Face {
    fn plain(id: FaceId) -> Self {
        Self {
            id,
            synthesis: Synthesis::default(),
        }
    }
}

/// The faces of the four styles of text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Faces {
    /// Text without style.
    pub regular: Face,
    /// Bold text.
    pub bold: Face,
    /// Italic text.
    pub italic: Face,
    /// Bold italic text.
    pub bold_italic: Face,
}

/// What keeps a family's faces from the styles of text.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum FaceError {
    /// A style that the configuration names, which the family lacks.
    #[error(
        "the family `{family}` has no style `{style}`, which `{key}` names; it has {}",
        available.join(", ")
    )]
    MissingStyle {
        /// The key of the configuration that names the style.
        key: &'static str,
        /// The family.
        family: String,
        /// The style that the key names.
        style: String,
        /// The family's styles, each once, in its order.
        available: Vec<String>,
    },
    /// A family without faces.
    #[error("the family `{family}` has no face")]
    NoFace {
        /// The family.
        family: String,
    },
}

/// The family that the text takes, and the faces of its four styles.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Resolved {
    /// The family's name, as the configuration or the backend gives it.
    pub family: String,
    /// The faces of the four styles.
    pub faces: Faces,
    /// The family that the configuration names, when it is not installed
    /// and the text takes the default family instead.
    pub missing_family: Option<String>,
}

/// The family that `config` names, or the default one when it names none or
/// one that is not installed, and the faces of its four styles. A family
/// that stands in for one that is missing takes its faces by their weights,
/// as the styles that the configuration names are the missing family's.
pub fn resolve(backend: &mut dyn FontBackend, config: &FontConfig) -> Result<Resolved, FaceError> {
    let wanted = config.family.as_ref().map(Name::as_str);
    if let Some(name) = wanted
        && let Some(faces) = backend.family(name)
    {
        return Ok(Resolved {
            family: name.to_owned(),
            faces: choose(name, &faces, config)?,
            missing_family: None,
        });
    }
    let family = backend.default_family();
    let faces = backend.family(&family).unwrap_or_default();
    let config = if wanted.is_some() {
        FontConfig {
            style: None,
            bold_style: None,
            italic_style: None,
            bold_italic_style: None,
            ..config.clone()
        }
    } else {
        config.clone()
    };
    Ok(Resolved {
        faces: choose(&family, &faces, &config)?,
        family,
        missing_family: wanted.map(str::to_owned),
    })
}

/// The weight of a family's regular face, as CSS numbers it.
const REGULAR: f32 = 400.0;

/// The lightest weight that bold takes, as CSS numbers it.
const BOLD: f32 = 700.0;

/// The widths that a style's name tells, as CoreText measures them, by the
/// words of the name without case or spaces, those that contain another
/// first.
const NAMED_WIDTHS: [(&str, f32); 12] = [
    ("ultracondensed", -0.4),
    ("extracondensed", -0.3),
    ("semicondensed", -0.1),
    ("condensed", -0.2),
    ("compressed", -0.3),
    ("narrow", -0.2),
    ("ultraexpanded", 0.4),
    ("extraexpanded", 0.3),
    ("semiexpanded", 0.1),
    ("expanded", 0.2),
    ("extended", 0.2),
    ("wide", 0.2),
];

/// The width of `face`: its font's, or, where the font tells none, the one
/// that its style's name tells, as `Condensed` or `Narrow`.
pub(crate) fn width(face: &FaceInfo) -> f32 {
    if face.width != 0.0 {
        return face.width;
    }
    let style = normalize(&face.style);
    NAMED_WIDTHS
        .iter()
        .find(|(word, _)| style.contains(word))
        .map_or(0.0, |&(_, width)| width)
}

/// The faces that `faces`, a family's, give to the four styles of text, as
/// `config` names them:
/// - regular: the style that it names, or the face nearest to the regular
///   weight, upright and of the normal width when the family has one, among
///   the faces of the style group that the family was named by when it was
///   named by one;
/// - bold: the style that it names, or the upright face heavier than the
///   regular one nearest to the heavier of bold's weight and one step past
///   the regular one, so that SemiBold takes Bold, and Bold ExtraBold;
///   otherwise the regular face, drawn heavier;
/// - italic: the style that it names, or the italic face nearest to the
///   regular one's weight; otherwise the regular face, slanted;
/// - bold italic: the style that it names, or the italic face nearest to
///   the bold one's weight, heavier than the italic one; otherwise the bold
///   face slanted, or the italic one drawn heavier.
///
/// The others are of the regular face's width, for a family of several. An
/// italic is a true italic rather than an oblique when the family has both,
/// and a bold italic the same; a family of italics alone takes its heavier
/// italic as bold. A face's width is its font's, or the one that its style's
/// name tells where the font tells none. Faces as near to a weight are told
/// apart as CSS does, the heavier taken for a weight of 500 or more and the
/// lighter below it; then the one that the system gives for the family
/// alone, as Hoefler Text's Regular rather than its Ornaments; then by
/// their styles' names, whatever order the backend gives them.
///
/// A style is named as the family names it, but for case, spaces, hyphens
/// and underscores: `semi-bold` is `SemiBold`.
pub fn choose(family: &str, faces: &[FaceInfo], config: &FontConfig) -> Result<Faces, FaceError> {
    let named = |key: &'static str, style: Option<&Name>| -> Result<Option<&FaceInfo>, FaceError> {
        let Some(style) = style else {
            return Ok(None);
        };
        let wanted = normalize(style.as_str());
        faces
            .iter()
            .find(|face| normalize(&face.style) == wanted)
            .map(Some)
            .ok_or_else(|| {
                let mut available: Vec<String> = Vec::new();
                for face in faces {
                    if !available.contains(&face.style) {
                        available.push(face.style.clone());
                    }
                }
                FaceError::MissingStyle {
                    key,
                    family: family.to_owned(),
                    style: style.to_string(),
                    available,
                }
            })
    };
    let grouped = faces.iter().any(|face| face.in_group);
    let group = || faces.iter().filter(move |face| face.in_group || !grouped);
    // The width nearest to the normal one, of the faces of a slant.
    let normal = |italic: bool| {
        group()
            .filter(|face| face.italic == italic)
            .map(width)
            .min_by(|a, b| a.abs().total_cmp(&b.abs()))
    };

    let regular = match named("style", config.style.as_ref())? {
        Some(face) => face,
        None => {
            let upright = normal(false);
            nearest(
                group().filter(|face| !face.italic && Some(width(face)) == upright),
                REGULAR,
            )
            .or_else(|| {
                let italic = normal(true);
                nearest(group().filter(|face| Some(width(face)) == italic), REGULAR)
            })
            .ok_or_else(|| FaceError::NoFace {
                family: family.to_owned(),
            })?
        }
    };
    // The faces of the regular one's width; of its italics, the true ones
    // when the family has them.
    let wide = |face: &&FaceInfo| width(face) == width(regular);
    let upright = || faces.iter().filter(wide).filter(|face| !face.italic);
    let true_italics = faces
        .iter()
        .filter(wide)
        .any(|face| face.italic && !face.oblique);
    let italics = move || {
        faces
            .iter()
            .filter(wide)
            .filter(move |face| face.italic && !(true_italics && face.oblique))
    };

    let bold = match named("bold-style", config.bold_style.as_ref())? {
        Some(face) => Some(face),
        None => {
            let target = BOLD.max(regular.weight + 100.0);
            let heavier = |face: &&FaceInfo| face.weight > regular.weight;
            // A family of italics alone takes its heavier italic.
            if upright().next().is_none() {
                nearest(italics().filter(heavier), target)
            } else {
                nearest(upright().filter(heavier), target)
            }
        }
    };
    let italic = match named("italic-style", config.italic_style.as_ref())? {
        Some(face) => Some(face),
        None => nearest(italics(), regular.weight),
    };
    let bold_italic = match named("bold-italic-style", config.bold_italic_style.as_ref())? {
        Some(face) => Some(face),
        None => {
            let floor = italic.map_or(regular.weight, |italic| italic.weight);
            let target = bold.map_or(BOLD.max(regular.weight + 100.0), |bold| bold.weight);
            nearest(italics().filter(|face| face.weight > floor), target)
        }
    };

    let regular_face = Face::plain(regular.id);
    let bold_face = bold.map_or(
        Face {
            id: regular.id,
            synthesis: Synthesis {
                bold: true,
                italic: false,
            },
        },
        |bold| Face::plain(bold.id),
    );
    let italic_face = italic.map_or(
        Face {
            id: regular.id,
            synthesis: Synthesis {
                bold: false,
                italic: true,
            },
        },
        |italic| Face::plain(italic.id),
    );
    let bold_italic_face = match (bold_italic, bold, italic) {
        (Some(face), _, _) => Face::plain(face.id),
        (None, Some(bold), _) => Face {
            id: bold.id,
            synthesis: Synthesis {
                bold: false,
                italic: true,
            },
        },
        (None, None, Some(italic)) => Face {
            id: italic.id,
            synthesis: Synthesis {
                bold: true,
                italic: false,
            },
        },
        (None, None, None) => Face {
            id: regular.id,
            synthesis: Synthesis {
                bold: true,
                italic: true,
            },
        },
    };
    Ok(Faces {
        regular: regular_face,
        bold: bold_face,
        italic: italic_face,
        bold_italic: bold_italic_face,
    })
}

/// The face of `faces` whose weight is nearest to `weight`; of those as
/// near, the heavier for a weight of 500 or more and the lighter below it,
/// as CSS has it, then the system's for the family alone, and then the
/// first by its style's name.
fn nearest<'a>(faces: impl Iterator<Item = &'a FaceInfo>, weight: f32) -> Option<&'a FaceInfo> {
    let side = |face: &FaceInfo| {
        if weight >= 500.0 {
            -face.weight
        } else {
            face.weight
        }
    };
    faces.min_by(|a, b| {
        (a.weight - weight)
            .abs()
            .total_cmp(&(b.weight - weight).abs())
            .then(side(a).total_cmp(&side(b)))
            .then(b.preferred.cmp(&a.preferred))
            .then_with(|| normalize(&a.style).cmp(&normalize(&b.style)))
    })
}

/// A style's name without case, spaces or hyphens.
fn normalize(style: &str) -> String {
    style
        .chars()
        .filter(|character| !matches!(character, ' ' | '-' | '_'))
        .flat_map(char::to_lowercase)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Name;

    /// A family of faces, of their styles, weights and slants.
    fn family(faces: &[(&str, f32, bool)]) -> Vec<FaceInfo> {
        faces
            .iter()
            .zip(0..)
            .map(|(&(style, weight, italic), id)| FaceInfo {
                id: FaceId(id),
                style: style.to_owned(),
                weight,
                italic,
                oblique: style.contains("Oblique"),
                width: if style.contains("Condensed") {
                    -0.2
                } else {
                    0.0
                },
                in_group: false,
                preferred: false,
            })
            .collect()
    }

    /// Cascadia Code's faces, as its Nerd Font's static ones are.
    fn cascadia() -> Vec<FaceInfo> {
        family(&[
            ("ExtraLight", 200.0, false),
            ("Light", 300.0, false),
            ("Regular", 400.0, false),
            ("SemiBold", 600.0, false),
            ("Bold", 700.0, false),
            ("ExtraLight Italic", 200.0, true),
            ("Light Italic", 300.0, true),
            ("Italic", 400.0, true),
            ("SemiBold Italic", 600.0, true),
            ("Bold Italic", 700.0, true),
        ])
    }

    fn style(cascadia: &[FaceInfo], face: Face) -> (&str, Synthesis) {
        let info = cascadia.iter().find(|info| info.id == face.id).unwrap();
        (info.style.as_str(), face.synthesis)
    }

    fn config(regular: Option<&str>) -> FontConfig {
        FontConfig {
            style: regular.map(|style| Name::try_from(style.to_owned()).unwrap()),
            ..FontConfig::default()
        }
    }

    const PLAIN: Synthesis = Synthesis {
        bold: false,
        italic: false,
    };

    #[test]
    fn a_family_s_regular_takes_its_bold_and_italics() {
        let faces = cascadia();
        let chosen = choose("Test", &faces, &config(None)).unwrap();
        assert_eq!(style(&faces, chosen.regular), ("Regular", PLAIN));
        assert_eq!(style(&faces, chosen.bold), ("Bold", PLAIN));
        assert_eq!(style(&faces, chosen.italic), ("Italic", PLAIN));
        assert_eq!(style(&faces, chosen.bold_italic), ("Bold Italic", PLAIN));
    }

    #[test]
    fn semibold_takes_bold_and_semibold_italic() {
        let faces = cascadia();
        let chosen = choose("Test", &faces, &config(Some("SemiBold"))).unwrap();
        assert_eq!(style(&faces, chosen.regular), ("SemiBold", PLAIN));
        assert_eq!(style(&faces, chosen.bold), ("Bold", PLAIN));
        assert_eq!(style(&faces, chosen.italic), ("SemiBold Italic", PLAIN));
        assert_eq!(style(&faces, chosen.bold_italic), ("Bold Italic", PLAIN));
    }

    #[test]
    fn a_style_group_s_name_takes_its_face_as_regular() {
        // As `CaskaydiaCove NFM SemiBold` names its SemiBold faces, within
        // the whole family.
        let mut faces = cascadia();
        for face in &mut faces {
            face.in_group = face.style.starts_with("SemiBold");
        }
        let chosen = choose("Test", &faces, &config(None)).unwrap();
        assert_eq!(style(&faces, chosen.regular), ("SemiBold", PLAIN));
        assert_eq!(style(&faces, chosen.bold), ("Bold", PLAIN));
        assert_eq!(style(&faces, chosen.italic), ("SemiBold Italic", PLAIN));
        assert_eq!(style(&faces, chosen.bold_italic), ("Bold Italic", PLAIN));
        // The styles that the configuration names are the family's.
        let chosen = choose("Test", &faces, &config(Some("Light"))).unwrap();
        assert_eq!(style(&faces, chosen.regular), ("Light", PLAIN));
        // A group of italics alone takes one as regular.
        for face in &mut faces {
            face.in_group = face.style == "Light Italic";
        }
        let chosen = choose("Test", &faces, &config(None)).unwrap();
        assert_eq!(style(&faces, chosen.regular), ("Light Italic", PLAIN));
    }

    #[test]
    fn the_heaviest_face_draws_its_bold_heavier() {
        let faces = cascadia();
        let chosen = choose("Test", &faces, &config(Some("Bold"))).unwrap();
        let heavier = Synthesis {
            bold: true,
            italic: false,
        };
        assert_eq!(style(&faces, chosen.bold), ("Bold", heavier));
        assert_eq!(style(&faces, chosen.italic), ("Bold Italic", PLAIN));
        // No italic heavier than Bold Italic, nor a bold heavier than Bold:
        // the italic, drawn heavier, as the bold is the regular drawn
        // heavier.
        assert_eq!(style(&faces, chosen.bold_italic), ("Bold Italic", heavier));
    }

    /// The styles of the four faces that `faces` give, as `config` names
    /// them, in the order given and the other way round, which must agree.
    fn chosen(faces: &[(&str, f32, bool)], config: &FontConfig) -> [String; 4] {
        let pick = |faces: Vec<FaceInfo>| {
            let chosen = choose("Family", &faces, config).unwrap();
            [
                chosen.regular,
                chosen.bold,
                chosen.italic,
                chosen.bold_italic,
            ]
            .map(|face| {
                let (name, synthesis) = style(&faces, face);
                format!(
                    "{name}{}{}",
                    if synthesis.bold { " +bold" } else { "" },
                    if synthesis.italic { " +slant" } else { "" }
                )
            })
        };
        let given = pick(family(faces));
        let mut reversed = family(faces);
        reversed.reverse();
        assert_eq!(pick(reversed), given, "the backend's order changes nothing");
        given
    }

    #[test]
    fn faces_as_near_are_told_apart_as_css_does() {
        // Below 500, the lighter: Light, of Light and Demi, for 400; from
        // 500, the heavier: Heavy, of Demibold and Heavy, for 700; where
        // the names' order would give the others.
        let faces = [
            ("Light", 300.0, false),
            ("Demi", 500.0, false),
            ("Demibold", 600.0, false),
            ("Heavy", 800.0, false),
        ];
        let [regular, bold, ..] = chosen(&faces, &config(None));
        assert_eq!((regular.as_str(), bold.as_str()), ("Light", "Heavy"));
    }

    #[test]
    fn an_italic_is_a_true_one_rather_than_an_oblique() {
        // Of names that come after the obliques', as the order of names
        // would not take them.
        let faces = [
            ("Regular", 400.0, false),
            ("Bold", 700.0, false),
            ("Oblique", 400.0, true),
            ("Text Italic", 400.0, true),
            ("Bold Oblique", 700.0, true),
            ("Bold Text Italic", 700.0, true),
        ];
        let [_, _, italic, bold_italic] = chosen(&faces, &config(None));
        assert_eq!(
            (italic.as_str(), bold_italic.as_str()),
            ("Text Italic", "Bold Text Italic")
        );
        // A family of obliques alone takes them.
        let faces = [
            ("Regular", 400.0, false),
            ("Oblique", 400.0, true),
            ("Bold Oblique", 700.0, true),
        ];
        let [_, _, italic, bold_italic] = chosen(&faces, &config(None));
        assert_eq!(
            (italic.as_str(), bold_italic.as_str()),
            ("Oblique", "Bold Oblique")
        );
    }

    #[test]
    fn a_family_of_italics_alone_takes_its_bold_italic_as_bold() {
        let faces = [("Italic", 400.0, true), ("Bold Italic", 700.0, true)];
        assert_eq!(
            chosen(&faces, &config(None)),
            ["Italic", "Bold Italic", "Italic", "Bold Italic"]
        );
    }

    #[test]
    fn the_faces_keep_the_regular_one_s_width() {
        let faces = [
            ("Condensed", 400.0, false),
            ("Condensed Bold", 700.0, false),
            ("Condensed Italic", 400.0, true),
            ("Regular", 400.0, false),
            ("Bold", 700.0, false),
            ("Italic", 400.0, true),
        ];
        assert_eq!(
            chosen(&faces, &config(None)),
            ["Regular", "Bold", "Italic", "Bold +slant"]
        );
        assert_eq!(
            chosen(&faces, &config(Some("Condensed"))),
            [
                "Condensed",
                "Condensed Bold",
                "Condensed Italic",
                "Condensed Bold +slant"
            ]
        );
    }

    #[test]
    fn faces_that_tell_no_width_take_their_names_or_the_system_s_choice() {
        // As 3270 Nerd Font's faces are, of one weight, whose fonts tell no
        // width, and Hoefler Text's Ornaments, of the regular's weight.
        let mut faces = family(&[
            ("Condensed", 400.0, false),
            ("SemiCondensed", 400.0, false),
            ("Regular", 400.0, false),
        ]);
        for face in &mut faces {
            face.width = 0.0;
        }
        let regular = |faces: &[FaceInfo]| {
            let chosen = choose("Test", faces, &config(None)).unwrap();
            style(faces, chosen.regular).0.to_owned()
        };
        assert_eq!(regular(&faces), "Regular");
        // Of faces that tie, the system's for the family alone, before the
        // order of their names.
        let mut ornaments = family(&[("Regular", 400.0, false), ("Ornaments", 400.0, false)]);
        assert_eq!(regular(&ornaments), "Ornaments");
        ornaments[0].preferred = true;
        assert_eq!(regular(&ornaments), "Regular");
        ornaments.reverse();
        assert_eq!(regular(&ornaments), "Regular");
        // A name of a width, whatever its case and spaces.
        for (style, wanted) in [
            ("Semi Condensed", -0.1),
            ("ExtraCondensed Bold", -0.3),
            ("Narrow", -0.2),
            ("Expanded", 0.2),
            ("Book", 0.0),
        ] {
            let mut face = family(&[(style, 400.0, false)]).remove(0);
            face.width = 0.0;
            assert_eq!(width(&face), wanted, "{style}");
        }
        // The font's width, where it tells one, before the name's.
        let mut face = family(&[("Condensed", 400.0, false)]).remove(0);
        face.width = -0.35;
        assert_eq!(width(&face), -0.35);
    }

    #[test]
    fn a_family_of_one_face_makes_the_others_of_it() {
        let faces = family(&[("Regular", 400.0, false)]);
        let chosen = choose("Test", &faces, &FontConfig::default()).unwrap();
        let id = FaceId(0);
        assert_eq!(chosen.regular, Face::plain(id));
        assert_eq!(
            chosen.bold.synthesis,
            Synthesis {
                bold: true,
                italic: false
            }
        );
        assert_eq!(
            chosen.italic.synthesis,
            Synthesis {
                bold: false,
                italic: true
            }
        );
        assert_eq!(
            chosen.bold_italic.synthesis,
            Synthesis {
                bold: true,
                italic: true
            }
        );
        assert!(
            [chosen.bold, chosen.italic, chosen.bold_italic]
                .iter()
                .all(|face| face.id == id)
        );
    }

    #[test]
    fn named_styles_take_their_faces_whatever_their_case() {
        let faces = cascadia();
        let named = |name: &str| Some(Name::try_from(name.to_owned()).unwrap());
        let config = FontConfig {
            style: named("light"),
            bold_style: named("semi-bold"),
            italic_style: named("light italic"),
            bold_italic_style: named("SEMIBOLD ITALIC"),
            ..FontConfig::default()
        };
        let chosen = choose("Test", &faces, &config).unwrap();
        assert_eq!(style(&faces, chosen.regular).0, "Light");
        assert_eq!(style(&faces, chosen.bold).0, "SemiBold");
        assert_eq!(style(&faces, chosen.italic).0, "Light Italic");
        assert_eq!(style(&faces, chosen.bold_italic).0, "SemiBold Italic");
    }

    #[test]
    fn a_style_that_the_family_lacks_lists_its_styles() {
        let faces = family(&[("Regular", 400.0, false), ("Bold", 700.0, false)]);
        let config = FontConfig {
            bold_style: Some(Name::try_from("Black".to_owned()).unwrap()),
            ..FontConfig::default()
        };
        let error = choose("Test", &faces, &config).unwrap_err();
        assert_eq!(
            error.to_string(),
            "the family `Test` has no style `Black`, which `bold-style` names; it has Regular, Bold"
        );
    }

    #[test]
    fn the_styles_that_an_error_lists_come_once() {
        let faces = family(&[
            ("Regular", 400.0, false),
            ("Regular", 400.0, false),
            ("Bold", 700.0, false),
        ]);
        let config = FontConfig {
            style: Some(Name::try_from("Light".to_owned()).unwrap()),
            ..FontConfig::default()
        };
        let error = choose("Test", &faces, &config).unwrap_err();
        let FaceError::MissingStyle { key, available, .. } = error else {
            panic!("a missing style: {error}");
        };
        assert_eq!(key, "style");
        assert_eq!(available, ["Regular", "Bold"]);
    }

    #[test]
    fn a_family_without_faces_says_so() {
        assert_eq!(
            choose("Empty", &[], &FontConfig::default())
                .unwrap_err()
                .to_string(),
            "the family `Empty` has no face"
        );
    }

    /// A backend of two families, Mono and Default, whose fonts it neither
    /// measures nor draws.
    struct Families;

    impl FontBackend for Families {
        fn default_family(&mut self) -> String {
            "Default".to_owned()
        }

        fn family(&mut self, name: &str) -> Option<Vec<FaceInfo>> {
            match name {
                "Mono" => Some(cascadia()),
                "Default" => Some(family(&[("Regular", 400.0, false), ("Bold", 700.0, false)])),
                _ => None,
            }
        }

        fn metrics(&mut self, _: FaceId, _: f32) -> crate::FaceMetrics {
            crate::FaceMetrics::default()
        }

        fn glyph(&mut self, _: FaceId, _: char) -> Option<crate::GlyphId> {
            None
        }

        fn fallback(&mut self, _: FaceId, _: &str) -> Option<FaceId> {
            None
        }

        fn shape(
            &mut self,
            _: FaceId,
            _: f32,
            _: &str,
            _: crate::Features,
            _: &mut Vec<crate::ShapedGlyph>,
        ) {
        }

        fn rasterize(
            &mut self,
            _: FaceId,
            _: crate::GlyphId,
            _: f32,
            _: Synthesis,
            _: crate::Rendering,
        ) -> crate::Bitmap {
            crate::Bitmap::empty(crate::BitmapFormat::Gray)
        }
    }

    fn named_family(family: &str, style: &str) -> FontConfig {
        FontConfig {
            family: Some(Name::try_from(family.to_owned()).unwrap()),
            style: Some(Name::try_from(style.to_owned()).unwrap()),
            ..FontConfig::default()
        }
    }

    #[test]
    fn a_family_takes_its_faces() {
        let resolved = resolve(&mut Families, &named_family("Mono", "SemiBold")).unwrap();
        assert_eq!(resolved.family, "Mono");
        assert_eq!(resolved.missing_family, None);
        assert_eq!(style(&cascadia(), resolved.faces.regular).0, "SemiBold");
    }

    #[test]
    fn a_family_that_is_not_installed_falls_back_to_the_default() {
        // The missing family's style, which the default one lacks, is not
        // looked for in the default one.
        let resolved = resolve(&mut Families, &named_family("Missing", "SemiBold")).unwrap();
        assert_eq!(resolved.family, "Default");
        assert_eq!(resolved.missing_family.as_deref(), Some("Missing"));
        assert_eq!(resolved.faces.regular, Face::plain(FaceId(0)));
        assert_eq!(resolved.faces.bold, Face::plain(FaceId(1)));
    }

    #[test]
    fn no_family_takes_the_default() {
        let resolved = resolve(&mut Families, &FontConfig::default()).unwrap();
        assert_eq!(resolved.family, "Default");
        assert_eq!(resolved.missing_family, None);
    }

    #[test]
    fn a_family_of_italics_alone_takes_one_as_regular() {
        let faces = family(&[("Italic", 400.0, true), ("Bold Italic", 700.0, true)]);
        let chosen = choose("Test", &faces, &FontConfig::default()).unwrap();
        assert_eq!(chosen.regular.id, FaceId(0));
    }
}
