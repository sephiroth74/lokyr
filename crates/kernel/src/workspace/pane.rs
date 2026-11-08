//! A pane: the program that runs in it, in a session, the screen that its
//! frames draw, and what its window does once the program ends.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use lokyr_pty::{Exit, Program, Size};
use lokyr_session::{Options, Reader, Session};
use lokyr_vt_core::input::{KeyEvent, KeyOptions, MouseEvent};
use lokyr_vt_core::page::Style;
use lokyr_vt_core::page::{Rgb, Scrollback};
use lokyr_vt_core::parser::Parser;
use lokyr_vt_core::terminal::{
    self, ClipboardRequest, Colors, InputModes, KeyboardFlags, Look, Origin, Scroll, Selected,
    SelectionShape, SelectionUnit, Snapshot, Terminal, WORD_SEPARATORS,
};
use lokyr_vt_core::unicode::{Normalization, Tables, Widths};

use lokyr_plugin_sdk::host::{ShellCommand, Viewport};

use super::PaneId;
use crate::config::CloseOnExit;
use crate::l10n::{Arg, Texts};

/// How long a pane waits for the status of a program whose output ended,
/// before it tells that the program ended, without its status.
const EXIT_WAIT: Duration = Duration::from_secs(1);

/// How often a pane tells its program a new size at most, as Ghostty
/// coalesces a resize's steps: the program, which redraws on each
/// SIGWINCH, learns of the steps up to 40 times a second, and of the last
/// one.
pub const RESIZE_PACE: Duration = Duration::from_millis(25);

/// How long a pane's screen waits, as its program learns of a new size, for
/// the program's output, its drawing at that size, before it shows the
/// terminal: shown at once, the screen would show the prompt that the
/// resize cleared, until the shell's drawing came, a frame later, so that
/// the prompt flickered through a resize. A program that draws nothing at
/// a new size has its screen shown this late.
pub const ANSWER_WAIT: Duration = Duration::from_millis(50);

/// The shortest time between two looks at what a pane's session told, its
/// bells and its terminal's events: a frame's at 60 Hz.
pub const TOLD_PACE: Duration = Duration::from_micros(16_667);

/// How long a pane's program is taken to print after its last output of its
/// own, which no input asked for: a few seconds, so that a program that
/// prints now and then, as a build between its steps, keeps printing.
pub const PRINTING: Duration = Duration::from_secs(3);

/// A pane's grid: its columns and rows, and the pixels of its cell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Grid {
    /// Its columns.
    pub columns: u16,
    /// Its rows.
    pub rows: u16,
    /// A cell's width, in pixels.
    pub cell_width: u16,
    /// A cell's height, in pixels.
    pub cell_height: u16,
}

impl Grid {
    /// The size that the PTY tells the program.
    fn size(self) -> Size {
        Size {
            cols: self.columns,
            rows: self.rows,
            width: self.columns.saturating_mul(self.cell_width),
            height: self.rows.saturating_mul(self.cell_height),
        }
    }
}

/// What the window does once its program has ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Exited {
    /// It closes.
    Close,
    /// It keeps its screen, with a line that tells how the program ended,
    /// until a key closes it.
    Keep,
}

/// The environment of the programs that panes start, past what the PTY
/// sets.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Environment {
    /// The app's resources, as its bundle has them, if it runs from one.
    pub resources: Option<PathBuf>,
    /// The locale that `LANG` takes when the environment sets none.
    pub locale: String,
    /// The directory of the terminal's own terminfo entry, if the app has
    /// one, which `TERMINFO` names for a `TERM` of it.
    pub terminfo: Option<PathBuf>,
    /// The directory of the app's command, as its bundle has it, if it runs
    /// from one, which the programs' `PATH` takes at its end.
    pub binaries: Option<PathBuf>,
    /// The directory that the program starts in; the home directory
    /// without one.
    pub directory: Option<PathBuf>,
    /// The app's version, which the programs find in
    /// `TERM_PROGRAM_VERSION` and which XTVERSION reports: this crate's by
    /// default.
    pub version: String,
    /// The directory of the shells' integration, which the user's shell
    /// loads, if the config has it.
    pub shell_integration: Option<PathBuf>,
}

impl Default for Environment {
    fn default() -> Self {
        Self {
            resources: None,
            locale: "en_US.UTF-8".to_owned(),
            terminfo: None,
            binaries: None,
            directory: None,
            version: env!("CARGO_PKG_VERSION").to_owned(),
            shell_integration: None,
        }
    }
}

/// What a pane's terminal is made with, as the config has it.
#[derive(Clone, Debug)]
pub struct Settings {
    /// The `TERM` that its programs see: `xterm-256color` by default, which
    /// remote hosts know too.
    pub term: String,
    /// How it counts the cells of text.
    pub widths: Widths,
    /// The normal form that printed text takes.
    pub normalization: Normalization,
    /// The limits of its scrollback, the budget that the panes share
    /// among them.
    pub scrollback: Scrollback,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            term: "xterm-256color".to_owned(),
            widths: Widths::default(),
            normalization: Normalization::default(),
            scrollback: Scrollback::default(),
        }
    }
}

/// The program of a pane, in its session, and the screen that its frames
/// draw.
pub struct Pane {
    id: PaneId,
    /// The session, unless the program could not start.
    session: Option<Session>,
    /// The screen, as the last frame copied it.
    snapshot: Snapshot,
    /// The grid that the session's terminal has.
    grid: Grid,
    /// The grid that the program was told last, through the PTY, and when,
    /// unless it was told only as it started.
    told: (Grid, Option<Instant>),
    /// The grid that a resize gave last and that the program cannot be told
    /// yet, within [`RESIZE_PACE`] of the last signal, of a live resize's
    /// steps, its end and the others alike, which the terminal takes with
    /// the program's signal; `None` once it took it.
    held: Option<Grid>,
    /// The slices of output that the session had parsed as the program last
    /// learnt of a new size, and when the screen stops waiting for more: the
    /// screen shows the terminal once a slice comes, or then, which is
    /// [`ANSWER_WAIT`] after the first of the sizes that the program has
    /// not answered; `None` once it does.
    answer: Option<(u64, Instant)>,
    /// Whether the terminal's grid changed since the program was told,
    /// though it may be back at the grid told: a resize there and back
    /// changes the screen, as it clears the prompt and cuts the alternate
    /// screen, which the program must draw again.
    changed: bool,
    /// When the program's output ended, while the pane waits for its status.
    ended: Option<Instant>,
    /// Whether the pane keeps the screen of a program that ended, or that
    /// could not start.
    kept: bool,
    /// Whether the program is the login shell, which `login` starts on
    /// macOS, hiding its status, which the shell's wrapper tells through
    /// the terminal instead.
    through_login: bool,
    /// How many times the bell had rung when the workspace last looked.
    bells: u64,
    /// When the workspace last took what the session told.
    told_at: Option<Instant>,
    /// Whether the app changed the selection since the last frame copied
    /// the screen, which the contexts read from the terminal until then.
    selection_touched: AtomicBool,
    /// Whether the workspace told that the program's output ended.
    ended_told: bool,
    /// The title that its program set last, which its tab takes while it
    /// has the tab's keyboard.
    pub(super) title: String,
    /// The tab's color that its program set last, if it set one.
    pub(super) tab_color: Option<Rgb>,
    /// The progress that its program tells last, OSC 9;4's, if it tells
    /// one.
    pub(super) progress: Option<terminal::Progress>,
    /// Whether its program prints of its own, as the workspace last found
    /// it: output that no input asked for came within [`PRINTING`].
    printing: bool,
    /// The title that its header showed as the workspace last looked, of
    /// [`Pane::header_title`], whose change tells `pane-changed`.
    pub(super) header: Option<String>,
    /// How many slices of output the session had parsed when the
    /// workspace last looked.
    slices: u64,
    /// The view that a frame showed as it was last told, whose change
    /// tells `viewport-changed`.
    told_viewport: Option<Viewport>,
    /// The command that started last, of its id, and when the workspace's
    /// look found its start, until its end.
    pub(super) command_started: Option<(u64, Instant)>,
}

