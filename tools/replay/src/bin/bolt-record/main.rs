//! `bolt-record`: runs a program in a pseudo-terminal, as a script drives
//! it, and writes what the program shows as an asciicast, for tests to
//! replay.
//!
//! `bolt-record SCRIPT...` writes each script's recording next to it, with
//! the extension `cast`. The program runs in a headless terminal, which
//! answers its queries as the session's does; it runs with no environment
//! but the one that every recording has and the script's, in a directory of
//! its own, which is also its home, so that nothing of the machine shows in
//! the recording. A recording that shows the user's name, the host's, the
//! user's home or the temporary directory is not written.

mod cast;
mod script;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use anyhow::{Context, Result, bail};
use bolt_replay::harness::{Event, Harness, HarnessOptions};
use bolt_vt_core::page::{Content, PageList};
use bolt_vt_core::parser::{Handler, Params, Parser};
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
    let cast = run(&script, path, &home.path())?;
    let out = path.with_extension("cast");
    fs::write(&out, cast.finish()).with_context(|| format!("writing {}", out.display()))?;
    Ok(out)
}

/// The directory that holds the directory that a program runs in, which is
/// also its home, and the recorder's mark, removed as it drops, however the
/// recording ends.
struct Home {
    root: PathBuf,
}

/// What the homes' names start with, before the script's name and the
/// recorder's process id.
const HOME_PREFIX: &str = "bolt-record-";

/// The file that marks a directory as a recorder's, next to the program's
/// home, where the program does not look.
const MARK: &str = ".bolt-record";

