//! The themes that the app can show, read off the main thread: the list of
//! the themes' folder's, of the held ones and of the installed packages',
//! then their terminal colors, which the views' previews draw, a part at a
//! time, the main thread woken as each part comes.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::{Duration, Instant};

use lokyr_kernel::config;
use lokyr_kernel::plugins::host::{ThemeInfo, ThemeOrigin};
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
/// the folder of the themes that it holds, and the folders of themes of the
/// installed packages of themes, each of its package's name, in the order
/// of their names.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Places {
    pub(crate) folder: Option<PathBuf>,
    pub(crate) held: Option<PathBuf>,
    pub(crate) installed: Vec<(String, PathBuf)>,
}

impl Places {
    /// The theme of `name`, as [`themes::resolve`] finds it of these places
    /// and of `builtins`.
    pub(crate) fn resolve(
        &self,
        name: &str,
        builtins: &[Theme],
    ) -> Result<themes::Resolved, config::Error> {
        let installed: Vec<PathBuf> = self
            .installed
            .iter()
            .map(|(_, folder)| folder.clone())
            .collect();
        themes::resolve(
            name,
            self.folder.as_deref(),
            self.held.as_deref(),
            builtins,
            &installed,
        )
    }
}

/// A part of a read of the themes.
#[derive(Debug)]
pub(crate) enum Part {
    /// The list of the folder's themes, of the held ones and of the
    /// installed packages', the first of a name alone, as a name resolves.
    Listed(Vec<ThemeInfo>),
    /// The terminal colors of some of them, by their names.
    Colors(Vec<(String, TerminalColors)>),
    /// The read's end, every listed theme that resolves colored.
    Ended,
}

/// The themes as the last read listed them, and the read under way.
#[derive(Debug, Default)]
pub(crate) struct ThemeCatalog {
    /// The folder's, the held and the installed themes, as the last read
    /// listed them; none before a read did.
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
    let mut sources: Vec<(Option<&Path>, ThemeOrigin, Option<&str>)> = vec![
        (places.folder.as_deref(), ThemeOrigin::Folder, None),
        (places.held.as_deref(), ThemeOrigin::Held, None),
    ];
    sources.extend(places.installed.iter().map(|(package, folder)| {
        (
            Some(folder.as_path()),
            ThemeOrigin::Installed,
            Some(package.as_str()),
        )
    }));
    // A name of a built-in theme is the built-in one's, before any
    // package's, as it resolves.
    let built_in = |name: &str| [themes::LIGHT, themes::DARK].contains(&name);
    for (directory, origin, package) in sources {
        for mut theme in directory
            .map(|directory| super::pickers::themes_in(directory, origin))
            .unwrap_or_default()
        {
            let taken = listed.iter().any(|known| known.name == theme.name)
                || (origin == ThemeOrigin::Installed && built_in(&theme.name));
            if !taken {
                theme.package = package.map(str::to_owned);
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