impl std::fmt::Debug for Pane {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Pane")
            .field("id", &self.id)
            .field("running", &self.session.is_some())
            .field("grid", &self.grid)
            .field("told", &self.told)
            .field("ended", &self.ended)
            .field("kept", &self.kept)
            .finish_non_exhaustive()
    }
}

/// A view of the first row `origin`, of `rows` rows, `offset` rows above
/// the screen, over a scrollback of `scrollback` rows, of the alternate
/// screen or not.
fn viewport(
    origin: Origin,
    rows: u16,
    offset: usize,
    scrollback: usize,
    alternate: bool,
) -> Viewport {
    // Fewer rows than an `i64` holds.
    let screen = origin.top.saturating_add(offset as i64);
    Viewport {
        top: origin.top,
        rows,
        screen,
        oldest: screen.saturating_sub(scrollback as i64),
        epoch: origin.epoch,
        alternate,
    }
}

/// The PTY's options for `program`, in `environment`, with `term`, at the
/// size of `grid`.
fn pty_options(
    program: &Program,
    environment: &Environment,
    term: &str,
    grid: Grid,
) -> lokyr_pty::Options {
    let mut pty = lokyr_pty::Options::new(program.clone(), term);
    pty.size = grid.size();
    pty.resources_dir.clone_from(&environment.resources);
    pty.terminfo_dir.clone_from(&environment.terminfo);
    pty.binaries_dir.clone_from(&environment.binaries);
    pty.working_directory.clone_from(&environment.directory);
    pty.fallback_locale.clone_from(&environment.locale);
    pty.version.clone_from(&environment.version);
    pty.shell_integration
        .clone_from(&environment.shell_integration);
    pty
}

/// What a pane's session told since the workspace last looked.
pub(super) struct Happened {
    /// The terminal's events.
    pub(super) events: Vec<terminal::Event>,
    /// The terminal's title, when it changed.
    pub(super) title: Option<String>,
    /// Whether the bell rang.
    pub(super) bell: bool,
    /// Whether the program's output ended, told once.
    pub(super) ended: bool,
    /// Whether output came.
    pub(super) output: bool,
}

impl Pane {
    /// A pane that runs `program` in a terminal of `settings` and `grid`,
    /// in `colors`, whose changes call `wake`, from the session's threads;
    /// or, when the program cannot start, a pane whose screen tells why, in
    /// `texts`.
    pub fn spawn(
        program: &Program,
        environment: &Environment,
        settings: &Settings,
        grid: Grid,
        colors: &Colors,
        texts: &Texts,
        wake: impl Fn() + Send + Sync + 'static,
    ) -> Self {
        let id = PaneId::new();
        Self::spawn_as(
            id,
            program,
            environment,
            settings,
            grid,
            colors,
            texts,
            wake,
        )
    }

    /// A pane of the id `id`, which nothing of the process took, as the
    /// host gives a plugin's new pane as it asks, as [`Pane::spawn`] makes
    /// one.
    #[allow(clippy::too_many_arguments)]
    pub fn spawn_as(
        id: PaneId,
        program: &Program,
        environment: &Environment,
        settings: &Settings,
        grid: Grid,
        colors: &Colors,
        texts: &Texts,
        wake: impl Fn() + Send + Sync + 'static,
    ) -> Self {
        let pty = pty_options(program, environment, &settings.term, grid);
        let mut pane = Self {
            id,
            session: None,
            snapshot: Snapshot::default(),
            grid,
            told: (grid, None),
            held: None,
            answer: None,
            changed: false,
            ended: None,
            kept: false,
            through_login: through_login(program),
            bells: 0,
            ended_told: false,
            told_at: None,
            printing: false,
            header: None,
            selection_touched: AtomicBool::new(false),
            title: String::new(),
            tab_color: None,
            progress: None,
            slices: 0,
            told_viewport: None,
            command_started: None,
        };
        let mut options = Options::new(pty);
        // The wheel scrolls programs of the alternate screen, as `less`, by
        // the cursor keys, unless they say otherwise.
        options.terminal.alternate_scroll = true;
        settings.term.clone_into(&mut options.terminal.term);
        environment
            .version
            .clone_into(&mut options.terminal.version);
        options.terminal.widths = settings.widths;
        options.terminal.normalization = settings.normalization;
        options.terminal.scrollback = settings.scrollback.clone();
        match Session::spawn(&options, wake) {
            Ok(session) => {
                {
                    let mut terminal = session.lock();
                    terminal.set_default_colors(colors.clone());
                    terminal.set_cell_size(grid.cell_width, grid.cell_height);
                }
                pane.session = Some(session);
            }
            Err(error) => {
                eprintln!("lokyr: the program cannot start: {error}");
                let mut terminal =
                    Terminal::new(grid.columns, grid.rows, terminal::Options::default());
                terminal.set_default_colors(colors.clone());
                let error = error.to_string();
                let why = texts.text_with("program-cannot-start", &[("error", Arg::Text(&error))]);
                let line = format!("\x1b[2m[{why}]\x1b[0m");
                Parser::new().advance(&mut terminal, line.as_bytes());
                terminal.snapshot(&mut pane.snapshot);
                // A key closes the window, as after a program that ended.
                pane.kept = true;
            }
        }
        pane
    }

    /// The pane's id.
    pub fn id(&self) -> PaneId {
        self.id
    }

    /// When the workspace can take what the session told next: a pace
    /// after the last time, so that a stream of bells or of events wakes
    /// the main thread that often at most; `None` before the first.
    pub(super) fn told_due(&self) -> Option<Instant> {
        self.told_at.map(|at| at + TOLD_PACE)
    }

    /// Whether its program prints of its own, as the workspace last found
    /// it: output that no input asked for, nor a new size, came within
    /// [`PRINTING`].
    pub fn printing(&self) -> bool {
        self.printing
    }

    /// When its program, found printing, is quiet unless it prints again:
    /// [`PRINTING`] after its last output of its own.
    pub(super) fn printing_due(&self) -> Option<Instant> {
        if !self.printing {
            return None;
        }
        Some(self.session.as_ref()?.printed_at()? + PRINTING)
    }

