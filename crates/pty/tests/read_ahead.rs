//! A program's output read ahead of its parse, as a terminal's parser takes
//! it.

use std::sync::mpsc;
use std::time::{Duration, Instant};

use lokyr_pty::{Next, Options, Program, ReadAhead, spawn};

/// The size of a batch, as a terminal's parser takes it.
const SIZE: usize = 64 * 1024;

fn sh(script: &str) -> Program {
    Program::command("/bin/sh", ["-c", script])
}

/// Runs `f` on a thread of its own, so that a test gives up rather than
/// hang.
fn within<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = sender.send(f());
    });
    receiver
        .recv_timeout(Duration::from_secs(10))
        .expect("it took more than ten seconds")
}

/// The batches until `needle` came, with deadlines that have passed, as a
/// parser that looks now and then takes them.
fn until(output: &mut ReadAhead, needle: &[u8]) -> Vec<u8> {
    let mut text = Vec::new();
    while !text.windows(needle.len()).any(|window| window == needle) {
        match output.next(Some(Instant::now())) {
            Next::Output(batch) => text.extend_from_slice(&batch),
            Next::TimedOut => std::thread::sleep(Duration::from_millis(5)),
            Next::End => panic!("the end, after {text:?}"),
        }
    }
    text
}

#[test]
fn what_is_read_ahead_comes_whole_and_in_order_in_large_batches() {
    // Through `cat`, which writes in blocks, where `seq` would write to a
    // terminal a line at a time.
    let pty = spawn(&Options::new(sh("seq 1 300000 | cat"), "xterm-256color")).unwrap();
    let mut output = pty.reader().unwrap().read_ahead(SIZE).unwrap();
    let (bytes, batches) = within(move || {
        let (mut bytes, mut batches) = (Vec::new(), 0);
        loop {
            match output.next(None) {
                Next::Output(batch) => {
                    assert!(batch.len() <= SIZE);
                    bytes.extend_from_slice(&batch);
                    batches += 1;
                    // A parse, as the reader reads on.
                    std::thread::sleep(Duration::from_micros(200));
                }
                Next::End => return (bytes, batches),
                Next::TimedOut => unreachable!("no deadline"),
            }
        }
    });
    // macOS's terminal, whose queue fills between the carriage return that
    // it puts before a line feed and the line feed, puts the carriage
    // return again as it goes on, which a terminal shows alike.
    let text = String::from_utf8(bytes).unwrap().replace("\r\r\n", "\r\n");
    let expected: String = (1..=300_000).map(|n| format!("{n}\r\n")).collect();
    assert!(
        text == expected,
        "{} bytes, of {}",
        text.len(),
        expected.len()
    );
    // Each batch holds many reads of the terminal's queue, about a KiB
    // each on macOS.
    let average = text.len() / batches;
    assert!(average > 4096, "{batches} batches of {average} bytes");
}

#[test]
fn output_that_comes_a_little_at_a_time_comes_at_once() {
    let pty = spawn(&Options::new(
        sh("printf one; sleep 2; printf two; sleep 30"),
        "xterm-256color",
    ))
    .unwrap();
    let mut output = pty.reader().unwrap().read_ahead(SIZE).unwrap();
    let first = within(move || match output.next(None) {
        Next::Output(batch) => batch.to_vec(),
        other => panic!("{other:?}"),
    });
    assert_eq!(first, b"one", "without waiting for what follows");
}

#[test]
fn a_wait_with_a_deadline_times_out() {
    let pty = spawn(&Options::new(sh("echo ready; sleep 30"), "xterm-256color")).unwrap();
    let mut output = pty.reader().unwrap().read_ahead(SIZE).unwrap();
    within(move || {
        assert_eq!(until(&mut output, b"ready\r\n"), b"ready\r\n");
        let start = Instant::now();
        let next = output.next(Some(start + Duration::from_millis(200)));
        assert!(matches!(next, Next::TimedOut), "{next:?}");
        assert!(start.elapsed() >= Duration::from_millis(200));
    });
}

#[test]
fn dropping_the_terminal_ends_what_is_read_ahead() {
    // A program that ignores SIGHUP, and keeps the terminal open.
    let pty = spawn(&Options::new(
        sh("trap '' HUP; echo ready; sleep 30"),
        "xterm-256color",
    ))
    .unwrap();
    let mut output = pty.reader().unwrap().read_ahead(SIZE).unwrap();
    until(&mut output, b"ready");
    let pid = pty.process_id();
    drop(pty);
    within(move || {
        loop {
            match output.next(None) {
                Next::Output(_) => {}
                Next::End => break,
                Next::TimedOut => unreachable!("no deadline"),
            }
        }
    });
    // SAFETY: ending a test's own child, which would outlive the test.
    unsafe { libc::kill(pid as libc::pid_t, libc::SIGKILL) };
}
