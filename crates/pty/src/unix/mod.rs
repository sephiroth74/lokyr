//! PTYs on Unix: a pseudo-terminal, and a child that leads its own session
//! with the PTY as its controlling terminal.

mod closing;
mod io;

use std::collections::BTreeMap;
use std::ffi::{CStr, CString, OsStr, OsString};
use std::io::{Error as IoError, ErrorKind};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::Arc;

pub use io::{PtyReader, PtyWriter};

use crate::{Error, Exit, Options, Program, Size};

/// A pseudo-terminal, with the program that runs in it.
///
/// Dropping it hangs the program up, as closing a terminal does, with
/// SIGHUP to the program and to the terminal's foreground process group,
/// and ends the reads and writes of its readers and writers, so that the
/// terminal closes once they drop, even for a program that ignores SIGHUP;
/// a thread then waits for the program to go, so that it leaves no zombie
/// behind.
#[derive(Debug)]
pub struct Pty {
    master: OwnedFd,
    /// What readers and writers wait on, which becomes ready once
    /// `_closing` drops with the PTY.
    closed: Arc<OwnedFd>,
    _closing: closing::Closing,
    /// The program, until the drop hands it to the thread that waits for
    /// it.
    child: Option<Child>,
    pid: u32,
    tty: PathBuf,
}

/// Opens a pseudo-terminal and starts the program of `options` in it.
pub fn spawn(options: &Options) -> Result<Pty, Error> {
    let (master, slave, tty) = open(options.size).map_err(Error::Open)?;
    let account = Account::current();
    let (program, args, arg0) = command_line(&options.program, &account);
    let env = environment(options, std::env::vars_os());
    let spawn_error = |source| Error::Spawn {
        program: program.display().to_string(),
        source,
    };
    let working_directory = options.working_directory.as_ref().unwrap_or(&account.home);
    // The working directory and the program, opened and looked for here
    // rather than by the child, so that either fails at once with an error
    // of its own.
    let dir = open_directory(working_directory).map_err(|source| Error::WorkingDirectory {
        path: working_directory.clone(),
        source,
    })?;
    let path = find_program(
        &program,
        env.get(OsStr::new("PATH")).map(OsString::as_os_str),
        working_directory,
    )
    .map_err(spawn_error)?;
    let mut command = Command::new(&path);
    command.args(&args);
    // The program's name, as it was given, rather than where it was found.
    command.arg0(arg0.as_deref().unwrap_or(program.as_os_str()));
    command.env_clear().envs(env);
    let dir_fd = dir.as_raw_fd();
    let descriptors = descriptor_limit();
    let mut listed = descriptor_list();
    for stdio in 0..3 {
        let slave = Stdio::from(slave.try_clone().map_err(spawn_error)?);
        match stdio {
            0 => command.stdin(slave),
            1 => command.stdout(slave),
            _ => command.stderr(slave),
        };
    }
    // SAFETY: the closure runs in the child between fork and exec, and calls
    // only functions that are async-signal-safe, without allocating; the
    // directory's descriptor stays open in the parent until the spawn ends.
    unsafe {
        command.pre_exec(move || {
            enter(dir_fd)?;
            lead_session()?;
            close_inherited(descriptors, &mut listed);
            Ok(())
        });
    }
    let (closed, closing) = closing::closing().map_err(Error::Open)?;
    // No lock keeps spawns apart. On macOS, `std` opens the pipe through
    // which a spawn learns that its child failed to exec, then marks it
    // closed on exec, so that a spawn on another thread in between gives
    // its child the pipe; that child marks it with its other descriptors,
    // and the pipe closes at its exec, rather than when its program ends.
    let child = command.spawn().map_err(spawn_error)?;
    // The child has its copies of the terminal's side; the parent keeps
    // none, so that the master reads the end when the program goes.
    drop(command);
    drop(slave);
    drop(dir);
    Ok(Pty {
        master,
        closed,
        _closing: closing,
        pid: child.id(),
        child: Some(child),
        tty,
    })
}

impl Pty {
    /// A reader of what the program writes. A read returns 0 once the
    /// program and its children have closed the terminal.
    pub fn reader(&self) -> std::io::Result<PtyReader> {
        Ok(PtyReader::new(
            self.master.try_clone()?,
            Arc::clone(&self.closed),
        ))
    }

    /// A writer of what is typed for the program.
    pub fn writer(&self) -> std::io::Result<PtyWriter> {
        Ok(PtyWriter::new(
            self.master.try_clone()?,
            Arc::clone(&self.closed),
        ))
    }

    /// The path of the terminal's device, as `tty` prints it.
    pub fn tty_name(&self) -> &Path {
        &self.tty
    }

