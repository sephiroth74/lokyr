//! The app's log: a file of its own, `~/Library/Logs/Lokyr/lokyr.log` on
//! the Mac, where Console shows it, and `~/.local/state/lokyr/lokyr.log`
//! elsewhere, or under `$XDG_STATE_HOME`. A panic writes there its version,
//! its thread, its message and place, and its backtrace, before the
//! process ends or its plugin is disabled; and the app's standard error
//! goes there too when no terminal reads it, as when the Finder or the Dock
//! starts Lokyr, so that what the app tells is kept. The file starts again
//! past [`MOST`], as the app starts and as it runs, its lines before kept
//! in `lokyr.1.log`.
//!
//! The standard error goes to the file through a pipe that a thread of its
//! own drains, which drops what the file does not take: the standard
//! library's printing panics on a write's error, as a full disk's, which a
//! pipe whose reader never stops never gives. As the process exits, or
//! aborts, what the pipe holds still goes to the file.

use std::fmt::Write as _;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write as _};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// The size past which the log starts again.
const MOST: u64 = 1024 * 1024;

/// The log's file of this system, if the user has a home.
pub fn file() -> Option<PathBuf> {
    if cfg!(target_os = "macos") {
        return Some(std::env::home_dir()?.join("Library/Logs/Lokyr/lokyr.log"));
    }
    let state = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| Some(std::env::home_dir()?.join(".local/state")))?;
    Some(state.join("lokyr/lokyr.log"))
}

/// Starts the log at `path` for the app of `version`: its directory made,
/// the file before kept aside past [`MOST`], the panics written there, and
/// the standard error too where `quiet_stderr` holds and no terminal reads
/// it. Whether the standard error goes to the log.
pub fn start(path: &Path, version: &str, quiet_stderr: bool) -> std::io::Result<bool> {
    start_with(path, version, quiet_stderr && !terminal_reads_stderr())
}

/// [`start`], the standard error going to the log when `to_log` holds,
/// whatever reads it now, as a test of it has it.
#[doc(hidden)]
pub fn start_with(path: &Path, version: &str, to_log: bool) -> std::io::Result<bool> {
    if let Some(directory) = path.parent() {
        fs::create_dir_all(directory)?;
    }
    if fs::metadata(path).is_ok_and(|metadata| metadata.len() > MOST) {
        fs::rename(path, kept(path))?;
    }
    let to_log = to_log && drain_stderr(path).is_ok();
    let (path, version) = (path.to_owned(), version.to_owned());
    let before = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let report = report(
            &version,
            &thread_name(),
            &info.to_string(),
            &std::backtrace::Backtrace::force_capture().to_string(),
            SystemTime::now(),
        );
        // Into the file at once, ahead of the pipe's lines, as the process
        // may end before its thread drains them; a write that fails goes.
        let _ = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .and_then(|mut file| file.write_all(report.as_bytes()));
        // The terminal's too, unless the standard error is the log.
        if !to_log {
            before(info);
        }
    }));
    Ok(to_log)
}

/// What the log tells of a panic: when, the app's `version`, the
/// `thread`, the `panic` as it tells itself, its place and its message,
/// and the `backtrace`.
fn report(version: &str, thread: &str, panic: &str, backtrace: &str, at: SystemTime) -> String {
    let mut report = String::new();
    let _ = writeln!(
        report,
        "{} lokyr {version}: the thread '{thread}' {panic}",
        timestamp(at)
    );
    let _ = writeln!(report, "stack backtrace:\n{backtrace}");
    report
}

/// The file that keeps the log's lines before it starts again.
fn kept(path: &Path) -> PathBuf {
    path.with_extension("1.log")
}

/// The name of the thread that runs, as the system has it, which the
/// standard library gives a named thread; `main` for the main thread's
/// empty one. It takes no thread's local data, which a panic as they go
/// would find gone.
#[allow(unsafe_code)]
fn thread_name() -> String {
    #[cfg(unix)]
    {
        let mut name = [0_u8; 64];
        // SAFETY: the buffer holds 64 bytes, which the call fills with a
        // string that ends with a NUL, of the thread that calls it.
        let told = unsafe {
            libc::pthread_getname_np(libc::pthread_self(), name.as_mut_ptr().cast(), name.len())
        };
        if told == 0 {
            let name = std::ffi::CStr::from_bytes_until_nul(&name)
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default();
            return if name.is_empty() {
                "main".to_owned()
            } else {
                name
            };
        }
    }
    "unnamed".to_owned()
}

/// The log's file, as its drain writes it: what it took since it opened,
/// and its start again past [`MOST`].
struct Log {
    path: PathBuf,
    file: Option<File>,
    size: u64,
}

