//! The themes that the app can show, read off the main thread: the list of
//! the themes' folder's and of the held ones, then their terminal colors,
//! which the views' previews draw, a part at a time, the main thread woken
//! as each part comes.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::{Duration, Instant};

use lokyr_kernel::plugins::host::{ThemeInfo, ThemeOrigin};
use lokyr_kernel::themes::{self, TerminalColors};
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

/// A part of a read of the themes.
#[derive(Debug)]
pub(crate) enum Part {
    /// The list of the folder's themes and of the held ones, the folder's
    /// first of a name of both.
    Listed(Vec<ThemeInfo>),
    /// The terminal colors of some of them, by their names.
    Colors(Vec<(String, TerminalColors)>),
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
    /// Starts a read of the themes of the folder `folder` and of `held`,
    /// whose parts wake the main thread of `waker`, unless one runs, or one
    /// started within [`AGAIN`] of `now`.
    pub(crate) fn refresh(
        &mut self,
        (folder, held): (Option<PathBuf>, Option<PathBuf>),
        waker: Option<&Waker>,
        now: Instant,
    ) {
        let recent = self.started.is_some_and(|at| now < at + AGAIN);
        let Some(waker) = waker.filter(|_| self.reading.is_none() && !recent) else {
            return;
        };
        let (parts, received) = mpsc::channel();
        let waker = waker.clone();
        let started = std::thread::Builder::new()
            .name("lokyr-themes".to_owned())
            .stack_size(STACK)
            .spawn(move || read((folder.as_deref(), held.as_deref()), &parts, &waker));
        if started.is_ok() {
            self.reading = Some(received);
            self.started = Some(now);
        }
    }

    /// The parts that the read under way sent since the last look; the
    /// read ends as its thread does.
    pub(crate) fn take(&mut self) -> Vec<Part> {
        let Some(reading) = &self.reading else {
            return Vec::new();
        };
        let mut parts = Vec::new();
        loop {
            match reading.try_recv() {
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

/// The read of the themes of `folder` and of `held`: their list, then
/// their colors, [`PART`] themes at a time, each part sent into `parts` and
/// the main thread woken of `waker`; it ends as the app no longer takes
/// them.
fn read((folder, held): (Option<&Path>, Option<&Path>), parts: &mpsc::Sender<Part>, waker: &Waker) {
    let mut listed: Vec<ThemeInfo> = Vec::new();
    let sources = [(folder, ThemeOrigin::Folder), (held, ThemeOrigin::Held)];
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
                let resolved = themes::resolve(&theme.name, folder, held, &builtins).ok()?;
                Some((theme.name.clone(), resolved.theme.terminal))
            })
            .collect();
        if parts.send(Part::Colors(colors)).is_err() {
            return;
        }
        waker.wake();
    }
}
