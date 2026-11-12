//! The session behind a pane.
//!
//! Owns a PTY, its child process and the terminal state, feeds the output to
//! the parser from a dedicated I/O thread, and produces the snapshots the
//! renderer reads.

#![forbid(unsafe_code)]

mod lock;
mod queue;

use std::io::{self, Write};
use std::panic::AssertUnwindSafe;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, MutexGuard};
use std::thread;
use std::time::{Duration, Instant};

use lokyr_pty::{Exit, Next, Pty, PtyWriter, ReadAhead, Size};
use lokyr_vt_core::input::{self, KeyEvent, KeyOptions, MouseEvent};
use lokyr_vt_core::parser::Parser;
use lokyr_vt_core::terminal::{self, ClipboardRequest, Colors, Scroll, Snapshot, Terminal};

use lock::FairMutex;
use queue::Queue;

/// The most bytes that the I/O thread parses at a time, with the lock held.
const SLICE: usize = 64 * 1024;

/// The most bytes that the writer writes at a time.
const PASTE_CHUNK: usize = 4 * 1024;

/// How long after the owner's last input, or a new size, the program's
/// output is taken for its answer, the echo of keys or a redraw, rather
/// than for output of its own: the round trip of a remote shell's echo
/// over a slow link, with room to spare.
pub const ECHO: Duration = Duration::from_millis(500);

/// How to start a session.
#[derive(Clone, Debug)]
pub struct Options {
    /// The program and its terminal; the terminal starts at the size that
    /// this gives.
    pub pty: lokyr_pty::Options,
    /// How the terminal behaves.
    pub terminal: terminal::Options,
    /// How long a synchronized update (mode 2026) may hold the screen
    /// before it is ended, from the moment it starts.
    pub sync_timeout: Duration,
    /// The theme's colors, which the terminal holds before the program
    /// writes anything, so that a program that turns the reports of the
    /// color scheme on at once is told of no change; the terminal's own
    /// if none.
    pub colors: Option<Colors>,
    /// The size of a cell in pixels, which CSI 14 t and 16 t report, held
    /// alike from the start, if it is known.
    pub cell_size: Option<(u16, u16)>,
}

impl Options {
    /// Options for `pty`, with the terminal's defaults and a second for
    /// synchronized updates.
    pub fn new(pty: lokyr_pty::Options) -> Self {
        Self {
            pty,
            terminal: terminal::Options::default(),
            sync_timeout: Duration::from_secs(1),
            colors: None,
            cell_size: None,
        }
    }
}

/// Why a session could not start.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The program, or its terminal, could not start.
    #[error(transparent)]
    Pty(#[from] lokyr_pty::Error),
    /// The session's threads, or their ends of the terminal, could not be
    /// made.
    #[error("cannot start the session's threads")]
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
    /// Whether a change of the grid waits for the main thread to look,
    /// which a snapshot lets wake again: a pane that is not drawn asks
    /// for no more.
    grid_woken: AtomicBool,
    /// Whether something told waits for the main thread to take it, a
    /// terminal's event or a bell, which taking the events lets wake again,
    /// whether the pane is drawn or not.
    told_woken: AtomicBool,
    /// Whether the program's output has ended.
    ended: AtomicBool,
    /// How many slices of output the I/O thread has parsed.
    slices: AtomicU64,
    /// How many bytes of output it has read.
    bytes: AtomicU64,
    /// How many times the session's owner locked the terminal.
    locks: AtomicU64,
    /// The moment that the times below count from.
    born: Instant,
    /// When the owner's input last reached the program, or the program was
    /// last told a new size, as [`Shared::stamp`] gives it; 0 before.
    asked: AtomicU64,
    /// When the last slice of output of the program's own came, as
    /// [`Shared::stamp`] gives it; 0 before.
    printed: AtomicU64,
    /// Whether a slice of the program's own woke the main thread, which
    /// [`Session::clear_printed`] lets wake again.
    printed_woken: AtomicBool,
}

impl Shared {
    /// `at`, as the stamps of the times above keep it: past 0, which is
    /// none.
    fn stamp(&self, at: Instant) -> u64 {
        u64::try_from(at.saturating_duration_since(self.born).as_nanos()).unwrap_or(u64::MAX - 1)
            + 1
    }

    /// The moment of `stamp`, as [`Shared::stamp`] gave it, if any.
    fn instant(&self, stamp: u64) -> Option<Instant> {
        (stamp != 0).then(|| self.born + Duration::from_nanos(stamp - 1))
    }

