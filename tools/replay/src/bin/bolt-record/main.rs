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
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::time::Instant;

use anyhow::{Context, Result, bail};
use bolt_pty::{Program, PtyReader, PtyWriter, ReadOutcome, Size};
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
    let home = std::env::temp_dir().join(format!("bolt-record-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&home);
    fs::create_dir_all(&home).with_context(|| format!("creating {}", home.display()))?;
    let result = run(&script, path, &home);
    let _ = fs::remove_dir_all(&home);
    let cast = result?;
    let out = path.with_extension("cast");
    fs::write(&out, cast.finish()).with_context(|| format!("writing {}", out.display()))?;
    Ok(out)
}

/// Runs the script's program in `home`, as the script says, and returns
/// what it recorded.
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
        private: private_names(home),
    };
    for step in &script.steps {
        recorder.step(step)?;
    }
    // A program that has not ended is hung up as the terminal closes.
    drop(pty);
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
    /// What must not show: the user's name, the host's, the user's home.
    private: Vec<String>,
}

/// What a read found.
enum Read {
    Output,
    TimedOut,
    End,
}

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
            Step::Send(bytes) => self.send(bytes)?,
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
    /// shows it on the terminal, and sends the terminal's replies back.
    fn read(&mut self, deadline: Instant) -> Result<Read> {
        let n = match self.reader.read_until(&mut self.buffer, Some(deadline))? {
            ReadOutcome::Read(n) => n,
            ReadOutcome::TimedOut => return Ok(Read::TimedOut),
            ReadOutcome::End => return Ok(Read::End),
        };
        let bytes = self.buffer[..n].to_vec();
        self.cast.output(&bytes);
        self.parser.advance(&mut self.terminal, &bytes);
        let replies = self.terminal.take_replies();
        if !replies.is_empty() {
            self.send(&replies)?;
        }
        let screen = self.screen();
        let output = String::from_utf8_lossy(&bytes);
        if let Some(name) = self
            .private
            .iter()
            .find(|name| screen.contains(name.as_str()) || output.contains(name.as_str()))
        {
            bail!("the program showed `{name}`, which a recording must not keep:\n{screen}");
        }
        Ok(Read::Output)
    }

    fn send(&mut self, bytes: &[u8]) -> Result<()> {
        self.cast.input(bytes);
        self.writer
            .write_all(bytes)
            .context("writing to the program")
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

/// The names that a recording must not show: the user's, the host's, and
/// the user's home, but for the program's own, `home`.
fn private_names(home: &Path) -> Vec<String> {
    let mut names: Vec<String> = ["USER", "LOGNAME", "HOME"]
        .into_iter()
        .filter_map(|name| std::env::var(name).ok())
        .collect();
    for args in [&["-s"][..], &[]] {
        if let Ok(output) = Command::new("hostname").args(args).output() {
            names.push(String::from_utf8_lossy(&output.stdout).trim().to_owned());
        }
    }
    let home = home.to_string_lossy();
    names.retain(|name| name.len() >= 3 && !home.contains(name.as_str()));
    names.sort();
    names.dedup();
    names
}