    /// The process id of the program.
    pub fn process_id(&self) -> u32 {
        self.pid
    }

    /// Gives the terminal `size`, in cells and in pixels; the program's
    /// foreground process group learns of it through SIGWINCH.
    pub fn resize(&self, size: Size) -> std::io::Result<()> {
        let window = winsize(size);
        // SAFETY: `TIOCSWINSZ` reads a `winsize`, which `window` is.
        let failed = unsafe { libc::ioctl(self.master.as_raw_fd(), libc::TIOCSWINSZ, &window) };
        if failed == -1 {
            return Err(IoError::last_os_error());
        }
        Ok(())
    }

    /// The terminal's size, in cells and in pixels.
    pub fn size(&self) -> std::io::Result<Size> {
        // SAFETY: an all-zero `winsize` is a valid value to overwrite.
        let mut window: libc::winsize = unsafe { std::mem::zeroed() };
        // SAFETY: `TIOCGWINSZ` writes a `winsize`, which `window` is.
        let failed = unsafe { libc::ioctl(self.master.as_raw_fd(), libc::TIOCGWINSZ, &mut window) };
        if failed == -1 {
            return Err(IoError::last_os_error());
        }
        Ok(Size {
            cols: window.ws_col,
            rows: window.ws_row,
            width: window.ws_xpixel,
            height: window.ws_ypixel,
        })
    }

    /// Waits for the program to end, as `waitpid` does, and tells how it
    /// did. The end that the master reads can come later, when processes
    /// that the program started still have the terminal open, or never.
    pub fn wait(&mut self) -> std::io::Result<Exit> {
        self.child().wait().map(exit)
    }

    /// How the program ended, if it has, without waiting.
    pub fn try_wait(&mut self) -> std::io::Result<Option<Exit>> {
        Ok(self.child().try_wait()?.map(exit))
    }

    fn child(&mut self) -> &mut Child {
        self.child.as_mut().expect("only the drop takes the child")
    }

    /// The process group in the terminal's foreground: the program's, or
    /// that of a job that it runs; none once the program's session has
    /// ended.
    pub fn foreground_process_group(&self) -> Option<u32> {
        // SAFETY: `tcgetpgrp` only reads the terminal's state, through a
        // descriptor that `self` owns.
        let group = unsafe { libc::tcgetpgrp(self.master.as_raw_fd()) };
        u32::try_from(group).ok().filter(|&group| group != 0)
    }
}

impl Drop for Pty {
    fn drop(&mut self) {
        let Some(mut child) = self.child.take() else {
            return;
        };
        if !matches!(child.try_wait(), Ok(None)) {
            return;
        }
        // SAFETY: `getpgrp` only reads this process's group.
        let own = unsafe { libc::getpgrp() } as u32;
        // A job in the foreground, not the program, nor this process's own
        // group, which a mistaken id would hang up.
        let group = self
            .foreground_process_group()
            .filter(|&group| group != child.id() && group != own);
        // SAFETY: sending SIGHUP touches no memory. The program has not
        // been waited for, so its id is still its own.
        unsafe {
            libc::kill(child.id() as libc::pid_t, libc::SIGHUP);
            if let Some(group) = group {
                libc::kill(-(group as libc::pid_t), libc::SIGHUP);
            }
        }
        // Without a thread, the program stays a zombie until the app ends.
        let _ = std::thread::Builder::new()
            .name("pty-wait".into())
            .spawn(move || child.wait());
    }
}

/// How a process that `status` describes ended.
fn exit(status: ExitStatus) -> Exit {
    match (status.code(), status.signal()) {
        (Some(code), _) => Exit::Code(code),
        (None, Some(signal)) => Exit::Signal(signal),
        (None, None) => unreachable!("a process that was waited for exited or was signalled"),
    }
}

/// The errno with which macOS's kernel asks itself to open a device again:
/// it reaches the caller of `open` at times when several processes open
/// `/dev/ptmx` at once, and opening again succeeds.
const EREDRIVEOPEN: i32 = -6;

/// How many times to open again when the kernel gives [`EREDRIVEOPEN`].
const REDRIVE_TRIES: usize = 1000;

