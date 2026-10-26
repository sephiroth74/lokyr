//! A window's pane: the program that runs in it, in a session, the screen
//! that its frames draw, and what the window does once the program ends.

use std::time::{Duration, Instant};

use bolt_kernel::config::CloseOnExit;
use bolt_platform_core::Waker;
use bolt_pty::{Exit, Program, Size};
use bolt_session::{Options, Session};
use bolt_vt_core::parser::Parser;
use bolt_vt_core::terminal::{self, Colors, Snapshot, Terminal};

/// The `TERM` that programs take, which remote hosts know too.
const TERM: &str = "xterm-256color";

/// How long a pane waits for the status of a program whose output ended,
/// before it tells that the program ended, without its status.
const EXIT_WAIT: Duration = Duration::from_secs(1);

/// A pane's grid: its columns and rows, and the pixels of its cell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Grid {
    /// Its columns.
    pub(crate) columns: u16,
    /// Its rows.
    pub(crate) rows: u16,
    /// A cell's width, in pixels.
    pub(crate) cell_width: u16,
    /// A cell's height, in pixels.
    pub(crate) cell_height: u16,
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
pub(crate) enum Exited {
    /// It closes.
    Close,
    /// It keeps its screen, with a line that tells how the program ended,
    /// until a key closes it.
    Keep,
}

/// The program of a window, in its session, and the screen that its frames
/// draw.
pub(crate) struct Pane {
    /// The session, unless the program could not start.
    session: Option<Session>,
    /// The screen, as the last frame copied it.
    snapshot: Snapshot,
    /// The grid that the session's terminal has.
    grid: Grid,
    /// When the program's output ended, while the pane waits for its status.
    ended: Option<Instant>,
    /// Whether the pane keeps the screen of a program that ended.
    kept: bool,
}

impl std::fmt::Debug for Pane {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Pane")
            .field("running", &self.session.is_some())
            .field("grid", &self.grid)
            .field("ended", &self.ended)
            .field("kept", &self.kept)
            .finish_non_exhaustive()
    }
}

impl Pane {
    /// A pane that runs `program` in a terminal of `grid`, in `colors`,
    /// whose changes wake `waker`; or, when the program cannot start, a pane
    /// whose screen tells why.
    pub(crate) fn spawn(program: &Program, grid: Grid, colors: &Colors, waker: Waker) -> Self {
        let mut pty = bolt_pty::Options::new(program.clone(), TERM);
        pty.size = grid.size();
        let mut pane = Self {
            session: None,
            snapshot: Snapshot::default(),
            grid,
            ended: None,
            kept: false,
        };
        match Session::spawn(&Options::new(pty), move || waker.wake()) {
            Ok(session) => {
                {
                    let mut terminal = session.lock();
                    terminal.set_default_colors(colors.clone());
                    terminal.set_cell_size(grid.cell_width, grid.cell_height);
                }
                pane.session = Some(session);
            }
            Err(error) => {
                eprintln!("bolt: the program cannot start: {error}");
                let mut terminal =
                    Terminal::new(grid.columns, grid.rows, terminal::Options::default());
                terminal.set_default_colors(colors.clone());
                let line = format!("\x1b[2m[the program cannot start: {error}]\x1b[0m");
                Parser::new().advance(&mut terminal, line.as_bytes());
                terminal.snapshot(&mut pane.snapshot);
            }
        }
        pane
    }

    /// The screen, as the last frame copied it.
    pub(crate) fn snapshot(&self) -> &Snapshot {
        &self.snapshot
    }

    /// Brings the screen up to date with the session's, unless a
    /// synchronized update holds it, when it stays as it was.
    pub(crate) fn update(&mut self) {
        if let Some(session) = &self.session {
            session.snapshot(&mut self.snapshot);
        }
    }

    /// Gives the terminal `grid`, when it has another, so that the program
    /// draws again at its size.
    pub(crate) fn resize(&mut self, grid: Grid) {
        if grid == self.grid {
            return;
        }
        self.grid = grid;
        if let Some(session) = &self.session {
            session
                .lock()
                .set_cell_size(grid.cell_width, grid.cell_height);
            if let Err(error) = session.resize(grid.size()) {
                eprintln!("bolt: the terminal cannot take its new size: {error}");
            }
        }
    }

    /// Gives the terminal the theme's `colors`.
    pub(crate) fn set_colors(&self, colors: &Colors) {
        if let Some(session) = &self.session {
            session.lock().set_default_colors(colors.clone());
        }
    }

    /// What the window does, as `close` says, once the program has ended:
    /// `None` while it runs, once the pane keeps its screen, and while its
    /// status is still to come, for [`EXIT_WAIT`] after its output ended.
    pub(crate) fn exited(&mut self, close: CloseOnExit, now: Instant) -> Option<Exited> {
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
                eprintln!("bolt: the program's status cannot be read: {error}");
                None
            }
        };
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
            Some(Exit::Code(code)) => format!("the program exited with code {code}"),
            Some(Exit::Signal(signal)) => format!("the program ended by signal {signal}"),
            None => "the program ended".to_owned(),
        };
        let line = format!("\r\n\x1b[0;2m[{ended}; a key closes the window]\x1b[0m");
        Parser::new().advance(&mut *session.lock(), line.as_bytes());
        Some(Exited::Keep)
    }

    /// Whether the pane waits for the status of a program whose output
    /// ended.
    pub(crate) fn is_waiting(&self) -> bool {
        self.ended.is_some() && !self.kept
    }

    /// Whether the pane keeps the screen of a program that ended.
    pub(crate) fn is_kept(&self) -> bool {
        self.kept
    }
}
