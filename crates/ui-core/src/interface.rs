//! The `[interface]` section of the configuration, the font of the app's
//! own interface, as its tab bar and its panels, and its icons.
//!
//! ```toml
//! [interface]
//! font-family = "Helvetica Neue"   # the system's interface font when absent
//! font-size = 13                   # points, 4 to 200
//! ```
//!
//! The window's [`Font`](crate::font::Font) takes the interface's face, so
//! that the interface's glyphs share the atlas of the terminal's: it
//! measures the view trees' texts and icons for their
//! [`layout`](crate::layout), and draws them.

use lokyr_text_core::{FaceError, FaceId, FontBackend, FontConfig, Name, Size, choose_faces};
use serde::{Deserialize, Serialize};

/// The `[interface]` section: the font of the app's own interface.
#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct InterfaceConfig {
    /// The family of the interface's text; `None` for the system's, as its
    /// menus and windows show it.
    pub font_family: Option<Name>,
    /// The size of the interface's text, in points.
    pub font_size: Size,
}

/// The face of the interface's text: its family's regular face, of the
/// optical size of its points, so that a display of twice the pixels draws
/// the same forms twice as large, as a family of optical sizes, as the
/// system's, draws each size of its own otherwise; and what it measures, in
/// points.
#[derive(Clone, Debug, PartialEq)]
pub struct InterfaceFace {
    family: String,
    missing_family: Option<String>,
    face: FaceId,
    size: f32,
    ascent: f32,
    line: f32,
}

impl InterfaceFace {
    /// The face that `config` names, through `backend`: of the system's
    /// interface family when it names none, or one that is not installed.
    pub fn new(backend: &mut dyn FontBackend, config: &InterfaceConfig) -> Result<Self, FaceError> {
        let wanted = config.font_family.as_ref().map(Name::as_str);
        let found = wanted.and_then(|name| Some((name.to_owned(), backend.family(name)?)));
        let missing_family = match (&found, wanted) {
            (None, Some(name)) => Some(name.to_owned()),
            _ => None,
        };
        let (family, faces) = match found {
            Some(found) => found,
            None => {
                let family = backend.interface_family();
                let faces = backend.family(&family).unwrap_or_default();
                (family, faces)
            }
        };
        let regular = choose_faces(&family, &faces, &FontConfig::default())?
            .regular
            .id;
        let size = config.font_size.points();
        Ok(Self::of(backend, family, missing_family, regular, size))
    }

    /// The face `regular` of `family`, at `size` points.
    pub(crate) fn of(
        backend: &mut dyn FontBackend,
        family: String,
        missing_family: Option<String>,
        regular: FaceId,
        size: f32,
    ) -> Self {
        let face = backend.optical(regular, size);
        let metrics = backend.metrics(face, size);
        Self {
            family,
            missing_family,
            face,
            size,
            ascent: metrics.ascent,
            line: metrics.ascent + metrics.descent + metrics.leading,
        }
    }

    /// The family's name.
    pub fn family(&self) -> &str {
        &self.family
    }

    /// The family that the configuration names, when it is not installed
    /// and the text takes the system's instead.
    pub fn missing_family(&self) -> Option<&str> {
        self.missing_family.as_deref()
    }

    /// The face, of the optical size of its points.
    pub fn face(&self) -> FaceId {
        self.face
    }

    /// The size of the text, in points.
    pub fn size(&self) -> f32 {
        self.size
    }

    /// The height of the text above its baseline, in points.
    pub fn ascent(&self) -> f32 {
        self.ascent
    }

    /// The height of a line of the text, in points, of which an icon takes a
    /// square.
    pub fn line(&self) -> f32 {
        self.line
    }
}

/// The icons of the host, by their names, which a view tree's icon names:
/// the codicons of Symbols Nerd Font, which the terminal's text takes too,
/// drawn in the text's color.
const ICONS: &[(&str, char)] = &[
    ("add", '\u{ea60}'),
    ("arrow-down", '\u{ea9a}'),
    ("arrow-up", '\u{eaa1}'),
    ("bell", '\u{eaa2}'),
    ("case-sensitive", '\u{eab1}'),
    ("check", '\u{eab2}'),
    ("chevron-down", '\u{eab4}'),
    ("chevron-left", '\u{eab5}'),
    ("chevron-right", '\u{eab6}'),
    ("chevron-up", '\u{eab7}'),
    ("circle", '\u{eabc}'),
    ("circle-filled", '\u{ea71}'),
    ("close", '\u{ea76}'),
    ("copy", '\u{ebcc}'),
    ("edit", '\u{ea73}'),
    ("ellipsis", '\u{ea7c}'),
    ("error", '\u{ea87}'),
    ("filter", '\u{eaf1}'),
    ("folder", '\u{ea83}'),
    ("history", '\u{ea82}'),
    ("info", '\u{ea74}'),
    ("pin", '\u{eb2b}'),
    ("regex", '\u{eb38}'),
    ("replace", '\u{eb3d}'),
    ("search", '\u{ea6d}'),
    ("settings", '\u{eb51}'),
    ("sparkle", '\u{ec10}'),
    ("split-horizontal", '\u{eb56}'),
    ("split-vertical", '\u{eb57}'),
    ("terminal", '\u{ea85}'),
    ("trash", '\u{ea81}'),
    ("warning", '\u{ea6c}'),
    ("whole-word", '\u{eb7e}'),
];

/// The character of the icon `name`, of the symbols' font, if the host has
/// it.
pub fn icon(name: &str) -> Option<char> {
    ICONS
        .binary_search_by(|(known, _)| known.cmp(&name))
        .ok()
        .map(|index| ICONS[index].1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_icons_are_named_in_order_and_found_by_their_names() {
        assert!(ICONS.windows(2).all(|pair| pair[0].0 < pair[1].0));
        assert_eq!(icon("terminal"), Some('\u{ea85}'));
        assert_eq!(icon("whole-word"), Some('\u{eb7e}'));
        assert_eq!(icon("add"), Some('\u{ea60}'));
        assert_eq!(icon("Terminal"), None);
        assert_eq!(icon(""), None);
    }
}
