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
//! pipe whose reader never stops never gives.

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
        }
    }
}

/// Writes what `pipe` gives into `log`, until the pipe ends, which it does
/// not while the process runs.
fn drain(mut pipe: impl Read, log: &mut Log) {
    let mut buffer = [0_u8; 8192];
    loop {
        match pipe.read(&mut buffer) {
            Ok(0) => return,
            Ok(read) => log.write(&buffer[..read]),
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
            Err(_) => return,
        }
    }
}

/// Has the standard error go to the log at `path`, through a pipe that a
/// thread drains; its reading end closes in the programs that the app
/// starts.
#[cfg(unix)]
fn drain_stderr(path: &Path) -> std::io::Result<()> {
    use std::os::fd::FromRawFd;
    let mut ends = [0; 2];
    // SAFETY: `pipe` fills the two descriptors of the array's two.
    if unsafe { libc::pipe(ends.as_mut_ptr()) } != 0 {
        return Err(std::io::Error::last_os_error());
    }
    let [reading, writing] = ends;
    // SAFETY: both descriptors are the pipe's, open: the reading end closes
    // as a program starts, and the writing end takes the standard error's
    // place, its own descriptor closed once copied.
    unsafe {
        libc::fcntl(reading, libc::F_SETFD, libc::FD_CLOEXEC);
        let copied = libc::dup2(writing, libc::STDERR_FILENO);
        libc::close(writing);
        if copied < 0 {
            libc::close(reading);
            return Err(std::io::Error::last_os_error());
        }
    }
    // SAFETY: the reading end is open, and this file owns it from now.
    let pipe = unsafe { File::from_raw_fd(reading) };
    let mut log = Log::open(path);
    std::thread::Builder::new()
        .name("lokyr-log".to_owned())
        .spawn(move || drain(pipe, &mut log))?;
    Ok(())
}

#[cfg(not(unix))]
fn drain_stderr(_path: &Path) -> std::io::Result<()> {
    Err(std::io::Error::other("the standard error stays"))
}

/// `at` as RFC 3339's date and time in UTC, to the second.
fn timestamp(at: SystemTime) -> String {
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
