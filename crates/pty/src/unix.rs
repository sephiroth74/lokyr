//! PTYs on Unix: `openpty`, and a child that leads its own session with
//! the PTY as its controlling terminal.

use std::collections::BTreeMap;
use std::ffi::{CStr, OsStr, OsString};
use std::fs::File;
use std::io::{self, Read, Write};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

use crate::{Error, Options, Program, Size};

/// A pseudo-terminal, with the program that runs in it.
///
/// Dropping it closes the terminal, which hangs the program up.
#[derive(Debug)]
pub struct Pty {
    master: OwnedFd,
    child: Child,
    tty: PathBuf,
}

/// What the program writes to the terminal.
#[derive(Debug)]
pub struct PtyReader(File);

/// What is typed into the terminal, for the program.
#[derive(Debug)]
pub struct PtyWriter(File);

/// Opens a pseudo-terminal and starts the program of `options` in it.
pub fn spawn(options: &Options) -> Result<Pty, Error> {
    let (master, slave) = open(options.size).map_err(Error::Open)?;
    let tty = tty_name(&slave).map_err(Error::Open)?;
    let account = Account::current();
    let (program, args, arg0) = command_line(&options.program, &account);
    let mut command = Command::new(&program);
    command.args(&args);
    if let Some(arg0) = arg0 {
        command.arg0(arg0);
    }
    command
        .env_clear()
        .envs(environment(options, std::env::vars_os()))
        .current_dir(options.working_directory.as_ref().unwrap_or(&account.home));
    let spawn_error = |source| Error::Spawn {
        program: program.display().to_string(),
        source,
    };
    for stdio in 0..3 {
        let slave = Stdio::from(slave.try_clone().map_err(spawn_error)?);
        match stdio {
            0 => command.stdin(slave),
            1 => command.stdout(slave),
            _ => command.stderr(slave),
        };
    }
    // SAFETY: the closure runs in the child between fork and exec, and calls
    // only functions that are async-signal-safe, without allocating.
    unsafe {
        command.pre_exec(lead_session);
    }
    let child = command.spawn().map_err(spawn_error)?;
    // The child has its copies of the terminal's side; the parent keeps
    // none, so that the master reads the end when the program goes.
    drop(command);
    drop(slave);
    Ok(Pty { master, child, tty })
}

impl Pty {
    /// A reader of what the program writes. A read returns 0 once the
    /// program and its children have closed the terminal.
    pub fn reader(&self) -> io::Result<PtyReader> {
        Ok(PtyReader(File::from(self.master.try_clone()?)))
    }

    /// A writer of what is typed for the program.
    pub fn writer(&self) -> io::Result<PtyWriter> {
        Ok(PtyWriter(File::from(self.master.try_clone()?)))
    }

    /// The path of the terminal's device, as `tty` prints it.
    pub fn tty_name(&self) -> &Path {
        &self.tty
    }

    /// The process id of the program.
    pub fn process_id(&self) -> u32 {
        self.child.id()
    }

    /// The process group in the terminal's foreground: the program's, or
    /// that of a job that it runs.
    pub fn foreground_process_group(&self) -> Option<u32> {
        // SAFETY: `tcgetpgrp` only reads the terminal's state, through a
        // descriptor that `self` owns.
        let group = unsafe { libc::tcgetpgrp(self.master.as_raw_fd()) };
        u32::try_from(group).ok()
    }
}

impl Read for PtyReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self.0.read(buf) {
            // Once no process has the terminal open, the master fails with
            // EIO rather than reading the end.
            Err(error) if error.raw_os_error() == Some(libc::EIO) => Ok(0),
            result => result,
        }
    }
}

impl Write for PtyWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}

/// Opens a pseudo-terminal of `size`, and returns its master and its slave,
/// neither inherited across exec.
fn open(size: Size) -> io::Result<(OwnedFd, OwnedFd)> {
    let mut window = libc::winsize {
        ws_row: size.rows,
        ws_col: size.cols,
        ws_xpixel: size.width,
        ws_ypixel: size.height,
    };
    let (mut master, mut slave) = (-1, -1);
    // SAFETY: the descriptors and the size are valid places to write and
    // read; without a name buffer, `openpty` writes no name.
    let opened = unsafe {
        libc::openpty(
            &mut master,
            &mut slave,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &mut window,
        )
    };
    if opened == -1 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: `openpty` opened both, and nothing else owns them.
    let (master, slave) = unsafe { (OwnedFd::from_raw_fd(master), OwnedFd::from_raw_fd(slave)) };
    for fd in [&master, &slave] {
        // SAFETY: `fd` is open, and setting its flags touches nothing else.
        if unsafe { libc::fcntl(fd.as_raw_fd(), libc::F_SETFD, libc::FD_CLOEXEC) } == -1 {
            return Err(io::Error::last_os_error());
        }
    }
    Ok((master, slave))
}