/// Opens a pseudo-terminal of `size`, and returns its master, its slave,
/// neither inherited across exec, and the slave's path. The master does not
/// block, for its readers and writers to wait on it with `poll`; the slave
/// does, as programs expect of their standard input.
///
/// `openpty` would do it in one call, but it closes the descriptors on exec
/// only after opening them, so that a fork elsewhere in the meantime can
/// inherit them, and it gives up on [`EREDRIVEOPEN`]. Each step here is
/// safe to run on several threads at once.
fn open(size: Size) -> std::io::Result<(OwnedFd, OwnedFd, PathBuf)> {
    let flags = libc::O_RDWR | libc::O_NOCTTY | libc::O_CLOEXEC;
    let open = |path: &CStr, more: libc::c_int| {
        for _ in 0..REDRIVE_TRIES {
            // SAFETY: `path` is a terminated string.
            let fd = unsafe { libc::open(path.as_ptr(), flags | more) };
            if fd != -1 {
                // SAFETY: `open` returned a descriptor that belongs to no
                // one else.
                return Ok(unsafe { OwnedFd::from_raw_fd(fd) });
            }
            let error = IoError::last_os_error();
            if error.raw_os_error() != Some(EREDRIVEOPEN) {
                return Err(error);
            }
            std::thread::yield_now();
        }
        Err(IoError::from_raw_os_error(EREDRIVEOPEN))
    };
    let master = open(c"/dev/ptmx", libc::O_NONBLOCK)?;
    // SAFETY: both act on the master, which `master` owns.
    unsafe {
        if libc::grantpt(master.as_raw_fd()) == -1 || libc::unlockpt(master.as_raw_fd()) == -1 {
            return Err(IoError::last_os_error());
        }
    }
    let tty = tty_name(&master)?;
    let path = CString::new(tty.as_os_str().as_bytes())
        .map_err(|_| IoError::from(ErrorKind::InvalidData))?;
    // Without O_NOCTTY, opening the slave would make it the app's own
    // controlling terminal, when the app leads a session without one.
    let slave = open(&path, 0)?;
    let window = winsize(size);
    // SAFETY: `TIOCSWINSZ` reads a `winsize`, which `window` is.
    if unsafe { libc::ioctl(master.as_raw_fd(), libc::TIOCSWINSZ, &window) } == -1 {
        return Err(IoError::last_os_error());
    }
    Ok((master, slave, tty))
}

fn winsize(size: Size) -> libc::winsize {
    libc::winsize {
        ws_row: size.rows,
        ws_col: size.cols,
        ws_xpixel: size.width,
        ws_ypixel: size.height,
    }
}

#[cfg(target_os = "macos")]
unsafe extern "C" {
    /// Writes the path of the slave of the pseudo-terminal whose master is
    /// `fd` to `buf`, `buflen` bytes at most: in macOS's libc since 10.13.4,
    /// though the `libc` crate does not declare it there.
    fn ptsname_r(fd: libc::c_int, buf: *mut libc::c_char, buflen: libc::size_t) -> libc::c_int;
}

#[cfg(not(target_os = "macos"))]
use libc::ptsname_r;

/// The path of the slave of the pseudo-terminal whose master is `master`.
fn tty_name(master: &OwnedFd) -> std::io::Result<PathBuf> {
    let mut name = [0 as libc::c_char; 128];
    // SAFETY: `ptsname_r` writes at most `name.len()` bytes, a terminated
    // string when it succeeds.
    let failed = unsafe { ptsname_r(master.as_raw_fd(), name.as_mut_ptr(), name.len()) };
    match failed {
        0 => {}
        // macOS returns -1 and sets errno; glibc returns the error.
        -1 => return Err(IoError::last_os_error()),
        error => return Err(IoError::from_raw_os_error(error)),
    }
    // SAFETY: `ptsname_r` succeeded, so `name` holds a terminated string.
    let name = unsafe { CStr::from_ptr(name.as_ptr()) };
    Ok(PathBuf::from(OsStr::from_bytes(name.to_bytes())))
}

/// The directories that `execvp` looks for a program in without a `PATH`:
/// the system's.
#[cfg(target_os = "macos")]
const DEFAULT_PATH: &str = "/usr/bin:/bin";
#[cfg(not(target_os = "macos"))]
const DEFAULT_PATH: &str = "/bin:/usr/bin";

