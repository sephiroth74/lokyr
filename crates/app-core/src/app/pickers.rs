//! The themes that a picker of the plugins', as the palette's, lists,
//! previews and chooses: those of the user's themes' folder, those that the
//! app holds, which it reads once, and the built-in ones, by name, a file
//! of the folder before one of its name that the app holds or builds in;
//! a theme previewed in place of the appearance in force's until the next,
//! the choice, or none; and the choice named in the config's `[theme]`.
//! And the system's monospaced font families that a picker lists, a family
//! previewed in every pane, and the one chosen, named in `[font] family`.

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::time::{Duration, Instant};

use lokyr_kernel::config::{self, ThemeConfig};
use lokyr_kernel::plugins::host::{FontFace, Fonts, ThemeInfo, ThemeOrigin, Themes};
use lokyr_kernel::themes::{self, ThemeVariant};
use lokyr_platform_core::{Appearance, Platform};
use lokyr_text_core::{FontBackend, FontConfig, Name, styles_within};

use super::{App, sized};
use crate::drawing::Backend;

/// How long the monospaced families that a picker lists stay as they were
/// read, so that a plugin that asks for them again and again reads the
/// system's fonts once in a while, some tens of milliseconds each.
const FONTS_KEPT: Duration = Duration::from_secs(30);

/// The system's font families as they were read: when, the monospaced
/// ones' names, every one's, and the default family's.
#[derive(Debug)]
pub(super) struct FontList {
    read: Instant,
    families: Vec<String>,
    all: Vec<String>,
    default: String,
}

/// The monospaced families that a picker lists, read of the system's fonts
/// past [`FONTS_KEPT`] into `kept`, in their names' order, their case
/// aside, and the family in force: `configured`, the config's, where the
/// system has it, else the default one.
pub(super) fn font_list<B: Backend>(
    kept: &RefCell<Option<FontList>>,
    configured: Option<&str>,
) -> Fonts {
    let mut kept = kept.borrow_mut();
    let fresh = kept
        .as_ref()
        .is_some_and(|list| list.read.elapsed() < FONTS_KEPT);
    if !fresh {
        let mut backend = B::fonts();
        let ordered = |mut names: Vec<String>| {
            names.sort_by_key(|name| name.to_lowercase());
            names.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
            names
        };
        let families = ordered(backend.monospaced_families());
        let all = ordered(backend.families());
        *kept = Some(FontList {
            read: Instant::now(),
            families,
            all,
            default: backend.default_family(),
        });
    }
    let Some(list) = kept.as_ref() else {
        return Fonts::default();
    };
    let named = |name: &str| {
        list.families
            .iter()
            .find(|family| family.eq_ignore_ascii_case(name))
            .cloned()
    };
    let current = configured
        .and_then(named)
        .or_else(|| named(&list.default))
        .unwrap_or_else(|| list.default.clone());
    Fonts {
        current,
        families: list.families.clone(),
        all: list.all.clone(),
    }
}

/// The styles of the font family `family`, by the names that its faces
/// give them, once each, from the lightest, each upright before its
/// italic, the narrower first; none of a family that the system lacks.
pub(super) fn font_styles<B: Backend>(family: &str) -> Vec<String> {
    let Some(mut faces) = B::fonts().family(family) else {
        return Vec::new();
    };
    faces.sort_by(|a, b| {
        a.weight
            .total_cmp(&b.weight)
            .then(a.italic.cmp(&b.italic))
            .then(a.width.total_cmp(&b.width))
    });
    let mut styles: Vec<String> = Vec::new();
    for face in faces {
        if !styles.contains(&face.style) {
            styles.push(face.style);
        }
    }
    styles
}