    /// Finds whether its program prints at `now`, of output of its own
    /// within [`PRINTING`]; whether that changed. Its session tells the
    /// main thread of the next output of its own of a program found quiet.
    pub(super) fn look_at_printing(&mut self, now: Instant) -> bool {
        let Some(session) = &self.session else {
            return false;
        };
        let printing =
            |session: &Session| session.printed_at().is_some_and(|at| now < at + PRINTING);
        // Cleared before the second look, so that output that came between
        // the two tells the main thread.
        let printing = printing(session) || {
            session.clear_printed();
            printing(session)
        };
        std::mem::replace(&mut self.printing, printing) != printing
    }

    /// What the session told since the last look, at `now`; `None` for a
    /// pane whose program could not start.
    pub(super) fn happened(&mut self, now: Instant) -> Option<Happened> {
        let session = self.session.as_ref()?;
        self.told_at = Some(now);
        let (events, title, bells) = {
            let mut terminal = session.lock();
            // What the terminal tells from now on wakes the main thread
            // again, whether the pane is drawn or not.
            session.clear_told();
            let events = terminal.take_events();
            let titled = events.contains(&terminal::Event::TitleChanged);
            let title = titled.then(|| terminal.title().to_owned());
            (events, title, terminal.bells())
        };
        let bell = bells != self.bells;
        self.bells = bells;
        let slices = session.slices();
        let output = slices != self.slices;
        self.slices = slices;
        let ended = !self.ended_told && session.has_ended();
        self.ended_told |= ended;
        Some(Happened {
            events,
            title,
            bell,
            ended,
            output,
        })
    }

    /// The terminal's grid.
    pub fn grid(&self) -> Grid {
        self.grid
    }

    /// The screen, as the last frame copied it.
    pub fn snapshot(&self) -> &Snapshot {
        &self.snapshot
    }

    /// Brings the screen up to date with the session's, unless a
    /// synchronized update holds it, or the program has not answered a new
    /// size that it learnt of within [`ANSWER_WAIT`], when it stays as it
    /// was. Returns whether the screen changed.
    pub fn update(&mut self) -> bool {
        // As its program draws again at a new size, the screen waits for its
        // drawing, as a synchronized update would.
        if let Some((slices, until)) = self.answer {
            let answered = self
                .session
                .as_ref()
                .is_none_or(|session| session.slices() > slices);
            if !answered && Instant::now() < until {
                return false;
            }
            self.answer = None;
        }
        let copied = self
            .session
            .as_ref()
            .and_then(|session| session.snapshot(&mut self.snapshot));
        if copied.is_some() {
            self.selection_touched.store(false, Ordering::Relaxed);
        }
        copied.is_some_and(|copied| copied > 0)
    }

    /// Notes that the app changed the selection, which the snapshot learns
    /// at its next update.
    fn touch_selection(&self) {
        self.selection_touched.store(true, Ordering::Relaxed);
    }

    /// Gives the terminal `grid`, when it has another, as the program learns
    /// of it: at once when the program was told no grid within
    /// [`RESIZE_PACE`], and at that pace after, holding it meanwhile, in
    /// place of one that it held. Taken before its signal, its screen
    /// reflowed would clear the prompt, which the shell draws again only as
    /// it learns of the grid, so that the prompt flickered, through a live
    /// resize as at its end. Returns when to take it, if it waits.
    pub fn resize(&mut self, grid: Grid, now: Instant) -> Option<Instant> {
        self.held = (grid != self.grid).then_some(grid);
        self.pace(now)
    }

    /// Ends the live resize: the terminal takes its last grid with the
    /// program's signal, within [`RESIZE_PACE`], as [`Pane::pace`] does.
    pub fn end_live_resize(&mut self, now: Instant) -> Option<Instant> {
        self.pace(now)
    }

    /// Gives the terminal `grid` now, reflowing its screen, for the program
    /// to learn of it.
    fn take_grid(&mut self, grid: Grid) {
        if grid == self.grid {
            return;
        }
        self.grid = grid;
        self.changed = true;
        if let Some(session) = &self.session {
            let mut terminal = session.lock();
            terminal.set_cell_size(grid.cell_width, grid.cell_height);
            terminal.resize(grid.columns, grid.rows);
        }
    }

    /// Tells the program the terminal's grid, when it changed since it was
    /// told, so that it draws again at that size; unless it was told one
    /// within [`RESIZE_PACE`] before `now`: then returns when to tell it,
    /// and when the terminal takes the grid that it holds. A grid back at
    /// the one told is told by SIGWINCH alone, as the PTY's size,
    /// unchanged, would not signal it.
    pub fn pace(&mut self, now: Instant) -> Option<Instant> {
        let (told, at) = self.told;
        // A grid held the terminal takes as the program learns of it, at the
        // pace.
        if let Some(grid) = self.held {
            if let Some(at) = at
                && now < at + RESIZE_PACE
            {
                return Some(at + RESIZE_PACE);
            }
            self.held = None;
            self.take_grid(grid);
        }
        if !self.changed || self.kept {
            return None;
        }
        if let Some(at) = at
            && now < at + RESIZE_PACE
        {
            return Some(at + RESIZE_PACE);
        }
        self.told = (self.grid, Some(now));
        self.changed = false;
        let Some(session) = &self.session else {
            return None;
        };
        // The screen waits for the program's drawing at this size, as its
        // answer to a size before, which a frame may not have shown yet,
        // drew another, whose prompt this size cleared: until output comes
        // after this signal. A program that has not answered the size
        // before waits no longer than from that size's signal.
        let slices = session.slices();
        let until = match self.answer {
            Some((before, until)) if slices == before => until,
            _ => now + ANSWER_WAIT,
        };
        self.answer = Some((slices, until));
        let told = if told == self.grid {
            session.signal_resize()
        } else {
            session.resize_pty(self.grid.size())
        };
        if let Err(error) = told {
            eprintln!("lokyr: the terminal cannot take its new size: {error}");
        }
        None
    }

    /// When the screen stops waiting for the program's drawing at the size
    /// that it learnt of, if it waits, as a frame then shows the terminal.
    pub fn answer_due(&self) -> Option<Instant> {
        self.answer.map(|(_, until)| until)
    }

    /// The grid that the program was told last.
    pub fn told(&self) -> Grid {
        self.told.0
    }

    /// Sends `event` to the program, with `options`. A key with Command
    /// held, which the menus did not take, is the app's: it reaches the
    /// program only through the kitty protocol, which reports Super, as
    /// xterm's sequences have no Command.
    pub fn key(&self, event: &KeyEvent<'_>, options: KeyOptions) {
        let Some(session) = &self.session else {
            return;
        };
        if event.mods.super_key && self.snapshot.keyboard_flags() == KeyboardFlags::NONE {
            return;
        }
        session.key(event, options);
    }

    /// Sends `text`, which an input method committed.
    pub fn text(&self, text: &str) {
        if let Some(session) = &self.session {
            session.text(text);
        }
    }

    /// Sends what the mouse's `event` sends, as the program's mouse modes
    /// have it, and returns whether the program took it, rather than the
    /// pane, which scrolls its view with the wheel.
    pub fn mouse(&self, event: &MouseEvent) -> bool {
        self.session
            .as_ref()
            .is_some_and(|session| session.mouse(event))
    }

    /// The modes that the program set of what the keys, the mouse and a
    /// paste send, as they are now, under the lock.
    pub fn input_modes(&self) -> Option<InputModes> {
        self.session
            .as_ref()
            .map(|session| session.lock().input_modes())
    }

