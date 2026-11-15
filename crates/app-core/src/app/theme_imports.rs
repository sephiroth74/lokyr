//! Themes imported, as View › Import Themes… and the Themes page's
//! Import… ask: the system's open panel, a sheet on the window that runs
//! the command, of `.itermcolors` and `.toml` files, several chosen at
//! once; each that holds themes copied into the themes' folder as it is,
//! never converted, one question for those of a name there already,
//! Replace or Skip; then one alert of the themes imported and of the files
//! that were not, as those that hold none.

use std::fs;
use std::path::PathBuf;
use std::time::Instant;

use lokyr_kernel::l10n::Arg;
use lokyr_kernel::themes;
use lokyr_platform_core::{Alert, AlertStyle, Platform};

use super::App;
use super::installs::{same_file, shown_path};
use crate::drawing::Backend;

/// A theme's file to import: its place, the place that it goes to, and
/// the names of its themes.
type Chosen = (PathBuf, PathBuf, Vec<String>);

/// The themes' files that the user chose to import, as a question asks of
/// those of a name in the folder already: each one's file, those of a name
/// there apart, and the files that hold no theme, each of why.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ThemeImport {
    fresh: Vec<Chosen>,
    taken: Vec<Chosen>,
    refused: Vec<String>,
}

/// The question's button that replaces the files there, the first.
const REPLACE: usize = 0;

impl<B: Backend> App<B> {
    /// Shows the open panel of the themes' files to import, on the window
    /// that the commands run in, or the one that has the focus; or tells
    /// why it cannot, as of no folder of themes. One shows at a time.
    pub(super) fn ask_import_themes(&mut self, platform: &mut dyn Platform) {
        if self.importing {
            return;
        }
        if self.theme_places().folder.is_none() {
            let why = self.texts.text("theme-install-no-folder");
            return self.tell_install_failed(platform, &why);
        }
        let Some(window) = self.acting.or_else(|| platform.focused_window()) else {
            return;
        };
        let message = self.texts.text("import-themes-panel");
        platform.choose_files(window, &themes::EXTENSIONS, &message);
        self.importing = true;
    }

    /// The files that the user chose, `paths`, none of a panel cancelled:
    /// each read as a theme's file, those that hold themes copied into the
    /// themes' folder, those of a name there asked of once, those that hold
    /// none told.
    pub(super) fn themes_chosen(&mut self, platform: &mut dyn Platform, paths: Vec<PathBuf>) {
        self.importing = false;
        let Some(folder) = self.theme_places().folder else {
            return;
        };
        let mut import = ThemeImport {
            fresh: Vec::new(),
            taken: Vec::new(),
            refused: Vec::new(),
        };
        for source in paths {
            let shown = source
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default();
            let names = match themes::themes_of(&source) {
                Ok(found) => found.into_iter().map(|(name, _)| name).collect(),
                Err(error) => {
                    import.refused.push(format!("{shown}: {}", error.message));
                    continue;
                }
            };
            let Some(name) = source.file_name() else {
                continue;
            };
            let target = folder.join(name);
            // A file of the folder itself, chosen again, is there already.
            if same_file(&source, &target) {
                continue;
            }
            if target.exists() {
                import.taken.push((source, target, names));
            } else {
                import.fresh.push((source, target, names));
            }
        }
        if import.taken.is_empty() {
            return self.import_themes(platform, &import, false);
        }
        let names: Vec<String> = import
            .taken
            .iter()
            .filter_map(|(_, target, _)| target.file_name())
            .map(|name| format!("“{}”", name.to_string_lossy()))
            .collect();
        let alert = Alert {
            style: AlertStyle::Informational,
            message: self.texts.text_with(
                "import-themes-taken-message",
                &[("count", Arg::Number(names.len() as i64))],
            ),
            detail: self.texts.text_with(
                "import-themes-taken-detail",
                &[
                    ("names", Arg::Text(&names.join(", "))),
                    ("folder", Arg::Text(&shown_path(&folder))),
                ],
            ),
            buttons: vec![
                self.texts.text("import-themes-replace"),
                self.texts.text("import-themes-skip"),
            ],
            suppression: None,
            input: None,
        };
        let window = self.acting.or_else(|| platform.focused_window());
        let id = platform.show_alert(window, &alert);
        self.import_asks.insert(id, import);
    }

    /// The answer to the question of `import`, of the files of a name in
    /// the folder: its `button`, Replace or Skip.
    pub(super) fn import_answered(
        &mut self,
        platform: &mut dyn Platform,
        import: &ThemeImport,
        button: usize,
    ) {
        self.import_themes(platform, import, button == REPLACE);
    }

    /// Copies the files of `import` into the themes' folder, those of a
    /// name there too where `replace` says so; lists the themes anew, and
    /// loads those in force, of a file replaced; then tells, in one alert,
    /// the themes imported and the files that were not, none of a file
    /// imported nor refused.
    fn import_themes(&mut self, platform: &mut dyn Platform, import: &ThemeImport, replace: bool) {
        let mut failed = import.refused.clone();
        let mut imported: Vec<&str> = Vec::new();
        let taken = if replace {
            import.taken.as_slice()
        } else {
            &[]
        };
        let mut copied = 0;
        for (source, target, names) in import.fresh.iter().chain(taken) {
            let done = target
                .parent()
                .map_or(Ok(()), fs::create_dir_all)
                .and_then(|()| fs::copy(source, target).map(|_| ()));
            match done {
                Ok(()) => {
                    copied += 1;
                    imported.extend(names.iter().map(String::as_str));
                }
                Err(error) => {
                    let name = source
                        .file_name()
                        .map(|name| name.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    failed.push(format!("{name}: {error}"));
                }
            }
        }
        if copied > 0 {
            self.load_themes(platform);
            let places = self.theme_places();
            self.theme_catalog
                .borrow_mut()
                .renew(places, self.waker.as_ref(), Instant::now());
        }
        if copied == 0 && failed.is_empty() {
            return;
        }
        let (style, message, mut detail) = if copied == 0 {
            let message = self.texts.text_with(
                "import-themes-failed",
                &[("count", Arg::Number(failed.len() as i64))],
            );
            (AlertStyle::Warning, message, String::new())
        } else {
            let message = self.texts.text_with(
                "import-themes-done",
                &[("count", Arg::Number(imported.len() as i64))],
            );
            let folder = self
                .theme_places()
                .folder
                .map(|folder| shown_path(&folder))
                .unwrap_or_default();
            let detail = self.texts.text_with(
                "import-themes-done-detail",
                &[
                    ("names", Arg::Text(&imported.join(", "))),
                    ("folder", Arg::Text(&folder)),
                ],
            );
            let style = if failed.is_empty() {
                AlertStyle::Informational
            } else {
                AlertStyle::Warning
            };
            (style, message, detail)
        };
        // The files not imported, each of why, under the themes imported.
        if !failed.is_empty() {
            if !detail.is_empty() {
                detail.push_str("\n\n");
                detail.push_str(&self.texts.text("import-themes-not-imported"));
                detail.push('\n');
            }
            detail.push_str(&failed.join("\n"));
        }
        let alert = Alert {
            style,
            message,
            detail,
            buttons: Vec::new(),
            suppression: None,
            input: None,
        };
        let window = self.acting.or_else(|| platform.focused_window());
        platform.show_alert(window, &alert);
    }
}
