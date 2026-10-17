//! `bolt-record`: runs a program in a pseudo-terminal, as a script drives
//! it, and writes what the program shows as an asciicast, for tests to
//! replay.
//!
//! `bolt-record SCRIPT...` writes each script's recording next to it, with
//! the extension `cast`. The program sees a terminal as the session gives
//! it, which answers its queries; it runs with no environment but the one
//! that every recording has and the script's, in a directory of its own,
//! which is also its home, so that nothing of the machine shows in the
//! recording. A recording that shows the user's name, the host's or the
//! user's home is not written.

mod cast;
mod script;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use bolt_pty::{Program, PtyReader, PtyWriter, ReadOutcome, Size};
use bolt_vt_core::page::Content;
use bolt_vt_core::parser::Parser;
use bolt_vt_core::terminal::{Options, Terminal};

use cast::Cast;
use script::{Script, Step};

/// The `TERM` of every recording, the session's default.
const TERM: &str = "xterm-256color";

/// Where the programs are found.
const PATH: &str = "/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin";

fn main() -> ExitCode {
    let scripts: Vec<String> = std::env::args().skip(1).collect();
    if scripts.is_empty() || scripts.iter().any(|arg| arg == "-h" || arg == "--help") {
        println!(
            "Usage: bolt-record SCRIPT...\n\n\
             Runs the program of each script in a pseudo-terminal, as the script drives it,\n\
             and writes what it shows next to the script, as an asciicast (.cast)."
        );
        return if scripts.is_empty() {
            ExitCode::FAILURE
        } else {
            ExitCode::SUCCESS
        };
    }
    for path in &scripts {
        match record(Path::new(path)) {
            Ok(out) => println!("recorded {}", out.display()),
            Err(error) => {
                eprintln!("error: {path}: {error:#}");
                return ExitCode::FAILURE;
            }
        }
    }
    ExitCode::SUCCESS
}

/// Records the script at `path`, and returns where the recording went.
fn record(path: &Path) -> Result<PathBuf> {
    let script = Script::parse(&fs::read_to_string(path).context("reading the script")?)?;
    let name = path
        .file_stem()
        .context("a script has a name")?
        .to_string_lossy();
    remove_stale_homes();
    let home = Home::new(&name)?;
    let cast = run(&script, path, &home.0)?;
    let out = path.with_extension("cast");
    fs::write(&out, cast.finish()).with_context(|| format!("writing {}", out.display()))?;
    Ok(out)
}

/// The directory that a program runs in, which is also its home, removed
/// as it drops, however the recording ends.
struct Home(PathBuf);

/// What the homes' names start with, before the script's name and the
/// recorder's process id.
const HOME_PREFIX: &str = "bolt-record-";

