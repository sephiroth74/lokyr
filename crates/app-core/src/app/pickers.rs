//! The themes that a picker of the plugins', as the palette's, lists,
//! previews and chooses: those of the user's themes' folder, those that the
//! app holds, which it reads once, and the built-in ones, by name, a file
//! of the folder before one of its name that the app holds or builds in;
//! a theme previewed in place of the appearance in force's until the next,
//! the choice, or none; and the choice named in the config's `[theme]`.

use std::cell::OnceCell;
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use lokyr_kernel::config::{self, ThemeConfig};
use lokyr_kernel::plugins::host::{ThemeInfo, ThemeOrigin, Themes};
use lokyr_kernel::themes::{self, ThemeVariant};
use lokyr_platform_core::{Appearance, Platform};

use super::App;
use crate::drawing::Backend;

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
}
