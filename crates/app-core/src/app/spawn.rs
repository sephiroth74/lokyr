//! The panes' programs: a window's, and those of the tabs and the splits
//! that plugins open, which start alike, the app's program in the
//! directory that the config gives, of the config's terminal.

use std::collections::BTreeMap;
use std::sync::Arc;

use lokyr_kernel::config::{self, ClipboardRead, Config, NewDirectory};
use lokyr_kernel::l10n::Texts;
use lokyr_kernel::plugins::Panes;
use lokyr_kernel::workspace::{Environment, Grid, Pane, PaneId, Settings};
use lokyr_platform_core::Waker;
use lokyr_pty::Program;
use lokyr_vt_core::page::{Budget, Scrollback};
use lokyr_vt_core::terminal::Colors;

use super::{FALLBACK_TERM, PaneWake};

/// What a new pane's terminal takes of `config`: its `TERM`, its widths and
/// normal form, and its scrollback's limit, within the `budget` that the
/// windows share. The terminal's own `TERM`, without its entry in
/// `environment`, as outside the bundle before `cargo xtask terminfo`,
/// gives way to `xterm-256color`, as the log tells, rather than leave the
/// programs a terminal that they cannot know.
pub(super) fn settings(
    config: &Config,
    environment: &Environment,
    budget: &Arc<Budget>,
) -> Settings {
    let terminal = &config.terminal;
    let mut term = terminal.term.as_str();
    if matches!(term, "xterm-lokyr" | "lokyr") && environment.terminfo.is_none() {
        eprintln!(
            "lokyr: `{term}` has no terminfo entry here, outside the bundle: the window takes `{FALLBACK_TERM}`; `cargo xtask terminfo` compiles the entry for `cargo run`"
        );
        term = FALLBACK_TERM;
    }
    Settings {
        term: term.to_owned(),
        widths: terminal.widths(),
        normalization: terminal.normalization.form(),
        scrollback: Scrollback {
            limit: config.scrollback.limit.bytes(),
            budget: Some(Arc::clone(budget)),
        },
    }
}

/// What a pane's program starts of: the program, where and of which
/// terminal it runs, the texts of a program that does not start, and
/// what wakes the app as its session tells something.
pub(super) struct Start<'a> {
    pub(super) program: &'a Program,
    pub(super) environment: &'a Environment,
    pub(super) settings: &'a Settings,
    pub(super) colors: &'a Colors,
    pub(super) texts: &'a Texts,
    pub(super) config: &'a Config,
    pub(super) waker: Waker,
    pub(super) wakes: &'a mut BTreeMap<PaneId, Arc<PaneWake>>,
}

impl Start<'_> {
    /// The pane of the id `id` and `grid`, whose session wakes the app as
    /// it tells something, of the config's scrolling and clipboard's reads.
    pub(super) fn pane(&mut self, id: PaneId, grid: Grid) -> Pane {
        let wake = Arc::new(PaneWake::default());
        let woken = Arc::clone(&wake);
        let waker = self.waker.clone();
        let pane = Pane::spawn_as(
            id,
            self.program,
            self.environment,
            self.settings,
            grid,
            self.colors,
            self.texts,
            move || {
                woken.wake();
                waker.wake();
            },
        );
        self.wakes.insert(id, wake);
        pane.set_scroll_on_output(self.config.scrollback.scroll_on_output);
        pane.set_clipboard_reads(self.config.clipboard.read == ClipboardRead::Ask);
        pane
    }
}

/// What starts the panes of the tabs and the splits that plugins open: the
/// app's program, in the directory that `new-directory` gives of the pane
/// beside, of that pane's size, which the window's layout then gives its
/// own.
pub(super) struct Spawner<'a> {
    pub(super) program: &'a Program,
    pub(super) environment: &'a Environment,
    pub(super) budget: &'a Arc<Budget>,
    pub(super) colors: &'a Colors,
    pub(super) texts: &'a Texts,
    pub(super) config: &'a Config,
    pub(super) waker: Waker,
    pub(super) wakes: &'a mut BTreeMap<PaneId, Arc<PaneWake>>,
}

/// The grid of a new pane beside no pane, which the window's layout then
/// gives its own.
const ALONE: Grid = Grid {
    columns: 80,
    rows: 24,
    cell_width: 8,
    cell_height: 16,
};

impl Panes for Spawner<'_> {
    fn spawn(&mut self, id: PaneId, near: Option<&Pane>) -> Pane {
        let directory = match &self.config.window.new_directory {
            NewDirectory::Home => None,
            // The directory that its shell told last, of this machine, as
            // through ssh its program's is the client's; its program's
            // otherwise.
            NewDirectory::Inherit => near.and_then(|pane| {
                pane.told_directory(&lokyr_pty::host_name())
                    .or_else(|| pane.working_directory())
            }),
            NewDirectory::Path(path) => config::expanded(path, std::env::home_dir().as_deref()),
        }
        .filter(|directory| directory.is_dir());
        let environment = Environment {
            directory,
            ..self.environment.clone()
        };
        let settings = settings(self.config, self.environment, self.budget);
        let mut start = Start {
            program: self.program,
            environment: &environment,
            settings: &settings,
            colors: self.colors,
            texts: self.texts,
            config: self.config,
            waker: self.waker.clone(),
            wakes: self.wakes,
        };
        start.pane(id, near.map_or(ALONE, Pane::grid))
    }
}