/// Where `program` is: itself, if it names a path, as a name with a `/`
/// does; otherwise the first file of that name that this user can execute
/// in the directories of `path`, or of [`DEFAULT_PATH`] without one, as
/// `execvp` looks for it, those that are relative, the empty one included,
/// in the child's `working_directory`, made absolute, as the child runs the
/// program from there. It fails as `execvp` does: with `EACCES` if a file
/// of that name could not be executed, or, off macOS, a directory on the
/// way could not be searched, which macOS's `execvp` passes over; off
/// macOS, with the first error other than those that glibc's passes over,
/// as a loop of links; and with `ENOENT` otherwise.
fn find_program(
    program: &Path,
    path: Option<&OsStr>,
    working_directory: &Path,
) -> std::io::Result<PathBuf> {
    let name = program.as_os_str();
    if name.as_bytes().contains(&b'/') {
        return Ok(program.to_owned());
    }
    let mut denied = false;
    if !name.is_empty() {
        let working_directory = std::path::absolute(working_directory)?;
        for dir in std::env::split_paths(path.unwrap_or(OsStr::new(DEFAULT_PATH))) {
            let candidate = working_directory.join(dir).join(program);
            match std::fs::metadata(&candidate) {
                Ok(metadata) if metadata.is_file() && executable(&candidate, &metadata) => {
                    return Ok(candidate);
                }
                // A file that cannot be executed, or a directory, which
                // `execve` refuses with `EACCES`.
                Ok(_) => denied = true,
                // A directory on the way that cannot be searched: `EACCES`
                // with glibc's `execvp`, while macOS's tells it only of a
                // file that is there.
                Err(error) if error.kind() == ErrorKind::PermissionDenied => {
                    denied |= cfg!(not(target_os = "macos"));
                }
                // glibc's `execvp` goes on past these errors alone, and
                // stops at the others; macOS's passes over them all.
                Err(error)
                    if cfg!(not(target_os = "macos"))
                        && !matches!(
                            error.raw_os_error(),
                            Some(
                                libc::ENOENT
                                    | libc::ESTALE
                                    | libc::ENOTDIR
                                    | libc::ENODEV
                                    | libc::ETIMEDOUT
                            )
                        ) =>
                {
                    return Err(error);
                }
                Err(_) => {}
            }
        }
    }
    Err(IoError::from_raw_os_error(if denied {
        libc::EACCES
    } else {
        libc::ENOENT
    }))
}

/// Whether this user can execute the file at `path`, whose `metadata`
/// these are: it has an execute bit, which `execve` asks for whatever an
/// ACL grants, and `access` lets this user execute it.
fn executable(path: &Path, metadata: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt;
    let Ok(name) = CString::new(path.as_os_str().as_bytes()) else {
        return false;
    };
    // SAFETY: `name` is a C string that lives through the call, which only
    // looks at the file.
    metadata.permissions().mode() & 0o111 != 0
        && unsafe { libc::access(name.as_ptr(), libc::X_OK) == 0 }
}

/// How a working directory is opened: to be entered, not read, so that a
/// directory that can be searched but not listed is entered, as `chdir`
/// enters it, and a path that is not a directory fails at once, a FIFO's
/// too, rather than wait for a writer.
#[cfg(target_os = "macos")]
const SEARCH: libc::c_int = libc::O_SEARCH;
#[cfg(not(target_os = "macos"))]
const SEARCH: libc::c_int = libc::O_PATH | libc::O_DIRECTORY;

/// Opens the directory at `path`, for the child to enter.
fn open_directory(path: &Path) -> std::io::Result<std::fs::File> {
    use std::os::unix::fs::OpenOptionsExt;
    std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(SEARCH)
        .open(path)
}

/// Enters the directory that `fd` holds open, in the child, between fork
/// and exec, as `fchdir` is async-signal-safe.
fn enter(fd: std::os::fd::RawFd) -> std::io::Result<()> {
    // SAFETY: `fchdir` changes only the process's working directory, to the
    // directory that `fd`, open in the parent too, holds.
    if unsafe { libc::fchdir(fd) } == -1 {
        return Err(IoError::last_os_error());
    }
    Ok(())
}

/// The descriptors that a process can have open, as its soft limit says,
/// 65 536 at most, read in the parent, as `getrlimit` is not
/// async-signal-safe.
fn descriptor_limit() -> libc::c_int {
    let mut limit = libc::rlimit {
        rlim_cur: 0,
        rlim_max: 0,
    };
    // SAFETY: `getrlimit` writes the limits into `limit`.
    if unsafe { libc::getrlimit(libc::RLIMIT_NOFILE, &raw mut limit) } == -1 {
        return 1024;
    }
    libc::c_int::try_from(limit.rlim_cur.min(1 << 16)).unwrap_or(1 << 16)
}

/// An entry of the list of the descriptors that the child has open: macOS
/// lists them, and Linux marks them in one call, with no list.
#[cfg(target_os = "macos")]
type Listed = libc::proc_fdinfo;
#[cfg(not(target_os = "macos"))]
type Listed = ();

/// Room for the list of the descriptors that the child has open, which it
/// asks for between fork and exec, where it cannot allocate. Without a
/// buffer, macOS tells the room for every slot of the process's table of
/// descriptors and 20 more, so for the open ones, which take a slot each;
/// the table grows with the highest descriptor opened, and does not shrink.
/// 64 more, as other threads can open some before the fork, and a list
/// that fills up falls back on marking each descriptor in turn.
#[cfg(target_os = "macos")]
fn descriptor_list() -> Vec<Listed> {
    // SAFETY: without a buffer, `proc_pidinfo` tells how many bytes the
    // list of the process's open descriptors may take, and writes nothing.
    let bytes = unsafe {
        libc::proc_pidinfo(
            libc::getpid(),
            libc::PROC_PIDLISTFDS,
            0,
            std::ptr::null_mut(),
            0,
        )
    };
    let slots = usize::try_from(bytes).unwrap_or(0) / size_of::<Listed>();
    let none = libc::proc_fdinfo {
        proc_fd: 0,
        proc_fdtype: 0,
    };
    vec![none; slots + 64]
}

