//! A pane: the program that runs in it, in a session, the screen that its
//! frames draw, and what its window does once the program ends.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use lokyr_pty::{Exit, Program, Size};
use lokyr_session::{Options, Session};
use lokyr_vt_core::input::{KeyEvent, KeyOptions, MouseEvent};
use lokyr_vt_core::page::Scrollback;
use lokyr_vt_core::parser::Parser;
use lokyr_vt_core::terminal::{
    self, ClipboardRequest, Colors, InputModes, KeyboardFlags, Origin, Scroll, Selected,
    SelectionShape, SelectionUnit, Snapshot, Terminal, WORD_SEPARATORS,
};
use lokyr_vt_core::unicode::{Normalization, Tables, Widths};

use super::PaneId;
use crate::config::CloseOnExit;
use crate::l10n::{Arg, Texts};

/// How long a pane waits for the status of a program whose output ended,
/// before it tells that the program ended, without its status.
const EXIT_WAIT: Duration = Duration::from_secs(1);

/// How often a pane tells its program a new size at most: a live resize
/// changes the grid at each step, and the program, which redraws on each
/// SIGWINCH, learns of the steps a few times a second, and of the last one.
pub const RESIZE_PACE: Duration = Duration::from_millis(50);

/// The shortest time between two looks at what a pane's session told, its
/// bells and its terminal's events: a frame's at 60 Hz.
pub const TOLD_PACE: Duration = Duration::from_micros(16_667);

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
        let pty = pty_options(program, environment, &settings.term, grid);
        let mut pane = Self {
            id: PaneId::new(),
            session: None,
            snapshot: Snapshot::default(),
            grid,
            told: (grid, None),
            changed: false,
            ended: None,
            kept: false,
            through_login: through_login(program),
            bells: 0,
            ended_told: false,
            told_at: None,
            selection_touched: AtomicBool::new(false),
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
        let ended = !self.ended_told && session.has_ended();
        self.ended_told |= ended;
        Some(Happened {
            events,
            title,
            bell,
            ended,
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
    /// synchronized update holds it, when it stays as it was. Returns
    /// whether the screen changed.
    pub fn update(&mut self) -> bool {
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

    /// Gives the terminal `grid` at once, when it has another, reflowing
    /// its screen, and tells the program, as [`Pane::pace`] does.
    pub fn resize(&mut self, grid: Grid, now: Instant) -> Option<Instant> {
        if grid != self.grid {
            self.grid = grid;
            self.changed = true;
            if let Some(session) = &self.session {
                let mut terminal = session.lock();
                terminal.set_cell_size(grid.cell_width, grid.cell_height);
                terminal.resize(grid.columns, grid.rows);
            }
        }
        self.pace(now)
    }

    /// Tells the program the terminal's grid, when it changed since it was
    /// told, so that it draws again at that size; unless it was told one
    /// within [`RESIZE_PACE`] before `now`: then returns when to tell it. A
    /// grid back at the one told is told by SIGWINCH alone, as the PTY's
    /// size, unchanged, would not signal it.
    pub fn pace(&mut self, now: Instant) -> Option<Instant> {
        let (told, at) = self.told;
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
            session.lock().set_default_colors(colors.clone());
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
        let is_shell = |name: &str| SHELLS.contains(&name.trim_start_matches('-'));
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
    fn a_grid_back_at_the_size_told_signals_the_program() {
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
        // grid told, which the PTY's size cannot tell again.
        let start = Instant::now();
        assert_eq!(pane.resize(grid(81), start), None);
        wait_for(&mut pane, "W", 1);
        let step = Duration::from_millis(10);
        assert!(pane.resize(grid(80), start + step).is_some());
        let due = pane
            .resize(grid(81), start + step * 2)
            .expect("the pace's time");
        assert_eq!(pane.told(), grid(81));
        assert_eq!(pane.pace(due), None);
        wait_for(&mut pane, "W", 2);
        // Unchanged since, it tells nothing more.
        assert_eq!(pane.pace(due + RESIZE_PACE), None);
    }

    #[test]
    fn a_live_resize_tells_the_program_at_its_pace() {
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
        // The first step at once, then one every 50 ms at most.
        assert_eq!(tells[0], start);
        assert!(
            tells
                .windows(2)
                .all(|pair| pair[1] - pair[0] >= RESIZE_PACE)
        );
        assert!(tells.len() <= 21, "{}", tells.len());
        // Unchanged, the grid tells nothing, and takes no timer.
        let later = timer.unwrap() + RESIZE_PACE;
        assert_eq!(pane.resize(grid(205), later), None);
        assert_eq!(pane.told(), grid(205));
    }
}
