//! A headless terminal: a program in a pseudo-terminal, with the VT core
//! as its terminal, which parses what the program writes, sends the
//! terminal's replies back, and types keys as a test or a script says. The
//! recordings are made with it, and test suites that drive a program, as
//! esctest and vttest do, run in it.

use std::collections::VecDeque;
use std::io::ErrorKind;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
pub use bolt_pty::Exit;
use bolt_pty::{Program, Pty, PtyReader, PtyWriter, ReadOutcome, Size};
use bolt_vt_core::page::RowFlag;
use bolt_vt_core::parser::Parser;
use bolt_vt_core::terminal::{self, Options, Terminal};

/// How a harness starts its program.
#[derive(Clone, Debug)]
pub struct HarnessOptions {
    /// The program and its arguments, found in `PATH` as `env` finds it.
    pub program: Vec<String>,
    /// The program's whole environment, but for `TERM`: nothing else is
    /// inherited.
    pub env: Vec<(String, String)>,
    /// The terminal's options, whose `term` is the `TERM` that the program
    /// sees, and that XTGETTCAP reports.
    pub terminal: Options,
    /// The directory that the program starts in.
    pub dir: PathBuf,
    /// The terminal's columns.
    pub cols: u16,
    /// The terminal's rows.
    pub rows: u16,
}

/// What happened, as a recording keeps it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    /// The program wrote these bytes.
    Output(Vec<u8>),
    /// These bytes were typed into the program, keys or the terminal's
    /// replies, as they were written to it.
    Input(Vec<u8>),
    /// A marker, for tests to look at the screen there.
    Mark(String),
}

/// A program running in a headless terminal.
pub struct Harness {
    /// The terminal's PTY, until it hangs up.
    pty: Option<Pty>,
    reader: PtyReader,
    writer: PtyWriter,
    terminal: Terminal,
    parser: Parser,
    buffer: Vec<u8>,
    /// What waits to be written to the program, oldest first.
    pending: VecDeque<Pending>,
    /// The bytes of the replies that wait.
    reply_bytes: usize,
    /// The id of the next bytes to wait.
    next_id: u64,
    /// The id of the last bytes written whole: those before it were too.
    written: u64,
    /// How many replies were dropped, past [`PENDING`].
    dropped: usize,
    /// Whether the program can no longer be written to, as it has ended or
    /// has been hung up.
    closed: bool,
    started: Instant,
    events: Vec<(Duration, Event)>,
    /// Texts that the screen must never show.
    forbidden: Vec<String>,
}

/// Bytes that wait to be written: keys that a test typed, or the
/// terminal's replies.
struct Pending {
    id: u64,
    /// What is left to write.
    bytes: Vec<u8>,
    reply: bool,
}

/// What a read found.
enum Read {
    Output,
    TimedOut,
    End,
}

/// How long a read or a write waits at most while the other has to go on.
const TURN: Duration = Duration::from_millis(10);

/// How long a marker waits at most for an update under way to end.
const UPDATE: Duration = Duration::from_secs(10);

/// How many bytes of the terminal's replies wait to be written at most:
/// past them, the replies of a read are dropped, and counted, as a program
/// that floods queries and never reads its input would grow them without
/// end. Typed keys always wait, and do not count.
const PENDING: usize = 1 << 20;

impl Harness {
    /// Starts the program of `options` in a new terminal.
    pub fn spawn(options: &HarnessOptions) -> Result<Self> {
        let term = &options.terminal.term;
        let mut args = vec!["-i".to_owned(), format!("TERM={term}")];
        args.extend(
            options
                .env
                .iter()
                .map(|(name, value)| format!("{name}={value}")),
        );
        args.extend(options.program.iter().cloned());
        // The terminal first, which clamps the size, for the PTY to have the
        // same.
        let terminal = Terminal::new(options.cols, options.rows, options.terminal.clone());
        let mut pty_options = bolt_pty::Options::new(Program::command("/usr/bin/env", args), term);
        pty_options.working_directory = Some(options.dir.clone());
        pty_options.size = size(&terminal);
        let pty = bolt_pty::spawn(&pty_options).context("starting the program")?;
        Ok(Self {
            reader: pty.reader()?,
            writer: pty.writer()?,
            pty: Some(pty),
            terminal,
            parser: Parser::new(),
            buffer: vec![0; 64 * 1024],
            pending: VecDeque::new(),
            reply_bytes: 0,
            next_id: 1,
            written: 0,
            dropped: 0,
            closed: false,
            started: Instant::now(),
            events: Vec::new(),
            forbidden: Vec::new(),
        })
    }

