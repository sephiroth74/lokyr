//! The app's log: a file of its own, `~/Library/Logs/Lokyr/lokyr.log` on
//! the Mac, where Console shows it, and `~/.local/state/lokyr/lokyr.log`
//! elsewhere, or under `$XDG_STATE_HOME`. A panic writes there its version,
//! its thread, its message and place, and its backtrace, before the
//! process ends or its plugin is disabled; and the app's standard error
//! goes there too when no terminal reads it, as when the Finder or the Dock
//! starts Lokyr, so that what the app tells is kept. The file starts again
//! past [`MOST`], its lines before kept in `lokyr.1.log`.

use std::fmt::Write as _;
use std::fs::{self, OpenOptions};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// The size past which the log starts again, as the app starts.
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
    if let Some(directory) = path.parent() {
        fs::create_dir_all(directory)?;
    }
    if fs::metadata(path).is_ok_and(|metadata| metadata.len() > MOST) {
        fs::rename(path, path.with_extension("1.log"))?;
    }
    let to_log = quiet_stderr && !terminal_reads_stderr() && redirect_stderr(path).is_ok();
    let (path, version) = (path.to_owned(), version.to_owned());
    let before = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let thread = std::thread::current();
        let report = report(
            &version,
            thread.name().unwrap_or("unnamed"),
            &info.to_string(),
            &std::backtrace::Backtrace::force_capture().to_string(),
            SystemTime::now(),
        );
        // Written once, by the standard error, when it goes to the log.
        if to_log {
            eprint!("{report}");
        } else {
            let _ = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
                .and_then(|mut file| file.write_all(report.as_bytes()));
            before(info);
        }
    }));
    Ok(to_log)
}

/// What the log tells of a panic: when, the app's `version`, the
/// `thread`, the panic's `message` with its place, and the `backtrace`.
fn report(version: &str, thread: &str, message: &str, backtrace: &str, at: SystemTime) -> String {
    let mut report = String::new();
    let _ = writeln!(
        report,
        "{} lokyr {version}: the thread '{thread}' panicked: {message}",
        timestamp(at)
    );
    let _ = writeln!(report, "stack backtrace:\n{backtrace}");
    report
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

/// Has the standard error append to the file at `path`.
#[cfg(unix)]
fn redirect_stderr(path: &Path) -> std::io::Result<()> {
    use std::os::fd::AsRawFd;
    let file = OpenOptions::new().create(true).append(true).open(path)?;
    // SAFETY: both descriptors are open; `dup2` makes the standard error a
    // copy of the file's, which outlives `file` as it closes.
    if unsafe { libc::dup2(file.as_raw_fd(), libc::STDERR_FILENO) } < 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(not(unix))]
fn redirect_stderr(_path: &Path) -> std::io::Result<()> {
    Err(std::io::Error::other("the standard error stays"))
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
        let report = report(
            "0.3.0-20261102",
            "lokyr-read",
            "a test's panic at crates/x.rs:1:2",
            "   0: lokyr::main\n",
            UNIX_EPOCH,
        );
        assert!(report.starts_with(
            "1970-01-01T00:00:00Z lokyr 0.3.0-20261102: the thread 'lokyr-read' panicked: a test's panic at crates/x.rs:1:2\n"
        ));
        assert!(report.contains("stack backtrace:\n   0: lokyr::main\n"));
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
