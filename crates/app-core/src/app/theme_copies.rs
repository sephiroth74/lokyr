//! A new theme made of an existing one, as View › New Theme from Current…
//! and the Themes page's Duplicate… ask, or of Lokyr's own values of an
//! appearance, as its New… asks: its name asked, the theme written whole
//! into the themes' folder, of Lokyr's format, a new one of its optional
//! keys as comments too, then named in `[theme]` for its variant and
//! opened in the editor, so that each save shows at once; or, as the Theme
//! Editor asks, its name told to the plugin, which shows it.
//!
//! And an `.itermcolors` theme of the folder converted to Lokyr's format,
//! as the Theme Editor asks, the user agreeing: a file of Lokyr's format
//! of its colors and its name, the `.itermcolors` file moved to the Trash.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Instant;

use lokyr_kernel::l10n::Arg;
use lokyr_kernel::themes::{self, ThemeVariant};
use lokyr_platform_core::{Alert, AlertStyle, Appearance, Platform, WindowId};
use lokyr_ui_core::theme as ui_theme;

use super::App;
use super::installs::shown_path;
use crate::drawing::Backend;

/// A new theme whose name the user is asked: the theme that it copies, by
/// its name, the themes' folder that it goes into, whether it is a new one
/// of Lokyr's own values, of every key, rather than a copy, the plugin and
/// its question that its name answers, of no editor opened, and the window
/// that the question shows on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct NewTheme {
    from: String,
    folder: PathBuf,
    fresh: bool,
    answer: Option<(String, u64)>,
    window: Option<WindowId>,
}

/// An `.itermcolors` theme of the folder whose conversion the user is
/// asked: its name, the themes' folder, the plugin and its question that
/// its name answers, and the window that the question shows on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ConvertTheme {
    name: String,
    folder: PathBuf,
    answer: (String, u64),
    window: Option<WindowId>,
}

/// How many names a first name of a new theme tries, of a number after it,
/// before it gives up the search of one free.
const MOST_TRIED: usize = 1000;

/// The question's button that makes the theme, the first.
const CREATE: usize = 0;

/// The question's button that converts the theme, the first.
const CONVERT: usize = 0;

impl<B: Backend> App<B> {
    /// Asks the name of a new theme of Lokyr's own values of the appearance
    /// `variant`, or else made of the theme `from`, or of the one in force
    /// for the appearance in force for none, of a first name that no theme
    /// has; or tells why none can be made: no folder of themes, or a theme
    /// that does not resolve. The new theme's name answers `answer`'s
    /// plugin's question where it is there, none answering it of no theme.
    pub(super) fn ask_new_theme(
        &mut self,
        platform: &mut dyn Platform,
        (from, variant): (Option<String>, Option<ThemeVariant>),
        answer: Option<(String, u64)>,
    ) {
        let from = match (variant, from) {
            (Some(ThemeVariant::Light), _) => themes::LIGHT.to_owned(),
            (Some(ThemeVariant::Dark), _) => themes::DARK.to_owned(),
            (None, Some(from)) => from,
            (None, None) => match platform.appearance() {
                Appearance::Light => self.config.theme.light.clone(),
                Appearance::Dark => self.config.theme.dark.clone(),
            },
        };
        let places = self.theme_places();
        let Some(folder) = places.folder.clone() else {
            self.unanswered(answer);
            let why = self.texts.text("theme-install-no-folder");
            return self.tell_theme_failed(platform, "new-theme-failed", why);
        };
        let builtins = [ui_theme::light_spec(), ui_theme::dark_spec()];
        if let Err(error) = places.resolve(&from, &builtins) {
            self.unanswered(answer);
            return self.tell_theme_failed(platform, "new-theme-failed", error.to_string());
        }
        let first = match variant {
            Some(variant) => self
                .texts
                .text_with("new-theme-fresh-name", &[("variant", variant_arg(variant))]),
            None => self
                .texts
                .text_with("new-theme-name", &[("name", Arg::Text(&from))]),
        };
        // The first of it and of a number after it that no theme has.
        let free = |name: &str| !taken(&folder, name) && places.resolve(name, &builtins).is_err();
        let name = std::iter::once(first.clone())
            .chain((2..MOST_TRIED).map(|number| format!("{first} {number}")))
            .find(|name| free(name))
            .unwrap_or(first);
        let ask = NewTheme {
            from,
            folder,
            fresh: variant.is_some(),
            answer,
            window: None,
        };
        self.show_new_theme_ask(platform, ask, &name, None);
    }