/// No list: Linux marks the descriptors in one call.
#[cfg(not(target_os = "macos"))]
fn descriptor_list() -> Vec<Listed> {
    Vec::new()
}

/// Marks every descriptor from 3 on closed on exec, in the child between
/// fork and exec, so that the program inherits none that this process did
/// without the mark, as one that a launcher or a CI runner leaves open:
/// the program has the terminal alone. Marked rather than closed, so that
/// `std`'s pipe, which tells the parent that exec failed, works until the
/// exec. Linux marks them in one call, and macOS marks those that the
/// system lists into `listed`, so that a spawn costs no more with a soft
/// limit of millions; on a kernel without the call, or with more open than
/// the list has room for, each descriptor below `limit` is marked in turn.
///
/// It runs between fork and exec, so it calls only async-signal-safe
/// functions: `fcntl`, `getpid`, Linux's `close_range` system call and
/// macOS's `proc_pidinfo`, which only wraps a system call.
fn close_inherited(limit: libc::c_int, listed: &mut [Listed]) {
    #[cfg(target_os = "macos")]
    let marked = {
        let room = libc::c_int::try_from(size_of_val(listed)).unwrap_or(libc::c_int::MAX);
        // SAFETY: `proc_pidinfo` writes the list of the process's open
        // descriptors into `listed`, `proc_fdinfo` entries, as many as its
        // bytes have room for, and returns how many bytes it wrote, or 0
        // if it failed.
        let bytes = unsafe {
            libc::proc_pidinfo(
                libc::getpid(),
                libc::PROC_PIDLISTFDS,
                0,
                listed.as_mut_ptr().cast(),
                room,
            )
        };
        mark_listed(bytes, listed, mark_closed_on_exec)
    };
    #[cfg(target_os = "linux")]
    let marked = {
        let _ = listed;
        // SAFETY: `close_range` with `CLOSE_RANGE_CLOEXEC` only marks the
        // process's descriptors from 3 on.
        let marked = unsafe {
            libc::syscall(
                libc::SYS_close_range,
                3,
                libc::c_uint::MAX,
                libc::CLOSE_RANGE_CLOEXEC,
            )
        };
        marked == 0
    };
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    let marked = {
        let _ = listed;
        false
    };
    if !marked {
        mark_each(limit, mark_closed_on_exec);
    }
}

/// Marks, with `mark`, the descriptors from 3 on that the first `bytes`
/// bytes of `listed` hold, which `proc_pidinfo` wrote, and tells whether
/// the list was whole: some written, and room left, as a full list may
/// have left descriptors out.
#[cfg(target_os = "macos")]
fn mark_listed(bytes: libc::c_int, listed: &[Listed], mut mark: impl FnMut(libc::c_int)) -> bool {
    let room = size_of_val(listed);
    let Ok(written) = usize::try_from(bytes) else {
        return false;
    };
    if written == 0 || written >= room {
        return false;
    }
    for info in &listed[..written / size_of::<Listed>()] {
        if info.proc_fd >= 3 {
            mark(info.proc_fd);
        }
    }
    true
}

/// Marks, with `mark`, each descriptor from 3 to `limit`, open or not.
fn mark_each(limit: libc::c_int, mut mark: impl FnMut(libc::c_int)) {
    for fd in 3..limit {
        mark(fd);
    }
}

/// Marks descriptor `fd` closed on exec, if it is open: async-signal-safe.
fn mark_closed_on_exec(fd: libc::c_int) {
    // SAFETY: `fcntl` reads and sets the flags of one descriptor, and fails
    // without effect for one that is not open.
    unsafe {
        let flags = libc::fcntl(fd, libc::F_GETFD);
        if flags != -1 && flags & libc::FD_CLOEXEC == 0 {
            libc::fcntl(fd, libc::F_SETFD, flags | libc::FD_CLOEXEC);
        }
    }
}