    /// Notes that the program is asked something now: input, or a new
    /// size.
    fn asked_now(&self) {
        self.asked
            .store(self.stamp(Instant::now()), Ordering::SeqCst);
    }

    /// Tells the main thread of a change of the grid, unless it has one to
    /// look at already; and of something `told`, unless something told
    /// waits for it already.
    fn changed(&self, told: bool) {
        let grid = !self.grid_woken.swap(true, Ordering::AcqRel);
        let told = told && !self.told_woken.swap(true, Ordering::AcqRel);
        if grid || told {
            (self.wake)();
        }
    }
}

/// A handle on a session's terminal for a thread of its owner's, as
/// [`Session::reader`] gives it, which outlives neither the terminal's
/// state nor its program: the session that it came from hangs the program
/// up as it goes.
#[derive(Clone)]
pub struct Reader {
    shared: Arc<Shared>,
}

impl Reader {
    /// Locks the terminal for a part of a read, once the threads that
    /// asked for it before, as a frame's copy, have had it, as the I/O
    /// thread takes its slices: a read takes it again and again, a part at
    /// a time, which the lock of `std`, unfair, would let keep a frame
    /// waiting for many parts.
    pub fn lock(&self) -> MutexGuard<'_, Terminal> {
        self.shared.terminal.lock_in_turn()
    }

    /// Wakes the main thread, as a change of the terminal does.
    pub fn wake(&self) {
        (self.shared.wake)();
    }
}

impl std::fmt::Debug for Reader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Reader").finish_non_exhaustive()
    }
}

/// A program running in a terminal: its PTY, the terminal's state, and the
/// threads that move the bytes between them.
///
/// A thread of the PTY's reads the program's output ahead of its parse, so
/// that the program does not wait for it; an I/O thread parses it, at most
/// [`SLICE`] bytes at a time, each slice under the grid lock, which it hands
/// over, between two slices, to any thread that waits for it; it also ends
/// a synchronized update that outlasts its deadline, its wait for output
/// lasting until that at most. A writer thread writes the keys, the pastes and the
/// terminal's replies to the program, so that a program that does not read
/// its input never keeps its output from being read; the replies that wait
/// take at most about a MiB, past which the next are dropped. Dropping the session
/// hangs the program up, ends its threads, and closes the terminal.
pub struct Session {
    shared: Arc<Shared>,
    pty: Pty,
}