    /// The same modes, as the last frame copied them, without the lock,
    /// for what the frame shows of them: the pointer's shape, and the moves
    /// that the platform tells.
    pub fn shown_modes(&self) -> Option<InputModes> {
        self.session.as_ref().map(|_| self.snapshot.input_modes())
    }

    /// The pane's terminal, locked ahead of its I/O thread's next slice
    /// and of the next part of a thread's read, as the main thread takes
    /// it; none for a pane whose program did not start.
    pub fn terminal(&self) -> Option<std::sync::MutexGuard<'_, Terminal>> {
        self.session.as_ref().map(Session::lock)
    }

    /// How many times the pane's terminal was locked, as each lock waits
    /// for a slice of its I/O thread at most.
    pub fn locks(&self) -> u64 {
        self.session.as_ref().map_or(0, Session::locks)
    }

    /// The selection, if there is one, as the terminal has it now, which
    /// the snapshot learns at its next update.
    pub fn selection(&self) -> Option<Selected> {
        self.session.as_ref()?.lock().selection()
    }

    /// The text of the selection, if there is one.
    pub fn selection_text(&self) -> Option<String> {
        self.session
            .as_ref()
            .and_then(|session| session.lock().selection_text())
    }

    /// Selects from the cell `from` to the cell `to`, each by its column
    /// and its row's number, in `shape`, grown to `unit`, as
    /// [`Terminal::select`] does.
    pub fn select(
        &self,
        from: (u16, i64),
        to: (u16, i64),
        shape: SelectionShape,
        unit: SelectionUnit,
    ) {
        if let Some(session) = &self.session {
            session
                .lock()
                .select(from, to, shape, unit, WORD_SEPARATORS);
            self.touch_selection();
        }
    }

    /// Scrolls the view, through the scrollback, so that the row numbered
    /// `row` shows: at the view's edge that it comes past, as it does
    /// already, or not at all, of a row that the list does not hold.
    pub fn show_row(&self, row: i64) {
        let Some(session) = &self.session else {
            return;
        };
        let mut terminal = session.lock();
        if terminal.row_index(row).is_none() {
            return;
        }
        let top = terminal.origin().top;
        let bottom = top.saturating_add(i64::from(terminal.rows()));
        let rows = |n: i64| usize::try_from(n).unwrap_or(usize::MAX);
        if row < top {
            terminal.scroll_viewport(Scroll::Up(rows(top - row)));
        } else if row >= bottom {
            terminal.scroll_viewport(Scroll::Down(rows(row - bottom + 1)));
        }
    }

    /// Anchors the selection that the mouse makes at the cell `at`, its
    /// column and its row's number, as [`Terminal::anchor_selection`] does:
    /// the anchor moves with its text.
    pub fn anchor_selection(&self, at: (u16, i64)) {
        if let Some(session) = &self.session {
            session.lock().anchor_selection(at);
            self.touch_selection();
        }
    }

    /// The cell that the selection by the mouse grows from, now, by its
    /// column and its row's number, as [`Terminal::selection_anchor`] tells
    /// it.
    pub fn selection_anchor(&self) -> Option<(u16, i64)> {
        self.session.as_ref()?.lock().selection_anchor()
    }

    /// How many rows the view is above the screen's top, as the terminal
    /// has it now, which the snapshot learns at its next update.
    pub fn view_offset(&self) -> usize {
        self.session
            .as_ref()
            .map_or(0, |session| session.lock().viewport_offset())
    }

    /// Where the rows are now, as [`Terminal::origin`] tells: their epoch,
    /// and the number of the view's first row, which output can have moved
    /// since the snapshot's, the drawn frame's.
    pub fn origin(&self) -> Option<Origin> {
        Some(self.session.as_ref()?.lock().origin())
    }

    /// The view, the rows that it shows by their numbers, as the last
    /// frame showed it, or as the terminal has it now before the first;
    /// none for a pane whose program did not start.
    pub fn viewport(&self) -> Option<Viewport> {
        let snapshot = &self.snapshot;
        if let Some(origin) = snapshot.origin() {
            return Some(viewport(
                origin,
                snapshot.rows(),
                snapshot.offset(),
                snapshot.scrollback(),
                snapshot.alternate_screen(),
            ));
        }
        let terminal = self.session.as_ref()?.lock();
        Some(viewport(
            terminal.origin(),
            terminal.rows(),
            terminal.viewport_offset(),
            terminal.scrollback_rows(),
            terminal.is_alternate_screen(),
        ))
    }

    /// The view as the last frame showed it, if it changed since the last
    /// time that this told it, which it notes: so that each frame tells its
    /// change once, and the first frame tells the view.
    pub fn viewport_changed(&mut self) -> Option<Viewport> {
        self.snapshot.origin()?;
        let now = self.viewport()?;
        (self.told_viewport.replace(now) != Some(now)).then_some(now)
    }

    /// The commands that the shell marked whose rows meet the rows
    /// numbered from `top` to before `bottom`, as
    /// [`Terminal::commands_meeting`] gives them.
    pub fn commands_meeting(&self, top: i64, bottom: i64) -> Vec<ShellCommand> {
        let Some(session) = &self.session else {
            return Vec::new();
        };
        session
            .lock()
            .commands_meeting(top, bottom)
            .into_iter()
            .map(|marks| ShellCommand {
                id: marks.id,
                prompt: marks.prompt,
                input: marks.input,
                output: marks.output,
                end: marks.end,
                status: marks.status,
            })
            .collect()
    }

    /// The command line of the command of the id `command`, as
    /// [`Terminal::command_line`] gives it.
    pub fn command_line(&self, command: u64) -> Option<String> {
        self.session.as_ref()?.lock().command_line(command)
    }

    /// Holds a cursor of a plugin's at the cell `at`, its column and its
    /// row's number, in place of the terminal's, or gives the terminal's
    /// back with `None`, as [`Terminal::hold_cursor`] does.
    pub fn hold_cursor(&self, at: Option<(u16, i64)>) {
        if let Some(session) = &self.session {
            session.lock().hold_cursor(at);
        }
    }

    /// The cell of the terminal's own cursor, as
    /// [`Terminal::cursor_cell`] gives it; none for a pane whose program
    /// did not start.
    pub fn cursor_cell(&self) -> Option<(u16, i64)> {
        Some(self.session.as_ref()?.lock().cursor_cell())
    }

    /// Lets the selection go.
    pub fn clear_selection(&self) {
        if let Some(session) = &self.session {
            self.touch_selection();
            session.lock().clear_selection();
        }
    }

    /// The text of the logical line that the row numbered `row` is in, as
    /// far as `reach` rows before it and after it, with each character's
    /// cell, as [`Terminal::line_text`] gives it.
    pub fn line_text(&self, row: i64, reach: usize) -> Option<(String, Vec<(u16, i64)>)> {
        self.session
            .as_ref()
            .and_then(|session| session.lock().line_text(row, reach))
    }

    /// The target of the OSC 8 link of the cell at `column` of the row
    /// numbered `row`, as [`Terminal::link_at`] gives it.
    pub fn link_at(&self, column: u16, row: i64) -> Option<String> {
        self.session
            .as_ref()
            .and_then(|session| session.lock().link_at(column, row))
    }

    /// The styles of the row numbered `row`, as runs, as
    /// [`Terminal::row_styles`] gives them.
    pub fn row_styles(&self, row: i64) -> Option<Vec<(u16, u16, Style)>> {
        self.session
            .as_ref()
            .and_then(|session| session.lock().row_styles(row))
    }

    /// Whether the terminal holds the rows numbered `rows`, of its active
    /// screen.
    pub fn holds(&self, rows: &[i64]) -> bool {
        self.session.as_ref().is_some_and(|session| {
            let terminal = session.lock();
            rows.iter().all(|&row| terminal.line_text(row, 0).is_some())
        })
    }

    /// Decorates the cells from `from` to `to` with `look`, as the
    /// decoration `id`, as [`Terminal::decorate`] does; whether it took it.
    /// The next frame shows it.
    pub fn decorate(&self, id: u64, from: (u16, i64), to: (u16, i64), look: Look) -> bool {
        self.session
            .as_ref()
            .is_some_and(|session| session.lock().decorate(id, from, to, look))
    }

    /// Takes the decoration `id` away, as [`Terminal::undecorate`] does.
    pub fn undecorate(&self, id: u64) {
        if let Some(session) = &self.session {
            session.lock().undecorate(id);
        }
    }

    /// Lays a mark of `color` in the gutter at the row numbered `row`, as
    /// the mark `id`, as [`Terminal::mark_gutter`] does; whether it took
    /// it. The next frame shows it.
    pub fn mark_gutter(&self, id: u64, row: i64, color: Rgb) -> bool {
        self.session
            .as_ref()
            .is_some_and(|session| session.lock().mark_gutter(id, row, color))
    }

    /// Takes the gutter's mark `id` away, as [`Terminal::unmark_gutter`]
    /// does.
    pub fn unmark_gutter(&self, id: u64) {
        if let Some(session) = &self.session {
            session.lock().unmark_gutter(id);
        }
    }

    /// A handle on the terminal for a thread of the host's, which reads
    /// it a part at a time, as [`Session::reader`] gives it; none for a
    /// pane whose program did not start.
    pub fn reader(&self) -> Option<Reader> {
        self.session.as_ref().map(Session::reader)
    }

    /// Marks the cell `at`, its column and its row's number, as the one
    /// that a menu opened on, as [`Terminal::mark_cell`] does: the mark
    /// moves with its text.
    pub fn mark_cell(&self, at: (u16, i64)) {
        if let Some(session) = &self.session {
            session.lock().mark_cell(at);
        }
    }

    /// The cell that [`Pane::mark_cell`] marked, now, by its column and its
    /// row's number, as [`Terminal::marked_cell`] tells it.
    pub fn marked_cell(&self) -> Option<(u16, i64)> {
        self.session.as_ref()?.lock().marked_cell()
    }

    /// Lets the cell's mark go, if there is one.
    pub fn unmark_cell(&self) {
        if let Some(session) = &self.session {
            session.lock().unmark_cell();
        }
    }

    /// Tells the program that the pane gained or lost the focus, if it asked
    /// to learn of it (mode 1004), while it runs.
    pub fn focus(&self, focused: bool) {
        if let Some(session) = self.session.as_ref().filter(|_| !self.kept) {
            session.focus(focused);
        }
    }

    /// The name of the program in the terminal's foreground, as `zsh` or
    /// `vim`, once the program runs.
    pub fn foreground_program(&self) -> Option<String> {
        let session = self.session.as_ref().filter(|_| self.ended.is_none())?;
        session.foreground_program()
    }

    /// Whether the terminal hides what is typed, as a password's prompt
    /// does, while the program runs.
    pub fn hides_input(&self) -> bool {
        self.session
            .as_ref()
            .filter(|_| self.ended.is_none())
            .is_some_and(Session::hides_input)
    }

    /// How many bytes of output the pane's program has written since it
    /// started.
    pub fn output_bytes(&self) -> u64 {
        self.session.as_ref().map_or(0, Session::output_bytes)
    }

    /// The working directory of the program in the terminal's foreground,
    /// once the program runs.
    pub fn working_directory(&self) -> Option<PathBuf> {
        let session = self.session.as_ref().filter(|_| self.ended.is_none())?;
        session.foreground_directory()
    }

    /// Its title as its header shows it: its program's; or, of `places`,
    /// its folder and the program in its foreground, as a tab without a
    /// title shows them; none without either.
    pub fn header_title(&self, places: bool) -> Option<String> {
        if !self.title.is_empty() {
            return Some(self.title.clone());
        }
        if !places {
            return None;
        }
        let directory = self.working_directory()?;
        let name = directory_name(&directory, std::env::home_dir().as_deref());
        Some(place_label(
            &name,
            &self.foreground_program().unwrap_or_default(),
        ))
    }

    /// The working directory that the shell told last, by OSC 7, if it is
    /// of this machine, whose name is `host`, and a directory there: where
    /// the shell works, as a program in its foreground, as ssh, may not.
    pub fn told_directory(&self, host: &str) -> Option<PathBuf> {
        let session = self.session.as_ref().filter(|_| self.ended.is_none())?;
        let told = session.lock().working_directory().cloned()?;
        if !lokyr_plugin_sdk::host::is_local_host(&told.host, host) {
            return None;
        }
        Some(PathBuf::from(told.path)).filter(|path| path.is_dir())
    }

    /// Selects the scrollback and the screen, whole.
    pub fn select_all(&self) {
        if let Some(session) = &self.session {
            session.lock().select_all();
            self.touch_selection();
        }
    }

    /// Clears the scrollback.
    pub fn clear_scrollback(&self) {
        if let Some(session) = &self.session {
            session.lock().clear_scrollback();
            self.touch_selection();
        }
    }

    /// Clears the scrollback, and the screen but the prompt that the cursor
    /// is in, which goes to the top.
    pub fn clear(&self) {
        if let Some(session) = &self.session {
            session.lock().clear_to_prompt();
            self.touch_selection();
        }
    }

    /// Sends `text` to the program as it is, as typed keys would, and
    /// brings the view back to the screen.
    pub fn send_text(&self, text: &str) {
        if let Some(session) = &self.session {
            session.send(text);
        }
    }

    /// Pastes `text`, bracketed when the program asks, without its control
    /// characters but tab and the line breaks.
    pub fn paste(&self, text: &str) {
        if let Some(session) = &self.session {
            session.paste(text);
        }
    }

    /// The marks of the searches' matches that the view shows, along the
    /// rows that the terminal holds, parted into `slots` runs, as
    /// [`Terminal::match_marks`] tells them.
    pub fn match_marks(&self, slots: usize) -> Vec<(usize, bool)> {
        self.session
            .as_ref()
            .map_or_else(Vec::new, |session| session.lock().match_marks(slots))
    }

    /// Scrolls the view, through the scrollback.
    pub fn scroll(&self, scroll: Scroll) {
        if let Some(session) = &self.session {
            session.scroll(scroll);
        }
    }

    /// Whether the program's output brings the view back to the screen.
    pub fn scroll_on_output(&self) -> bool {
        self.session
            .as_ref()
            .is_some_and(|session| session.lock().scroll_on_output())
    }

    /// Sets whether the program's output brings the view back to the
    /// screen.
    pub fn set_scroll_on_output(&self, scroll: bool) {
        if let Some(session) = &self.session {
            session.lock().set_scroll_on_output(scroll);
        }
    }

    /// Sets whether the program's reads of the clipboard come as events,
    /// for the embedder to answer, or get no reply.
    pub fn set_clipboard_reads(&self, reads: bool) {
        if let Some(session) = &self.session {
            session.lock().set_clipboard_reads(reads);
        }
    }

    /// Answers the program's read of the clipboard, `request`, with
    /// `text`, what the clipboard holds.
    pub fn reply_clipboard(&self, request: ClipboardRequest, text: &str) {
        if let Some(session) = &self.session {
            session.reply_clipboard(request, text.as_bytes());
        }
    }

    /// Sets the width tables of the text to come, and whether East Asian
    /// Ambiguous characters take two cells.
    pub fn set_width_tables(&self, tables: Tables, ambiguous_wide: bool) {
        if let Some(session) = &self.session {
            session.lock().set_width_tables(tables, ambiguous_wide);
        }
    }

    /// Sets the normal form of the text to come.
    pub fn set_normalization(&self, normalization: Normalization) {
        if let Some(session) = &self.session {
            session.lock().set_normalization(normalization);
        }
    }

    /// Sets the bytes that the scrollback may take, which its oldest rows
    /// give back as they pass them.
    pub fn set_scrollback_limit(&self, limit: usize) {
        if let Some(session) = &self.session {
            session.lock().set_scrollback_limit(limit);
        }
    }

    /// Gives the terminal the theme's `colors`.
    pub fn set_colors(&self, colors: &Colors) {
        if let Some(session) = &self.session {
            session.set_default_colors(colors.clone());
        }
    }

    /// What the window does, as `close` says, once the program has ended:
    /// `None` while it runs, once the pane keeps its screen, and while its
    /// status is still to come, for [`EXIT_WAIT`] after its output ended.
    /// The line that a kept screen shows is in `texts`.
    pub fn exited(&mut self, close: CloseOnExit, now: Instant, texts: &Texts) -> Option<Exited> {
        if self.kept {
            return None;
        }
        let session = self.session.as_mut()?;
        if !session.has_ended() {
            return None;
        }
        let since = *self.ended.get_or_insert(now);
        let exit = match session.exit() {
            Ok(Some(exit)) => Some(exit),
            Ok(None) if now < since + EXIT_WAIT => return None,
            Ok(None) => None,
            Err(error) => {
                eprintln!("lokyr: the program's status cannot be read: {error}");
                None
            }
        };
        let told = session.lock().command_status();
        let exit = status(exit, told, self.through_login);
        let closes = match close {
            CloseOnExit::Always => true,
            CloseOnExit::Never => false,
            CloseOnExit::Clean => exit.is_some_and(Exit::success),
        };
        if closes {
            return Some(Exited::Close);
        }
        self.kept = true;
        let ended = match exit {
            Some(Exit::Code(code)) => {
                texts.text_with("program-exited", &[("code", Arg::Number(code.into()))])
            }
            Some(Exit::Signal(signal)) => texts.text_with(
                "program-signalled",
                &[("signal", Arg::Number(signal.into()))],
            ),
            None => texts.text("program-ended"),
        };
        // The character sets back to ASCII, insert mode off and autowrap on,
        // which a program that ended may have left otherwise, so that the
        // line reads as it is written.
        let line = format!("\x1b(B\x1b)B\x0f\x1b[4l\x1b[?7h\r\n\x1b[0;2m[{ended}]\x1b[0m");
        Parser::new().advance(&mut *session.lock(), line.as_bytes());
        Some(Exited::Keep)
    }

    /// Whether the pane waits for the status of a program whose output
    /// ended.
    pub fn is_waiting(&self) -> bool {
        self.ended.is_some() && !self.kept
    }

    /// Whether the program shows the alternate screen.
    pub fn is_alternate_screen(&self) -> bool {
        self.session
            .as_ref()
            .is_some_and(|session| session.lock().is_alternate_screen())
    }

    /// The keymap's contexts that the pane holds, as a binding's `when`
    /// names them: `alternate-screen` while its program shows a screen of
    /// its own, and `selection` while it has a selection. As the last frame
    /// showed them, without the lock; as the terminal has them now, once
    /// the app changed the selection since.
    pub fn contexts(&self) -> Vec<&'static str> {
        let Some(session) = &self.session else {
            return Vec::new();
        };
        let (alternate, selection) = if self.selection_touched.load(Ordering::Relaxed) {
            let mut terminal = session.lock();
            (
                terminal.is_alternate_screen(),
                terminal.selection().is_some(),
            )
        } else {
            (
                self.snapshot.alternate_screen(),
                self.snapshot.selection().is_some(),
            )
        };
        let mut names = Vec::new();
        if alternate {
            names.push("alternate-screen");
        }
        if selection {
            names.push("selection");
        }
        names
    }

    /// Whether its program started: a pane whose program could not start
    /// shows why, and never tells an exit.
    pub fn started(&self) -> bool {
        self.session.is_some()
    }

    /// Whether the pane keeps the screen of a program that ended.
    pub fn is_kept(&self) -> bool {
        self.kept
    }

    /// The progress that its program tells last, OSC 9;4's, if it tells
    /// one.
    pub fn progress(&self) -> Option<terminal::Progress> {
        self.progress
    }

    /// What closing the pane would end, by its name, as `vim`, or empty
    /// when its name cannot be read: the first process of its terminal, the
    /// foreground's first, that is no shell, nor tmux or screen, whose
    /// sessions outlive the window. A job that the shell runs counts,
    /// stopped or in the background too, and a program that started a
    /// shell, as vim's `:sh`. `None` at a shell's prompt without a job, or
    /// once the program ended.
    pub fn running(&self) -> Option<String> {
        let session = self.session.as_ref().filter(|_| self.ended.is_none())?;
        if session.has_ended() {
            return None;
        }
        let programs = session.programs();
        if programs.is_empty() {
            // Without a list of the processes, the foreground tells.
            return match session.foreground_program() {
                Some(name) if is_shell(&name) => None,
                Some(name) => Some(name),
                None => Some(String::new()),
            };
        }
        programs.into_iter().find(|name| !is_shell(name))
    }
}