/// Makes the child the leader of a session, with the terminal on its
/// standard input as the controlling terminal, empties the signal mask,
/// which the child has from the thread that spawned it, and gives every
/// signal its default action, which an ignored one would not get across
/// exec.
///
/// It runs between fork and exec, so it calls only async-signal-safe
/// functions, and does not allocate.
fn lead_session() -> std::io::Result<()> {
    // SAFETY: `setsid` and `ioctl` only change the process's session and
    // terminal, `sigemptyset` and `sigprocmask` its signal mask, with a set
    // on the stack; `signal` with `SIG_DFL` installs no handler. All are
    // async-signal-safe.
    unsafe {
        if libc::setsid() == -1 {
            return Err(IoError::last_os_error());
        }
        if libc::ioctl(0, libc::TIOCSCTTY as _, 0) == -1 {
            return Err(IoError::last_os_error());
        }
        let mut none: libc::sigset_t = std::mem::zeroed();
        libc::sigemptyset(&mut none);
        if libc::sigprocmask(libc::SIG_SETMASK, &none, std::ptr::null_mut()) == -1 {
            return Err(IoError::last_os_error());
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
    set("TERM_PROGRAM", "bolt".into());
    set("TERM_PROGRAM_VERSION", env!("CARGO_PKG_VERSION").into());
    match &options.resources_dir {
        Some(dir) => {
            set("BOLT_RESOURCES_DIR", dir.clone().into());
            // The terminal's own entry, which the app keeps with its
            // resources, where programs would not otherwise find it.
            if matches!(options.term.as_str(), "xterm-bolt" | "bolt") {
                set("TERMINFO", dir.join("terminfo").into());
            }
        }
        None => {
            env.remove(OsStr::new("BOLT_RESOURCES_DIR"));
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

    #[cfg(target_os = "macos")]
    #[test]
    fn a_list_is_trusted_only_when_written_with_room_left() {
        let entry = |fd| libc::proc_fdinfo {
            proc_fd: fd,
            proc_fdtype: 0,
        };
        let listed = [entry(0), entry(1), entry(2), entry(7), entry(9), entry(0)];
        let size = libc::c_int::try_from(size_of::<Listed>()).unwrap();
        let marked = |bytes| {
            let mut fds = Vec::new();
            let whole = mark_listed(bytes, &listed, |fd| fds.push(fd));
            (whole, fds)
        };
        // Five entries of six: whole, and the standard descriptors left.
        assert_eq!(marked(5 * size), (true, vec![7, 9]));
        // A full list may have left descriptors out, and a call that
        // failed wrote nothing.
        assert_eq!(marked(6 * size), (false, vec![]));
        assert_eq!(marked(0), (false, vec![]));
        assert_eq!(marked(-1), (false, vec![]));
    }

    /// Whether `sh` kept a descriptor without the mark, at 100 or above,
    /// after `marking` ran in the child between fork and exec.
    fn kept_after(marking: impl Fn() + Send + Sync + 'static) -> bool {
        let file = std::fs::File::open("/dev/null").unwrap();
        // SAFETY: `fcntl` duplicates the file's descriptor, which stays
        // open, and the copy has no mark.
        let fd = unsafe { libc::fcntl(file.as_raw_fd(), libc::F_DUPFD, 100) };
        assert!(fd >= 100, "{}", IoError::last_os_error());
        let mut command = Command::new("/bin/sh");
        command.args([
            "-c",
            &format!("[ -e /dev/fd/{fd} ] && echo kept || echo closed"),
        ]);
        // SAFETY: the tests' `marking` calls only async-signal-safe
        // functions, and does not allocate.
        unsafe {
            command.pre_exec(move || {
                marking();
                Ok(())
            });
        }
        let output = command.output().unwrap();
        // SAFETY: the descriptor is this test's, and nothing else uses it.
        unsafe { libc::close(fd) };
        match String::from_utf8_lossy(&output.stdout).trim() {
            "kept" => true,
            "closed" => false,
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_descriptor_that_no_list_holds_is_marked_in_turn() {
        let limit = descriptor_limit();
        // Without the mark, the program has it.
        assert!(kept_after(|| {}));
        assert!(!kept_after(move || mark_each(limit, mark_closed_on_exec)));
        // With no room for a list, macOS marks each descriptor in turn, and
        // Linux all of them in one call.
        assert!(!kept_after(move || close_inherited(limit, &mut [])));
    }

    #[test]
    fn a_program_that_is_not_there_fails_as_not_found() {
        let options = Options::new(
            Program::command("bolt-no-such-program", [""; 0]),
            "xterm-256color",
        );
        let failed = spawn(&options).map(drop);
        assert!(
            matches!(&failed, Err(Error::Spawn { source, .. }) if source.kind() == ErrorKind::NotFound),
            "{failed:?}"
        );
    }

    /// A directory of the temporary one, removed as it drops, however the
    /// test ends.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("bolt-pty-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn a_program_is_found_as_execvp_finds_it() {
        use std::os::unix::fs::PermissionsExt;
        let temp = TempDir::new("find");
        let dir = &temp.0;
        for sub in ["a", "b", "c/tool"] {
            std::fs::create_dir_all(dir.join(sub)).unwrap();
        }
        // Not executable in `a`, a directory in `c`, and executable in `b`.
        std::fs::write(dir.join("a/tool"), "").unwrap();
        std::fs::write(dir.join("b/tool"), "").unwrap();
        std::fs::set_permissions(dir.join("b/tool"), std::fs::Permissions::from_mode(0o755))
            .unwrap();
        let path = |dirs: &str| OsString::from(dirs);
        let find = |program: &str, path: Option<&OsString>| {
            find_program(Path::new(program), path.map(OsString::as_os_str), dir)
        };
        let error = |found: std::io::Result<PathBuf>| found.unwrap_err().raw_os_error();
        let all = path(&format!("/nonexistent:{0}/a:{0}/c:{0}/b", dir.display()));
        assert_eq!(find("tool", Some(&all)).unwrap(), dir.join("b/tool"));
        // Relative to the working directory, the empty entry too.
        assert_eq!(
            find("tool", Some(&path("a:c:b"))).unwrap(),
            dir.join("b/tool")
        );
        assert_eq!(error(find("tool", Some(&path("")))), Some(libc::ENOENT));
        // A file that cannot be executed, and a directory, are refused.
        assert_eq!(error(find("tool", Some(&path("a")))), Some(libc::EACCES));
        assert_eq!(error(find("tool", Some(&path("c")))), Some(libc::EACCES));
        assert_eq!(
            error(find("tool", Some(&path("nonexistent")))),
            Some(libc::ENOENT)
        );
        // A directory that cannot be searched, which a file of that name may
        // be in.
        std::fs::create_dir(dir.join("d")).unwrap();
        std::fs::write(dir.join("d/tool"), "").unwrap();
        std::fs::set_permissions(dir.join("d"), std::fs::Permissions::from_mode(0o000)).unwrap();
        let unsearchable = error(find("tool", Some(&path("d"))));
        std::fs::set_permissions(dir.join("d"), std::fs::Permissions::from_mode(0o755)).unwrap();
        // macOS's `execvp` tells `EACCES` only of a file that is there.
        let expected = if cfg!(target_os = "macos") {
            libc::ENOENT
        } else {
            libc::EACCES
        };
        assert_eq!(unsearchable, Some(expected));
        // A name with a slash is a path; without `PATH`, the system's.
        assert_eq!(find("./tool", Some(&all)).unwrap(), PathBuf::from("./tool"));
        let system = find("sh", None).unwrap();
        assert!(
            ["/usr/bin/sh", "/bin/sh"]
                .map(PathBuf::from)
                .contains(&system)
        );
        assert_eq!(error(find("", Some(&all))), Some(libc::ENOENT));
    }

    #[test]
    fn a_loop_of_links_stops_the_lookup_off_macos() {
        use std::os::unix::fs::PermissionsExt;
        let temp = TempDir::new("loop");
        for dir in ["a", "b"] {
            std::fs::create_dir(temp.0.join(dir)).unwrap();
        }
        // A link to itself, which `stat` refuses with `ELOOP`, then the
        // program in the next directory.
        std::os::unix::fs::symlink("tool", temp.0.join("a/tool")).unwrap();
        let tool = temp.0.join("b/tool");
        std::fs::write(&tool, "").unwrap();
        std::fs::set_permissions(&tool, std::fs::Permissions::from_mode(0o755)).unwrap();
        let found = find_program(Path::new("tool"), Some(OsStr::new("a:b")), &temp.0);
        if cfg!(target_os = "macos") {
            assert_eq!(found.unwrap(), tool);
        } else {
            assert_eq!(found.unwrap_err().raw_os_error(), Some(libc::ELOOP));
        }
    }

    #[test]
    fn a_relative_working_directory_is_where_relative_entries_are() {
        use std::os::unix::fs::PermissionsExt;
        let temp = TempDir::new("relative");
        let tool = temp.0.join("wd/bin/tool");
        std::fs::create_dir_all(tool.parent().unwrap()).unwrap();
        std::fs::write(&tool, "").unwrap();
        std::fs::set_permissions(&tool, std::fs::Permissions::from_mode(0o755)).unwrap();
        // The working directory, relative to this one.
        let here = std::env::current_dir().unwrap();
        let mut relative = PathBuf::new();
        for _ in here.components().skip(1) {
            relative.push("..");
        }
        relative.push(temp.0.join("wd").strip_prefix("/").unwrap());
        let found = find_program(Path::new("tool"), Some(OsStr::new("bin")), &relative).unwrap();
        assert!(found.is_absolute(), "{}", found.display());
        assert_eq!(found.canonicalize().unwrap(), tool.canonicalize().unwrap());
    }

    #[test]
    fn a_directory_that_cannot_be_opened_fails_with_its_name() {
        let mut options = Options::new(Program::command("sh", ["-c", "true"]), "xterm-256color");
        options.working_directory = Some("/no/such/directory".into());
        let failed = spawn(&options).map(drop);
        assert!(
            matches!(&failed, Err(Error::WorkingDirectory { source, .. }) if source.kind() == ErrorKind::NotFound),
            "{failed:?}"
        );
        // The message names the directory, and the cause comes once, as
        // the source.
        let error = failed.unwrap_err();
        assert_eq!(error.to_string(), "cannot enter /no/such/directory");
        let source = std::error::Error::source(&error).map(ToString::to_string);
        assert_eq!(
            source.as_deref(),
            Some("No such file or directory (os error 2)")
        );
    }

    #[test]
    fn an_empty_working_directory_is_not_there() {
        let mut options = Options::new(Program::command("sh", ["-c", "true"]), "xterm-256color");
        options.working_directory = Some(PathBuf::new());
        let error = spawn(&options).unwrap_err();
        assert!(
            matches!(&error, Error::WorkingDirectory { source, .. } if source.raw_os_error() == Some(libc::ENOENT)),
            "{error:?}"
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn a_file_that_only_an_acl_lets_run_is_passed_over() {
        use std::os::unix::fs::PermissionsExt;
        let temp = TempDir::new("acl");
        for (dir, mode) in [("a", 0o644), ("b", 0o755)] {
            std::fs::create_dir(temp.0.join(dir)).unwrap();
            let tool = temp.0.join(dir).join("tool");
            std::fs::write(&tool, "").unwrap();
            std::fs::set_permissions(&tool, std::fs::Permissions::from_mode(mode)).unwrap();
        }
        let user =
            String::from_utf8(Command::new("id").arg("-un").output().unwrap().stdout).unwrap();
        let granted = Command::new("chmod")
            .arg("+a")
            .arg(format!("{} allow execute", user.trim()))
            .arg(temp.0.join("a/tool"))
            .status()
            .unwrap();
        assert!(granted.success());
        let found = find_program(Path::new("tool"), Some(OsStr::new("a:b")), &temp.0).unwrap();
        assert_eq!(found, temp.0.join("b/tool"));
    }

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
        options.resources_dir = Some("/Applications/Bolt.app/Contents/Resources".into());
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
        assert_eq!(get(&env, "TERM_PROGRAM"), Some("bolt"));
        assert_eq!(
            get(&env, "TERM_PROGRAM_VERSION"),
            Some(env!("CARGO_PKG_VERSION"))
        );
        assert_eq!(
            get(&env, "BOLT_RESOURCES_DIR"),
            Some("/Applications/Bolt.app/Contents/Resources")
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
        assert_eq!(get(&env, "BOLT_RESOURCES_DIR"), None);
        // An empty variable sets no locale; any of the three does.
        let env = environment(&options, vars(&[("LANG", "")]));
        assert_eq!(get(&env, "LANG"), Some("en_US.UTF-8"));
        for name in ["LC_ALL", "LC_CTYPE"] {
            let env = environment(&options, vars(&[(name, "C.UTF-8")]));
            assert_eq!(get(&env, "LANG"), None, "{name}");
        }
        let stale = environment(&options, vars(&[("BOLT_RESOURCES_DIR", "/old")]));
        assert_eq!(get(&stale, "BOLT_RESOURCES_DIR"), None);
    }

    #[test]
    fn terminfo_points_to_the_entry_that_the_app_keeps() {
        let mut options = Options::new(Program::command("sh", ["-c", "true"]), "xterm-bolt");
        let inherited = || vars(&[("TERMINFO", "/opt/other/terminfo")]);
        // Without the resources, there is no entry to point to.
        assert_eq!(get(&environment(&options, inherited()), "TERMINFO"), None);
        options.resources_dir = Some("/Applications/Bolt.app/Contents/Resources".into());
        assert_eq!(
            get(&environment(&options, inherited()), "TERMINFO"),
            Some("/Applications/Bolt.app/Contents/Resources/terminfo")
        );
        // And for the entry's other name.
        options.term = "bolt".to_owned();
        assert_eq!(
            get(&environment(&options, inherited()), "TERMINFO"),
            Some("/Applications/Bolt.app/Contents/Resources/terminfo")
        );
        // Another TERM finds its entry where the system keeps it.
        options.term = "xterm-256color".to_owned();
        assert_eq!(get(&environment(&options, inherited()), "TERMINFO"), None);
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