impl Home {
    fn new(name: &str) -> Result<Self> {
        let root = std::env::temp_dir().join(format!("{HOME_PREFIX}{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).with_context(|| format!("creating {}", root.display()))?;
        let home = Self { root };
        fs::write(home.root.join(MARK), b"")?;
        fs::create_dir(home.path())?;
        Ok(home)
    }

    /// The program's home.
    fn path(&self) -> PathBuf {
        self.root.join("home")
    }
}

impl Drop for Home {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

/// Removes the homes that recorders which were killed left: those that a
/// recorder marked, whose process has gone.
fn remove_stale_homes() {
    let Ok(entries) = fs::read_dir(std::env::temp_dir()) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        // A process id as a recorder writes it, without a sign or leading
        // zeros.
        let Some(pid) = name
            .strip_prefix(HOME_PREFIX)
            .and_then(|rest| rest.rsplit_once('-'))
            .and_then(|(_, pid)| {
                pid.parse::<libc::pid_t>()
                    .ok()
                    .filter(|id| id.to_string() == pid)
            })
        else {
            continue;
        };
        if pid > 0 && entry.path().join(MARK).is_file() && gone(pid) {
            let _ = fs::remove_dir_all(entry.path());
        }
    }
}

/// Whether no process has the id `pid`, which is positive: one that is not
/// this user's still has it.
fn gone(pid: libc::pid_t) -> bool {
    // SAFETY: with no signal, `kill` sends nothing, and only looks for the
    // process; a positive `pid` names one process.
    let found = unsafe { libc::kill(pid, 0) } == 0;
    !found && std::io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH)
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

/// Fails if the recording, whole, shows one of `private`, in any of the
/// texts that [`texts`] reads it as.
fn check_all(harness: &Harness, private: &[String]) -> Result<()> {
    let texts = texts(harness);
    for name in private {
        if texts.iter().any(|text| text.contains(name.as_str())) {
            bail!("the program showed `{name}`, which a recording must not keep");
        }
    }
    Ok(())
}

/// The texts that a recording holds, which a name must show in none of:
/// - the bytes that the program wrote, as they are, and those typed into
///   it, so that the recording holds no name whole, in a string that the
///   terminal drops or a sequence that it ignores;
/// - the text that the VT core's parser prints from the output, without its
///   escape sequences and control characters but for the line breaks, so
///   that a name that an escape breaks shows;
/// - the payload of each OSC, DCS and APC string, as it is and with its
///   percent-encoding undone, as links and the working directory have it;
/// - the titles and the working directory, as the terminal keeps them;
/// - every row of both screens, the scrollback's too, so that a name that
///   the cursor's moves put together shows.
fn texts(harness: &Harness) -> Vec<String> {
    let output = harness.output();
    let mut plain = Plain::default();
    Parser::new().advance(&mut plain, &output);
    let mut texts = vec![String::from_utf8_lossy(&output).into_owned(), plain.text];
    for (_, event) in harness.events() {
        if let Event::Input(bytes) = event {
            texts.push(String::from_utf8_lossy(bytes).into_owned());
        }
    }
    for payload in &plain.strings {
        texts.push(String::from_utf8_lossy(payload).into_owned());
        texts.push(String::from_utf8_lossy(&percent_decoded(payload)).into_owned());
    }
    let terminal = harness.terminal();
    texts.push(terminal.title().to_owned());
    texts.push(terminal.icon_title().to_owned());
    if let Some(dir) = terminal.working_directory() {
        texts.push(format!("{}{}", dir.host, dir.path));
    }
    for list in [terminal.screen_rows(), terminal.inactive_screen_rows()] {
        texts.extend(rows(list));
    }
    texts
}

/// What the parser finds in the program's output: the text that it prints,
/// with the line breaks, and the payloads of the control strings.
#[derive(Default)]
struct Plain {
    text: String,
    strings: Vec<Vec<u8>>,
}

impl Plain {
    fn put(&mut self, data: &[u8]) {
        if let Some(string) = self.strings.last_mut() {
            string.extend_from_slice(data);
        }
    }
}

impl Handler for Plain {
    fn print(&mut self, c: char) {
        self.text.push(c);
    }

    fn execute(&mut self, byte: u8) {
        if byte == b'\n' {
            self.text.push('\n');
        }
    }

    fn osc_start(&mut self) {
        self.strings.push(Vec::new());
    }

    fn osc_put(&mut self, data: &[u8]) {
        self.put(data);
    }

    fn dcs_start(&mut self, _: &Params, _: &[u8], _: u8) {
        self.strings.push(Vec::new());
    }

    fn dcs_put(&mut self, data: &[u8]) {
        self.put(data);
    }

    fn apc_start(&mut self) {
        self.strings.push(Vec::new());
    }

    fn apc_put(&mut self, data: &[u8]) {
        self.put(data);
    }
}

/// `bytes` with each `%` and two hexadecimal digits made the byte that
/// they stand for.
fn percent_decoded(bytes: &[u8]) -> Vec<u8> {
    let digit = |byte: u8| char::from(byte).to_digit(16);
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while let Some(&byte) = bytes.get(at) {
        if byte == b'%'
            && let Some(&[high, low]) = bytes.get(at + 1..at + 3)
            && let (Some(high), Some(low)) = (digit(high), digit(low))
        {
            // Two hexadecimal digits make a byte.
            out.push((high * 16 + low) as u8);
            at += 3;
        } else {
            out.push(byte);
            at += 1;
        }
    }
    out
}

/// The text of each row of `list`, its clusters whole.
fn rows(list: &PageList) -> impl Iterator<Item = String> + '_ {
    list.rows_from(0).map(|row| {
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
}

/// The names that a recording must not show: the user's, the user's full
/// name, the host's and the computer's, the user's home, the temporary
/// directory, when it is the user's own, and the program's home, which is
/// the machine's; longest first, so that a failure names the most of what
/// showed. A name that cannot be looked up is said, and one shorter than 3
/// characters, which would match too much, is left out.
fn private_names(home: &Path) -> Vec<String> {
    let mut names: Vec<String> = ["USER", "LOGNAME", "HOME"]
        .into_iter()
        .filter_map(|name| std::env::var(name).ok())
        .collect();
    let mut commands: Vec<(&str, &[&str])> =
        vec![("id", &["-un"]), ("hostname", &["-s"]), ("hostname", &[])];
    if cfg!(target_os = "macos") {
        commands.extend([
            ("scutil", &["--get", "ComputerName"][..]),
            ("scutil", &["--get", "LocalHostName"]),
            ("id", &["-F"]),
        ]);
    }
    for (program, args) in commands {
        match Command::new(program)
            .args(args)
            .stderr(std::process::Stdio::null())
            .output()
        {
            Ok(output) if output.status.success() => {
                names.push(String::from_utf8_lossy(&output.stdout).trim().to_owned());
            }
            _ => eprintln!(
                "warning: `{}` failed, so the name that it gives is not checked",
                [program]
                    .iter()
                    .chain(args)
                    .copied()
                    .collect::<Vec<_>>()
                    .join(" ")
            ),
        }
    }
    let temp = std::env::temp_dir();
    let mut paths = vec![home.to_owned()];
    if users_own(&temp) {
        paths.push(temp);
    }
    for path in paths {
        names.push(path.display().to_string().trim_end_matches('/').to_owned());
        if let Ok(resolved) = path.canonicalize() {
            names.push(resolved.display().to_string());
        }
    }
    names.retain(|name| name.chars().count() >= 3);
    names.sort_by(|a, b| b.len().cmp(&a.len()).then_with(|| a.cmp(b)));
    names.dedup();
    names
}

/// Whether the directory at `path` is the user's own, rather than one that
/// every user shares, as `/tmp` is, whose path tells nothing of the user.
fn users_own(path: &Path) -> bool {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    fs::metadata(path).is_ok_and(|metadata| {
        // SAFETY: `getuid` reads the process's user id, and always succeeds.
        let uid = unsafe { libc::getuid() };
        metadata.uid() == uid && metadata.permissions().mode() & 0o002 == 0
    })
}
