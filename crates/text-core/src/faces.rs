//! The faces of a family that the terminal's four styles of text take, as
//! the configuration names them or the family's weights choose them.

use std::fmt;

use crate::backend::{FaceId, FaceInfo, Synthesis};
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

/// A style that the configuration names, which the family lacks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MissingStyle {
    /// The style that the configuration names.
    pub style: String,
    /// The family's styles, in its order.
    pub available: Vec<String>,
}

impl fmt::Display for MissingStyle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "the family has no style `{}`; it has {}",
            self.style,
            self.available.join(", ")
        )
    }
}

impl std::error::Error for MissingStyle {}

/// The weight of a family's regular face, as CSS numbers it.
const REGULAR: f32 = 400.0;

/// The lightest weight that bold takes, as CSS numbers it.
const BOLD: f32 = 700.0;

/// The faces that `faces`, a family's, give to the four styles of text, as
/// `config` names them:
/// - regular: the style that it names, or the face nearest to the regular
///   weight, upright;
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
/// A style is named as the family names it, but for case, spaces and
/// hyphens: `semi-bold` is `SemiBold`.
pub fn choose(faces: &[FaceInfo], config: &FontConfig) -> Result<Faces, MissingStyle> {
    let named = |style: Option<&Name>| -> Result<Option<&FaceInfo>, MissingStyle> {
        let Some(style) = style else {
            return Ok(None);
        };
        let wanted = normalize(style.as_str());
        faces
            .iter()
            .find(|face| normalize(&face.style) == wanted)
            .map(Some)
            .ok_or_else(|| MissingStyle {
                style: style.to_string(),
                available: faces.iter().map(|face| face.style.clone()).collect(),
            })
    };
    let upright = || faces.iter().filter(|face| !face.italic);
    let italics = || faces.iter().filter(|face| face.italic);

    let regular = match named(config.style.as_ref())? {
        Some(face) => face,
        None => nearest(upright(), REGULAR)
            .or_else(|| nearest(faces.iter(), REGULAR))
            .ok_or_else(|| MissingStyle {
                style: "Regular".to_owned(),
                available: Vec::new(),
            })?,
    };

    let bold = match named(config.bold_style.as_ref())? {
        Some(face) => Some(face),
        None => {
            let target = BOLD.max(regular.weight + 100.0);
            nearest(
                upright().filter(|face| face.weight > regular.weight),
                target,
            )
        }
    };
    let italic = match named(config.italic_style.as_ref())? {
        Some(face) => Some(face),
        None => nearest(italics(), regular.weight),
    };
    let bold_italic = match named(config.bold_italic_style.as_ref())? {
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
        let chosen = choose(&faces, &config(None)).unwrap();
        assert_eq!(style(&faces, chosen.regular), ("Regular", PLAIN));
        assert_eq!(style(&faces, chosen.bold), ("Bold", PLAIN));
        assert_eq!(style(&faces, chosen.italic), ("Italic", PLAIN));
        assert_eq!(style(&faces, chosen.bold_italic), ("Bold Italic", PLAIN));
    }

    #[test]
    fn semibold_takes_bold_and_semibold_italic() {
        let faces = cascadia();
        let chosen = choose(&faces, &config(Some("SemiBold"))).unwrap();
        assert_eq!(style(&faces, chosen.regular), ("SemiBold", PLAIN));
        assert_eq!(style(&faces, chosen.bold), ("Bold", PLAIN));
        assert_eq!(style(&faces, chosen.italic), ("SemiBold Italic", PLAIN));
        assert_eq!(style(&faces, chosen.bold_italic), ("Bold Italic", PLAIN));
    }

    #[test]
    fn the_heaviest_face_draws_its_bold_heavier() {
        let faces = cascadia();
        let chosen = choose(&faces, &config(Some("Bold"))).unwrap();
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
        let chosen = choose(&faces, &FontConfig::default()).unwrap();
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
        let chosen = choose(&faces, &config).unwrap();
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
        let error = choose(&faces, &config).unwrap_err();
        assert_eq!(error.available, ["Regular", "Bold"]);
        assert_eq!(
            error.to_string(),
            "the family has no style `Black`; it has Regular, Bold"
        );
    }

    #[test]
    fn a_family_of_italics_alone_takes_one_as_regular() {
        let faces = family(&[("Italic", 400.0, true), ("Bold Italic", 700.0, true)]);
        let chosen = choose(&faces, &FontConfig::default()).unwrap();
        assert_eq!(chosen.regular.id, FaceId(0));
        assert!(choose(&[], &FontConfig::default()).is_err());
    }
}