    /// Asks the name of the new theme of `ask`, of a field that holds
    /// `text`, telling first `why` the last name given cannot be one.
    fn show_new_theme_ask(
        &mut self,
        platform: &mut dyn Platform,
        mut ask: NewTheme,
        text: &str,
        why: Option<String>,
    ) {
        // Shown in the Theme Editor that asked, else opened in the editor.
        let detail = match ask.answer {
            Some(_) => "new-theme-detail-editor",
            None => "new-theme-detail",
        };
        let mut detail = self
            .texts
            .text_with(detail, &[("folder", Arg::Text(&shown_path(&ask.folder)))]);
        if let Some(why) = why {
            detail = format!("{why}\n\n{detail}");
        }
        let message = if ask.fresh {
            let variant = if ask.from == themes::LIGHT {
                ThemeVariant::Light
            } else {
                ThemeVariant::Dark
            };
            self.texts.text_with(
                "new-theme-fresh-message",
                &[("variant", variant_arg(variant))],
            )
        } else {
            self.texts
                .text_with("new-theme-message", &[("name", Arg::Text(&ask.from))])
        };
        let alert = Alert {
            style: AlertStyle::Informational,
            message,
            detail,
            buttons: vec![
                self.texts.text("new-theme-create"),
                self.texts.text("new-theme-cancel"),
            ],
            suppression: None,
            input: Some(text.to_owned()),
        };
        let window = self.acting.or_else(|| platform.focused_window());
        ask.window = window;
        let id = platform.show_alert(window, &alert);
        self.new_theme_asks.insert(id, ask);
    }

