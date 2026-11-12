//! A panic writes its report to the app's log, in a process of its own, as
//! the hook is the process's.

use std::time::Duration;

#[test]
fn a_panic_writes_its_version_its_thread_and_its_backtrace_to_the_log() {
    let directory = std::env::temp_dir().join(format!("lokyr-log-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    let log = directory.join("Logs/lokyr.log");
    let to_log = lokyr_app_core::log::start(&log, "9.9.9-test", false).unwrap();
    assert!(!to_log);
    let worker = std::thread::Builder::new()
        .name("worker".to_owned())
        .spawn(|| panic!("a test's panic"))
        .unwrap();
    assert!(worker.join().is_err());
    // The hook wrote it into the file, which the test reads.
    let mut text = String::new();
    for _ in 0..100 {
        text = std::fs::read_to_string(&log).unwrap_or_default();
        if !text.is_empty() {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        text.contains(" lokyr 9.9.9-test: the thread 'worker' panicked at "),
        "{text}"
    );
    assert!(text.contains("a test's panic"), "{text}");
    assert!(text.contains("panic_log.rs:"), "{text}");
    assert!(text.contains("stack backtrace:\n"), "{text}");
    let _ = std::fs::remove_dir_all(directory);
}
