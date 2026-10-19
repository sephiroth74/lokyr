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
use bolt_replay::harness::{Event, Harness, HarnessOptions, line_of, lines};
use bolt_vt_core::parser::{Handler, Limits, Params, Parser, StringEnd};
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
        remove(&root);
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
        remove(&self.root);
    }
}

/// Removes the directory at `path`, if it is there, making its directories
/// writable first if they keep it from going, as a program can leave them
/// read-only; says so if it stays still.
fn remove(path: &Path) {
    if fs::remove_dir_all(path).is_ok() || fs::symlink_metadata(path).is_err() {
        return;
    }
    writable(path);
    if let Err(error) = fs::remove_dir_all(path) {
        eprintln!("warning: {} stays: {error}", path.display());
    }
}

/// Makes the directory at `path`, and those in it, writable, and readable,
/// by the user, without following links.
fn writable(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return;
    };
    if !metadata.is_dir() {
        return;
    }
    let mode = metadata.permissions().mode() | 0o700;
    let _ = fs::set_permissions(path, fs::Permissions::from_mode(mode));
    for entry in fs::read_dir(path).into_iter().flatten().flatten() {
        writable(&entry.path());
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
            remove(&entry.path());
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
            window_ops: script.window_ops,
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
    if harness.dropped_replies() > 0 {
        eprintln!(
            "warning: the program left a MiB of replies unread, so that the replies of {} reads were dropped",
            harness.dropped_replies()
        );
    }
    check_all(&harness, &private, script)?;
    let mut cast = Cast::new(script.cols, script.rows, TERM, script.window_ops);
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
/// texts that [`texts`] reads it as, or on the screen at any moment of the
/// output, which [`Watch`] replays on a terminal as the script's.
fn check_all(harness: &Harness, private: &[String], script: &Script) -> Result<()> {
    let texts = texts(harness);
    for name in private {
        if texts.iter().any(|text| text.contains(name.as_str())) {
            bail!("the program showed `{name}`, which a recording must not keep");
        }
    }
    let mut watch = Watch {
        terminal: Terminal::new(
            script.cols,
            script.rows,
            Options {
                term: TERM.to_owned(),
                window_ops: script.window_ops,
                ..Options::default()
            },
        ),
        private,
        shown: None,
        printing: false,
    };
    Parser::new().advance(&mut watch, &harness.output());
    if let Some(name) = watch.shown {
        bail!("the program showed `{name}` in passing, which a recording must not keep");
    }
    Ok(())
}

/// A terminal that the output replays on, which looks at the screen at
/// every moment that a name could show in and go at the next: the whole
/// screen before each control and escape sequence, which can take text
/// away, by an erase, a scroll, a switch of screens, or a move of the
/// cursor that the text after overwrites, and before each run of printed
/// text, which can break what a control put together; and the cursor's
/// line after each printed character, which can complete a name that the
/// next character overwrites, or pushes aside in insert mode, or that the
/// next row's text breaks as a pending wrap goes on. A name that a read
/// draws and takes away, which no read ends with, shows there.
struct Watch<'a> {
    terminal: Terminal,
    private: &'a [String],
    /// The first name that showed.
    shown: Option<String>,
    /// Whether text is being printed, since the last control, escape
    /// sequence or string: the screen was looked at as it started.
    printing: bool,
}

impl Watch<'_> {
    /// Notes the first name that the screen shows now, if none showed yet.
    fn look(&mut self) {
        if self.shown.is_some() {
            return;
        }
        let list = self.terminal.screen_rows();
        let lines = lines(list, list.scrollback_rows());
        self.shown = self
            .private
            .iter()
            .find(|name| lines.iter().any(|line| line.contains(name.as_str())))
            .cloned();
    }

    /// Looks at the screen before a control or an escape sequence, which
    /// ends a run of printed text.
    fn before_control(&mut self) {
        self.look();
        self.printing = false;
    }

    /// Looks at the screen as a run of printed text starts.
    fn start_printing(&mut self) {
        if !self.printing {
            self.look();
            self.printing = true;
        }
    }

    /// Notes the first name that the cursor's line shows now, if none
    /// showed yet.
    fn look_at_the_cursor(&mut self) {
        if self.shown.is_some() {
            return;
        }
        let list = self.terminal.screen_rows();
        let index = list.scrollback_rows() + usize::from(self.terminal.cursor().y);
        let line = line_of(list, index);
        self.shown = self
            .private
            .iter()
            .find(|name| line.contains(name.as_str()))
            .cloned();
    }
}

