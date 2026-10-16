//! `bolt-replay` run as a program, on the recordings in `tests/data/`.

use std::io::Write as _;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

fn data(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/data")
        .join(name)
}

fn replay(args: &[&str], stdin: Option<&[u8]>) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_bolt-replay"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("bolt-replay runs");
    let mut input = child.stdin.take().expect("a pipe");
    if let Some(bytes) = stdin {
        input.write_all(bytes).expect("writing standard input");
    }
    drop(input);
    child.wait_with_output().expect("bolt-replay ends")
}

fn stdout(output: &Output) -> &str {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    std::str::from_utf8(&output.stdout).expect("UTF-8")
}

#[test]
fn a_raw_stream_prints_its_screen() {
    let path = data("hello.txt");
    let output = replay(
        &["--cols", "20", "--rows", "3", path.to_str().unwrap()],
        None,
    );
    assert_eq!(stdout(&output), "hello\nworld 中文\n\n");
}

#[test]
fn an_asciicast_prints_its_screen_at_its_size() {
    let path = data("session.cast");
    let output = replay(&["--attributes", path.to_str().unwrap()], None);
    // The input event is left out, and the last line wraps and scrolls.
    assert_eq!(stdout(&output), "hi\n$ 01234567\n89abc\ncursor 5,2\n");
}

#[test]
fn standard_input_works_too() {
    let output = replay(&["--cols", "5", "--rows", "1", "-"], Some(b"abc"));
    assert_eq!(stdout(&output), "abc\n");
}

#[test]
fn resizes_are_left_out_with_a_warning() {
    let path = data("resized.cast");
    let output = replay(&[path.to_str().unwrap()], None);
    assert_eq!(stdout(&output), "one two\n\n");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("resizes the terminal 1 times"), "{stderr}");
}

#[test]
fn bench_times_the_replay() {
    let path = data("session.cast");
    let output = replay(&["--runs", "3", path.to_str().unwrap()], None);
    let line = stdout(&output);
    assert!(line.starts_with("30 bytes in "), "{line}");
    assert!(
        line.contains("the median of 3 runs") && line.trim_end().ends_with("MiB/s"),
        "{line}"
    );
}

#[test]
fn a_missing_file_is_an_error() {
    let output = replay(&["no-such-file"], None);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("reading no-such-file"));
}
