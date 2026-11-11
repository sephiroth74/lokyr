//! The themes that a picker of the plugins', as the palette's, lists,
//! previews and chooses: those of the user's themes' folder, those that the
//! app holds, which it reads once, and the built-in ones, by name, a file
//! of the folder before one of its name that the app holds or builds in;
//! a theme previewed in place of the appearance in force's until the next,
//! the choice, or none; and the choice named in the config's `[theme]`.
//! And the system's monospaced font families that a picker lists, a family
//! previewed in every pane, and the one chosen, named in `[font] family`.

use std::cell::{OnceCell, RefCell};
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::time::{Duration, Instant};

use lokyr_kernel::config::{self, ThemeConfig};
use lokyr_kernel::plugins::host::{Fonts, ThemeInfo, ThemeOrigin, Themes};
use lokyr_kernel::themes::{self, ThemeVariant};
use lokyr_platform_core::{Appearance, Platform};
use lokyr_text_core::{FontBackend, FontConfig, Name};

use super::{App, sized};
use crate::drawing::Backend;

/// How long the monospaced families that a picker lists stay as they were
/// read, so that a plugin that asks for them again and again reads the
/// system's fonts once in a while, some tens of milliseconds each.
const FONTS_KEPT: Duration = Duration::from_secs(30);

/// The system's monospaced font families as they were read: when, their
/// names, and the default family's.
#[derive(Debug)]
pub(super) struct FontList {
    read: Instant,
    families: Vec<String>,
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
        let mut families = backend.monospaced_families();
        families.sort_by_key(|name| name.to_lowercase());
        families.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
        *kept = Some(FontList {
            read: Instant::now(),
            families,
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
    }
}

/// The themes of the files of `directory`, each of `origin`, as their
/// files name them; none of a folder that is not there, nor of a file that
/// is no theme's.
fn themes_in(directory: &Path, origin: ThemeOrigin) -> Vec<ThemeInfo> {
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
/// `folder`, those of `held`, the folder of the themes that the app holds,
/// read once into `held_list`, and the built-in ones, by name, the first of
/// a name alone, in the order of their names.
pub(super) fn theme_list(
    (folder, held): (Option<&Path>, Option<&Path>),
    held_list: &OnceCell<Vec<ThemeInfo>>,
    appearance: Appearance,
    config: &ThemeConfig,
) -> Themes {
    let held = held_list.get_or_init(|| {
        held.map(|held| themes_in(held, ThemeOrigin::Held))
            .unwrap_or_default()
    });
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
    let mut list: Vec<ThemeInfo> = folder
        .map(|folder| themes_in(folder, ThemeOrigin::Folder))
        .unwrap_or_default()
        .into_iter()
        .chain(held.iter().cloned())
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
    /// its own, if one does.
    pub(super) fn font_in_force(&self) -> FontConfig {
        let preview = self
            .font_preview
            .clone()
            .and_then(|family| Name::try_from(family).ok());
        match preview {
            Some(family) => FontConfig {
                family: Some(family),
                ..self.config.font.clone()
            },
            None => self.config.font.clone(),
        }
    }

    /// Shows the monospaced family `family` in every pane in place of the
    /// config's, as a plugin's picker previews it; `None` shows the
    /// config's again. A family changes the cell's size: every pane takes
    /// its new grid, and its program the signal of it, as a picker
    /// previews a family once its selection rests.
    pub(super) fn preview_font(&mut self, platform: &mut dyn Platform, family: Option<String>) {
        if self.font_preview == family {
            return;
        }
        self.font_preview = family;
        self.show_font(platform);
    }

    /// Names the monospaced family `family` in the config's `[font]
    /// family`, as a plugin's picker chooses it, which shows at once, a
    /// preview ending.
    pub(super) fn set_font(&mut self, platform: &mut dyn Platform, family: &str) {
        self.font_preview = None;
        let written = match &self.config_file {
            Some(file) => config::set_string(&file.path, "font", "family", family)
                .map(|()| file.path.clone())
                .map_err(|error| error.to_string()),
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