impl Handler for Watch<'_> {
    fn print(&mut self, c: char) {
        self.start_printing();
        self.terminal.print(c);
        self.look_at_the_cursor();
    }

    fn print_ascii(&mut self, text: &[u8]) {
        self.start_printing();
        for byte in text.chunks(1) {
            self.terminal.print_ascii(byte);
            self.look_at_the_cursor();
        }
    }

    fn execute(&mut self, byte: u8) {
        self.before_control();
        self.terminal.execute(byte);
    }

    fn esc_dispatch(&mut self, intermediates: &[u8], final_byte: u8) {
        self.before_control();
        self.terminal.esc_dispatch(intermediates, final_byte);
    }

    fn csi_dispatch(&mut self, params: &Params, intermediates: &[u8], final_byte: u8) {
        self.before_control();
        self.terminal
            .csi_dispatch(params, intermediates, final_byte);
    }

    fn osc_start(&mut self) {
        self.printing = false;
        self.terminal.osc_start();
    }

    fn osc_put(&mut self, data: &[u8]) {
        self.terminal.osc_put(data);
    }

    fn osc_end(&mut self, end: StringEnd) {
        self.terminal.osc_end(end);
    }

    fn dcs_start(&mut self, params: &Params, intermediates: &[u8], final_byte: u8) {
        self.printing = false;
        self.terminal.dcs_start(params, intermediates, final_byte);
    }

    fn dcs_put(&mut self, data: &[u8]) {
        self.terminal.dcs_put(data);
    }

    fn dcs_end(&mut self, end: StringEnd) {
        self.terminal.dcs_end(end);
    }

    fn apc_start(&mut self) {
        self.printing = false;
        self.terminal.apc_start();
    }

    fn apc_put(&mut self, data: &[u8]) {
        self.terminal.apc_put(data);
    }

    fn apc_end(&mut self, end: StringEnd) {
        self.terminal.apc_end(end);
    }
}

/// The texts that a recording holds, which a name must show in none of:
/// - the bytes that the program wrote, as they are, without their C0
///   controls and DEL but for the line breaks and the tabs, which separate
///   text as the screen does, and with their percent-encoding undone too,
///   so that the recording holds no name whole, in a string that the
///   terminal drops, or one that passes its limit;
/// - the payload of each OSC, DCS and APC string, as the parser reads it
///   with no limit, and the text of each SOS and PM string, which it drops,
///   and of each DCS string that it ignores: each as it is and without its
///   controls, line breaks and DEL included, which the string keeps, and
///   each of those with its percent-encoding undone, as links and the
///   working directory have it;
/// - what each run of base64 in the output stands for, from each place
///   that its groups can start, and the same without the output's controls,
///   which split a run, and in the payloads of a transfer in chunks,
///   joined: OSC 52 carries the clipboard so, kitty's graphics, OSC 1337's
///   files and OSC 99 their paths;
/// - the bytes typed into the program, all of them, as two writes can
///   split a name typed;
/// - the text that the VT core's parser prints from the output, without its
///   escape sequences and control characters but for the line breaks and
///   the tabs, so that a name that an escape breaks shows;
/// - the titles and the working directory, as the terminal keeps them;
/// - every line of both screens, the scrollback's too, its soft-wrapped
///   rows joined, so that a name that the cursor's moves put together, or
///   that wraps, shows.
fn texts(harness: &Harness) -> Vec<String> {
    let lossy = |bytes: &[u8]| String::from_utf8_lossy(bytes).into_owned();
    let output = harness.output();
    let mut plain = Plain::default();
    // No limit, so that the whole of a string past the terminal's shows.
    let limits = Limits {
        osc: usize::MAX,
        osc_commands: Vec::new(),
        dcs: usize::MAX,
        apc: usize::MAX,
    };
    Parser::with_limits(limits).advance(&mut plain, &output);
    let input: Vec<u8> = harness
        .events()
        .iter()
        .filter_map(|(_, event)| match event {
            Event::Input(bytes) => Some(&bytes[..]),
            _ => None,
        })
        .collect::<Vec<_>>()
        .concat();
    let joined: Vec<u8> = output
        .iter()
        .copied()
        .filter(|&byte| byte >= 0x20 && byte != 0x7F)
        .collect();
    let mut texts = vec![
        lossy(&output),
        lossy(&without_controls(&output)),
        lossy(&without_controls(&percent_decoded(&output))),
        lossy(&input),
        plain.text,
    ];
    let payloads = plain.strings.iter().map(|(_, payload)| payload.clone());
    for string in payloads.chain(control_strings(&output)) {
        let bare: Vec<u8> = string
            .iter()
            .copied()
            .filter(|&byte| byte >= 0x20 && byte != 0x7F)
            .collect();
        for text in [string, bare] {
            texts.push(lossy(&percent_decoded(&text)));
            texts.push(lossy(&text));
        }
    }
    // Runs of base64 decode to bytes that hold no NUL to match, so that one
    // text, of them all, keeps them apart.
    for bytes in [&output, &joined]
        .into_iter()
        .chain(&transfers(&plain.strings))
    {
        texts.push(lossy(&base64_runs(bytes).join(&0)));
    }
    let terminal = harness.terminal();
    texts.push(terminal.title().to_owned());
    texts.push(terminal.icon_title().to_owned());
    if let Some(dir) = terminal.working_directory() {
        texts.push(format!("{}{}", dir.host, dir.path));
    }
    for list in [terminal.screen_rows(), terminal.inactive_screen_rows()] {
        texts.extend(lines(list, 0));
    }
    texts
}

