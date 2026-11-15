//! The themes that the app can show, read off the main thread: the list of
//! the themes' folder's and of the held ones, then their terminal colors, which the views' previews draw, a part at a
//! time, the main thread woken as each part comes.

use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::{Duration, Instant};

use lokyr_kernel::config;
use lokyr_kernel::plugins::host::{ThemeInfo, ThemeOrigin};
use lokyr_kernel::themes::rules::PALETTE;
use lokyr_kernel::themes::{self, TerminalColors, Theme};
use lokyr_platform_core::Waker;
use lokyr_ui_core::theme as ui_theme;

/// How long after a read of the list a read of it starts again, as a
/// plugin asks for the list, so that a theme added to the folder shows.
const AGAIN: Duration = Duration::from_secs(2);

/// How many themes' colors a part of a read holds.
const PART: usize = 24;

/// The stack of the reading thread, as the main thread's: a theme's file
/// parses within its bounds, of any nesting that they let through.
const STACK: usize = 8 * 1024 * 1024;

/// Where the app finds themes: the themes' folder beside the config file,
/// and the folder of the themes that it holds.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Places {
    pub(crate) folder: Option<PathBuf>,
    pub(crate) held: Option<PathBuf>,
}

impl Places {
    /// The theme of `name`, as [`themes::resolve`] finds it of these places
    /// and of `builtins`.
    pub(crate) fn resolve(
        &self,
        name: &str,
        builtins: &[Theme],
    ) -> Result<themes::Resolved, config::Error> {
        themes::resolve(name, self.folder.as_deref(), self.held.as_deref(), builtins)
    }
}

/// Every token of the theme of `name`, or of the one that `[theme]` of
/// `config` names for `appearance` of none, of `places` and the built-in
/// themes, as it resolves: each of its color and its origin, its file's
/// line, a theme of no file's, or its rule's; Lokyr's in their rules'
/// order, then a plugin's; none of a name of no theme.
pub(crate) fn theme_tokens(
    places: &Places,
    name: Option<&str>,
    (appearance, config): (lokyr_platform_core::Appearance, &config::ThemeConfig),
) -> Option<lokyr_kernel::plugins::host::ThemeTokens> {
    use lokyr_kernel::plugins::host::{ThemeToken, ThemeTokens, TokenOrigin};
    use lokyr_kernel::themes::rules;
    let name = name.unwrap_or(match appearance {
        lokyr_platform_core::Appearance::Light => config.light.as_str(),
        lokyr_platform_core::Appearance::Dark => config.dark.as_str(),
    });
    let builtins = [ui_theme::light_spec(), ui_theme::dark_spec()];
    let resolved = places.resolve(name, &builtins).ok()?;
    let theme = &resolved.theme;
    let colors = rules::resolve(&theme.terminal, &theme.tokens);
    let dark = rules::is_dark(&theme.terminal);
    let byte = |value: f32| (value.clamp(0.0, 1.0) * 255.0).round() as u8;
    let names: Vec<String> = rules::tokens()
        .map(str::to_owned)
        .chain(
            colors
                .keys()
                .filter(|token| rules::rule(token).is_none())
                .cloned(),
        )
        .collect();
    let origin_of = |source: &themes::Source| match source {
        themes::Source::File(path, position) => TokenOrigin::File {
            path: path.display().to_string(),
            line: position.map_or(0, |at| u32::try_from(at.line).unwrap_or(u32::MAX)),
            theme: path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or_default()
                .to_owned(),
        },
        themes::Source::Theme(theme) => TokenOrigin::Theme {
            theme: theme.clone(),
        },
    };
    let color_of = |color: &rules::Rgba| themes::ThemeColor {
        red: byte(color.red),
        green: byte(color.green),
        blue: byte(color.blue),
        alpha: byte(color.alpha),
    };
    let tokens = names
        .into_iter()
        .filter_map(|token| {
            let color = colors.get(&token)?;
            let (origin, value) = match resolved.sources.get(&token) {
                Some(source) => (
                    origin_of(source),
                    theme.tokens.get(&token).map(ToString::to_string),
                ),
                None => (
                    TokenOrigin::Rule {
                        text: rules::rule(&token)?.text(dark).to_owned(),
                    },
                    None,
                ),
            };
            Some(ThemeToken {
                name: token,
                color: color_of(color),
                origin,
                value,
            })
        })
        .collect();
    // The terminal's colors, each of its key's source, or of the default
    // that it takes where the theme leaves it out.
    let terminal = &theme.terminal;
    let (fg, bg) = (terminal.foreground, terminal.background);
    let selection = colors.get("field.selection").map_or(fg, color_of);
    let keys: [(&str, Option<themes::ThemeColor>, (themes::ThemeColor, &str)); 9] = [
        ("background", Some(bg), (bg, "")),
        ("foreground", Some(fg), (fg, "")),
        ("bold", terminal.bold, (fg, "its own color")),
        ("link", terminal.link, (terminal.palette[4], "blue")),
        (
            "selection",
            terminal.selection,
            (selection, "field.selection"),
        ),
        (
            "selection-text",
            terminal.selection_text,
            (fg, "its own color"),
        ),
        ("cursor", terminal.cursor, (fg, "foreground")),
        ("cursor-text", terminal.cursor_text, (bg, "background")),
        ("underline", terminal.underline, (fg, "foreground")),
    ];
    let source = |key: &str| {
        resolved.terminal_sources.get(key).map_or_else(
            || TokenOrigin::Theme {
                theme: name.to_owned(),
            },
            origin_of,
        )
    };
    let mut terminal_colors: Vec<ThemeToken> = keys
        .into_iter()
        .map(|(key, given, (default, rule))| match given {
            Some(color) => ThemeToken {
                name: key.to_owned(),
                color,
                origin: source(key),
                value: Some(color.to_string()),
            },
            None => ThemeToken {
                name: key.to_owned(),
                color: default,
                origin: TokenOrigin::Rule {
                    text: rule.to_owned(),
                },
                value: None,
            },
        })
        .collect();
    terminal_colors.extend(PALETTE.iter().enumerate().map(|(at, palette)| ThemeToken {
        name: (*palette).to_owned(),
        color: terminal.palette[at],
        origin: source(&format!("palette.{at}")),
        value: Some(terminal.palette[at].to_string()),
    }));
    Some(ThemeTokens {
        theme: name.to_owned(),
        variant: theme.variant,
        terminal: terminal_colors,
        tokens,
    })
}