/// The themes of the files of `directory`, each of `origin`, as their
/// files name them; none of a folder that is not there, nor of a file that
/// is no theme's.
pub(super) fn themes_in(directory: &Path, origin: ThemeOrigin) -> Vec<ThemeInfo> {
    let Ok(entries) = fs::read_dir(directory) else {
        return Vec::new();
    };
    let mut found = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let theme_file = path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| themes::EXTENSIONS.contains(&extension));
        if !theme_file {
            continue;
        }
        for (name, variant) in themes::themes_of(&path).unwrap_or_default() {
            found.push(ThemeInfo {
                name,
                variant,
                origin,
            });
        }
    }
    found
}

/// The themes that a picker lists, with the appearance in force, as
/// `appearance` is, and its theme of `config`: those of the themes' folder
/// `folder` and of `held`, the folder of the themes that the app holds, as
/// the last read of `catalog` off the main thread listed them, a read woken
/// of `waker` starting where none did lately, and the built-in ones, by
/// name, the first of a name alone, in the order of their names; the
/// built-in ones alone, as loading, while the first read runs.
pub(super) fn theme_list(
    catalog: &RefCell<super::theme_catalog::ThemeCatalog>,
    (folder, held, waker): (
        Option<&Path>,
        Option<&Path>,
        Option<&lokyr_platform_core::Waker>,
    ),
    appearance: Appearance,
    config: &ThemeConfig,
) -> Themes {
    // The folder's and the held ones as the last read off the main thread
    // listed them, a read starting where none did lately; none while the
    // first runs.
    let listed = {
        let mut catalog = catalog.borrow_mut();
        catalog.refresh(
            (folder.map(Path::to_path_buf), held.map(Path::to_path_buf)),
            waker,
            Instant::now(),
        );
        catalog.listed.clone()
    };
    let loading = listed.is_none();
    let built_in = [
        (themes::LIGHT, ThemeVariant::Light),
        (themes::DARK, ThemeVariant::Dark),
    ]
    .map(|(name, variant)| ThemeInfo {
        name: name.to_owned(),
        variant,
        origin: ThemeOrigin::BuiltIn,
    });
    let mut seen = BTreeSet::new();
    let mut list: Vec<ThemeInfo> = listed
        .unwrap_or_default()
        .into_iter()
        .chain(built_in)
        .filter(|theme| seen.insert(theme.name.clone()))
        .collect();
    list.sort_by_key(|theme| theme.name.to_lowercase());
    let (appearance, current) = match appearance {
        Appearance::Light => (ThemeVariant::Light, config.light.clone()),
        Appearance::Dark => (ThemeVariant::Dark, config.dark.clone()),
    };
    Themes {
        appearance,
        current,
        themes: list,
        loading,
    }
}

impl<B: Backend> App<B> {
    /// Shows the theme `name` in place of the appearance in force's, as a
    /// plugin's picker previews it; `None` shows the config's again.
    pub(super) fn preview_theme(&mut self, platform: &mut dyn Platform, name: Option<String>) {
        if self.theme_preview == name {
            return;
        }
        self.theme_preview = name;
        self.load_themes(platform);
    }

    /// Names the theme `name` in the config's `[theme]` for the appearance
    /// of `variant`, as a plugin's picker chooses it, which shows at once,
    /// a preview ending.
    pub(super) fn set_theme(
        &mut self,
        platform: &mut dyn Platform,
        variant: ThemeVariant,
        name: &str,
    ) {
        self.theme_preview = None;
        let key = match variant {
            ThemeVariant::Light => "light",
            ThemeVariant::Dark => "dark",
        };
        match &self.config_file {
            Some(file) => {
                let path = file.path.clone();
                if let Err(error) = config::set_string(&path, "theme", key, name) {
                    eprintln!("lokyr: the theme cannot be named in the config: {error}");
                }
                // The config read again, which loads the theme that it
                // names.
                self.reload(platform);
            }
            None => eprintln!("lokyr: the theme cannot be named, as there is no config file"),
        }
        self.load_themes(platform);
    }