/// `bytes` without their C0 controls and DEL, but for the line breaks and
/// the tabs.
fn without_controls(bytes: &[u8]) -> Vec<u8> {
    bytes
        .iter()
        .copied()
        .filter(|&byte| (byte >= 0x20 && byte != 0x7F) || matches!(byte, b'\n' | b'\t'))
        .collect()
}

/// The bytes of each DCS, SOS, PM and APC string in `bytes`, from its
/// introducer, `P`, `X`, `^` or `_`, to the ESC that ends it, or the CAN or
/// SUB that cancels it, or the end. The introducer follows an ESC, with the
/// bytes between them that the parser's escape state passes over: the C0
/// controls but CAN and SUB, which cancel the escape, and ESC, which starts
/// it again; DEL; and the bytes from 0x80 up.
fn control_strings(bytes: &[u8]) -> Vec<Vec<u8>> {
    let mut strings = Vec::new();
    let mut at = 0;
    while let Some(escape) = bytes[at..].iter().position(|&byte| byte == 0x1B) {
        at += escape + 1;
        while let Some(&byte) = bytes.get(at)
            && ((byte < 0x20 && !matches!(byte, 0x18 | 0x1A | 0x1B)) || byte >= 0x7F)
        {
            at += 1;
        }
        if !bytes.get(at).is_some_and(|byte| b"PX^_".contains(byte)) {
            continue;
        }
        let start = at + 1;
        at = start;
        while bytes
            .get(at)
            .is_some_and(|byte| !matches!(byte, 0x1B | 0x18 | 0x1A))
        {
            at += 1;
        }
        strings.push(bytes[start..at].to_vec());
    }
    strings
}

/// The payloads of the transfers that come in chunks, each joined, as their
/// base64 is split between them: kitty's graphics, whose APC strings
/// `G<keys>;<data>` go on while their keys hold `m=1`, and iTerm2's files,
/// whose OSC 1337 strings `FilePart=<data>` go on to `FileEnd`. A transfer
/// that does not end is kept too.
fn transfers(strings: &[(Kind, Vec<u8>)]) -> Vec<Vec<u8>> {
    let (mut kitty, mut iterm) = (Vec::new(), Vec::new());
    let mut transfers = Vec::new();
    for (kind, payload) in strings {
        match kind {
            Kind::Apc => {
                if let Some(rest) = payload.strip_prefix(b"G") {
                    let (keys, data) = match rest.iter().position(|&byte| byte == b';') {
                        Some(at) => (&rest[..at], &rest[at + 1..]),
                        None => (rest, &[][..]),
                    };
                    kitty.extend_from_slice(data);
                    if !keys.split(|&byte| byte == b',').any(|key| key == b"m=1") {
                        transfers.push(std::mem::take(&mut kitty));
                    }
                }
            }
            Kind::Osc => {
                if let Some(data) = payload.strip_prefix(b"1337;FilePart=") {
                    iterm.extend_from_slice(data);
                } else if payload == b"1337;FileEnd" {
                    transfers.push(std::mem::take(&mut iterm));
                }
            }
            Kind::Dcs => {}
        }
    }
    transfers.extend([kitty, iterm].into_iter().filter(|data| !data.is_empty()));
    transfers
}

