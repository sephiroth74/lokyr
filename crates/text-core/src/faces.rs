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

/// The faces that `faces`, a family's, give to the four styles of text, as
/// `config` names them:
/// - regular: the style that it names, or the face nearest to the regular
///   weight, upright, among the faces of the style group that the family
///   was named by when it was named by one;
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
    let upright = || faces.iter().filter(|face| !face.italic);
    let italics = || faces.iter().filter(|face| face.italic);
    let grouped = faces.iter().any(|face| face.in_group);
    let group = || faces.iter().filter(move |face| face.in_group || !grouped);

    let regular = match named("style", config.style.as_ref())? {
        Some(face) => face,
        None => nearest(group().filter(|face| !face.italic), REGULAR)
            .or_else(|| nearest(group(), REGULAR))
            .ok_or_else(|| FaceError::NoFace {
                family: family.to_owned(),
            })?,
    };

    let bold = match named("bold-style", config.bold_style.as_ref())? {
        Some(face) => Some(face),
        None => {
            let target = BOLD.max(regular.weight + 100.0);
            nearest(
                upright().filter(|face| face.weight > regular.weight),
                target,
            )
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

/// The face of `faces` whose weight is nearest to `weight`, the first of
/// those as near.
fn nearest<'a>(faces: impl Iterator<Item = &'a FaceInfo>, weight: f32) -> Option<&'a FaceInfo> {
    faces.fold(None, |best: Option<&FaceInfo>, face| match best {
        Some(best) if (best.weight - weight).abs() <= (face.weight - weight).abs() => Some(best),
        _ => Some(face),
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
                in_group: false,
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