/// The path of the terminal that `fd` is open on.
fn tty_name(fd: &OwnedFd) -> io::Result<PathBuf> {
    let mut name = [0 as libc::c_char; 128];
    // SAFETY: `ttyname_r` writes at most `name.len()` bytes, a terminated
    // string when it succeeds.
    let failed = unsafe { libc::ttyname_r(fd.as_raw_fd(), name.as_mut_ptr(), name.len()) };
    if failed != 0 {
        return Err(io::Error::from_raw_os_error(failed));
    }
    // SAFETY: `ttyname_r` succeeded, so `name` holds a terminated string.
    let name = unsafe { CStr::from_ptr(name.as_ptr()) };
    Ok(PathBuf::from(OsStr::from_bytes(name.to_bytes())))
}

/// Makes the child the leader of a session, with the terminal on its
/// standard input as the controlling terminal, and gives every signal its
/// default action, which an ignored one would not get across exec. The
/// standard library has already emptied the signal mask.
///
/// It runs between fork and exec, so it calls only async-signal-safe
/// functions, and does not allocate.
fn lead_session() -> io::Result<()> {
    // SAFETY: `setsid` and `ioctl` only change the process's session and
    // terminal; `signal` with `SIG_DFL` installs no handler. All three are
    // async-signal-safe.
    unsafe {
        if libc::setsid() == -1 {
            return Err(io::Error::last_os_error());
        }
        if libc::ioctl(0, libc::TIOCSCTTY as _, 0) == -1 {
            return Err(io::Error::last_os_error());
        }
        // SIGKILL and SIGSTOP refuse, which changes nothing.
        for signal in 1..32 {
            libc::signal(signal, libc::SIG_DFL);
        }
    }
    Ok(())
}

/// The user who runs the terminal, as their account has them, or as the
/// environment says when the account cannot be read.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Account {
    name: OsString,
    home: PathBuf,
    shell: PathBuf,
}

impl Account {
    fn current() -> Self {
        let from_env =
            |name, default: &str| std::env::var_os(name).unwrap_or_else(|| OsString::from(default));
        Self::read().unwrap_or_else(|| Self {
            name: from_env("USER", ""),
            home: from_env("HOME", "/").into(),
            shell: from_env("SHELL", "/bin/sh").into(),
        })
    }

    /// The account of the process's user, from the user database.
    fn read() -> Option<Self> {
        let mut buffer = vec![0 as libc::c_char; 4096];
        loop {
            // SAFETY: an all-zero `passwd` is a valid value to overwrite.
            let mut entry: libc::passwd = unsafe { std::mem::zeroed() };
            let mut found = std::ptr::null_mut();
            // SAFETY: `getpwuid_r` writes the entry, whose strings point into
            // `buffer`, of the length given, and `found`.
            let failed = unsafe {
                libc::getpwuid_r(
                    libc::getuid(),
                    &mut entry,
                    buffer.as_mut_ptr(),
                    buffer.len(),
                    &mut found,
                )
            };
            if failed == libc::ERANGE && buffer.len() < 1 << 20 {
                buffer.resize(buffer.len() * 2, 0);
                continue;
            }
            if failed != 0 || found.is_null() {
                return None;
            }
            let field = |pointer: *const libc::c_char| {
                if pointer.is_null() {
                    return None;
                }
                // SAFETY: the entry's strings are terminated, in `buffer`,
                // which outlives this call.
                let text = unsafe { CStr::from_ptr(pointer) };
                Some(OsString::from_vec(text.to_bytes().to_vec()))
            };
            return Some(Self {
                name: field(entry.pw_name)?,
                home: field(entry.pw_dir)?.into(),
                shell: field(entry.pw_shell)
                    .filter(|shell| !shell.is_empty())
                    .unwrap_or_else(|| "/bin/sh".into())
                    .into(),
            });
        }
    }
}

