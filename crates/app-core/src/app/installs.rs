//! The themes' files that the system opens with the app, as a double click
//! in the Finder on an `.itermcolors` file, or a theme's file dropped on
//! the app's icon: a question asks to install each into the themes' folder
//! beside the config file, and to use it; a theme of the appearance not in
//! force offers to switch the app to its own; a file that is no theme
//! tells why.

use std::fs;
use std::path::{Path, PathBuf};

use lokyr_kernel::config::{self, AppearanceMode};
use lokyr_kernel::l10n::Arg;
use lokyr_kernel::themes::{self, ThemeVariant};
use lokyr_platform_core::{Alert, AlertStyle, Appearance, Platform};

use super::App;
use crate::drawing::Backend;

/// A theme's file that the user is asked to install: where it is, where it
/// goes, its themes, each one's name and variant, and the appearance that
/// the question's checkbox switches the app to, of a theme of the one not
/// in force.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ThemeInstall {
    source: PathBuf,
    target: PathBuf,
    themes: Vec<(String, ThemeVariant)>,
    switch: Option<AppearanceMode>,
}

/// What the user answered: the question's buttons, from the first.
const USE: usize = 0;
const INSTALL: usize = 1;

impl<B: Backend> App<B> {
    /// The files that the system asks the app to open: the folders, as
    /// dropped on its icon, a window each; the scripts and the executables,
    /// as the default terminal opens them, each run in a window of its own;
    /// each other file asks to install it as a theme's, or tells why it
    /// cannot.
    pub(super) fn open_files(&mut self, platform: &mut dyn Platform, paths: Vec<PathBuf>) {
        let (folders, files): (Vec<PathBuf>, Vec<PathBuf>) =
            paths.into_iter().partition(|path| path.is_dir());
        if !folders.is_empty() {
            self.open_folders(platform, folders, false);
        }
        let (scripts, themes): (Vec<PathBuf>, Vec<PathBuf>) =
            files.into_iter().partition(|path| is_script(path));
        for path in themes {
            self.ask_install(platform, path);
        }
        if !scripts.is_empty() {
            self.run_scripts(platform, scripts);
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
        if target.exists() && !same_file(&source, &target) {
            detail.push(' ');
            detail.push_str(&self.texts.text("theme-install-replace"));
        }
        // A theme of one appearance, which is not the one in force: using
        // it changes nothing that shows, unless the app switches to its
        // appearance, as the question offers.
        let in_force = match platform.appearance() {
            Appearance::Light => ThemeVariant::Light,
            Appearance::Dark => ThemeVariant::Dark,
        };
        let other = match found.as_slice() {
            [(_, variant)] if *variant != in_force => Some(*variant),
            _ => None,
        };
        let mut suppression = None;
        let mut switch = None;
        if let Some(variant) = other {
            let (name, mode) = match variant {
                ThemeVariant::Light => ("light", AppearanceMode::Light),
                ThemeVariant::Dark => ("dark", AppearanceMode::Dark),
            };
            let args = [("variant", Arg::Text(name))];
            detail.push('\n');
            detail.push_str(&self.texts.text_with("theme-install-variant", &args));
            suppression = Some(self.texts.text_with("theme-install-switch", &args));
            switch = Some(mode);
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
            suppression,
            input: None,
        };
        let id = platform.show_alert(None, &alert);
        self.theme_asks.insert(
            id,
            ThemeInstall {
                source,
                target,
                themes: found,
                switch,
            },
        );
    }

    /// The answer to the question of `ask`: its `button`, from the first,
    /// and whether its checkbox, `switch`, was ticked. Installing copies the
    /// file into the themes' folder, its own place being already there;
    /// using it names its themes in `[theme]`, each for its variant's
    /// appearance, which the config's reading then loads; the checkbox
    /// switches the app to the theme's appearance, with either button.
    pub(super) fn theme_answered(
        &mut self,
        platform: &mut dyn Platform,
        ask: &ThemeInstall,
        (button, switch): (usize, bool),
    ) {
        if button != USE && button != INSTALL {
            return;
        }
        // A copy onto itself would empty it.
        if !same_file(&ask.source, &ask.target) {
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
        }
        if switch && let Some(mode) = ask.switch {
            self.choose_appearance(platform, mode);
        }
        if button == USE {
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

/// Whether `one` and `other` are the same file, as the file system tells,
/// whatever their paths, as of a folder that a link reaches: of their real
/// paths, or of the same device and inode, as of a hard link.
fn same_file(one: &Path, other: &Path) -> bool {
    if let (Ok(one), Ok(other)) = (fs::canonicalize(one), fs::canonicalize(other))
        && one == other
    {
        return true;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if let (Ok(one), Ok(other)) = (fs::metadata(one), fs::metadata(other)) {
            return (one.dev(), one.ino()) == (other.dev(), other.ino());
        }
    }
    false
}

/// `path` as a person reads it: of `~` for the home directory.
fn shown_path(path: &Path) -> String {
    match std::env::home_dir().and_then(|home| path.strip_prefix(home).ok().map(Path::to_path_buf))
    {
        Some(inner) => format!("~/{}", inner.display()),
        None => path.display().to_string(),
    }
}

/// Whether `path` is a file that the default terminal runs: a script of an
/// interpreter's extension, `.command`, `.sh`, `.py` and the others that
/// the system takes for scripts, whatever its case, one that names its
/// interpreter on its first line, or an executable; but a theme's file,
/// `.itermcolors` or `.toml`.
fn is_script(path: &Path) -> bool {
    let theme = path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            extension.eq_ignore_ascii_case("itermcolors") || extension.eq_ignore_ascii_case("toml")
        });
    path.is_file()
        && !theme
        && (super::opens::interpreter_of(path).is_some()
            || super::opens::is_executable(path)
            || !super::opens::first_line(path).is_empty())
}
