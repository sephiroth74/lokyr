//! Pseudo-terminals and child processes.
//!
//! Opens a PTY, starts the shell as its child process and resizes the
//! terminal. The public API exposes no raw file descriptors, so other
//! platforms can be supported behind the same interface.

use std::ffi::OsString;
use std::io;
use std::path::PathBuf;

#[cfg(unix)]
mod unix;

#[cfg(unix)]
pub use unix::{Pty, PtyReader, PtyWriter, spawn};

/// The size of a terminal, in cells and in pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Size {
    /// Columns.
    pub cols: u16,
    /// Rows.
    pub rows: u16,
    /// The width of the cells, all of them, in pixels.
    pub width: u16,
    /// Their height, in pixels.
    pub height: u16,
}

/// What a read of a PTY gave.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReadOutcome {
    /// That many bytes.
    Read(usize),
    /// The end: the program and the processes that it left have closed the
    /// terminal, or the PTY has dropped.
    End,
    /// Nothing, by the deadline.
    TimedOut,
}

/// What a wait for both output and room found, as
/// [`PtyReader::wait_either`] tells it. A PTY that has dropped makes both
/// ready, for the read to tell the end and the write to fail.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Readiness {
    /// The program wrote something, or its output ended: a read takes it
    /// without waiting.
    pub output: bool,
    /// The program has room for what is typed: a write takes some of it
    /// without waiting.
    pub room: bool,
}

/// How a PTY's program ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Exit {
    /// It exited, with this status.
    Code(i32),
    /// This signal ended it.
    Signal(i32),
}

impl Exit {
    /// Whether it exited with status 0.
    pub fn success(self) -> bool {
        self == Self::Code(0)
    }
}

/// What a PTY runs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Program {
    /// The user's shell, from their account, started as a login shell: on
    /// macOS through `login`, as Terminal.app starts it, and elsewhere with
    /// `-` before its name.
    LoginShell,
    /// A program, found as a shell finds it, and its arguments.
    Command {
        /// The program.
        program: PathBuf,
        /// Its arguments.
        args: Vec<OsString>,
    },
}

impl Program {
    /// `program`, with `args`.
    pub fn command(
        program: impl Into<PathBuf>,
        args: impl IntoIterator<Item = impl Into<OsString>>,
    ) -> Self {
        Self::Command {
            program: program.into(),
            args: args.into_iter().map(Into::into).collect(),
        }
    }
}

/// How to start a PTY's program.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Options {
    /// The program.
    pub program: Program,
    /// The value of `TERM`, which says what the terminal can do.
    pub term: String,
    /// The directory that the program starts in: the user's home directory
    /// if `None`.
    pub working_directory: Option<PathBuf>,
    /// The directory of the app's resources, which the program finds in
    /// `BOLT_RESOURCES_DIR`.
    pub resources_dir: Option<PathBuf>,
    /// The locale that `LANG` gets when the environment sets none, as when
    /// the app starts from the Dock: a UTF-8 one.
    pub fallback_locale: String,
    /// More environment variables, set after the others.
    pub env: Vec<(OsString, OsString)>,
    /// The terminal's size.
    pub size: Size,
}

impl Options {
    /// Options to start `program` with `TERM` set to `term`, in the user's
    /// home directory, with `en_US.UTF-8` as the locale when none is set.
    pub fn new(program: Program, term: impl Into<String>) -> Self {
        Self {
            program,
            term: term.into(),
            working_directory: None,
            resources_dir: None,
            fallback_locale: "en_US.UTF-8".to_owned(),
            env: Vec::new(),
            size: Size {
                cols: 80,
                rows: 24,
                width: 0,
                height: 0,
            },
        }
    }
}

/// Why a PTY's program could not start.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// No pseudo-terminal could be opened.
    #[error("cannot open a pseudo-terminal: {0}")]
    Open(#[source] io::Error),
    /// The program could not start.
    #[error("cannot start {program}: {source}")]
    Spawn {
        /// The program, as it was asked for.
        program: String,
        /// Why.
        #[source]
        source: io::Error,
    },
}