    /// The answer to the question of `ask`: its `button`, from the first,
    /// and the name given, `text`. A name that cannot name a theme, or that
    /// a theme has already, of a file of the folder, held, built in or
    /// installed, asks again, saying why; else the theme that it copies is
    /// written whole into the folder, named in `[theme]` for its variant,
    /// listed anew and opened in the editor, or its name told to the
    /// plugin that asked.
    pub(super) fn new_theme_answered(
        &mut self,
        platform: &mut dyn Platform,
        ask: NewTheme,
        button: usize,
        text: Option<&str>,
    ) {
        if button != CREATE {
            return self.unanswered(ask.answer);
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
            Err(error) => {
                self.unanswered(ask.answer);
                return self.tell_theme_failed(platform, "new-theme-failed", error.to_string());
            }
        };
        let path = ask.folder.join(format!("{name}.toml"));
        // A new one of every key, its optional ones as comments; a copy of
        // what its original gives, of its credits.
        let text = if ask.fresh {
            themes::new_theme_of(&resolved.theme, &name)
        } else {
            themes::copy_of(&resolved, &name)
        };
        // A file made new alone, never one written over.
        let written = fs::create_dir_all(&ask.folder).and_then(|()| {
            fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)?
                .write_all(text.as_bytes())
        });
        if let Err(error) = written {
            self.unanswered(ask.answer);
            let why = format!("{}: {error}", shown_path(&path));
            return self.tell_theme_failed(platform, "new-theme-failed", why);
        }
        self.set_theme(platform, resolved.theme.variant, &name);
        let places = self.theme_places();
        self.theme_catalog
            .borrow_mut()
            .renew(places, self.waker.as_ref(), Instant::now());
        if let Some((plugin, id)) = ask.answer {
            return self.plugins.answer(&plugin, id, Some(name));
        }
        let opened = platform
            .resolve_target(&path.display().to_string())
            .and_then(|target| self.open_resolved(platform, &target, Some((1, None))));
        if let Err(error) = opened {
            eprintln!("lokyr: the new theme's file did not open: {error}");
        }
    }

    /// Asks whether to convert the `.itermcolors` theme `name` of the
    /// themes' folder to Lokyr's format, as `answer`'s plugin asks, its
    /// name the answer once converted; or tells why it cannot be, none
    /// answering it: no folder of themes, or no such file.
    pub(super) fn ask_convert_theme(
        &mut self,
        platform: &mut dyn Platform,
        answer: (String, u64),
        name: String,
    ) {
        let Some(folder) = self.theme_places().folder.clone() else {
            self.unanswered(Some(answer));
            let why = self.texts.text("theme-install-no-folder");
            return self.tell_theme_failed(platform, "convert-theme-failed", why);
        };
        let file = folder.join(format!("{name}.itermcolors"));
        if let Err(error) = fs::symlink_metadata(&file) {
            self.unanswered(Some(answer));
            let why = format!("{}: {error}", shown_path(&file));
            return self.tell_theme_failed(platform, "convert-theme-failed", why);
        }
        let alert = Alert {
            style: AlertStyle::Informational,
            message: self
                .texts
                .text_with("convert-theme-message", &[("name", Arg::Text(&name))]),
            detail: self.texts.text_with(
                "convert-theme-detail",
                &[("folder", Arg::Text(&shown_path(&folder)))],
            ),
            buttons: vec![
                self.texts.text("convert-theme-convert"),
                self.texts.text("convert-theme-cancel"),
            ],
            suppression: None,
            input: None,
        };
        let window = self.acting.or_else(|| platform.focused_window());
        let id = platform.show_alert(window, &alert);
        let ask = ConvertTheme {
            name,
            folder,
            answer,
            window,
        };
        self.convert_asks.insert(id, ask);
    }

    /// The answer to the question of `ask`, its `button`, from the first:
    /// converted, the theme is written of Lokyr's format, of its colors and
    /// its name, its `.itermcolors` file moved to the Trash, the themes read
    /// anew and its name told to the plugin that asked; else none told, a
    /// failure in an alert too, the folder as it was.
    pub(super) fn convert_theme_answered(
        &mut self,
        platform: &mut dyn Platform,
        ask: ConvertTheme,
        button: usize,
    ) {
        if button != CONVERT {
            return self.unanswered(Some(ask.answer));
        }
        let places = self.theme_places();
        let builtins = [ui_theme::light_spec(), ui_theme::dark_spec()];
        let original = ask.folder.join(format!("{}.itermcolors", ask.name));
        let path = ask.folder.join(format!("{}.toml", ask.name));
        let converted = places
            .resolve(&ask.name, &builtins)
            .map_err(|error| error.to_string())
            .and_then(|resolved| {
                let text = themes::copy_of(&resolved, &ask.name);
                // A file made new alone, never one written over.
                fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&path)
                    .and_then(|mut file| file.write_all(text.as_bytes()))
                    .map_err(|error| format!("{}: {error}", shown_path(&path)))
            })
            .and_then(|()| {
                // Its `.itermcolors` file gone, or its new file too.
                platform.trash(&original).map_err(|error| {
                    let _ = fs::remove_file(&path);
                    error.to_string()
                })
            });
        if let Err(why) = converted {
            self.unanswered(Some(ask.answer));
            return self.tell_theme_failed(platform, "convert-theme-failed", why);
        }
        self.theme_catalog.borrow_mut().renew(
            self.theme_places(),
            self.waker.as_ref(),
            Instant::now(),
        );
        self.load_themes(platform);
        let (plugin, id) = ask.answer;
        self.plugins.answer(&plugin, id, Some(ask.name));
    }

    /// Forgets the questions of a new theme and of a conversion shown on
    /// `window`, which closes: the plugins that asked them answered none.
    pub(super) fn forget_theme_asks(&mut self, window: WindowId) {
        let new: Vec<_> = self
            .new_theme_asks
            .iter()
            .filter(|(_, ask)| ask.window == Some(window))
            .map(|(&alert, _)| alert)
            .collect();
        for alert in new {
            if let Some(ask) = self.new_theme_asks.remove(&alert) {
                self.unanswered(ask.answer);
            }
        }
        let converted: Vec<_> = self
            .convert_asks
            .iter()
            .filter(|(_, ask)| ask.window == Some(window))
            .map(|(&alert, _)| alert)
            .collect();
        for alert in converted {
            if let Some(ask) = self.convert_asks.remove(&alert) {
                self.unanswered(Some(ask.answer));
            }
        }
    }

    /// Answers none to `answer`'s plugin's question, if there is one.
    fn unanswered(&mut self, answer: Option<(String, u64)>) {
        if let Some((plugin, id)) = answer {
            self.plugins.answer(&plugin, id, None);
        }
    }

    /// Tells, in an alert of the window that the commands run in, or of the
    /// one that has the focus, of the message `message`, that the theme
    /// cannot be made or converted, and `why`, as the log does.
    fn tell_theme_failed(&mut self, platform: &mut dyn Platform, message: &str, why: String) {
        eprintln!("lokyr: {why}");
        let alert = Alert {
            style: AlertStyle::Warning,
            message: self.texts.text(message),
            detail: why,
            buttons: Vec::new(),
            suppression: None,
            input: None,
        };
        let window = self.acting.or_else(|| platform.focused_window());
        platform.show_alert(window, &alert);
    }
}

/// The argument of the appearance `variant` of a message, `light` or
/// `dark`, which its text selects of.
fn variant_arg(variant: ThemeVariant) -> Arg<'static> {
    Arg::Text(match variant {
        ThemeVariant::Light => "light",
        ThemeVariant::Dark => "dark",
    })
}

/// Whether a file of the folder `folder` names a theme `name`, a link that
/// leads nowhere too: of Lokyr's format or of iTerm2's.
fn taken(folder: &Path, name: &str) -> bool {
    themes::EXTENSIONS
        .iter()
        .any(|extension| fs::symlink_metadata(folder.join(format!("{name}.{extension}"))).is_ok())
}