    /// Makes each read fail once the screen shows one of `texts`.
    pub fn forbid(&mut self, texts: Vec<String>) {
        self.forbidden = texts;
    }

    /// Waits until a row of the screen shows `text`, with no synchronized
    /// update under way, for `limit` at most.
    pub fn wait(&mut self, text: &str, limit: Duration) -> Result<()> {
        let deadline = Instant::now() + limit;
        while !self.shows(text) || self.terminal.synchronized_output() {
            match self.read(deadline)? {
                Read::Output => {}
                Read::TimedOut => bail!("`{text}` did not show in {limit:?}:\n{}", self.screen()),
                Read::End if self.shows(text) => {
                    bail!("the program ended inside a synchronized update, with `{text}` shown")
                }
                Read::End => bail!("the program ended before `{text}` showed"),
            }
        }
        Ok(())
    }

    /// Waits until the program writes nothing for `time`, with no
    /// synchronized update under way, for `limit` at most.
    pub fn quiet(&mut self, time: Duration, limit: Duration) -> Result<()> {
        let deadline = Instant::now() + limit;
        loop {
            match self.read(Instant::now() + time)? {
                Read::End => return Ok(()),
                Read::TimedOut if !self.terminal.synchronized_output() => return Ok(()),
                Read::Output | Read::TimedOut => {}
            }
            if Instant::now() >= deadline {
                bail!(
                    "the program did not go quiet in {limit:?}:\n{}",
                    self.screen()
                );
            }
        }
    }

    /// Lets the program run for `time`, as one that redraws all the time is
    /// never quiet; it ends with the time, inside a synchronized update or
    /// not.
    pub fn sleep(&mut self, time: Duration) -> Result<()> {
        let deadline = Instant::now() + time;
        while let Read::Output = self.read(deadline)? {}
        Ok(())
    }

    /// Types `bytes`, once the program takes them, for `limit` at most,
    /// reading what it writes meanwhile; it ends once they are written,
    /// inside a synchronized update or not, even if the program ends just
    /// after. If the limit comes first, what of them was not written is
    /// taken back, rather than left for the next step to type.
    pub fn send(&mut self, bytes: &[u8], limit: Duration) -> Result<()> {
        if self.closed {
            bail!("the program ended before it took what was typed");
        }
        if bytes.is_empty() {
            return Ok(());
        }
        let id = self.queue(bytes.to_vec(), false);
        let deadline = Instant::now() + limit;
        loop {
            self.write_some(Instant::now() + TURN)?;
            if self.written >= id {
                return Ok(());
            }
            if self.closed {
                bail!("the program ended before it took what was typed");
            }
            if Instant::now() >= deadline {
                self.take_back(id);
                bail!("the program did not take what was typed in {limit:?}");
            }
            self.read(Instant::now() + TURN)?;
        }
    }

    /// Marks the moment, once no synchronized update is under way.
    pub fn mark(&mut self, label: &str) -> Result<()> {
        let deadline = Instant::now() + UPDATE;
        while self.terminal.synchronized_output() {
            match self.read(deadline)? {
                Read::Output => {}
                Read::TimedOut => bail!("an update did not end in {UPDATE:?} for `{label}`"),
                Read::End => break,
            }
        }
        self.event(Event::Mark(label.to_owned()));
        Ok(())
    }

    /// Waits for the program's output to end, for `limit` at most, inside
    /// a synchronized update or not.
    pub fn exit(&mut self, limit: Duration) -> Result<()> {
        let deadline = Instant::now() + limit;
        loop {
            match self.read(deadline)? {
                Read::Output => {}
                Read::TimedOut => {
                    bail!("the program did not end in {limit:?}:\n{}", self.screen())
                }
                Read::End => return Ok(()),
            }
        }
    }

    /// How the program ended, if it has, without waiting for it or reading
    /// what it writes: `None` while it runs. On macOS, a program that ends
    /// with output unread is not gone until the output is read, as closing
    /// a terminal waits for its output to drain.
    pub fn try_exit_status(&mut self) -> Result<Option<Exit>> {
        let Some(pty) = &mut self.pty else {
            bail!("the program was hung up, which leaves how it ended untold");
        };
        pty.try_wait().context("waiting for the program")
    }