/// Whether the color `name` of the theme `theme` of `places` can be set to
/// `value`, or taken away of none, in its file: a theme of a file of
/// Lokyr's format of the themes' folder; a token's value of no problem
/// that the theme had not, as a name of no color or a cycle, as the rules
/// resolve it; or why not. Whether the value reads, the request's check
/// tells.
pub(crate) fn theme_value(
    places: &Places,
    theme: &str,
    name: &str,
    value: Option<&str>,
) -> Result<(), String> {
    use lokyr_kernel::themes::rules;
    let file = places
        .folder
        .as_deref()
        .map(|folder| folder.join(format!("{theme}.toml")));
    if !file.is_some_and(|file| file.is_file()) {
        return Err(format!(
            "`{theme}` has no file of Lokyr's format in the themes folder"
        ));
    }
    let Some(value) = value else {
        return Ok(());
    };
    if lokyr_kernel::themes::is_terminal_color(name) {
        return Ok(());
    }
    let builtins = [ui_theme::light_spec(), ui_theme::dark_spec()];
    let resolved = places
        .resolve(theme, &builtins)
        .map_err(|error| error.to_string())?;
    let given = value
        .parse()
        .map_err(|error| format!("`{name}` is left out, as {error}"))?;
    let before = rules::problems(&resolved.theme.terminal, &resolved.theme.tokens);
    let mut tokens = resolved.theme.tokens.clone();
    tokens.insert(name.to_owned(), given);
    let after = rules::problems(&resolved.theme.terminal, &tokens);
    match after.into_iter().find(|problem| !before.contains(problem)) {
        Some((token, why)) if token == name => Err(why.to_string()),
        Some((token, why)) => Err(format!("`{token}`: {why}")),
        None => Ok(()),
    }
}

/// A part of a read of the themes.
#[derive(Debug)]
pub(crate) enum Part {
    /// The list of the folder's themes and of the held ones, the folder's
    /// first of a name of both.
    Listed(Vec<ThemeInfo>),
    /// The terminal colors of some of them, by their names.
    Colors(Vec<(String, TerminalColors)>),
    /// The read's end, every listed theme that resolves colored.
    Ended,
}

