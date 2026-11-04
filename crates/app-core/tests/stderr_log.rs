//! The standard error in the log, each test's log in a process of its own,
//! as the standard error and the panic's hook are the process's, which
//! would take the failures' messages of the tests' own process: lines past
//! a pipe's buffer go without a panic, a panic's report reaches the file,
//! an abort's last line reaches the file that started again, and a file
//! that does not open leaves the standard error as it was.

use std::io::Write as _;
use std::time::Duration;

/// Waits until `done` holds, for ten seconds at most.
fn wait_until(what: &str, done: impl Fn() -> bool) {
    for _ in 0..1000 {
        if done() {
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("{what} never came");
}

/// Reads the log until `done` holds of it, for ten seconds at most.
fn read_until(log: &std::path::Path, done: impl Fn(&str) -> bool) -> String {
    let read = || std::fs::read_to_string(log).unwrap_or_default();
    wait_until("the log's text", || done(&read()));
    read()
}

/// The child's side of the test below: with `LOKYR_LOG_LINES`, starts the
/// log there, writes more than a pipe's buffer to the standard error, then
/// panics on a thread of its own; without, nothing.
#[test]
fn child_writes_past_a_pipe_and_panics() {
    let Some(log) = std::env::var_os("LOKYR_LOG_LINES") else {
        return;
    };
    let log = std::path::PathBuf::from(log);
    let to_log = lokyr_app_core::log::start_with(&log, "9.9.9-test", true).unwrap();
    assert!(to_log);
    // More than a pipe's buffer, which its thread drains, through the
    // standard error's descriptor, as `eprintln!` writes it in the app;
    // the tests' harness keeps `eprintln!`'s for itself.
    let mut stderr = std::io::stderr();
    for line in 0..5000 {
        writeln!(
            stderr,
            "a line of the log, the {line}th, which goes to its file"
        )
        .unwrap();
    }
    read_until(&log, |text| text.contains("the 4999th"));
    let worker = std::thread::Builder::new()
        .name("worker".to_owned())
        .spawn(|| panic!("a test's panic"))
        .unwrap();
    assert!(worker.join().is_err());
    read_until(&log, |text| text.contains("a test's panic"));
}

#[test]
fn the_standard_error_s_lines_and_a_panic_s_report_reach_the_log() {
    let directory = std::env::temp_dir().join(format!("lokyr-stderr-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    let log = directory.join("Logs/lokyr.log");
    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "child_writes_past_a_pipe_and_panics",
            "--test-threads=1",
        ])
        .env("LOKYR_LOG_LINES", &log)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .unwrap();
    let text = std::fs::read_to_string(&log).unwrap_or_default();
    assert!(status.success(), "{status}: {text}");
    assert!(text.contains("the 4999th"));
    assert!(text.contains(" lokyr 9.9.9-test: the thread 'worker' panicked at "));
    // Once: the hook writes it, and the terminal's hook does not.
    assert_eq!(text.matches("a test's panic").count(), 1, "{text}");
    let _ = std::fs::remove_dir_all(directory);
}

/// The exit's code of a child that aborted, as its own handler of the
/// signal ends it, which the log's handler gives the signal back to: no
/// crash's report of the system for a test.
const ABORTED: i32 = 3;

/// The child's side of the test below: with `LOKYR_LOG_CHILD`, starts the
/// log there, writes a line to the standard error, and exits at once, or
/// aborts with `LOKYR_LOG_ABORT`; without, nothing.
#[test]
fn child_writes_a_line_and_ends() {
    let Some(log) = std::env::var_os("LOKYR_LOG_CHILD") else {
        return;
    };
    if std::env::var_os("LOKYR_LOG_ABORT").is_some() {
        extern "C" fn ended(_: libc::c_int) {
            // SAFETY: `_exit` may be called from a signal's handler.
            unsafe { libc::_exit(ABORTED) };
        }
        // SAFETY: an action of the signal that a handler of C's convention
        // takes, set before the log's, which keeps it as the one before.
        unsafe {
            let mut action: libc::sigaction = std::mem::zeroed();
            action.sa_sigaction = ended as extern "C" fn(libc::c_int) as libc::sighandler_t;
            libc::sigaction(libc::SIGABRT, &action, std::ptr::null_mut());
        }
    }
    // The drain reads nothing before the end, which alone takes the line.
    lokyr_app_core::log::hold_drain();
    lokyr_app_core::log::start_with(std::path::Path::new(&log), "9.9.9-test", true).unwrap();
    writeln!(std::io::stderr(), "the last line before the end").unwrap();
    if std::env::var_os("LOKYR_LOG_ABORT").is_some() {
        std::process::abort();
    }
    std::process::exit(0);
}

#[test]
fn the_last_line_before_the_end_reaches_the_log() {
    let directory = std::env::temp_dir().join(format!("lokyr-stderr-end-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).unwrap();
    let test = std::env::current_exe().unwrap();
    // An exit, and an abort, as a stack's overflow ends the process, their
    // drains holding the line, which the end alone writes.
    for (abort, runs) in [(false, 5), (true, 5)] {
        for run in 0..runs {
            let log = directory.join(format!("{abort}-{run}.log"));
            let mut child = std::process::Command::new(&test);
            child
                .args([
                    "--exact",
                    "child_writes_a_line_and_ends",
                    "--test-threads=1",
                ])
                .env("LOKYR_LOG_CHILD", &log)
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null());
            if abort {
                child.env("LOKYR_LOG_ABORT", "1");
            }
            let status = child.status().unwrap();
            assert_eq!(status.code(), Some(if abort { ABORTED } else { 0 }));
            let text = std::fs::read_to_string(&log).unwrap_or_default();
            assert!(
                text.contains("the last line before the end"),
                "{} {run}: {text:?}",
                if abort { "abort" } else { "exit" }
            );
        }
    }
    let _ = std::fs::remove_dir_all(directory);
}

/// The child's side of the test below: with `LOKYR_LOG_ROTATED`, starts the
/// log there, writes past its size, so that it starts again, holds its
/// drain once it wrote all, and aborts after a last line, which the abort's
/// handler alone writes; without, nothing.
#[test]
fn child_rotates_the_log_and_aborts() {
    let Some(log) = std::env::var_os("LOKYR_LOG_ROTATED") else {
        return;
    };
    let log = std::path::PathBuf::from(log);
    extern "C" fn ended(_: libc::c_int) {
        // SAFETY: `_exit` may be called from a signal's handler.
        unsafe { libc::_exit(ABORTED) };
    }
    // SAFETY: as the child's above.
    unsafe {
        let mut action: libc::sigaction = std::mem::zeroed();
        action.sa_sigaction = ended as extern "C" fn(libc::c_int) as libc::sighandler_t;
        libc::sigaction(libc::SIGABRT, &action, std::ptr::null_mut());
    }
    lokyr_app_core::log::start_with(&log, "9.9.9-test", true).unwrap();
    let mut stderr = std::io::stderr();
    let line = "x".repeat(1023);
    for _ in 0..1100 {
        writeln!(stderr, "{line}").unwrap();
    }
    // The pipe empty once the new file holds the line after the others, as
    // a slow drain keeps a pipe's buffer of them, which a hold would keep.
    writeln!(stderr, "the line after the rotation's").unwrap();
    let kept = log.with_extension("1.log");
    read_until(&log, |text| {
        kept.exists() && text.contains("the line after the rotation's")
    });
    // The drain holds at its next turn, which a line ends if it waits in a
    // read; once it holds, the last line waits in the pipe.
    lokyr_app_core::log::hold_drain();
    writeln!(stderr, "a line after the hold").unwrap();
    wait_until("the drain's hold", lokyr_app_core::log::drain_holds);
    writeln!(stderr, "the last line before the abort").unwrap();
    std::process::abort();
}

#[test]
fn an_abort_s_last_line_reaches_the_log_that_started_again() {
    let directory =
        std::env::temp_dir().join(format!("lokyr-stderr-rotated-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).unwrap();
    let log = directory.join("lokyr.log");
    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "child_rotates_the_log_and_aborts",
            "--test-threads=1",
        ])
        .env("LOKYR_LOG_ROTATED", &log)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .unwrap();
    assert_eq!(status.code(), Some(ABORTED));
    let text = std::fs::read_to_string(&log).unwrap_or_default();
    assert!(
        text.contains("the last line before the abort"),
        "{}",
        text.len()
    );
    let kept = std::fs::read_to_string(directory.join("lokyr.1.log")).unwrap_or_default();
    assert!(!kept.contains("the last line before the abort"));
    let _ = std::fs::remove_dir_all(directory);
}

/// The child's side of the test below: with `LOKYR_LOG_UNOPENED`, starts
/// the log of a directory that it cannot write, tells on its standard
/// output whether the standard error went to the log, and writes a line
/// to the standard error; without, nothing.
#[test]
fn child_starts_a_log_that_does_not_open() {
    let Some(log) = std::env::var_os("LOKYR_LOG_UNOPENED") else {
        return;
    };
    let to_log =
        lokyr_app_core::log::start_with(std::path::Path::new(&log), "9.9.9-test", true).unwrap();
    println!("to the log: {to_log}");
    writeln!(std::io::stderr(), "a line that stays on the standard error").unwrap();
}

#[test]
fn a_log_that_does_not_open_leaves_the_standard_error_as_it_was() {
    use std::os::unix::fs::PermissionsExt as _;
    // SAFETY: a call of no arguments.
    if unsafe { libc::geteuid() } == 0 {
        return;
    }
    let directory =
        std::env::temp_dir().join(format!("lokyr-stderr-unopened-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o500)).unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "child_starts_a_log_that_does_not_open",
            "--test-threads=1",
            "--nocapture",
        ])
        .env("LOKYR_LOG_UNOPENED", directory.join("lokyr.log"))
        .output()
        .unwrap();
    let _ = std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700));
    let _ = std::fs::remove_dir_all(&directory);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stdout.contains("to the log: false"), "{stdout}");
    assert!(
        stderr.contains("a line that stays on the standard error"),
        "{stderr}"
    );
}
