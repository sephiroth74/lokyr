//! A new theme made of an existing one, as View › New Theme from Current…
//! and the Themes page's Duplicate… ask: its name asked, the theme as it
//! resolves written whole into the themes' folder, of Lokyr's format, then
//! named in `[theme]` for its variant and opened in the editor, so that
//! each save shows at once.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Instant;

use lokyr_kernel::l10n::Arg;
use lokyr_kernel::themes;
use lokyr_platform_core::{Alert, AlertStyle, Appearance, Platform};
use lokyr_ui_core::theme as ui_theme;

use super::App;
use super::installs::shown_path;
use crate::drawing::Backend;

/// A new theme whose name the user is asked: the theme that it copies, by
/// its name, and the themes' folder that it goes into.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct NewTheme {
    from: String,
    folder: PathBuf,
}

/// The question's button that makes the theme, the first.
const CREATE: usize = 0;

impl<B: Backend> App<B> {
    /// Asks the name of a new theme made of the theme `from`, or of the one
    /// in force for the appearance in force for none; or tells why none
    /// can be made: no folder of themes, or a theme that does not resolve.
    pub(super) fn ask_new_theme(&mut self, platform: &mut dyn Platform, from: Option<String>) {
        let from = from.unwrap_or_else(|| match platform.appearance() {
            Appearance::Light => self.config.theme.light.clone(),
            Appearance::Dark => self.config.theme.dark.clone(),
        });
        let places = self.theme_places();
        let Some(folder) = places.folder.clone() else {
            let why = self.texts.text("theme-install-no-folder");
            return self.tell_new_theme_failed(platform, why);
        };
        let builtins = [ui_theme::light_spec(), ui_theme::dark_spec()];
        if let Err(error) = places.resolve(&from, &builtins) {
            return self.tell_new_theme_failed(platform, error.to_string());
        }
        let name = self
            .texts
            .text_with("new-theme-name", &[("name", Arg::Text(&from))]);
        self.show_new_theme_ask(platform, NewTheme { from, folder }, &name, None);
    }

    /// Asks the name of the new theme of `ask`, of a field that holds
    /// `text`, telling first `why` the last name given cannot be one.
    fn show_new_theme_ask(
        &mut self,
        platform: &mut dyn Platform,
        ask: NewTheme,
        text: &str,
        why: Option<String>,
    ) {
        let mut detail = self.texts.text_with(
            "new-theme-detail",
            &[("folder", Arg::Text(&shown_path(&ask.folder)))],
        );
        if let Some(why) = why {
            detail = format!("{why}\n\n{detail}");
        }
        let alert = Alert {
            style: AlertStyle::Informational,
            message: self
                .texts
                .text_with("new-theme-message", &[("name", Arg::Text(&ask.from))]),
            detail,
            buttons: vec![
                self.texts.text("new-theme-create"),
                self.texts.text("new-theme-cancel"),
            ],
            suppression: None,
            input: Some(text.to_owned()),
        };
        let window = self.acting.or_else(|| platform.focused_window());
        let id = platform.show_alert(window, &alert);
        self.new_theme_asks.insert(id, ask);
    }

    /// The answer to the question of `ask`: its `button`, from the first,
    /// and the name given, `text`. A name that cannot name a theme, or that
    /// a theme has already, of a file of the folder, held, built in or
    /// installed, asks again, saying why; else the theme that it copies is
    /// written whole into the folder, named in `[theme]` for its variant,
    /// listed anew and opened in the editor.
    pub(super) fn new_theme_answered(
        &mut self,
        platform: &mut dyn Platform,
        ask: NewTheme,
        button: usize,
        text: Option<&str>,
    ) {
        if button != CREATE {
            return;
        }
        let name = text.unwrap_or_default().trim().to_owned();
        let places = self.theme_places();
        let builtins = [ui_theme::light_spec(), ui_theme::dark_spec()];
        let why = if !themes::is_name(&name) {
            Some("new-theme-not-a-name")
        } else if taken(&ask.folder, &name) || places.resolve(&name, &builtins).is_ok() {
            Some("new-theme-taken")
        } else {
            None
        };
        if let Some(why) = why {
            let why = self.texts.text_with(why, &[("name", Arg::Text(&name))]);
            return self.show_new_theme_ask(platform, ask, &name, Some(why));
        }
        let resolved = match places.resolve(&ask.from, &builtins) {
            Ok(resolved) => resolved,
            Err(error) => return self.tell_new_theme_failed(platform, error.to_string()),
        };
        let path = ask.folder.join(format!("{name}.toml"));
        // A file made new alone, never one written over.
        let written = fs::create_dir_all(&ask.folder).and_then(|()| {
            fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)?
                .write_all(themes::copy_of(&resolved, &name).as_bytes())
        });
        if let Err(error) = written {
            let why = format!("{}: {error}", shown_path(&path));
            return self.tell_new_theme_failed(platform, why);
        }
        self.set_theme(platform, resolved.theme.variant, &name);
        let places = self.theme_places();
        self.theme_catalog
            .borrow_mut()
            .renew(places, self.waker.as_ref(), Instant::now());
        let opened = platform
            .resolve_target(&path.display().to_string())
            .and_then(|target| self.open_resolved(platform, &target, Some((1, None))));
        if let Err(error) = opened {
            eprintln!("lokyr: the new theme's file did not open: {error}");
        }
    }

    /// Tells, in an alert of the window that the commands run in, or of the
    /// one that has the focus, that the new theme cannot be made, and
    /// `why`, as the log does.
    fn tell_new_theme_failed(&mut self, platform: &mut dyn Platform, why: String) {
        eprintln!("lokyr: {why}");
        let alert = Alert {
            style: AlertStyle::Warning,
            message: self.texts.text("new-theme-failed"),
            detail: why,
            buttons: Vec::new(),
            suppression: None,
            input: None,
        };
        let window = self.acting.or_else(|| platform.focused_window());
        platform.show_alert(window, &alert);
    }
}

/// Whether a file of the folder `folder` names a theme `name`, a link that
/// leads nowhere too: of Lokyr's format or of iTerm2's.
fn taken(folder: &Path, name: &str) -> bool {
    themes::EXTENSIONS
        .iter()
        .any(|extension| fs::symlink_metadata(folder.join(format!("{name}.{extension}"))).is_ok())
}