    /// How the program ended, once it has, for `limit` at most, reading
    /// what it writes meanwhile: its output can end before it does, and
    /// it can end while processes that it started still write.
    pub fn exit_status(&mut self, limit: Duration) -> Result<Exit> {
        let deadline = Instant::now() + limit;
        loop {
            let Some(pty) = &mut self.pty else {
                bail!("the program was hung up, which leaves how it ended untold");
            };
            if let Some(exit) = pty.try_wait().context("waiting for the program")? {
                return Ok(exit);
            }
            if Instant::now() >= deadline {
                bail!("the program did not end in {limit:?}:\n{}", self.screen());
            }
            if let Read::End = self.read(deadline.min(Instant::now() + TURN))? {
                std::thread::sleep(Duration::from_millis(1));
            }
        }
    }

    /// Hangs the program up, as closing the terminal does, if it has not
    /// ended; what it showed stays to be looked at, and what waited to be
    /// written goes.
    pub fn hang_up(&mut self) {
        self.pty = None;
        self.close();
    }

    /// Resizes the terminal and tells the program, as a window that
    /// changes size does, with the size that the terminal takes, in cells
    /// and, once a cell's size is set, in pixels.
    pub fn resize(&mut self, cols: u16, rows: u16) -> Result<()> {
        self.terminal.resize(cols, rows);
        if let Some(pty) = &self.pty {
            pty.resize(size(&self.terminal))
                .context("resizing the terminal")?;
        }
        Ok(())
    }

    /// Sets the size of a cell in pixels, which the terminal reports, and
    /// tells the program the terminal's size in pixels.
    pub fn set_cell_size(&mut self, width: u16, height: u16) -> Result<()> {
        self.terminal.set_cell_size(width, height);
        let (cols, rows) = (self.terminal.cols(), self.terminal.rows());
        self.resize(cols, rows)
    }

