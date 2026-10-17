//! `bolt-record`: runs a program in a pseudo-terminal, as a script drives
//! it, and writes what the program shows as an asciicast, for tests to
//! replay.
//!
//! `bolt-record SCRIPT...` writes each script's recording next to it, with
//! the extension `cast`. The program runs in a headless terminal, which
//! answers its queries as the session's does; it runs with no environment
//! but the one
//! that every recording has and the script's, in a directory of its own,
//! which is also its home, so that nothing of the machine shows in the
//! recording. A recording that shows the user's name, the host's or the
//! user's home is not written.

mod cast;
mod script;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use anyhow::{Context, Result, bail};
use bolt_replay::harness::{Event, Harness, HarnessOptions};
use bolt_vt_core::page::Content;
use bolt_vt_core::terminal::Options;

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
    let mut env = vec![
        ("HOME".to_owned(), home.display().to_string()),
        ("PATH".to_owned(), PATH.to_owned()),
        ("LANG".to_owned(), "en_US.UTF-8".to_owned()),
        ("COLORTERM".to_owned(), "truecolor".to_owned()),
    ];
    env.extend(script.env.iter().cloned());
    let options = HarnessOptions {
        program: script.run.clone(),
        env,
        terminal: Options {
            term: TERM.to_owned(),
            ..Options::default()
        },
        dir: home.to_owned(),
        cols: script.cols,
        rows: script.rows,
    };
    let mut harness = Harness::spawn(&options)?;
    let private = private_names(home);
    harness.forbid(private.clone());
    for step in &script.steps {
        match step {
            Step::Wait(text, limit) => harness.wait(text, *limit)?,
            Step::Quiet(time, limit) => harness.quiet(*time, *limit)?,
            Step::Sleep(time) => harness.sleep(*time)?,
            Step::Send(bytes, limit) => harness.send(bytes, *limit)?,
            Step::Mark(label) => harness.mark(label)?,
            Step::Exit(limit) => harness.exit(*limit)?,
        }
    }
    // A program that has not ended is hung up as the terminal closes.
    harness.hang_up();
    check_all(&harness, &private)?;
    let mut cast = Cast::new(script.cols, script.rows, TERM);
    for (at, event) in harness.events() {
        match event {
            Event::Output(bytes) => cast.output(*at, bytes),
            Event::Input(bytes) => cast.input(*at, bytes),
            Event::Mark(label) => cast.mark(*at, label),
        }
    }
    Ok(cast)
}

/// Fails if the recording, whole, shows one of `private`: in what the
/// program wrote, without its escape sequences, so that a name that two
/// reads split, or that an escape breaks, shows; and in every row that the
/// terminal keeps, the scrollback's too.
fn check_all(harness: &Harness, private: &[String]) -> Result<()> {
    let plain = plain(&harness.output());
    let rows: Vec<String> = harness
        .terminal()
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
    for name in private {
        if plain.contains(name.as_str()) || rows.iter().any(|row| row.contains(name.as_str())) {
            bail!("the program showed `{name}`, which a recording must not keep");
        }
    }
    Ok(())
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
