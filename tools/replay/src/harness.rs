//! A headless terminal: a program in a pseudo-terminal, with the VT core
//! as its terminal, which parses what the program writes, sends the
//! terminal's replies back, and types keys as a test or a script says. The
//! recordings are made with it, and test suites that drive a program, as
//! esctest and vttest do, run in it.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use bolt_pty::{Program, Pty, PtyReader, PtyWriter, ReadOutcome, Size};
use bolt_vt_core::parser::Parser;
use bolt_vt_core::terminal::{Options, Terminal};

/// How a harness starts its program.
#[derive(Clone, Debug)]
pub struct HarnessOptions {
    /// The program and its arguments, found in `PATH` as `env` finds it.
    pub program: Vec<String>,
    /// The program's whole environment, but for `TERM`: nothing else is
    /// inherited.
    pub env: Vec<(String, String)>,
    /// The `TERM` that the program sees, and that XTGETTCAP reports.
    pub term: String,
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
    /// These bytes were typed into the program: keys, or the terminal's
    /// replies.
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
    /// What waits to be written to the program.
    pending: Vec<u8>,
    /// Everything that the program wrote.
    output: Vec<u8>,
    started: Instant,
    events: Vec<(Duration, Event)>,
    /// Texts that the screen must never show.
    forbidden: Vec<String>,
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

impl Harness {
    /// Starts the program of `options` in a new terminal.
    pub fn spawn(options: &HarnessOptions) -> Result<Self> {
        let mut args = vec!["-i".to_owned(), format!("TERM={}", options.term)];
        args.extend(
            options
                .env
                .iter()
                .map(|(name, value)| format!("{name}={value}")),
        );
        args.extend(options.program.iter().cloned());
        let mut pty_options =
            bolt_pty::Options::new(Program::command("/usr/bin/env", args), &options.term);
        pty_options.working_directory = Some(options.dir.clone());
        pty_options.size = Size {
            cols: options.cols,
            rows: options.rows,
            width: 0,
            height: 0,
        };
        let pty = bolt_pty::spawn(&pty_options).context("starting the program")?;
        let terminal = Options {
            term: options.term.clone(),
            ..Options::default()
        };
        Ok(Self {
            reader: pty.reader()?,
            writer: pty.writer()?,
            pty: Some(pty),
            terminal: Terminal::new(options.cols, options.rows, terminal),
            parser: Parser::new(),
            buffer: vec![0; 64 * 1024],
            pending: Vec::new(),
            output: Vec::new(),
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
    /// never quiet.
    pub fn sleep(&mut self, time: Duration) -> Result<()> {
        let deadline = Instant::now() + time;
        while let Read::Output = self.read(deadline)? {}
        Ok(())
    }

    /// Types `bytes`, once the program takes them, for `limit` at most,
    /// reading what it writes meanwhile.
    pub fn send(&mut self, bytes: &[u8], limit: Duration) -> Result<()> {
        self.event(Event::Input(bytes.to_vec()));
        self.pending.extend_from_slice(bytes);
        self.flush(limit)
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

    /// Waits for the program's output to end, for `limit` at most.
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

    /// Hangs the program up, as closing the terminal does, if it has not
    /// ended; what it showed stays to be looked at.
    pub fn hang_up(&mut self) {
        self.pty = None;
    }

    /// The terminal.
    pub fn terminal(&self) -> &Terminal {
        &self.terminal
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

    /// Everything that the program wrote.
    pub fn output(&self) -> &[u8] {
        &self.output
    }

    /// What happened, each with how long after the start.
    pub fn events(&self) -> &[(Duration, Event)] {
        &self.events
    }

    fn event(&mut self, event: Event) {
        self.events.push((self.started.elapsed(), event));
    }

    /// Reads what the program writes, until `deadline` at most: keeps it,
    /// shows it on the terminal, and queues the terminal's replies, which
    /// go as soon as the program has room for them.
    fn read(&mut self, deadline: Instant) -> Result<Read> {
        self.write_some(Instant::now() + Duration::from_millis(1))?;
        let n = match self.reader.read_until(&mut self.buffer, Some(deadline))? {
            ReadOutcome::Read(n) => n,
            ReadOutcome::TimedOut => return Ok(Read::TimedOut),
            ReadOutcome::End => return Ok(Read::End),
        };
        let bytes = self.buffer[..n].to_vec();
        self.output.extend_from_slice(&bytes);
        self.parser.advance(&mut self.terminal, &bytes);
        self.event(Event::Output(bytes));
        let replies = self.terminal.take_replies();
        if !replies.is_empty() {
            self.pending.extend_from_slice(&replies);
            self.event(Event::Input(replies));
        }
        // What the screen shows now, which the next output can take away.
        let screen = self.screen();
        if let Some(text) = self
            .forbidden
            .iter()
            .find(|text| screen.contains(text.as_str()))
        {
            bail!("the program showed `{text}`, which a recording must not keep:\n{screen}");
        }
        Ok(Read::Output)
    }

    /// Writes what waits, reading the output meanwhile, as a program can
    /// wait to write before it reads more; for `limit` at most.
    fn flush(&mut self, limit: Duration) -> Result<()> {
        let deadline = Instant::now() + limit;
        while !self.pending.is_empty() {
            self.write_some(Instant::now() + TURN)?;
            if self.pending.is_empty() {
                break;
            }
            if Instant::now() >= deadline {
                bail!("the program did not take what was typed in {limit:?}");
            }
            if let Read::End = self.read(Instant::now() + TURN)? {
                bail!("the program ended before it took what was typed");
            }
        }
        Ok(())
    }

    /// Writes what of the pending bytes the program has room for, by
    /// `deadline`.
    fn write_some(&mut self, deadline: Instant) -> Result<()> {
        if self.pending.is_empty() {
            return Ok(());
        }
        let written = self
            .writer
            .write_until(&self.pending, Some(deadline))
            .context("writing to the program")?;
        self.pending.drain(..written.unwrap_or(0));
        Ok(())
    }
}