    /// Runs `work` while another thread serves the program, reading what it
    /// writes, showing it and sending the terminal's replies, so that a
    /// program that waits for a reply gets it while the test does something
    /// else than drive the harness, which alone reads the program's output
    /// otherwise. Fails, once `work` is done, if a read did.
    pub fn serve<T>(&mut self, work: impl FnOnce() -> T) -> Result<T> {
        /// Stops the thread that serves as it drops, `work` having ended or
        /// panicked.
        struct Stop<'a>(&'a AtomicBool);
        impl Drop for Stop<'_> {
            fn drop(&mut self) {
                self.0.store(true, Ordering::Release);
            }
        }
        let stop = AtomicBool::new(false);
        std::thread::scope(|scope| {
            let server = scope.spawn(|| -> Result<()> {
                while !stop.load(Ordering::Acquire) {
                    if let Read::End = self.read(Instant::now() + TURN)? {
                        break;
                    }
                }
                Ok(())
            });
            let done = {
                let _stop = Stop(&stop);
                work()
            };
            match server.join() {
                Ok(served) => served.map(|()| done),
                Err(panic) => std::panic::resume_unwind(panic),
            }
        })
    }

    /// The terminal.
    pub fn terminal(&self) -> &Terminal {
        &self.terminal
    }

    /// The terminal, for what the harness does not do itself; a resize and
    /// a cell's size go through [`Harness::resize`] and
    /// [`Harness::set_cell_size`], which tell the program too, and the
    /// replies are the harness's to take.
    pub fn terminal_mut(&mut self) -> &mut Terminal {
        &mut self.terminal
    }

    /// Takes the terminal's events, those that an embedder acts on, oldest
    /// first: the title's changes, the clipboard's, notifications.
    pub fn take_terminal_events(&mut self) -> Vec<terminal::Event> {
        self.terminal.take_events()
    }

    /// Whether a row of the screen shows `text`.
    pub fn shows(&self, text: &str) -> bool {
        (0..self.terminal.rows()).any(|y| self.terminal.row_text(y).contains(text))
    }

    /// The screen's rows, a line each.
    pub fn screen(&self) -> String {
        (0..self.terminal.rows())
            .map(|y| self.terminal.row_text(y))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// The screen's lines, a line each, a row that soft-wraps into the next
    /// joined to it.
    fn lines(&self) -> String {
        let list = self.terminal.screen_rows();
        let flag = |y, flag| {
            let (page, row) = list.screen_row(y);
            page.row(row).has(flag)
        };
        let rows = self.terminal.rows();
        let mut text = String::new();
        for y in 0..rows {
            text.push_str(&self.terminal.row_text(y));
            let goes_on =
                y + 1 < rows && flag(y, RowFlag::SoftWrapped) && flag(y + 1, RowFlag::Continuation);
            if !goes_on {
                text.push('\n');
            }
        }
        text
    }

    /// Everything that the program wrote.
    pub fn output(&self) -> Vec<u8> {
        self.events
            .iter()
            .filter_map(|(_, event)| match event {
                Event::Output(bytes) => Some(&bytes[..]),
                _ => None,
            })
            .collect::<Vec<_>>()
            .concat()
    }

    /// What happened, each with how long after the start.
    pub fn events(&self) -> &[(Duration, Event)] {
        &self.events
    }

    /// How many reads' replies were dropped, as a MiB of replies waited for
    /// a program that did not read them.
    pub fn dropped_replies(&self) -> usize {
        self.dropped
    }

    fn event(&mut self, event: Event) {
        self.events.push((self.started.elapsed(), event));
    }

    /// Ends the writes, as the program can take no more.
    fn close(&mut self) {
        self.closed = true;
        self.pending = VecDeque::new();
        self.reply_bytes = 0;
    }

    /// Queues `bytes` to be written, and returns their id.
    fn queue(&mut self, bytes: Vec<u8>, reply: bool) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        if reply {
            self.reply_bytes += bytes.len();
        }
        self.pending.push_back(Pending { id, bytes, reply });
        id
    }

    /// Takes back what of the bytes with `id` waits still.
    fn take_back(&mut self, id: u64) {
        self.pending.retain(|pending| pending.id != id);
    }

    /// Reads what the program writes, until `deadline` at most: shows it on
    /// the terminal, and queues the terminal's replies, which go as soon as
    /// the program has room for them. Once the deadline has passed, it
    /// times out without reading, as a read past it would take what is
    /// there, and a program that writes all the time would keep the steps
    /// from ever timing out.
    fn read(&mut self, deadline: Instant) -> Result<Read> {
        let n = loop {
            let now = Instant::now();
            if now >= deadline {
                return Ok(Read::TimedOut);
            }
            self.write_some(now + Duration::from_millis(1))?;
            // While bytes wait, the read gives way to the writes now and
            // then, as the program can make room for them without writing.
            let until = if self.pending.is_empty() {
                deadline
            } else {
                deadline.min(Instant::now() + TURN)
            };
            match self.reader.read_until(&mut self.buffer, Some(until))? {
                ReadOutcome::Read(n) => break n,
                ReadOutcome::TimedOut => {}
                ReadOutcome::End => {
                    self.close();
                    return Ok(Read::End);
                }
            }
        };
        let bytes = self.buffer[..n].to_vec();
        self.parser.advance(&mut self.terminal, &bytes);
        self.event(Event::Output(bytes));
        let replies = self.terminal.take_replies();
        if !replies.is_empty() && !self.closed {
            if self.reply_bytes + replies.len() <= PENDING {
                self.queue(replies, true);
            } else {
                self.dropped += 1;
            }
        }
        if self.forbidden.is_empty() {
            return Ok(Read::Output);
        }
        // What the screen shows now, which the next output can take away,
        // its rows that soft-wrap joined to the next.
        let lines = self.lines();
        if let Some(text) = self
            .forbidden
            .iter()
            .find(|text| lines.contains(text.as_str()))
        {
            bail!(
                "the program showed `{text}`, which a recording must not keep:\n{}",
                self.screen()
            );
        }
        Ok(Read::Output)
    }

    /// Writes what of the first bytes that wait the program has room for,
    /// by `deadline`, and keeps them as input. A program that has ended, so
    /// that the write fails with EIO, or that has been hung up, so that it
    /// fails with a broken pipe, takes nothing more: what waits goes.
    fn write_some(&mut self, deadline: Instant) -> Result<()> {
        let Some(first) = self.pending.front_mut() else {
            return Ok(());
        };
        match self.writer.write_until(&first.bytes, Some(deadline)) {
            Ok(Some(written @ 1..)) => {
                let bytes: Vec<u8> = first.bytes.drain(..written).collect();
                if first.reply {
                    self.reply_bytes -= written;
                }
                if first.bytes.is_empty() {
                    self.written = first.id;
                    self.pending.pop_front();
                }
                self.event(Event::Input(bytes));
            }
            Ok(_) => {}
            Err(error)
                if error.kind() == ErrorKind::BrokenPipe
                    || error.raw_os_error() == Some(libc::EIO) =>
            {
                self.close();
            }
            Err(error) => return Err(error).context("writing to the program"),
        }
        Ok(())
    }
}