/// The program to run for `program`, its arguments, and the name that it
/// gets as `argv[0]`, if not its path.
fn command_line(
    program: &Program,
    account: &Account,
) -> (PathBuf, Vec<OsString>, Option<OsString>) {
    match program {
        Program::Command { program, args } => (program.clone(), args.clone(), None),
        Program::LoginShell => login_shell(account),
    }
}

/// The user's shell as a login shell, as Terminal.app starts it: through
/// `login`, so that the session is recorded as a terminal's, with `-f`,
/// since the user is logged in already, `-p` to keep the environment and
/// `-l` to stay in the working directory. As `-l` also keeps the shell from
/// being a login shell, bash starts it with `-` before its name.
#[cfg(target_os = "macos")]
fn login_shell(account: &Account) -> (PathBuf, Vec<OsString>, Option<OsString>) {
    let name = account
        .shell
        .file_name()
        .map_or_else(|| OsString::from("sh"), OsStr::to_owned);
    let mut exec = b"exec -l -a ".to_vec();
    exec.extend(quoted(&name));
    exec.push(b' ');
    exec.extend(quoted(account.shell.as_os_str()));
    let args = vec![
        "-flp".into(),
        account.name.clone(),
        "/bin/bash".into(),
        "--noprofile".into(),
        "--norc".into(),
        "-c".into(),
        OsString::from_vec(exec),
    ];
    ("/usr/bin/login".into(), args, None)
}

/// The user's shell as a login shell: with `-` before its name.
#[cfg(not(target_os = "macos"))]
fn login_shell(account: &Account) -> (PathBuf, Vec<OsString>, Option<OsString>) {
    let mut arg0 = OsString::from("-");
    arg0.push(account.shell.file_name().unwrap_or(OsStr::new("sh")));
    (account.shell.clone(), Vec::new(), Some(arg0))
}

/// `text` in single quotes, for a shell, with each quote in it closed,
/// escaped and opened again.
#[cfg(any(target_os = "macos", test))]
fn quoted(text: &OsStr) -> Vec<u8> {
    let mut quoted = vec![b'\''];
    for &byte in text.as_bytes() {
        if byte == b'\'' {
            quoted.extend_from_slice(b"'\\''");
        } else {
            quoted.push(byte);
        }
    }
    quoted.push(b'\'');
    quoted
}

/// The variables that describe the terminal that started this one, which
/// the program must not see: `TERMINFO` would send it to another
/// terminal's description, `COLUMNS` and `LINES` to another size.
const LEFT_OUT: [&str; 3] = ["TERMINFO", "COLUMNS", "LINES"];

