//! The session behind a pane.
//!
//! Owns a PTY, its child process and the terminal state, feeds the output to
//! the parser from a dedicated I/O thread, and produces the snapshots the
//! renderer reads.

#![forbid(unsafe_code)]

mod lock;
mod queue;

use std::io::{self, ErrorKind, Write};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, MutexGuard};
use std::thread;
use std::time::{Duration, Instant};

use bolt_pty::{Exit, Pty, PtyReader, PtyWriter, ReadOutcome, Size};
use bolt_vt_core::input::{self, KeyEvent, KeyOptions, MouseEvent};
use bolt_vt_core::parser::Parser;
use bolt_vt_core::terminal::{self, Terminal};

use lock::FairMutex;
use queue::Queue;

/// The most bytes that the I/O thread parses at a time, with the lock held.
const SLICE: usize = 64 * 1024;

/// The most bytes that the writer writes at a time.
const PASTE_CHUNK: usize = 4 * 1024;

/// How to start a session.
#[derive(Clone, Debug)]
pub struct Options {
    /// The program and its terminal; the terminal starts at the size that
    /// this gives.
    pub pty: bolt_pty::Options,
    /// How the terminal behaves.
    pub terminal: terminal::Options,
    /// How long a synchronized update (mode 2026) may hold the screen
    /// before it is ended, from the moment it starts.
    pub sync_timeout: Duration,
}

impl Options {
    /// Options for `pty`, with the terminal's defaults and a second for
    /// synchronized updates.
    pub fn new(pty: bolt_pty::Options) -> Self {
        Self {
            pty,
            terminal: terminal::Options::default(),
            sync_timeout: Duration::from_secs(1),
        }
    }
}

/// Why a session could not start.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The program, or its terminal, could not start.
    #[error(transparent)]
    Pty(#[from] bolt_pty::Error),
    /// The session's threads, or their ends of the terminal, could not be
    /// made.
    #[error("cannot start the session's threads: {0}")]
    Threads(#[source] io::Error),
}

/// What the threads of a session share.
struct Shared {
    /// The terminal, under the grid lock.
    terminal: FairMutex<Terminal>,
    /// The writes on their way to the program.
    queue: Queue,
    /// Calls the main thread back, once a change is waiting for it.
    wake: Box<dyn Fn() + Send + Sync>,
    /// Whether a change waits for the main thread to look.
    woken: AtomicBool,
    /// Whether the program's output has ended.
    ended: AtomicBool,
    /// How many slices of output the I/O thread has parsed.
    slices: AtomicU64,
}

impl Shared {
    /// Tells the main thread of a change, unless it has a change to look
    /// at already.
    fn changed(&self) {
        if !self.woken.swap(true, Ordering::AcqRel) {
            (self.wake)();
        }
    }
}

/// A program running in a terminal: its PTY, the terminal's state, and the
/// threads that move the bytes between them.
///
/// An I/O thread reads the program's output, at most [`SLICE`] bytes at a
/// time, and parses each slice under the grid lock, which it hands over,
/// between two slices, to any thread that waits for it; it also ends a
/// synchronized update that outlasts its deadline, its read waiting for
/// that at most. A writer thread writes the keys, the pastes and the
/// terminal's replies to the program, so that a program that does not read
/// its input never keeps its output from being read. Dropping the session
/// hangs the program up, ends both threads, and closes the terminal.
pub struct Session {
    shared: Arc<Shared>,
    pty: Pty,
}

impl Session {
    /// Starts the program of `options` in a new terminal. `wake` is called,
    /// from the session's threads, when the terminal changes and the main
    /// thread has looked at the last change, and when the output ends.
    pub fn spawn(
        options: &Options,
        wake: impl Fn() + Send + Sync + 'static,
    ) -> Result<Self, Error> {
        let size = options.pty.size;
        let pty = bolt_pty::spawn(&options.pty)?;
        let reader = pty.reader().map_err(Error::Threads)?;
        let writer = pty.writer().map_err(Error::Threads)?;
        let shared = Arc::new(Shared {
            terminal: FairMutex::new(Terminal::new(
                size.cols,
                size.rows,
                options.terminal.clone(),
            )),
            queue: Queue::default(),
            wake: Box::new(wake),
            woken: AtomicBool::new(false),
            ended: AtomicBool::new(false),
            slices: AtomicU64::new(0),
        });
        let sync_timeout = options.sync_timeout;
        {
            let shared = Arc::clone(&shared);
            thread::Builder::new()
                .name("session-io".into())
                .spawn(move || read_output(&shared, reader, sync_timeout))
                .map_err(Error::Threads)?;
        }
        {
            let shared = Arc::clone(&shared);
            thread::Builder::new()
                .name("session-write".into())
                .spawn(move || write_input(&shared, writer))
                .map_err(Error::Threads)?;
        }
        Ok(Self { shared, pty })
    }

    /// Locks the terminal, ahead of the I/O thread's next slice.
    pub fn lock(&self) -> MutexGuard<'_, Terminal> {
        self.shared.terminal.lock()
    }

    /// Lets the next change call `wake` again. The main thread calls it
    /// before it looks at the terminal, or with the lock held, so that a
    /// change made after its look calls `wake`.
    pub fn clear_wake(&self) {
        self.shared.woken.store(false, Ordering::Release);
    }

    /// Whether the program's output has ended: the program, and every
    /// process that it left, have closed the terminal.
    pub fn has_ended(&self) -> bool {
        self.shared.ended.load(Ordering::Acquire)
    }

    /// How the program ended, if it has.
    pub fn exit(&mut self) -> io::Result<Option<Exit>> {
        self.pty.try_wait()
    }

    /// Sends `bytes` to the program, as they are.
    pub fn write(&self, bytes: impl Into<Vec<u8>>) {
        self.shared.queue.push(bytes.into());
    }

    /// Sends what `event` sends, as the terminal's modes and keyboard flags
    /// have it, with the keyboard's `options`.
    pub fn key(&self, event: &KeyEvent<'_>, options: KeyOptions) {
        let mut bytes = Vec::new();
        {
            let terminal = self.lock();
            let flags = terminal.keyboard_flags();
            input::encode_key(event, &terminal.input_modes(), flags, options, &mut bytes);
        }
        self.write(bytes);
    }

    /// Sends what `event` sends, and returns whether the program takes the
    /// mouse, as [`input::encode_mouse`] does.
    pub fn mouse(&self, event: &MouseEvent) -> bool {
        let mut bytes = Vec::new();
        let taken = {
            let terminal = self.lock();
            let alternate = terminal.is_alternate_screen();
            input::encode_mouse(event, &terminal.input_modes(), alternate, &mut bytes)
        };
        self.write(bytes);
        taken
    }

    /// Tells the program that the terminal gained or lost the focus, if it
    /// asked.
    pub fn focus(&self, focused: bool) {
        let mut bytes = Vec::new();
        input::encode_focus(focused, &self.lock().input_modes(), &mut bytes);
        self.write(bytes);
    }

    /// Pastes `text`, in one entry of the queue, so that no reply comes
    /// between its brackets; the writer writes it in chunks.
    pub fn paste(&self, text: &str) {
        let mut bytes = Vec::new();
        input::encode_paste(text, &self.lock().input_modes(), &mut bytes);
        self.write(bytes);
    }

    /// Gives the terminal and the program `size`: the grid first, so that
    /// the program redraws, on SIGWINCH, at the new size.
    pub fn resize(&self, size: Size) -> io::Result<()> {
        self.lock().resize(size.cols, size.rows);
        self.pty.resize(size)
    }

    /// How many slices of output the I/O thread has parsed.
    #[cfg(test)]
    fn slices(&self) -> u64 {
        self.shared.slices.load(Ordering::Acquire)
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        // The writer stops; the program's hang-up, as the PTY drops, ends
        // the output, and with it the I/O thread.
        self.shared.queue.close();
    }
}