impl Log {
    fn open(path: &Path) -> Self {
        let file = OpenOptions::new().create(true).append(true).open(path).ok();
        let size = file
            .as_ref()
            .and_then(|file| file.metadata().ok())
            .map_or(0, |metadata| metadata.len());
        Self {
            path: path.to_owned(),
            file,
            size,
        }
    }

    /// Writes `bytes`, or drops them where the file does not take them;
    /// past [`MOST`], the file starts again, the one before kept aside.
    fn write(&mut self, bytes: &[u8]) {
        if let Some(file) = &mut self.file
            && file.write_all(bytes).is_ok()
        {
            self.size += bytes.len() as u64;
        }
        if self.size > MOST {
            self.file = None;
            let _ = fs::rename(&self.path, kept(&self.path));
            *self = Self::open(&self.path);
            follow(self.file.as_ref());
        }
    }
}

/// Has the abort's handler write to `file`, the log's file once it started
/// again, in place of the one kept aside, which its descriptor still named:
/// the file's descriptor copied onto the handler's, whose number stays.
#[cfg(unix)]
#[allow(unsafe_code)]
fn follow(file: Option<&File>) {
    use std::os::fd::AsRawFd;
    let abort = FILE.load(std::sync::atomic::Ordering::SeqCst);
    if let Some(file) = file
        && abort >= 0
    {
        // SAFETY: two open descriptors: `dup2` puts the file's in the
        // handler's place at once, which closes the one before.
        unsafe { libc::dup2(file.as_raw_fd(), abort) };
    }
}

#[cfg(not(unix))]
fn follow(_: Option<&File>) {}

/// Whether the drain holds what the pipe gives until the process ends, as
/// a test of the end's way to the log has it.
static HELD: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Has the drain of the log that starts next hold what the pipe gives until
/// the process ends, which then writes it, as a test of that end has it.
#[doc(hidden)]
pub fn hold_drain() {
    HELD.store(true, std::sync::atomic::Ordering::SeqCst);
}

/// Whether the drain holds, past its last read, as [`hold_drain`] asked.
static HOLDING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Whether the drain holds, as [`hold_drain`] asked: it reads no more, and
/// what the standard error writes from now stays in the pipe until the
/// process ends, as a test of that end has it. A drain that waits in a read
/// holds once the read returns and its bytes are written.
#[doc(hidden)]
pub fn drain_holds() -> bool {
    HOLDING.load(std::sync::atomic::Ordering::SeqCst)
}

