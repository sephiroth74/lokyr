//! A window's pane: the program that runs in it, in a session, the screen
//! that its frames draw, and what the window does once the program ends.

use std::time::{Duration, Instant};

use lokyr_kernel::config::CloseOnExit;
use lokyr_platform_core::Waker;
use lokyr_pty::{Exit, Program, Size};
use lokyr_session::{Options, Session};
use lokyr_vt_core::input::{KeyEvent, KeyOptions};
use lokyr_vt_core::parser::Parser;
use lokyr_vt_core::terminal::{self, Colors, KeyboardFlags, Snapshot, Terminal};

/// The `TERM` that programs take, which remote hosts know too.
const TERM: &str = "xterm-256color";

/// How long a pane waits for the status of a program whose output ended,
/// before it tells that the program ended, without its status.
const EXIT_WAIT: Duration = Duration::from_secs(1);

/// How often a pane tells its program a new size at most: a live resize
/// changes the grid at each step, and the program, which redraws on each
/// SIGWINCH, learns of the steps a few times a second, and of the last one.
pub(crate) const RESIZE_PACE: Duration = Duration::from_millis(50);

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
    /// The grid that the program was told last, through the PTY, and when,
    /// unless it was told only as it started.
    told: (Grid, Option<Instant>),
    /// When the program's output ended, while the pane waits for its status.
    ended: Option<Instant>,
    /// Whether the pane keeps the screen of a program that ended, or that
    /// could not start.
    kept: bool,
    /// Whether the program is the login shell, which `login` starts on
    /// macOS, hiding its status, which the shell's wrapper tells through
    /// the terminal instead.
    through_login: bool,
}

impl std::fmt::Debug for Pane {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Pane")
            .field("running", &self.session.is_some())
            .field("grid", &self.grid)
            .field("told", &self.told)
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
        let mut pty = lokyr_pty::Options::new(program.clone(), TERM);
        pty.size = grid.size();
        let mut pane = Self {
            session: None,
            snapshot: Snapshot::default(),
            grid,
            told: (grid, None),
            ended: None,
            kept: false,
            through_login: matches!(program, Program::LoginShell),
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
                eprintln!("lokyr: the program cannot start: {error}");
                let mut terminal =
                    Terminal::new(grid.columns, grid.rows, terminal::Options::default());
                terminal.set_default_colors(colors.clone());
                let line = format!("\x1b[2m[the program cannot start: {error}]\x1b[0m");
                Parser::new().advance(&mut terminal, line.as_bytes());
                terminal.snapshot(&mut pane.snapshot);
                // A key closes the window, as after a program that ended.
                pane.kept = true;
            }
        }
        pane
    }

    /// The screen, as the last frame copied it.
    pub(crate) fn snapshot(&self) -> &Snapshot {
        &self.snapshot
    }

    /// Brings the screen up to date with the session's, unless a
    /// synchronized update holds it, when it stays as it was. Returns
    /// whether the screen changed.
    pub(crate) fn update(&mut self) -> bool {
        self.session
            .as_ref()
            .and_then(|session| session.snapshot(&mut self.snapshot))
            .is_some_and(|copied| copied > 0)
    }

    /// Gives the terminal `grid` at once, when it has another, reflowing
    /// its screen, and tells the program, as [`Pane::pace`] does.
    pub(crate) fn resize(&mut self, grid: Grid, now: Instant) -> Option<Instant> {
        if grid != self.grid {
            self.grid = grid;
            if let Some(session) = &self.session {
                let mut terminal = session.lock();
                terminal.set_cell_size(grid.cell_width, grid.cell_height);
                terminal.resize(grid.columns, grid.rows);
            }
        }
        self.pace(now)
    }

    /// Tells the program the terminal's grid, when it was told another, so
    /// that it draws again at that size; unless it was told one within
    /// [`RESIZE_PACE`] before `now`: then returns when to tell it.
    pub(crate) fn pace(&mut self, now: Instant) -> Option<Instant> {
        let (told, at) = self.told;
        if told == self.grid || self.kept {
            return None;
        }
        if let Some(at) = at
            && now < at + RESIZE_PACE
        {
            return Some(at + RESIZE_PACE);
        }
        self.told = (self.grid, Some(now));
        if let Some(session) = &self.session
            && let Err(error) = session.resize_pty(self.grid.size())
        {
            eprintln!("lokyr: the terminal cannot take its new size: {error}");
        }
        None
    }

    /// The grid that the program was told last.
    #[cfg(test)]
    pub(crate) fn told(&self) -> Grid {
        self.told.0
    }

    /// Sends `event` to the program, with `options`. A key with Command
    /// held, which the menus did not take, is the app's: it reaches the
    /// program only through the kitty protocol, which reports Super, as
    /// xterm's sequences have no Command.
    pub(crate) fn key(&self, event: &KeyEvent<'_>, options: KeyOptions) {
        let Some(session) = &self.session else {
            return;
        };
        if event.mods.super_key && session.lock().keyboard_flags() == KeyboardFlags::NONE {
            return;
        }
        session.key(event, options);
    }

    /// Sends `text`, which an input method committed.
    pub(crate) fn text(&self, text: &str) {
        if let Some(session) = &self.session {
            session.text(text);
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
            Some(Exit::Code(code)) => format!("the program exited with code {code}"),
            Some(Exit::Signal(signal)) => format!("the program ended by signal {signal}"),
            None => "the program ended".to_owned(),
        };
        // The character sets back to ASCII, insert mode off and autowrap on,
        // which a program that ended may have left otherwise, so that the
        // line reads as it is written.
        let line = format!(
            "\x1b(B\x1b)B\x0f\x1b[4l\x1b[?7h\r\n\x1b[0;2m[{ended}; a key closes the window]\x1b[0m"
        );
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
        let mut pane = Pane::spawn(&cat, grid(80), &Colors::default(), Waker::new(|| {}));
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