/// Whether `program`, by its name, is a shell at its prompt, or tmux or
/// screen, whose sessions go on without the pane, as a login shell's `-`
/// leaves it.
pub fn is_shell(program: &str) -> bool {
    SHELLS.contains(&program.trim_start_matches('-'))
}

/// The programs that closing a pane does not ask about: shells at their
/// prompt, and tmux and screen, whose sessions go on without the pane.
const SHELLS: [&str; 19] = [
    "login", "sh", "bash", "zsh", "fish", "ksh", "mksh", "tcsh", "csh", "dash", "yash", "nu",
    "pwsh", "elvish", "xonsh", "osh", "ysh", "tmux", "screen",
];

/// Whether `program` starts through `login`, whose own status tells
/// nothing, as the login shell does, a command too.
fn through_login(program: &Program) -> bool {
    matches!(program, Program::LoginShell | Program::LoginCommand { .. })
}

/// The status of a program that ended: its own, which `exit` gives; or, for
/// the login shell that `login` starts, whose own status is `login`'s, the
/// one that the shell's wrapper `told` through the terminal, past 128 for a
/// signal, as shells give it.
fn status(exit: Option<Exit>, told: Option<i32>, through_login: bool) -> Option<Exit> {
    match told {
        Some(code) if through_login && code > 128 => Some(Exit::Signal(code - 128)),
        Some(code) if through_login => Some(Exit::Code(code)),
        _ => exit,
    }
}

