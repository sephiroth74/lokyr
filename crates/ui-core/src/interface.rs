//! The `[interface]` section of the configuration, the font of the app's
//! own interface, as its tab bar and its panels, and what measures its texts
//! and icons for the view trees' [`layout`](crate::layout).
//!
//! ```toml
//! [interface]
//! font-family = "Helvetica Neue"   # the system's interface font when absent
//! font-size = 13                   # points, 4 to 200
//! ```

use lokyr_text_core::{
    FaceError, FaceId, Features, FontBackend, FontConfig, Name, Size, choose_faces,
};
use serde::{Deserialize, Serialize};

use crate::layout::Measure;

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

/// The font of the interface's text, its family's regular face at its size
/// of points, which measures the view trees' texts and icons in points: at
/// its size of points, rather than of pixels, as a family of optical sizes,
/// as the system's, draws a size of its own at each, which the display's
/// scale enlarges.
pub struct InterfaceFont<B> {
    backend: B,
    family: String,
    missing_family: Option<String>,
    face: FaceId,
    size: f32,
    line: f32,
}

impl<B: FontBackend> InterfaceFont<B> {
    /// The font that `config` names, through `backend`: the system's
    /// interface family when it names none, or one that is not installed.
    pub fn new(mut backend: B, config: &InterfaceConfig) -> Result<Self, FaceError> {
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
        let face = choose_faces(&family, &faces, &FontConfig::default())?
            .regular
            .id;
        let size = config.font_size.points();
        let metrics = backend.metrics(face, size);
        Ok(Self {
            backend,
            family,
            missing_family,
            face,
            size,
            line: metrics.ascent + metrics.descent + metrics.leading,
        })
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

    /// The face of the text.
    pub fn face(&self) -> FaceId {
        self.face
    }

    /// The size of the text, in points.
    pub fn size(&self) -> f32 {
        self.size
    }

    /// The height of a line of the text, in points.
    pub fn line(&self) -> f32 {
        self.line
    }
}

impl<B: FontBackend> Measure for InterfaceFont<B> {
    /// Its advances, ligatures and kerning as the face makes them, and a
    /// line's height.
    fn text(&mut self, text: &str) -> (f32, f32) {
        let features = Features { ligatures: true };
        let width = self.backend.width(self.face, self.size, text, features);
        (width, self.line)
    }

    /// A square of a line's height.
    fn icon(&mut self, _name: &str) -> (f32, f32) {
        (self.line, self.line)
    }
}

/// The system's interface font, as CoreText gives it.
#[cfg(all(test, target_os = "macos"))]
mod system {
    use lokyr_font_coretext::CoreText;

    use super::*;
    use crate::layout::Layout;
    use crate::tree::Tree;
    use lokyr_plugin_sdk::view::{self, Node};

    #[test]
    fn the_system_s_font_measures_a_bar_at_two_scales() {
        let mut font = InterfaceFont::new(CoreText::new(), &InterfaceConfig::default()).unwrap();
        assert_eq!(font.missing_family(), None);
        assert_eq!(font.size(), 13.0);
        // A line of 13 points is about 16 high.
        assert!((14.0..18.0).contains(&font.line()), "{}", font.line());
        let (narrow, _) = font.text("iiii");
        let (wide, _) = font.text("mmmm");
        assert!(narrow * 2.0 < wide, "{narrow} {wide}");
        let bar = Node::container(vec![Node::icon("terminal"), Node::text("~/Projects")]).layout(
            view::Layout {
                gap: 4.0,
                ..view::Layout::default()
            },
        );
        let mut tree = Tree::new();
        let changes = tree.update(&bar).unwrap();
        let mut frames = Vec::new();
        for scale in [1.0, 2.0] {
            let mut layout = Layout::new(scale);
            layout.update(&tree, &changes);
            layout.compute(&tree, None, None, &mut font);
            let root = tree.root().unwrap();
            let title = tree.children(root)[1];
            frames.push(layout.frame(&tree, title).unwrap());
        }
        // Twice the scale, twice the pixels, as the points are measured at
        // their own size, within the pixel that each rounds to.
        let [one, two] = [frames[0], frames[1]];
        assert!(
            (two.width - one.width * 2.0).abs() <= 2.0,
            "{one:?} {two:?}"
        );
        assert!((two.x - one.x * 2.0).abs() <= 2.0, "{one:?} {two:?}");
    }

    #[test]
    fn a_family_that_is_not_installed_takes_the_system_s() {
        let config = InterfaceConfig {
            font_family: Some(Name::try_from("No Such Family Here".to_owned()).unwrap()),
            ..InterfaceConfig::default()
        };
        let font = InterfaceFont::new(CoreText::new(), &config).unwrap();
        assert_eq!(font.missing_family(), Some("No Such Family Here"));
        let mut backend = CoreText::new();
        assert_eq!(font.family(), backend.interface_family());
        // A family that is: its own.
        let config = InterfaceConfig {
            font_family: Some(Name::try_from("Helvetica".to_owned()).unwrap()),
            ..InterfaceConfig::default()
        };
        let font = InterfaceFont::new(CoreText::new(), &config).unwrap();
        assert_eq!((font.family(), font.missing_family()), ("Helvetica", None));
    }
}