/// Writes what `pipe` gives into `log`, until the pipe ends, which it does
/// not while the process runs.
fn drain(mut pipe: impl Read, log: &mut Log) {
    let mut buffer = [0_u8; 8192];
    loop {
        while HELD.load(std::sync::atomic::Ordering::SeqCst) {
            HOLDING.store(true, std::sync::atomic::Ordering::SeqCst);
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        match pipe.read(&mut buffer) {
            Ok(0) => return,
            Ok(read) => log.write(&buffer[..read]),
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
            Err(_) => return,
        }
    }
}

/// The thread that drains the standard error's pipe into the log, and the
/// log's file, until the process ends, as [`finish`] takes them.
#[cfg(unix)]
static DRAINING: std::sync::Mutex<Option<(std::thread::JoinHandle<()>, PathBuf)>> =
    std::sync::Mutex::new(None);

/// The pipe's reading end, and a descriptor of the log's file, through
/// which an abort's handler copies what the pipe holds: -1 for none.
#[cfg(unix)]
static PIPE: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(-1);
#[cfg(unix)]
static FILE: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(-1);

/// The abort's action before the log's handler, which the handler gives
/// the signal back to.
#[cfg(unix)]
static ABORT_BEFORE: std::sync::OnceLock<libc::sigaction> = std::sync::OnceLock::new();

/// How long the end of the process waits for the drain to write what the
/// pipe holds.
#[cfg(unix)]
const FINISH: std::time::Duration = std::time::Duration::from_millis(500);

/// Has the standard error go to the log at `path`, through a pipe that a
/// thread drains, started before the pipe takes the standard error, so that
/// its failure, or the file's, leaves the standard error as it was; its
/// reading end closes in the programs that the app starts. As the process exits, the standard
/// error takes the file itself and the drain writes what the pipe holds;
/// as it aborts, a handler of the signal copies it.
#[cfg(unix)]
#[allow(unsafe_code)]
fn drain_stderr(path: &Path) -> std::io::Result<()> {
    use std::os::fd::{FromRawFd, IntoRawFd};
    // A file that does not open, as of a directory that another user owns,
    // leaves the standard error as it was, where a pipe would drop all.
    let mut log = Log::open(path);
    if log.file.is_none() {
        return Err(std::io::Error::other("the log's file does not open"));
    }
    let mut ends = [0; 2];
    // SAFETY: `pipe` fills the two descriptors of the array's two.
    if unsafe { libc::pipe(ends.as_mut_ptr()) } != 0 {
        return Err(std::io::Error::last_os_error());
    }
    let [reading, writing] = ends;
    // SAFETY: the reading end is open, and closes as a program starts; this
    // file owns it from now.
    let pipe = unsafe {
        libc::fcntl(reading, libc::F_SETFD, libc::FD_CLOEXEC);
        File::from_raw_fd(reading)
    };
    let thread = std::thread::Builder::new()
        .name("lokyr-log".to_owned())
        .spawn(move || drain(pipe, &mut log));
    let thread = match thread {
        Ok(thread) => thread,
        Err(error) => {
            // SAFETY: the writing end is open, and this function's alone.
            unsafe { libc::close(writing) };
            return Err(error);
        }
    };
    // The abort's way to the file, opened before the pipe takes the
    // standard error.
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_or(-1, IntoRawFd::into_raw_fd);
    // SAFETY: the writing end takes the standard error's place, its own
    // descriptor closed once copied; without the copy the drain finds the
    // pipe's end and stops.
    unsafe {
        let copied = libc::dup2(writing, libc::STDERR_FILENO);
        libc::close(writing);
        if copied < 0 {
            if file >= 0 {
                libc::close(file);
            }
            return Err(std::io::Error::last_os_error());
        }
    }
    PIPE.store(reading, std::sync::atomic::Ordering::SeqCst);
    FILE.store(file, std::sync::atomic::Ordering::SeqCst);
    let mut draining = DRAINING
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let first = draining.is_none();
    *draining = Some((thread, path.to_owned()));
    drop(draining);
    if first {
        // SAFETY: a function of C's calling convention, which the exit
        // calls once, and an action of the signal that a handler of its
        // convention takes, the one before kept to give the signal back.
        unsafe {
            libc::atexit(finish_at_exit);
            let mut action: libc::sigaction = std::mem::zeroed();
            action.sa_sigaction = on_abort as extern "C" fn(libc::c_int) as libc::sighandler_t;
            let mut before: libc::sigaction = std::mem::zeroed();
            if libc::sigaction(libc::SIGABRT, &action, &mut before) == 0 {
                let _ = ABORT_BEFORE.set(before);
            }
        }
    }
    Ok(())
}

/// Ends the standard error's way through the pipe, as the process ends:
/// the standard error takes the log's file itself, which closes the
/// pipe's writing end, and the drain writes what the pipe holds, which it
/// then ends, within [`FINISH`]. What the app writes after goes to the file
/// at once.
#[cfg(unix)]
#[allow(unsafe_code)]
pub fn finish() {
    use std::os::fd::AsRawFd;
    let Some((thread, path)) = DRAINING
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .take()
    else {
        return;
    };
    let Ok(file) = OpenOptions::new().create(true).append(true).open(&path) else {
        return;
    };
    // SAFETY: the file's descriptor, open, takes the standard error's place.
    if unsafe { libc::dup2(file.as_raw_fd(), libc::STDERR_FILENO) } < 0 {
        return;
    }
    // The abort's handler has nothing left to copy; a drain held goes on.
    PIPE.store(-1, std::sync::atomic::Ordering::SeqCst);
    HELD.store(false, std::sync::atomic::Ordering::SeqCst);
    let until = std::time::Instant::now() + FINISH;
    while !thread.is_finished() && std::time::Instant::now() < until {
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    if thread.is_finished() {
        let _ = thread.join();
    }
}

/// [`finish`], as the process exits.
#[cfg(unix)]
extern "C" fn finish_at_exit() {
    // A panic must not cross into C's exit.
    let _ = std::panic::catch_unwind(finish);
}

/// Copies what the standard error's pipe holds into the log as the process
/// aborts, as after a stack's overflow, whose message is the pipe's, then
/// gives the signal to the action before, the system's, which ends the
/// process and reports its crash. Only calls that a signal's handler may
/// make: `fcntl`, `read`, `write`, `sigaction` and `raise`.
#[cfg(unix)]
#[allow(unsafe_code)]
extern "C" fn on_abort(signal: libc::c_int) {
    let pipe = PIPE.load(std::sync::atomic::Ordering::SeqCst);
    let file = FILE.load(std::sync::atomic::Ordering::SeqCst);
    // SAFETY: descriptors that stay open as the process runs, a buffer on
    // the stack of its length, and the action before, as `sigaction` gave
    // it, or the system's default.
    unsafe {
        if pipe >= 0 && file >= 0 {
            let flags = libc::fcntl(pipe, libc::F_GETFL);
            libc::fcntl(pipe, libc::F_SETFL, flags | libc::O_NONBLOCK);
            let mut buffer = [0_u8; 4096];
            loop {
                let read = libc::read(pipe, buffer.as_mut_ptr().cast(), buffer.len());
                if read <= 0 {
                    break;
                }
                libc::write(file, buffer.as_ptr().cast(), read as usize);
            }
        }
        match ABORT_BEFORE.get() {
            Some(before) => {
                libc::sigaction(libc::SIGABRT, before, std::ptr::null_mut());
            }
            None => {
                libc::signal(libc::SIGABRT, libc::SIG_DFL);
            }
        }
        libc::raise(signal);
    }
}

#[cfg(not(unix))]
fn drain_stderr(_path: &Path) -> std::io::Result<()> {
    Err(std::io::Error::other("the standard error stays"))
}

/// Ends the standard error's way to the log, which has none here.
#[cfg(not(unix))]
pub fn finish() {}

/// `at` as RFC 3339's date and time in UTC, to the second.
pub(crate) fn timestamp(at: SystemTime) -> String {
    let seconds = at
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs());
    let (days, rest) = (seconds / 86_400, seconds % 86_400);
    // The civil date of the days since 1970-01-01, after Howard Hinnant's
    // `civil_from_days` (https://howardhinnant.github.io/date_algorithms.html).
    let days = i64::try_from(days).unwrap_or(0) + 719_468;
    let era = days.div_euclid(146_097);
    let of_era = days.rem_euclid(146_097);
    let year_of_era = (of_era - of_era / 1460 + of_era / 36_524 - of_era / 146_096) / 365;
    let of_year = of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted_month = (5 * of_year + 2) / 153;
    let day = of_year - (153 * shifted_month + 2) / 5 + 1;
    let month = if shifted_month < 10 {
        shifted_month + 3
    } else {
        shifted_month - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rest / 3600,
        rest % 3600 / 60,
        rest % 60
    )
}

/// Whether a terminal reads the standard error.
fn terminal_reads_stderr() -> bool {
    use std::io::IsTerminal;
    std::io::stderr().is_terminal()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn a_time_tells_its_date_in_utc() {
        assert_eq!(timestamp(UNIX_EPOCH), "1970-01-01T00:00:00Z");
        // 2 November 2026, 13:25:07 UTC.
        let at = UNIX_EPOCH + Duration::from_secs(1_793_625_907);
        assert_eq!(timestamp(at), "2026-11-02T13:25:07Z");
        // A leap day.
        let at = UNIX_EPOCH + Duration::from_secs(1_709_164_800);
        assert_eq!(timestamp(at), "2024-02-29T00:00:00Z");
    }

    #[test]
    fn a_panic_s_report_tells_the_version_the_thread_and_the_backtrace() {
        // The panic tells itself, as its `Display` does, its place first.
        let report = report(
            "0.3.0-20261102",
            "lokyr-read",
            "panicked at crates/x.rs:1:2:\na test's panic",
            "   0: lokyr::main\n",
            UNIX_EPOCH,
        );
        assert!(report.starts_with(
            "1970-01-01T00:00:00Z lokyr 0.3.0-20261102: the thread 'lokyr-read' panicked at crates/x.rs:1:2:\na test's panic\n"
        ));
        assert_eq!(report.matches("panicked").count(), 1);
        assert!(report.contains("stack backtrace:\n   0: lokyr::main\n"));
    }

    #[test]
    fn the_running_thread_s_name_is_told_without_its_locals() {
        let name = std::thread::Builder::new()
            .name("lokyr-test".to_owned())
            .spawn(thread_name)
            .unwrap()
            .join()
            .unwrap();
        assert_eq!(name, "lokyr-test");
    }

    #[test]
    fn the_log_starts_again_past_its_size_as_it_runs() {
        let directory = std::env::temp_dir().join(format!("lokyr-log-turn-{}", std::process::id()));
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join("lokyr.log");
        let mut log = Log::open(&path);
        let line = [b'x'; 1000];
        for _ in 0..1100 {
            log.write(&line);
        }
        // Past 1 MiB, the lines before went aside, the file started again.
        assert!(fs::metadata(kept(&path)).unwrap().len() > MOST);
        assert!(fs::metadata(&path).unwrap().len() < MOST);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn a_drain_whose_file_takes_nothing_reads_the_pipe_to_its_end() {
        // A file that cannot open, as a disk that takes nothing: the drain
        // drops the bytes, and reads on.
        let mut log = Log::open(Path::new("/nonexistent-directory-of-lokyr/lokyr.log"));
        assert!(log.file.is_none());
        let mut pipe = std::io::Cursor::new(vec![b'x'; 100_000]);
        drain(&mut pipe, &mut log);
        assert_eq!(pipe.position(), 100_000);
    }

    #[test]
    fn the_log_s_file_is_the_system_s_place() {
        let file = file().unwrap();
        if cfg!(target_os = "macos") {
            assert!(file.ends_with("Library/Logs/Lokyr/lokyr.log"));
        } else {
            assert!(file.ends_with("lokyr/lokyr.log"));
        }
    }
}