/// Whether `byte` is of base64, in its alphabet or in the one for URLs and
/// file names, which has `-` and `_` for `+` and `/`.
fn base64(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'/' | b'-' | b'_')
}

/// What each run of base64 in `bytes`, of 4 characters or more, the most
/// that 3 bytes take, stands for, from each of the four places that its
/// groups can start, so that a name in base64 shows wherever its run
/// starts. Shorter runs stand for less than a name.
fn base64_runs(bytes: &[u8]) -> Vec<Vec<u8>> {
    bytes
        .split(|&byte| !base64(byte))
        .filter(|run| run.len() >= 4)
        .flat_map(|run| (0..4).map(move |skip| base64_decoded(&run[skip..])))
        .collect()
}

/// The bytes that `text` stands for in base64, in either alphabet, up to
/// the first byte that is not of it, its padding included.
fn base64_decoded(text: &[u8]) -> Vec<u8> {
    let value = |byte: u8| match byte {
        b'A'..=b'Z' => Some(byte - b'A'),
        b'a'..=b'z' => Some(byte - b'a' + 26),
        b'0'..=b'9' => Some(byte - b'0' + 52),
        b'+' | b'-' => Some(62),
        b'/' | b'_' => Some(63),
        _ => None,
    };
    let mut out = Vec::with_capacity(text.len() * 3 / 4);
    let (mut bits, mut count) = (0_u32, 0);
    for value in text.iter().map_while(|&byte| value(byte)) {
        bits = (bits << 6) | u32::from(value);
        count += 6;
        if count >= 8 {
            count -= 8;
            // The byte that the last bits make.
            out.push((bits >> count) as u8);
            bits &= (1 << count) - 1;
        }
    }
    out
}

/// The kind of a control string that the parser reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Osc,
    Dcs,
    Apc,
}

/// What the parser finds in the program's output: the text that it prints,
/// with the line breaks and the tabs, and the payloads of the control
/// strings, by kind.
#[derive(Default)]
struct Plain {
    text: String,
    strings: Vec<(Kind, Vec<u8>)>,
}

impl Plain {
    fn put(&mut self, data: &[u8]) {
        if let Some((_, string)) = self.strings.last_mut() {
            string.extend_from_slice(data);
        }
    }
}

impl Handler for Plain {
    fn print(&mut self, c: char) {
        self.text.push(c);
    }

    fn execute(&mut self, byte: u8) {
        if matches!(byte, b'\n' | b'\t') {
            self.text.push(char::from(byte));
        }
    }

    fn osc_start(&mut self) {
        self.strings.push((Kind::Osc, Vec::new()));
    }

    fn osc_put(&mut self, data: &[u8]) {
        self.put(data);
    }

    fn dcs_start(&mut self, _: &Params, _: &[u8], _: u8) {
        self.strings.push((Kind::Dcs, Vec::new()));
    }

    fn dcs_put(&mut self, data: &[u8]) {
        self.put(data);
    }

