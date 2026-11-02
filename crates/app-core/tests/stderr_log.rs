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