/// The themes as the last read listed them, and the read under way.
#[derive(Debug, Default)]
pub(crate) struct ThemeCatalog {
    /// The folder's and the held themes, as the last read listed them;
    /// none before a read did.
    pub(crate) listed: Option<Vec<ThemeInfo>>,
    /// The parts of the read under way.
    reading: Option<Receiver<Part>>,
    /// When the last read started.
    started: Option<Instant>,
}

impl ThemeCatalog {
    /// Starts a read of the themes of `places`, whose parts wake the main
    /// thread of `waker`, unless one runs, or one started within [`AGAIN`]
    /// of `now`.
    pub(crate) fn refresh(&mut self, places: Places, waker: Option<&Waker>, now: Instant) {
        let recent = self.started.is_some_and(|at| now < at + AGAIN);
        if self.reading.is_some() || recent {
            return;
        }
        self.start(places, waker, now);
    }

    /// Starts a read of the themes of `places` at once, as they changed,
    /// in place of one under way, which ends as its parts go untaken;
    /// none where no read listed them yet, as the first that a plugin asks
    /// for reads them of the places then.
    pub(crate) fn renew(&mut self, places: Places, waker: Option<&Waker>, now: Instant) {
        if self.listed.is_none() && self.reading.is_none() {
            return;
        }
        self.reading = None;
        self.start(places, waker, now);
    }

    /// Starts a read of the themes of `places`, whose parts wake the main
    /// thread of `waker`.
    fn start(&mut self, places: Places, waker: Option<&Waker>, now: Instant) {
        let Some(waker) = waker.cloned() else {
            return;
        };
        let (parts, received) = mpsc::channel();
        let started = std::thread::Builder::new()
            .name("lokyr-themes".to_owned())
            .stack_size(STACK)
            .spawn(move || read(&places, &parts, &waker));
        if started.is_ok() {
            self.reading = Some(received);
            self.started = Some(now);
        }
    }

    /// Whether a read listed the themes and colored them, and none runs:
    /// a name that it did not color is no theme's, until the list changes.
    pub(crate) fn finished(&self) -> bool {
        self.listed.is_some() && self.reading.is_none()
    }

    /// The parts that the read under way sent since the last look; the
    /// read ends with its last, or as its thread does.
    pub(crate) fn take(&mut self) -> Vec<Part> {
        let Some(reading) = &self.reading else {
            return Vec::new();
        };
        let mut parts = Vec::new();
        loop {
            match reading.try_recv() {
                Ok(Part::Ended) => {
                    parts.push(Part::Ended);
                    self.reading = None;
                    break;
                }
                Ok(part) => parts.push(part),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.reading = None;
                    break;
                }
            }
        }
        parts
    }
}

/// The read of the themes of `places`: their list, then their colors,
/// [`PART`] themes at a time, each part sent into `parts` and the main
/// thread woken of `waker`; it ends as the app no longer takes them.
fn read(places: &Places, parts: &mpsc::Sender<Part>, waker: &Waker) {
    let mut listed: Vec<ThemeInfo> = Vec::new();
    let sources = [
        (places.folder.as_deref(), ThemeOrigin::Folder),
        (places.held.as_deref(), ThemeOrigin::Held),
    ];
    for (directory, origin) in sources {
        for theme in directory
            .map(|directory| super::pickers::themes_in(directory, origin))
            .unwrap_or_default()
        {
            if !listed.iter().any(|known| known.name == theme.name) {
                listed.push(theme);
            }
        }
    }
    if parts.send(Part::Listed(listed.clone())).is_err() {
        return;
    }
    waker.wake();
    let builtins = [ui_theme::light_spec(), ui_theme::dark_spec()];
    for chunk in listed.chunks(PART) {
        let colors = chunk
            .iter()
            .filter_map(|theme| {
                let resolved = places.resolve(&theme.name, &builtins).ok()?;
                Some((theme.name.clone(), resolved.theme.terminal))
            })
            .collect();
        if parts.send(Part::Colors(colors)).is_err() {
            return;
        }
        waker.wake();
    }
    if parts.send(Part::Ended).is_ok() {
        waker.wake();
    }
}