    fn apc_start(&mut self) {
        self.strings.push((Kind::Apc, Vec::new()));
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

/// The names that a recording must not show: the user's, the user's full
/// name, the host's and the computer's, the user's home, the temporary
/// directory and the folders above it, when they are the user's own, and
/// the program's home, which is
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
    // The temporary directory, and the folders above it, as long as they
    // are the user's own, as macOS's per-user folder is, which holds the
    // user's caches too; a long name of one, as that folder's is, alone.
    for form in [temp.clone(), temp.canonicalize().unwrap_or(temp)] {
        for folder in private_folders(&form, users_own) {
            if let Some(name) = folder.file_name().and_then(|name| name.to_str())
                && name.len() >= 12
            {
                names.push(name.to_owned());
            }
            paths.push(folder);
        }
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

/// The system's folders, which the folders of a temporary directory stop
/// before, though root owns them: a temporary directory of root's would
/// otherwise take them in.
const SYSTEM_FOLDERS: [&str; 6] = [
    "/",
    "/private",
    "/var",
    "/private/var",
    "/var/folders",
    "/private/var/folders",
];

/// The folders of the temporary directory `temp`, from it up, as long as
/// `own` says that each is the user's own, and before the system's.
fn private_folders(temp: &Path, own: impl Fn(&Path) -> bool) -> Vec<PathBuf> {
    temp.ancestors()
        .take_while(|dir| {
            own(dir)
                && !SYSTEM_FOLDERS
                    .iter()
                    .any(|system| Path::new(system) == *dir)
        })
        .map(Path::to_owned)
        .collect()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_folders_of_a_temporary_directory_stop_before_the_systems() {
        let temp = Path::new("/private/var/folders/zz/abcdefghijklmnop/T");
        // As root, who owns every folder.
        let folders: Vec<PathBuf> = private_folders(temp, |_| true);
        assert_eq!(
            folders,
            [
                "/private/var/folders/zz/abcdefghijklmnop/T",
                "/private/var/folders/zz/abcdefghijklmnop",
                "/private/var/folders/zz"
            ]
            .map(PathBuf::from)
        );
        // As a user, whose own folders end below.
        let folders = private_folders(temp, |dir| dir.ends_with("T"));
        assert_eq!(folders, [PathBuf::from(temp)]);
    }

    #[test]
    fn base64_shows_from_wherever_its_run_starts() {
        let named = |bytes: &[u8], name: &str| {
            base64_runs(bytes)
                .iter()
                .any(|data| String::from_utf8_lossy(data).contains(name))
        };
        // `zz-someone-zz` in base64, after one to three characters that
        // shift its groups, and after a space.
        for before in ["", "a", "ab", "abc", "YWJj "] {
            assert!(
                named(
                    format!("x;{before}enotc29tZW9uZS16eg==").as_bytes(),
                    "zz-someone-zz"
                ),
                "{before:?}"
            );
        }
        assert!(!named(b"x;enotc29t", "zz-someone-zz"), "a part");
        // A name of 3 characters, in 4, and one in the alphabet for URLs.
        assert!(named(b"x;enpx;y", "zzq"));
        assert!(named(b"x;enp-c29tZW9uZX56eg", "zz~someone~zz"));
        assert!(named(b"x;enp+c29tZW9uZX56eg", "zz~someone~zz"));
    }

    #[test]
    fn the_chunks_of_a_transfer_are_joined() {
        let strings = [
            (Kind::Apc, b"Ga=T,f=100,m=1;L2hvbWUvenotc29t".to_vec()),
            (Kind::Osc, b"1337;MultipartFile=size=4".to_vec()),
            (Kind::Osc, b"1337;FilePart=enot".to_vec()),
            (Kind::Apc, b"Gm=0;ZW9uZS16ei9hLnBuZw==".to_vec()),
            (Kind::Osc, b"1337;FilePart=c29tZW9uZS16eg==".to_vec()),
            (Kind::Osc, b"1337;FileEnd".to_vec()),
            (Kind::Apc, b"Gm=1;YWJj".to_vec()),
        ];
        assert_eq!(
            transfers(&strings),
            [
                b"L2hvbWUvenotc29tZW9uZS16ei9hLnBuZw==".to_vec(),
                b"enotc29tZW9uZS16eg==".to_vec(),
                b"YWJj".to_vec(),
            ]
        );
    }

    #[test]
    fn control_strings_start_where_the_parser_starts_them() {
        let strings = control_strings(
            b"a\x1bPpzz-some\none-zz\x1b\\b\x1b^x\x7fy\x18z\x1b_end\x1b\x01\x7fPdcs\x1b\x18Xno\x1b\x1bXsos\x1b\xc3\x80^pm",
        );
        assert_eq!(
            strings,
            [
                b"pzz-some\none-zz".to_vec(),
                b"x\x7fy".to_vec(),
                b"end".to_vec(),
                b"dcs".to_vec(),
                b"sos".to_vec(),
                b"pm".to_vec(),
            ]
        );
    }
}