    /// The config's font, of the family that a picker previews in place of
    /// its own, if one does, of the config's styles that it has, its own in
    /// place of those that it lacks.
    pub(super) fn font_in_force(&self) -> FontConfig {
        let mut font = self.family_in_force();
        for (face, style) in &self.style_preview {
            let Ok(style) = Name::try_from(style.clone()) else {
                continue;
            };
            let slot = match face {
                FontFace::Regular => &mut font.style,
                FontFace::Bold => &mut font.bold_style,
                FontFace::Italic => &mut font.italic_style,
                FontFace::BoldItalic => &mut font.bold_italic_style,
            };
            *slot = Some(style);
        }
        font
    }

    /// The font of the config, of the family that a picker previews, if
    /// one does, of the config's styles that the family has.
    fn family_in_force(&self) -> FontConfig {
        let Some(preview) = self.font_preview.clone() else {
            return self.config.font.clone();
        };
        let Ok(family) = Name::try_from(preview.clone()) else {
            return self.config.font.clone();
        };
        let font = FontConfig {
            family: Some(family),
            ..self.config.font.clone()
        };
        match B::fonts().family(&preview) {
            Some(faces) => styles_within(&font, &faces).0,
            None => font,
        }
    }

    /// Shows the monospaced family `family` in every pane in place of the
    /// config's, as a plugin's picker previews it; `None` shows the
    /// config's again. A family changes the cell's size: every pane takes
    /// its new grid, and its program the signal of it, as a picker
    /// previews a family once its selection rests.
    pub(super) fn preview_font(&mut self, platform: &mut dyn Platform, family: Option<String>) {
        let ended = family.is_none() && !self.style_preview.is_empty();
        if self.font_preview == family && !ended {
            return;
        }
        // None ends the styles' previews too.
        if family.is_none() {
            self.style_preview.clear();
        }
        self.font_preview = family;
        self.show_font(platform);
    }

    /// Shows the face `face` of the terminal's text in every pane of the
    /// style `style` of the family in force, in place of the config's, as
    /// a plugin previews it, until a preview of no family ends it.
    pub(super) fn preview_font_style(
        &mut self,
        platform: &mut dyn Platform,
        face: FontFace,
        style: String,
    ) {
        if self
            .style_preview
            .iter()
            .any(|(own, shown)| *own == face && *shown == style)
        {
            return;
        }
        self.style_preview.retain(|(own, _)| *own != face);
        self.style_preview.push((face, style));
        self.show_font(platform);
    }

    /// Names the monospaced family `family` in the config's `[font]
    /// family`, as a plugin's picker chooses it, which shows at once, a
    /// preview ending: the config's styles that it lacks commented out
    /// where they were, as the config would refuse them, for the user to
    /// take back.
    pub(super) fn set_font(&mut self, platform: &mut dyn Platform, family: &str) {
        self.font_preview = None;
        self.style_preview.clear();
        let lacking = B::fonts()
            .family(family)
            .map(|faces| styles_within(&self.config.font, &faces).1)
            .unwrap_or_default();
        let written = match &self.config_file {
            Some(file) => {
                config::set_string_hiding(&file.path, "font", ("family", family), &lacking)
                    .map(|()| file.path.clone())
                    .map_err(|error| error.to_string())
            }
            None => Err("there is no config file".to_owned()),
        };
        match written {
            // The config read again, which takes the family that it names.
            Ok(_) => self.reload(platform),
            Err(error) => {
                eprintln!("lokyr: the font family cannot be named in the config: {error}");
                self.show_font(platform);
            }
        }
    }

    /// Shows the font in force, of the text's size in force, in every
    /// window.
    fn show_font(&mut self, platform: &mut dyn Platform) {
        let font = sized(&self.font_in_force(), self.font_size);
        if let Some(drawing) = &mut self.drawing
            && let Err(error) = drawing.set_font(font)
        {
            eprintln!("lokyr: {error}");
            return;
        }
        for &window in self.windows.keys() {
            platform.request_frame(window);
        }
    }
}