/// The I/O thread: reads the program's output and parses it, a slice at a
/// time, until it ends.
fn read_output(shared: &Shared, mut reader: PtyReader, sync_timeout: Duration) {
    let mut parser = Parser::new();
    let mut buffer = vec![0; SLICE];
    // The synchronized update under way, by its number, and its deadline.
    let mut update = 0;
    let mut deadline = None;
    loop {
        let n = match reader.read_until(&mut buffer, deadline) {
            Ok(ReadOutcome::Read(n)) => n,
            Ok(ReadOutcome::TimedOut) => {
                deadline = None;
                let mut terminal = shared.terminal.lock_in_turn();
                let late =
                    terminal.synchronized_output() && terminal.synchronized_updates() == update;
                if late {
                    terminal.end_synchronized_output();
                    drop(terminal);
                    shared.changed();
                }
                continue;
            }
            Err(error) if error.kind() == ErrorKind::Interrupted => continue,
            Ok(ReadOutcome::End) | Err(_) => break,
        };
        let mut terminal = shared.terminal.lock_in_turn();
        parser.advance(&mut *terminal, &buffer[..n]);
        let replies = terminal.take_replies();
        let synchronized = terminal
            .synchronized_output()
            .then(|| terminal.synchronized_updates());
        shared.slices.fetch_add(1, Ordering::AcqRel);
        drop(terminal);
        shared.queue.push(replies);
        // Each update that starts has its own deadline, which output does
        // not put off.
        if let Some(started) = synchronized.filter(|&started| started != update) {
            update = started;
            deadline = Some(Instant::now() + sync_timeout);
        }
        shared.changed();
    }
    // No more output can end the update under way.
    shared.terminal.lock_in_turn().end_synchronized_output();
    shared.ended.store(true, Ordering::Release);
    shared.woken.store(true, Ordering::Release);
    (shared.wake)();
}

/// The writer thread: writes what the queue holds, in order, each entry in
/// writes of [`PASTE_CHUNK`] bytes at most, so that it stops between two
/// of them once the session closes.
fn write_input(shared: &Shared, mut writer: PtyWriter) {
    while let Some(bytes) = shared.queue.next() {
        for chunk in bytes.chunks(PASTE_CHUNK) {
            if shared.queue.is_closed() {
                return;
            }
            // The program has gone, or the PTY has: what follows has no one
            // to read it.
            if writer.write_all(chunk).is_err() {
                shared.queue.close();
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests;