/// What parts a title's parts.
pub const TITLE_DASH: &str = " — ";

/// A place of its folder's `name` and its `program`, as a window's title
/// and a pane's header have them: `src — vim`, the folder's alone of no
/// program or of one of its name.
pub fn place_label(name: &str, program: &str) -> String {
    if program.is_empty() || program == name {
        name.to_owned()
    } else {
        format!("{name}{TITLE_DASH}{program}")
    }
}

/// A title's name of `directory`: its last part, `~` for `home`, and `/`
/// for the root.
pub fn directory_name(directory: &std::path::Path, home: Option<&std::path::Path>) -> String {
    if Some(directory) == home {
        return "~".to_owned();
    }
    directory.file_name().map_or_else(
        || "/".to_owned(),
        |name| name.to_string_lossy().into_owned(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::l10n::KERNEL;

    #[test]
    fn a_program_starts_in_the_app_s_environment() {
        let environment = Environment {
            resources: Some("/Applications/Lokyr.app/Contents/Resources".into()),
            locale: "it_CH.UTF-8".to_owned(),
            terminfo: Some("/Applications/Lokyr.app/Contents/Resources/terminfo".into()),
            binaries: Some("/Applications/Lokyr.app/Contents/MacOS".into()),
            directory: Some("/tmp".into()),
            version: "1.2.3-20261101".to_owned(),
            shell_integration: Some(
                "/Applications/Lokyr.app/Contents/Resources/shell-integration".into(),
            ),
        };
        let grid = Grid {
            columns: 80,
            rows: 24,
            cell_width: 8,
            cell_height: 16,
        };
        let options = pty_options(&Program::LoginShell, &environment, "xterm-lokyr", grid);
        assert_eq!(options.resources_dir, environment.resources);
        assert_eq!(options.fallback_locale, "it_CH.UTF-8");
        assert_eq!(options.term, "xterm-lokyr");
        assert_eq!(options.terminfo_dir, environment.terminfo);
        assert_eq!(options.binaries_dir, environment.binaries);
        assert_eq!(options.working_directory, environment.directory);
        assert_eq!(options.version, "1.2.3-20261101");
        assert_eq!(options.shell_integration, environment.shell_integration);
        assert_eq!(Settings::default().term, "xterm-256color");
        assert_eq!(options.size, grid.size());
    }

    #[test]
    fn the_login_shell_s_status_is_the_one_that_it_told() {
        // `login`'s 0, and what the wrapper told.
        let login = Some(Exit::Code(0));
        assert_eq!(status(login, Some(3), true), Some(Exit::Code(3)));
        assert_eq!(status(login, Some(137), true), Some(Exit::Signal(9)));
        assert_eq!(status(login, None, true), login);
        // Another program's status is its own, whatever a shell in it told.
        let own = Some(Exit::Code(2));
        assert_eq!(status(own, Some(0), false), own);
        assert!(through_login(&Program::LoginShell));
        assert!(through_login(&Program::LoginCommand {
            command: "exec man -- ls".into()
        }));
        assert!(!through_login(&Program::command("/bin/sh", ["-c", "true"])));
    }

    /// The rows of `pane`'s screen, once its output reaches it.
    fn rows(pane: &mut Pane) -> Vec<String> {
        pane.update();
        let snapshot = pane.snapshot();
        (0..snapshot.rows())
            .map(|row| {
                let line = snapshot.line(row);
                let text: String = line
                    .cells()
                    .iter()
                    .map(|cell| match cell.content() {
                        lokyr_vt_core::page::Content::Char(c) if c != '\0' => c,
                        _ => ' ',
                    })
                    .collect();
                text.trim_end().to_owned()
            })
            .collect()
    }

    /// The slices of output that `pane`'s session parsed.
    fn pane_slices(pane: &Pane) -> u64 {
        pane.session.as_ref().map_or(0, Session::slices)
    }

    /// Waits until `pane`'s screen has `count` rows of `text`, for a few
    /// seconds at most.
    fn wait_for(pane: &mut Pane, text: &str, count: usize) {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let rows = rows(pane);
            if rows.iter().filter(|row| *row == text).count() >= count {
                return;
            }
            assert!(Instant::now() < deadline, "{count} of {text:?}: {rows:?}");
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn a_grid_back_at_the_size_told_within_the_pace_tells_nothing() {
        let grid = |columns| Grid {
            columns,
            rows: 24,
            cell_width: 8,
            cell_height: 16,
        };
        // A program that writes a line at each SIGWINCH.
        let script = "trap 'echo W' WINCH; echo ready; while :; do sleep 0.01; done";
        let program = Program::command("/bin/sh", ["-c", script]);
        let mut pane = Pane::spawn(
            &program,
            &Environment::default(),
            &Settings::default(),
            grid(80),
            &Colors::default(),
            &Texts::new("en", &KERNEL),
            || {},
        );
        wait_for(&mut pane, "ready", 1);
        // A step, told at once; then back and forth within the pace, to the
        // grid told, which the terminal never took, and tells nothing.
        let start = Instant::now();
        assert_eq!(pane.resize(grid(81), start), None);
        wait_for(&mut pane, "W", 1);
        let step = Duration::from_millis(10);
        assert!(pane.resize(grid(80), start + step).is_some());
        assert_eq!(pane.grid(), grid(81));
        assert_eq!(pane.resize(grid(81), start + step * 2), None);
        assert_eq!((pane.grid(), pane.told()), (grid(81), grid(81)));
        // Unchanged since, it tells nothing more.
        assert_eq!(pane.pace(start + RESIZE_PACE), None);
        assert_eq!(pane.told(), grid(81));
    }

    #[test]
    fn a_live_resize_s_grid_goes_with_its_signal_at_the_pace() {
        let grid = |columns| Grid {
            columns,
            rows: 24,
            cell_width: 8,
            cell_height: 16,
        };
        let script = "trap 'echo W' WINCH; echo ready; while :; do sleep 0.01; done";
        let program = Program::command("/bin/sh", ["-c", script]);
        let mut pane = Pane::spawn(
            &program,
            &Environment::default(),
            &Settings::default(),
            grid(80),
            &Colors::default(),
            &Texts::new("en", &KERNEL),
            || {},
        );
        wait_for(&mut pane, "ready", 1);
        // The first step at once, the terminal's grid and the program's
        // together.
        let start = Instant::now();
        assert_eq!(pane.resize(grid(90), start), None);
        assert_eq!((pane.grid(), pane.told()), (grid(90), grid(90)));
        wait_for(&mut pane, "W", 1);
        // A step within the pace waits for it, the terminal as it was.
        let step = Duration::from_millis(10);
        assert_eq!(
            pane.resize(grid(100), start + step),
            Some(start + RESIZE_PACE)
        );
        assert_eq!((pane.grid(), pane.told()), (grid(90), grid(90)));
        assert_eq!(pane.pace(start + RESIZE_PACE), None);
        assert_eq!((pane.grid(), pane.told()), (grid(100), grid(100)));
        wait_for(&mut pane, "W", 2);
        // A step back to the terminal's grid holds nothing; the end takes
        // the last with the program's signal, within the pace, the terminal
        // as it was until then.
        let later = start + RESIZE_PACE + step;
        assert!(pane.resize(grid(95), later).is_some());
        assert_eq!(pane.resize(grid(100), later), None);
        assert!(pane.resize(grid(95), later).is_some());
        let due = pane.end_live_resize(later).expect("the pace");
        assert_eq!((pane.grid(), pane.told()), (grid(100), grid(100)));
        assert_eq!(pane.pace(due), None);
        assert_eq!((pane.grid(), pane.told()), (grid(95), grid(95)));
        wait_for(&mut pane, "W", 3);
    }

    #[test]
    fn a_new_size_s_screen_waits_for_the_program_s_drawing_at_it() {
        let grid = |columns| Grid {
            columns,
            rows: 24,
            cell_width: 8,
            cell_height: 16,
        };
        let spawned = |script: &str| {
            Pane::spawn(
                &Program::command("/bin/sh", ["-c", script]),
                &Environment::default(),
                &Settings::default(),
                grid(80),
                &Colors::default(),
                &Texts::new("en", &KERNEL),
                || {},
            )
        };
        // A program that draws a while after its signal: the screen stays
        // as it was until its drawing comes.
        let mut answering =
            spawned("trap 'sleep 0.2; echo W' WINCH; echo ready; while :; do sleep 0.01; done");
        wait_for(&mut answering, "ready", 1);
        assert_eq!(answering.resize(grid(90), Instant::now()), None);
        assert!(answering.answer_due().is_some());
        assert!(!answering.update());
        assert_eq!(answering.snapshot().cols(), 80);
        wait_for(&mut answering, "W", 1);
        assert_eq!(answering.snapshot().cols(), 90);
        assert_eq!(answering.answer_due(), None);
        // One that draws nothing: the screen shows once the wait ends, which
        // a size after, unanswered too, does not put off.
        let mut quiet = spawned("echo ready; sleep 30");
        wait_for(&mut quiet, "ready", 1);
        let start = Instant::now();
        assert_eq!(quiet.resize(grid(90), start), None);
        assert_eq!(quiet.answer_due(), Some(start + ANSWER_WAIT));
        assert_eq!(quiet.resize(grid(95), start + RESIZE_PACE), None);
        assert_eq!(quiet.grid().columns, 95);
        assert_eq!(quiet.answer_due(), Some(start + ANSWER_WAIT));
        // Before the wait's end, as a loaded machine can take it whole
        // between the sizes and this look.
        if start.elapsed() < ANSWER_WAIT {
            assert!(!quiet.update());
        }
        std::thread::sleep(ANSWER_WAIT);
        assert!(quiet.update());
        assert_eq!(quiet.snapshot().cols(), 95);
    }

    #[test]
    fn a_new_size_waits_for_its_own_drawing_not_for_the_answer_to_the_size_before() {
        let grid = |columns| Grid {
            columns,
            rows: 24,
            cell_width: 8,
            cell_height: 16,
        };
        // A shell that draws its prompt again a while after each signal.
        let mut pane = Pane::spawn(
            &Program::command(
                "/bin/sh",
                [
                    "-c",
                    "trap 'sleep 0.05; echo W' WINCH; echo ready; while :; do sleep 0.01; done",
                ],
            ),
            &Environment::default(),
            &Settings::default(),
            grid(80),
            &Colors::default(),
            &Texts::new("en", &KERNEL),
            || {},
        );
        wait_for(&mut pane, "ready", 1);
        let start = Instant::now();
        assert_eq!(pane.resize(grid(90), start), None);
        // Its answer to 90 comes before any frame copies the screen.
        let before = pane_slices(&pane);
        let deadline = Instant::now() + Duration::from_secs(5);
        while pane_slices(&pane) == before {
            assert!(Instant::now() < deadline, "no answer");
            std::thread::sleep(Duration::from_millis(5));
        }
        // The next size, as a live resize's step, takes the grid, which
        // clears the prompt, and tells the program: the screen waits for
        // the drawing at that size, as the one before is another's.
        let now = Instant::now();
        assert_eq!(pane.resize(grid(100), now), None);
        assert_eq!(pane.answer_due(), Some(now + ANSWER_WAIT));
        assert!(!pane.update());
        assert_eq!(pane.snapshot().cols(), 80);
        wait_for(&mut pane, "W", 2);
        assert_eq!(pane.snapshot().cols(), 100);
    }

    #[test]
    fn a_resize_s_steps_tell_the_program_at_its_pace() {
        let grid = |columns| Grid {
            columns,
            rows: 24,
            cell_width: 8,
            cell_height: 16,
        };
        let cat = Program::command("/bin/cat", [] as [&str; 0]);
        let mut pane = Pane::spawn(
            &cat,
            &Environment::default(),
            &Settings::default(),
            grid(80),
            &Colors::default(),
            &Texts::new("en", &KERNEL),
            || {},
        );
        // A second of steps, one every 8 ms, as a display of 120 Hz draws
        // them, with the timer that the app keeps for the pace.
        let start = Instant::now();
        let step = Duration::from_millis(8);
        let mut timer: Option<Instant> = None;
        let mut tells = Vec::new();
        for n in 0..125_u16 {
            let now = start + step * u32::from(n);
            if let Some(at) = timer.filter(|&at| at <= now) {
                let told = pane.told();
                timer = pane.pace(at);
                if pane.told() != told {
                    tells.push(at);
                }
            }
            let told = pane.told();
            timer = timer.or(pane.resize(grid(81 + n), now));
            if pane.told() != told {
                tells.push(now);
            }
        }
        // The steps' last size comes once more, at the end.
        assert_ne!(pane.told(), grid(205));
        assert_eq!(pane.pace(timer.unwrap()), None);
        assert_eq!(pane.told(), grid(205));
        // The first step at once, then one each pace at most.
        assert_eq!(tells[0], start);
        assert!(
            tells
                .windows(2)
                .all(|pair| pair[1] - pair[0] >= RESIZE_PACE)
        );
        let most = 1 + (step * 125).as_millis() / RESIZE_PACE.as_millis();
        assert!(tells.len() as u128 <= most, "{}", tells.len());
        // Unchanged, the grid tells nothing, and takes no timer.
        let later = timer.unwrap() + RESIZE_PACE;
        assert_eq!(pane.resize(grid(205), later), None);
        assert_eq!(pane.told(), grid(205));
    }

    #[test]
    fn a_told_directory_is_of_this_machine_alone() {
        for host in ["", "localhost", "LocalHost", "mac", "MAC", "mac.local"] {
            assert!(
                lokyr_plugin_sdk::host::is_local_host(host, "mac.local"),
                "{host}"
            );
        }
        assert!(lokyr_plugin_sdk::host::is_local_host(
            "mac.example.com",
            "mac"
        ));
        for host in ["server", "server.example.com", "mac-mini"] {
            assert!(
                !lokyr_plugin_sdk::host::is_local_host(host, "mac.local"),
                "{host}"
            );
        }
        // A machine of no name knows its own by none, or `localhost`.
        assert!(!lokyr_plugin_sdk::host::is_local_host("mac", ""));
    }
}
