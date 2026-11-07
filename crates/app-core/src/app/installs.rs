//! The themes' files that the system opens with the app, as a double click
//! in the Finder on an `.itermcolors` file, or a theme's file dropped on
//! the app's icon: a question asks to install each into the themes' folder
//! beside the config file, and to use it; a file that is no theme tells
//! why.

use std::fs;
use std::path::{Path, PathBuf};

use lokyr_kernel::config;
use lokyr_kernel::l10n::Arg;
use lokyr_kernel::themes::{self, ThemeVariant};
use lokyr_platform_core::{Alert, AlertStyle, Platform};

use super::App;
use crate::drawing::Backend;

/// A theme's file that the user is asked to install: where it is, where it
/// goes, and its themes, each one's name and variant.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ThemeInstall {
    source: PathBuf,
    target: PathBuf,
    themes: Vec<(String, ThemeVariant)>,
}

/// What the user answered: the question's buttons, from the first.
const USE: usize = 0;
const INSTALL: usize = 1;

impl<B: Backend> App<B> {
    /// The files that the system asks the app to open: each theme's file
    /// asks to install it; any other is told as no theme's.
    pub(super) fn open_files(&mut self, platform: &mut dyn Platform, paths: Vec<PathBuf>) {
        for path in paths {
            self.ask_install(platform, path);
        }
    }

    /// Asks to install the theme's file at `source` into the themes'
    /// folder, and to use it, saying so where a file of its name is there
    /// already; or tells why it cannot.
    fn ask_install(&mut self, platform: &mut dyn Platform, source: PathBuf) {
        let directory = self
            .config_file
            .as_ref()
            .and_then(|file| themes::directory(&file.path));
        let found = themes::themes_of(&source);
        let (directory, found) = match (directory, found) {
            (Some(directory), Ok(found)) => (directory, found),
            (_, Err(error)) => return self.tell_install_failed(platform, &error.to_string()),
            (None, Ok(_)) => {
                let why = self.texts.text("theme-install-no-folder");
                return self.tell_install_failed(platform, &why);
            }
        };
        let Some(file_name) = source.file_name() else {
            return;
        };
        let target = directory.join(file_name);
        let names: Vec<&str> = found.iter().map(|(name, _)| name.as_str()).collect();
        let name = names.join("”, “");
        let mut detail = self.texts.text_with(
            "theme-install-detail",
            &[("folder", Arg::Text(&shown_path(&directory)))],
        );
        if target.exists() && target != source {
            detail.push(' ');
            detail.push_str(&self.texts.text("theme-install-replace"));
        }
        let alert = Alert {
            style: AlertStyle::Informational,
            message: self
                .texts
                .text_with("theme-install-message", &[("name", Arg::Text(&name))]),
            detail,
            buttons: vec![
                self.texts.text("theme-install-use"),
                self.texts.text("theme-install-only"),
                self.texts.text("theme-install-cancel"),
            ],
            suppression: None,
            input: None,
        };
        let id = platform.show_alert(None, &alert);
        self.theme_asks.insert(
            id,
            ThemeInstall {
                source,
                target,
                themes: found,
            },
        );
    }

    /// The answer to the question of `ask`: its `button`, from the first.
    /// Installing copies the file into the themes' folder, its own place
    /// being already there; using it names its themes in `[theme]`, each
    /// for its variant's appearance, which the config's reading then loads.
    pub(super) fn theme_answered(
        &mut self,
        platform: &mut dyn Platform,
        ask: &ThemeInstall,
        button: usize,
    ) {
        if button != USE && button != INSTALL {
            return;
        }
        if ask.source != ask.target {
            let copied = ask
                .target
                .parent()
                .map_or(Ok(()), fs::create_dir_all)
                .and_then(|()| fs::copy(&ask.source, &ask.target).map(|_| ()));
            if let Err(error) = copied {
                let folder = ask.target.parent().map(shown_path).unwrap_or_default();
                let why = self.texts.text_with(
                    "theme-install-not-copied",
                    &[
                        ("folder", Arg::Text(&folder)),
                        ("error", Arg::Text(&error.to_string())),
                    ],
                );
                return self.tell_install_failed(platform, &why);
            }
        }
        if button == USE
            && let Some(file) = &self.config_file
        {
            let path = file.path.clone();
            for (name, variant) in &ask.themes {
                let key = match variant {
                    ThemeVariant::Light => "light",
                    ThemeVariant::Dark => "dark",
                };
                if let Err(error) = config::set_string(&path, "theme", key, name) {
                    return self.tell_install_failed(platform, &error.to_string());
                }
            }
            // The config read again, which loads the themes that it names.
            self.reload(platform);
        }
        // A theme in force of the file's name, replaced.
        self.load_themes(platform);
    }

    /// Tells, in an alert, why a theme's file cannot be installed: `why`.
    fn tell_install_failed(&mut self, platform: &mut dyn Platform, why: &str) {
        let alert = Alert {
            style: AlertStyle::Warning,
            message: self.texts.text("theme-install-failed"),
            detail: why.to_owned(),
            buttons: Vec::new(),
            suppression: None,
            input: None,
        };
        platform.show_alert(None, &alert);
    }
}

/// `path` as a person reads it: of `~` for the home directory.
fn shown_path(path: &Path) -> String {
    match std::env::home_dir().and_then(|home| path.strip_prefix(home).ok().map(Path::to_path_buf))
    {
        Some(inner) => format!("~/{}", inner.display()),
        None => path.display().to_string(),
    }
}