impl Session {
    /// Starts the program of `options` in a new terminal. `wake` is called,
    /// from the session's threads, when the terminal changes and the main
    /// thread has looked at the last change, as [`Session::snapshot`] does;
    /// when the terminal tells something, an event or a bell, and the main
    /// thread has taken what it told last, as [`Session::clear_told`] lets
    /// it; when the program prints of its own, the first time and the
    /// first after [`Session::clear_printed`]; and when the output ends.
    pub fn spawn(
        options: &Options,
        wake: impl Fn() + Send + Sync + 'static,
    ) -> Result<Self, Error> {
        let size = options.pty.size;
        let pty = lokyr_pty::spawn(&options.pty)?;
        let output = pty
            .reader()
            .and_then(|reader| reader.read_ahead(SLICE))
            .map_err(Error::Threads)?;
        let writer = pty.writer().map_err(Error::Threads)?;
        // Of the theme's colors and the cell's size before the output's
        // thread reads the program's first bytes.
        let mut terminal = Terminal::new(size.cols, size.rows, options.terminal.clone());
        if let Some(colors) = &options.colors {
            terminal.set_default_colors(colors.clone());
        }
        if let Some((width, height)) = options.cell_size {
            terminal.set_cell_size(width, height);
        }
        let shared = Arc::new(Shared {
            terminal: FairMutex::new(terminal),
            queue: Queue::default(),
            wake: Box::new(wake),
            grid_woken: AtomicBool::new(false),
            told_woken: AtomicBool::new(false),
            ended: AtomicBool::new(false),
            slices: AtomicU64::new(0),
            bytes: AtomicU64::new(0),
            locks: AtomicU64::new(0),
            born: Instant::now(),
            asked: AtomicU64::new(0),
            printed: AtomicU64::new(0),
            printed_woken: AtomicBool::new(false),
        });
        let sync_timeout = options.sync_timeout;
        {
            let shared = Arc::clone(&shared);
            thread::Builder::new()
                .name("session-io".into())
                .spawn(move || read_output(&shared, output, sync_timeout))
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
        self.shared.locks.fetch_add(1, Ordering::Relaxed);
        self.shared.terminal.lock()
    }

    /// A handle on the terminal for a thread of the owner's, as one that
    /// reads much of its text, under the lock a part at a time, apart from
    /// the main thread, which it wakes once it is done.
    pub fn reader(&self) -> Reader {
        Reader {
            shared: Arc::clone(&self.shared),
        }
    }

    /// How many times the session's owner locked the terminal, as each
    /// lock waits for a slice of the I/O thread at most.
    pub fn locks(&self) -> u64 {
        self.shared.locks.load(Ordering::Relaxed)
    }

    /// Brings `snapshot` up to date with the terminal, under the lock, and
    /// returns how many rows it copied, as [`Terminal::snapshot`] does; or
    /// `None`, leaving it as it was, while a synchronized update holds the
    /// screen. Either way, the next change calls `wake` again.
    ///
    /// The I/O thread waits for the copy only, never for the drawing that
    /// follows it, and a renderer that falls behind copies the latest
    /// state.
    pub fn snapshot(&self, snapshot: &mut Snapshot) -> Option<usize> {
        let mut terminal = self.lock();
        self.clear_wake();
        if terminal.synchronized_output() {
            return None;
        }
        Some(terminal.snapshot(snapshot))
    }

    /// Lets the next change call `wake` again. The main thread calls it
    /// before it looks at the terminal, or with the lock held, so that a
    /// change made after its look calls `wake`.
    pub fn clear_wake(&self) {
        self.shared.grid_woken.store(false, Ordering::Release);
    }

    /// Lets the next event or bell call `wake` again, as the main thread
    /// takes the events: it calls it with the lock held, so that one told
    /// after it took them calls `wake`.
    pub fn clear_told(&self) {
        self.shared.told_woken.store(false, Ordering::Release);
    }

    /// When the program last printed of its own: the last slice of output
    /// that came [`ECHO`] or more after the owner's input last reached it,
    /// or it was last told a new size, rather than as its answer; `None`
    /// before the first. The first slice of its own calls `wake`, as one
    /// told does, and the next after [`Session::clear_printed`].
    pub fn printed_at(&self) -> Option<Instant> {
        self.shared
            .instant(self.shared.printed.load(Ordering::SeqCst))
    }

    /// Lets the program's next slice of its own call `wake` again, as the
    /// owner, having found it quiet, waits for it to print: it calls it
    /// before it looks at [`Session::printed_at`], so that a slice that
    /// came after its look calls `wake`.
    pub fn clear_printed(&self) {
        self.shared.printed_woken.store(false, Ordering::SeqCst);
    }

    /// Whether the program's output has ended: the program, and every
    /// process that it left, have closed the terminal; or the session can
    /// no longer read it, as when its I/O thread panicked, with the program
    /// still there.
    pub fn has_ended(&self) -> bool {
        self.shared.ended.load(Ordering::Acquire)
    }

    /// The name of the program in the terminal's foreground, as
    /// [`Pty::foreground_program`] gives it.
    pub fn foreground_program(&self) -> Option<String> {
        self.pty.foreground_program()
    }

    /// Whether the terminal hides what is typed, as a password's prompt
    /// does, as [`Pty::hides_input`] tells.
    pub fn hides_input(&self) -> bool {
        self.pty.hides_input()
    }

    /// How many bytes of output the program has written, which the I/O
    /// thread has read, since the session started.
    pub fn output_bytes(&self) -> u64 {
        self.shared.bytes.load(Ordering::Relaxed)
    }

    /// The names of the processes of the terminal, the program's and its
    /// jobs', as [`Pty::programs`] gives them.
    pub fn programs(&self) -> Vec<String> {
        self.pty.programs()
    }

    /// The process group in the terminal's foreground, as
    /// [`Pty::foreground_process_group`] gives it.
    pub fn foreground_group(&self) -> Option<u32> {
        self.pty.foreground_process_group()
    }

    /// The names that the programs in the terminal's foreground go by, as
    /// [`Pty::foreground_names`] gives them.
    pub fn foreground_names(&self) -> Vec<String> {
        self.pty.foreground_names()
    }

    /// The working directory of the program in the terminal's foreground,
    /// as [`Pty::foreground_directory`] gives it.
    pub fn foreground_directory(&self) -> Option<std::path::PathBuf> {
        self.pty.foreground_directory()
    }

    /// How the program ended, if it has.
    pub fn exit(&mut self) -> io::Result<Option<Exit>> {
        self.pty.try_wait()
    }

    /// Sends `bytes` to the program, as they are, leaving the view where it
    /// is.
    pub fn write(&self, bytes: impl Into<Vec<u8>>) {
        self.shared.queue.push(bytes.into());
    }

    /// Scrolls the view of the terminal's primary screen, as
    /// [`Terminal::scroll_viewport`] does.
    pub fn scroll(&self, scroll: Scroll) {
        self.lock().scroll_viewport(scroll);
    }

    /// Sends what `event` sends, as the terminal's modes and keyboard flags
    /// have it, with the keyboard's `options`. A key that sends something
    /// brings the view back to the screen, as typing does.
    pub fn key(&self, event: &KeyEvent<'_>, options: KeyOptions) {
        let mut bytes = Vec::new();
        {
            let mut terminal = self.lock();
            let flags = terminal.keyboard_flags();
            input::encode_key(event, &terminal.input_modes(), flags, options, &mut bytes);
            typed(&mut terminal, &bytes);
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

    /// Gives the terminal the theme's `colors`: a program that asked is told
    /// their scheme at once, as their change's report waits for no output.
    pub fn set_default_colors(&self, colors: Colors) {
        let replies = {
            let mut terminal = self.lock();
            terminal.set_default_colors(colors);
            terminal.take_replies()
        };
        if !replies.is_empty() {
            self.shared.queue.push_replies(replies);
        }
    }

    /// Answers `request`, a program's read of the clipboard that the user
    /// allowed, with `data`, what the clipboard holds: the answer goes to
    /// the program at once, whole, as keys do, as a program can wait for it
    /// without writing anything.
    pub fn reply_clipboard(&self, request: ClipboardRequest, data: &[u8]) {
        self.shared
            .queue
            .push(Terminal::clipboard_answer(request, data));
    }

    /// Sends `text`, which an input method committed rather than a key
    /// typed, as the kitty protocol's flags have it, and brings the view
    /// back to the screen.
    pub fn text(&self, text: &str) {
        let mut bytes = Vec::new();
        {
            let mut terminal = self.lock();
            input::encode_text(text, terminal.keyboard_flags(), &mut bytes);
            typed(&mut terminal, &bytes);
        }
        self.write(bytes);
    }

    /// Sends `text` as it is, its control characters too, as keys that
    /// typed it would, and brings the view back to the screen.
    pub fn send(&self, text: &str) {
        let bytes = text.as_bytes().to_vec();
        typed(&mut self.lock(), &bytes);
        self.write(bytes);
    }

    /// Pastes `text`, in one entry of the queue, so that no reply comes
    /// between its brackets; the writer writes it in chunks. The view comes
    /// back to the screen.
    pub fn paste(&self, text: &str) {
        let mut bytes = Vec::new();
        {
            let mut terminal = self.lock();
            input::encode_paste(text, &terminal.input_modes(), &mut bytes);
            typed(&mut terminal, &bytes);
        }
        self.write(bytes);
    }

    /// Gives the terminal and the program `size`: the grid first, so that
    /// the program redraws, on SIGWINCH, at the new size.
    pub fn resize(&self, size: Size) -> io::Result<()> {
        self.lock().resize(size.cols, size.rows);
        self.shared.asked_now();
        self.pty.resize(size)
    }

    /// Tells the program that its terminal's size changed, through SIGWINCH,
    /// without changing the PTY's: for an embedder whose grid came back to
    /// the size that the program was told last, which the program must
    /// draw again, as the grid's prompt or screen changed meanwhile.
    pub fn signal_resize(&self) -> io::Result<()> {
        self.shared.asked_now();
        self.pty.signal_resize()
    }

    /// Tells the program `size` through the PTY, without resizing the grid,
    /// which the embedder resized first: one that resizes the grid at each
    /// step of a live resize tells the program at its own pace, so that it
    /// redraws a few times a second rather than at each step.
    pub fn resize_pty(&self, size: Size) -> io::Result<()> {
        self.shared.asked_now();
        self.pty.resize(size)
    }

    /// How many slices of output the I/O thread has parsed, as an embedder
    /// that waits for the program's answer to its new size counts them.
    pub fn slices(&self) -> u64 {
        self.shared.slices.load(Ordering::Acquire)
    }
}

/// Brings the view of `terminal` back to the screen for `bytes`, input
/// that the user sent, if it sends anything: a modifier alone sends
/// nothing, and leaves the view.
fn typed(terminal: &mut Terminal, bytes: &[u8]) {
    if !bytes.is_empty() {
        terminal.scroll_viewport(Scroll::Bottom);
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        // The writer stops; the program's hang-up, as the PTY drops, ends
        // the output, and with it the I/O thread.
        self.shared.queue.close();
    }
}

/// The I/O thread: parses the program's output, a slice at a time, until it
/// ends.
fn read_output(shared: &Shared, mut output: ReadAhead, sync_timeout: Duration) {
    let _ending = Ending(shared);
    let mut parser = Parser::new();
    // The synchronized update under way, by its number, and its deadline.
    let mut update = 0;
    let mut deadline = None;
    loop {
        // A wait past the deadline still takes what is there, so that output
        // that never stops never times a wait out: the deadline is looked
        // at before each slice, rather than left to the wait.
        if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
            deadline = None;
            let mut terminal = shared.terminal.lock_in_turn();
            let late = terminal.synchronized_output() && terminal.synchronized_updates() == update;
            if late {
                terminal.end_synchronized_output();
                drop(terminal);
                shared.changed(false);
            }
        }
        let slice = match output.next(deadline) {
            Next::Output(slice) => slice,
            Next::TimedOut => continue,
            Next::End => break,
        };
        // Output of the program's own, rather than its answer to input or
        // to a new size; its first since the main thread found it quiet
        // tells it.
        let came = Instant::now();
        let own = shared
            .instant(shared.asked.load(Ordering::SeqCst))
            .is_none_or(|asked| came.saturating_duration_since(asked) >= ECHO);
        let started = own && {
            shared.printed.store(shared.stamp(came), Ordering::SeqCst);
            !shared.printed_woken.swap(true, Ordering::SeqCst)
        };
        let n = slice.len();
        let mut terminal = shared.terminal.lock_in_turn();
        let bells = terminal.bells();
        parser.advance(&mut *terminal, &slice);
        // Its buffer goes back to the reader.
        drop(slice);
        let told = started || terminal.has_events() || terminal.bells() != bells;
        if terminal.output_scrolls_view() {
            terminal.scroll_viewport(Scroll::Bottom);
        }
        let replies = terminal.take_replies();
        let synchronized = terminal
            .synchronized_output()
            .then(|| terminal.synchronized_updates());
        shared.slices.fetch_add(1, Ordering::AcqRel);
        drop(terminal);
        shared.bytes.fetch_add(n as u64, Ordering::Relaxed);
        shared.queue.push_replies(replies);
        // Each update that starts has its own deadline, which output does
        // not put off.
        if let Some(started) = synchronized.filter(|&started| started != update) {
            update = started;
            deadline = Some(Instant::now() + sync_timeout);
        }
        shared.changed(told);
    }
}

/// Ends the session as the I/O thread ends, as the output does or by a
/// panic, so that a pane whose thread panicked does not hang without a
/// word: the update under way ends, and the main thread is told.
struct Ending<'a>(&'a Shared);

impl Drop for Ending<'_> {
    fn drop(&mut self) {
        // No more output can end the update under way.
        self.0.terminal.lock_in_turn().end_synchronized_output();
        self.0.ended.store(true, Ordering::Release);
        self.0.grid_woken.store(true, Ordering::Release);
        self.0.told_woken.store(true, Ordering::Release);
        if thread::panicking() {
            // A wake that panics again, as the thread unwinds, would abort
            // the app.
            let _ = std::panic::catch_unwind(AssertUnwindSafe(|| (self.0.wake)()));
        } else {
            (self.0.wake)();
        }
    }
}

/// The writer thread: writes what the queue holds, in order, each entry in
/// writes of [`PASTE_CHUNK`] bytes at most, so that it stops between two
/// of them once the session closes.
fn write_input(shared: &Shared, mut writer: PtyWriter) {
    while let Some((bytes, reply)) = shared.queue.next() {
        for chunk in bytes.chunks(PASTE_CHUNK) {
            if shared.queue.is_closed() {
                return;
            }
            // The owner's input, whose echo, which the terminal's line
            // discipline may give as it is written, is not the program's
            // own output; the replies to the program's queries are not
            // asked.
            if !reply {
                shared.asked_now();
            }
            // The program has gone, or the PTY has: what follows has no one
            // to read it.
            if writer.write_all(chunk).is_err() {
                shared.queue.close();
                return;
            }
            if !reply {
                shared.asked_now();
            }
        }
    }
}

#[cfg(test)]
mod tests;
