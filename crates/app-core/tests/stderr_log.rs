//! The standard error in the log, in a process of its own, as the
//! standard error and the hook are the process's: lines past a pipe's
//! buffer go without a panic, and a panic's report reaches the file.

use std::io::Write as _;
use std::time::Duration;

/// Reads the log until `done` holds of it, for ten seconds at most.
fn read_until(log: &std::path::Path, done: impl Fn(&str) -> bool) -> String {
    for _ in 0..1000 {
        let text = std::fs::read_to_string(log).unwrap_or_default();
        if done(&text) {
            return text;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("the log never held it");
}

#[test]
fn the_standard_error_s_lines_and_a_panic_s_report_reach_the_log() {
    let directory = std::env::temp_dir().join(format!("lokyr-stderr-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    let log = directory.join("Logs/lokyr.log");
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
    let text = read_until(&log, |text| text.contains("a test's panic"));
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