/// The size of `terminal`, for its PTY: in cells, and in pixels once a
/// cell's size is set.
fn size(terminal: &Terminal) -> Size {
    let (cols, rows) = (terminal.cols(), terminal.rows());
    let (width, height) = terminal.cell_size().unwrap_or((0, 0));
    Size {
        cols,
        rows,
        width: cols.saturating_mul(width),
        height: rows.saturating_mul(height),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_replies_that_wait_are_bounded() {
        // Queries of 1.8 MB of replies, which the program never reads.
        let script =
            r#"stty raw -echo; yes "$(printf '\033[c')" | head -n 200000; echo done; sleep 30"#;
        let options = HarnessOptions {
            program: vec!["sh".to_owned(), "-c".to_owned(), script.to_owned()],
            env: vec![("PATH".to_owned(), "/usr/bin:/bin".to_owned())],
            terminal: Options::default(),
            dir: PathBuf::from("/"),
            cols: 20,
            rows: 3,
        };
        let mut harness = Harness::spawn(&options).unwrap();
        harness.wait("done", Duration::from_secs(30)).unwrap();
        let waiting = harness.reply_bytes;
        let held: usize = harness
            .pending
            .iter()
            .map(|pending| pending.bytes.len())
            .sum();
        let dropped = harness.dropped_replies();
        harness.hang_up();
        assert!(waiting > 0, "replies wait");
        assert!(waiting <= PENDING, "{waiting} bytes wait");
        assert_eq!(held, waiting, "replies alone");
        assert!(dropped > 0, "and some were dropped");
    }

    #[test]
    fn a_reply_to_a_program_that_has_ended_goes_with_the_writes() {
        let options = HarnessOptions {
            program: vec![
                "sh".to_owned(),
                "-c".to_owned(),
                r"printf 'bye\033[c'".to_owned(),
            ],
            env: vec![("PATH".to_owned(), "/usr/bin:/bin".to_owned())],
            terminal: Options::default(),
            dir: PathBuf::from("/"),
            cols: 20,
            rows: 3,
        };
        let mut harness = Harness::spawn(&options).unwrap();
        // The output read to its end without the harness, which would write
        // the reply, and the program gone.
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut output = Vec::new();
        while let ReadOutcome::Read(n) = harness
            .reader
            .read_until(&mut harness.buffer, Some(deadline))
            .unwrap()
        {
            output.extend_from_slice(&harness.buffer[..n]);
        }
        assert_eq!(output, b"bye\x1b[c");
        while harness.try_exit_status().unwrap().is_none() {
            assert!(Instant::now() < deadline, "the program did not end");
            std::thread::sleep(Duration::from_millis(10));
        }
        harness.queue(b"\x1b[?62;22c".to_vec(), true);
        harness.exit(Duration::from_secs(10)).unwrap();
        harness.sleep(Duration::from_millis(10)).unwrap();
        assert!(
            !harness
                .events()
                .iter()
                .any(|(_, event)| matches!(event, Event::Input(_))),
            "the reply was never written"
        );
        let error = harness.send(b"x", Duration::from_secs(1)).unwrap_err();
        assert!(error.to_string().contains("ended before"), "{error:#}");
    }

    #[test]
    fn the_pty_has_the_terminals_size_in_cells_and_pixels() {
        let options = HarnessOptions {
            program: vec!["sleep".to_owned(), "30".to_owned()],
            env: vec![("PATH".to_owned(), "/usr/bin:/bin".to_owned())],
            terminal: Options::default(),
            dir: PathBuf::from("/"),
            cols: 1,
            rows: 3,
        };
        let mut harness = Harness::spawn(&options).unwrap();
        let pty_size = |harness: &Harness| {
            let size = harness.pty.as_ref().unwrap().size().unwrap();
            (size.cols, size.rows, size.width, size.height)
        };
        // The terminal has two columns at least.
        assert_eq!(pty_size(&harness), (2, 3, 0, 0));
        harness.set_cell_size(8, 16).unwrap();
        assert_eq!(pty_size(&harness), (2, 3, 16, 48));
        harness.resize(10, 4).unwrap();
        assert_eq!(pty_size(&harness), (10, 4, 80, 64));
        harness.hang_up();
    }
}