impl Home {
    fn new(name: &str) -> Result<Self> {
        let path = std::env::temp_dir().join(format!("{HOME_PREFIX}{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).with_context(|| format!("creating {}", path.display()))?;
        Ok(Self(path))
    }
}

impl Drop for Home {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Removes the homes that recorders that were killed left, those whose
/// process has gone.
fn remove_stale_homes() {
    let Ok(entries) = fs::read_dir(std::env::temp_dir()) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let Some(pid) = name
            .strip_prefix(HOME_PREFIX)
            .and_then(|rest| rest.rsplit('-').next())
            .and_then(|pid| pid.parse::<u32>().ok())
        else {
            continue;
        };
        let alive = Command::new("kill")
            .args(["-0", &pid.to_string()])
            .stderr(std::process::Stdio::null())
            .status()
            .is_ok_and(|status| status.success());
        if !alive {
            let _ = fs::remove_dir_all(entry.path());
        }
    }
}

/// Runs the script's program in `home`, as the script says, and returns
/// what it recorded, once nothing private shows in it.
fn run(script: &Script, path: &Path, home: &Path) -> Result<Cast> {
    let dir = path.parent().unwrap_or(Path::new("."));
    for copy in &script.copies {
        let from = dir.join(copy);
        let name = from.file_name().context("a file to copy")?;
        fs::copy(&from, home.join(name)).with_context(|| format!("copying {}", from.display()))?;
    }
    let mut args = vec![
        "-i".to_owned(),
        format!("HOME={}", home.display()),
        format!("PATH={PATH}"),
        format!("TERM={TERM}"),
        "LANG=en_US.UTF-8".to_owned(),
        "COLORTERM=truecolor".to_owned(),
    ];
    args.extend(
        script
            .env
            .iter()
            .map(|(name, value)| format!("{name}={value}")),
    );
    args.extend(script.run.iter().cloned());
    let mut options = bolt_pty::Options::new(Program::command("/usr/bin/env", args), TERM);
    options.working_directory = Some(home.to_owned());
    options.size = Size {
        cols: script.cols,
        rows: script.rows,
        width: 0,
        height: 0,
    };
    let pty = bolt_pty::spawn(&options).context("starting the program")?;
    let terminal = Options {
        term: TERM.to_owned(),
        ..Options::default()
    };
    let mut recorder = Recorder {
        reader: pty.reader()?,
        writer: pty.writer()?,
        terminal: Terminal::new(script.cols, script.rows, terminal),
        parser: Parser::new(),
        cast: Cast::new(script.cols, script.rows, TERM),
        buffer: vec![0; 64 * 1024],
        pending: Vec::new(),
        output: Vec::new(),
        private: private_names(home),
    };
    for step in &script.steps {
        recorder.step(step)?;
    }
    // A program that has not ended is hung up as the terminal closes.
    drop(pty);
    recorder.check_all()?;
    Ok(recorder.cast)
}

/// What the program and the terminal do together, as the recording keeps
/// it.
struct Recorder {
    reader: PtyReader,
    writer: PtyWriter,
    terminal: Terminal,
    parser: Parser,
    cast: Cast,
    buffer: Vec<u8>,
    /// What waits to be written to the program: keys, and the terminal's
    /// replies.
    pending: Vec<u8>,
    /// Everything that the program wrote.
    output: Vec<u8>,
    /// What must not show: the user's name, the host's, the user's home,
    /// the program's.
    private: Vec<String>,
}

/// What a read found.
enum Read {
    Output,
    TimedOut,
    End,
}

/// How long a read or a write waits at most while the other has to go on.
const TURN: Duration = Duration::from_millis(10);

impl Recorder {
    fn step(&mut self, step: &Step) -> Result<()> {
        match step {
            Step::Wait(text, limit) => {
                let deadline = Instant::now() + *limit;
                while !self.shows(text) {
                    match self.read(deadline)? {
                        Read::Output => {}
                        Read::TimedOut => {
                            bail!("`{text}` did not show in {limit:?}:\n{}", self.screen())
                        }
                        Read::End => bail!("the program ended before `{text}` showed"),
                    }
                }
            }
            Step::Quiet(time, limit) => {
                let deadline = Instant::now() + *limit;
                while let Read::Output = self.read(Instant::now() + *time)? {
                    if Instant::now() >= deadline {
                        bail!(
                            "the program did not go quiet in {limit:?}:\n{}",
                            self.screen()
                        );
                    }
                }
            }
            Step::Sleep(time) => {
                let deadline = Instant::now() + *time;
                while let Read::Output = self.read(deadline)? {}
            }
            Step::Send(bytes, limit) => {
                self.cast.input(bytes);
                self.pending.extend_from_slice(bytes);
                self.flush(*limit)?;
            }
            Step::Mark(label) => self.cast.mark(label),
            Step::Exit(limit) => {
                let deadline = Instant::now() + *limit;
                loop {
                    match self.read(deadline)? {
                        Read::Output => {}
                        Read::TimedOut => {
                            bail!("the program did not end in {limit:?}:\n{}", self.screen())
                        }
                        Read::End => break,
                    }
                }
            }
        }
        Ok(())
    }

    /// Reads what the program writes, until `deadline` at most: records it,
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
        self.cast.output(&bytes);
        self.output.extend_from_slice(&bytes);
        self.parser.advance(&mut self.terminal, &bytes);
        let replies = self.terminal.take_replies();
        if !replies.is_empty() {
            self.cast.input(&replies);
            self.pending.extend_from_slice(&replies);
        }
        // What the screen shows now, which the next output can take away.
        let screen = self.screen();
        if let Some(name) = self
            .private
            .iter()
            .find(|name| screen.contains(name.as_str()))
        {
            bail!("the program showed `{name}`, which a recording must not keep:\n{screen}");
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

    /// Fails if the recording, whole, shows something private: in what the
    /// program wrote, without its escape sequences, so that a name that two
    /// reads split, or that an escape breaks, shows; and in every row that
    /// the terminal keeps, the scrollback's too.
    fn check_all(&self) -> Result<()> {
        let plain = plain(&self.output);
        let rows: Vec<String> = self
            .terminal
            .screen_rows()
            .rows_from(0)
            .map(|row| {
                row.cells()
                    .enumerate()
                    .map(|(x, cell)| {
                        let mut text = String::new();
                        if let Content::Char(c) | Content::Grapheme(c) = cell.content()
                            && c != '\0'
                        {
                            text.push(c);
                            // A row has fewer than 2^16 columns.
                            text.extend(row.grapheme(x as u16));
                        }
                        text
                    })
                    .collect()
            })
            .collect();
        for name in &self.private {
            if plain.contains(name.as_str()) || rows.iter().any(|row| row.contains(name.as_str())) {
                bail!("the program showed `{name}`, which a recording must not keep");
            }
        }
        Ok(())
    }

    fn shows(&self, text: &str) -> bool {
        (0..self.terminal.rows()).any(|y| self.terminal.row_text(y).contains(text))
    }

    fn screen(&self) -> String {
        (0..self.terminal.rows())
            .map(|y| self.terminal.row_text(y))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// The text of `bytes` without escape sequences and control characters,
/// but for the line breaks.
fn plain(bytes: &[u8]) -> String {
    let text = String::from_utf8_lossy(bytes);
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\x1b' => match chars.next() {
                // CSI: up to its final byte.
                Some('[') => {
                    for c in chars.by_ref() {
                        if ('\x40'..='\x7e').contains(&c) {
                            break;
                        }
                    }
                }
                // OSC, DCS, APC, PM and SOS: up to BEL or ST.
                Some(']' | 'P' | '_' | '^' | 'X') => {
                    while let Some(c) = chars.next() {
                        if c == '\x07' || (c == '\x1b' && chars.next_if_eq(&'\\').is_some()) {
                            break;
                        }
                    }
                }
                _ => {}
            },
            '\n' => out.push('\n'),
            c if c.is_control() => {}
            c => out.push(c),
        }
    }
    out
}

/// The names that a recording must not show: the user's, the user's full
/// name, the host's and the computer's, the user's home, and the program's
/// own, `home`, which is the machine's.
fn private_names(home: &Path) -> Vec<String> {
    let mut names: Vec<String> = ["USER", "LOGNAME", "HOME"]
        .into_iter()
        .filter_map(|name| std::env::var(name).ok())
        .collect();
    let commands: [(&str, &[&str]); 5] = [
        ("hostname", &["-s"]),
        ("hostname", &[]),
        ("scutil", &["--get", "ComputerName"]),
        ("scutil", &["--get", "LocalHostName"]),
        ("id", &["-F"]),
    ];
    for (program, args) in commands {
        if let Ok(output) = Command::new(program)
            .args(args)
            .stderr(std::process::Stdio::null())
            .output()
            && output.status.success()
        {
            names.push(String::from_utf8_lossy(&output.stdout).trim().to_owned());
        }
    }
    names.push(home.display().to_string());
    if let Ok(resolved) = home.canonicalize() {
        names.push(resolved.display().to_string());
    }
    names.retain(|name| name.len() >= 3);
    names.sort();
    names.dedup();
    names
}