/// The environment of the program: `inherited`, without [`LEFT_OUT`], with
/// the terminal's variables, those of `options`, and a UTF-8 locale when no
/// locale is set.
fn environment(
    options: &Options,
    inherited: impl IntoIterator<Item = (OsString, OsString)>,
) -> BTreeMap<OsString, OsString> {
    let mut env: BTreeMap<OsString, OsString> = inherited.into_iter().collect();
    for name in LEFT_OUT {
        env.remove(OsStr::new(name));
    }
    let mut set = |name: &str, value: OsString| {
        env.insert(name.into(), value);
    };
    set("TERM", options.term.clone().into());
    set("COLORTERM", "truecolor".into());
    set("TERM_PROGRAM", "saetta".into());
    set("TERM_PROGRAM_VERSION", env!("CARGO_PKG_VERSION").into());
    match &options.resources_dir {
        Some(dir) => set("SAETTA_RESOURCES_DIR", dir.clone().into()),
        None => {
            env.remove(OsStr::new("SAETTA_RESOURCES_DIR"));
        }
    }
    env.extend(options.env.iter().cloned());
    let has_locale = ["LC_ALL", "LC_CTYPE", "LANG"].into_iter().any(|name| {
        env.get(OsStr::new(name))
            .is_some_and(|value| !value.is_empty())
    });
    if !has_locale {
        env.insert("LANG".into(), options.fallback_locale.clone().into());
    }
    env
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vars(pairs: &[(&str, &str)]) -> Vec<(OsString, OsString)> {
        pairs
            .iter()
            .map(|&(name, value)| (name.into(), value.into()))
            .collect()
    }

    fn get<'a>(env: &'a BTreeMap<OsString, OsString>, name: &str) -> Option<&'a str> {
        env.get(OsStr::new(name)).and_then(|value| value.to_str())
    }

    #[test]
    fn the_environment_describes_this_terminal() {
        let mut options = Options::new(Program::command("sh", ["-c", "true"]), "xterm-256color");
        options.resources_dir = Some("/Applications/Saetta.app/Contents/Resources".into());
        options.env = vars(&[("EDITOR", "vi"), ("COLORTERM", "24bit")]);
        let inherited = vars(&[
            ("PATH", "/usr/bin:/bin"),
            ("TERM", "dumb"),
            ("TERM_PROGRAM", "Apple_Terminal"),
            ("TERMINFO", "/opt/other/terminfo"),
            ("COLUMNS", "132"),
            ("LINES", "50"),
            ("LANG", "it_IT.UTF-8"),
        ]);
        let env = environment(&options, inherited);
        assert_eq!(get(&env, "PATH"), Some("/usr/bin:/bin"));
        assert_eq!(get(&env, "TERM"), Some("xterm-256color"));
        assert_eq!(get(&env, "TERM_PROGRAM"), Some("saetta"));
        assert_eq!(
            get(&env, "TERM_PROGRAM_VERSION"),
            Some(env!("CARGO_PKG_VERSION"))
        );
        assert_eq!(
            get(&env, "SAETTA_RESOURCES_DIR"),
            Some("/Applications/Saetta.app/Contents/Resources")
        );
        assert_eq!(get(&env, "EDITOR"), Some("vi"));
        assert_eq!(
            get(&env, "COLORTERM"),
            Some("24bit"),
            "the options come last"
        );
        for name in LEFT_OUT {
            assert_eq!(get(&env, name), None, "{name}");
        }
        assert_eq!(get(&env, "LANG"), Some("it_IT.UTF-8"), "a locale set stays");
    }

    #[test]
    fn a_utf8_locale_is_set_when_none_is() {
        let options = Options::new(Program::LoginShell, "xterm-256color");
        let env = environment(&options, vars(&[("HOME", "/Users/me")]));
        assert_eq!(get(&env, "LANG"), Some("en_US.UTF-8"));
        assert_eq!(get(&env, "COLORTERM"), Some("truecolor"));
        assert_eq!(get(&env, "SAETTA_RESOURCES_DIR"), None);
        // An empty variable sets no locale; any of the three does.
        let env = environment(&options, vars(&[("LANG", "")]));
        assert_eq!(get(&env, "LANG"), Some("en_US.UTF-8"));
        for name in ["LC_ALL", "LC_CTYPE"] {
            let env = environment(&options, vars(&[(name, "C.UTF-8")]));
            assert_eq!(get(&env, "LANG"), None, "{name}");
        }
        let stale = environment(&options, vars(&[("SAETTA_RESOURCES_DIR", "/old")]));
        assert_eq!(get(&stale, "SAETTA_RESOURCES_DIR"), None);
    }

    #[test]
    fn quotes_survive_a_shell() {
        assert_eq!(quoted(OsStr::new("/bin/zsh")), b"'/bin/zsh'");
        assert_eq!(quoted(OsStr::new("it's")), b"'it'\\''s'");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn the_login_shell_starts_through_login() {
        let account = Account {
            name: "me".into(),
            home: "/Users/me".into(),
            shell: "/opt/my shell/fish".into(),
        };
        let (program, args, arg0) = command_line(&Program::LoginShell, &account);
        assert_eq!(program, Path::new("/usr/bin/login"));
        assert_eq!(
            args,
            [
                "-flp",
                "me",
                "/bin/bash",
                "--noprofile",
                "--norc",
                "-c",
                "exec -l -a 'fish' '/opt/my shell/fish'"
            ]
        );
        assert_eq!(arg0, None);
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn the_login_shell_has_a_dash_before_its_name() {
        let account = Account {
            name: "me".into(),
            home: "/home/me".into(),
            shell: "/usr/bin/fish".into(),
        };
        let (program, args, arg0) = command_line(&Program::LoginShell, &account);
        assert_eq!(program, Path::new("/usr/bin/fish"));
        assert!(args.is_empty());
        assert_eq!(arg0, Some("-fish".into()));
    }

    #[test]
    fn the_account_is_read() {
        let account = Account::current();
        assert!(!account.name.is_empty());
        assert!(account.home.is_absolute());
        assert!(account.shell.is_absolute());
    }
}
