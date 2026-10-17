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

/// The recorded sessions of the VT core's tests.
fn recordings() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../crates/vt-core/tests/recordings")
}

#[test]
fn the_recorded_sessions_replay() {
    for name in ["vim", "htop", "tmux", "less", "fzf"] {
        let path = recordings().join(format!("{name}.cast"));
        let cast = std::fs::read_to_string(&path).expect("the recording exists");
        let marks = cast
            .lines()
            .filter(|line| line.contains(r#","m","#))
            .count();
        assert!(marks >= 3, "{name}: {marks} marks");
        let output = replay(&[path.to_str().unwrap()], None);
        let screen = stdout(&output);
        assert_eq!(screen.lines().count(), 24, "{name}");
        if name == "fzf" {
            // What fzf printed as it ended: the choice.
            assert!(screen.contains("blueberry"), "{screen}");
        }
    }
}

/// Runs `bolt-record` on a script made of `lines`, in a directory of its
/// own, with `env` set, and returns its output and the recording, if it
/// wrote one.
fn record(name: &str, lines: &str, env: &[(&str, &str)]) -> (Output, Option<String>) {
    let dir = std::env::temp_dir().join(format!("bolt-replay-test-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let script = dir.join(format!("{name}.script"));
    std::fs::write(&script, lines).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_bolt-record"))
        .arg(&script)
        .envs(env.iter().copied())
        .output()
        .expect("bolt-record runs");
    let cast = std::fs::read_to_string(script.with_extension("cast")).ok();
    let _ = std::fs::remove_dir_all(&dir);
    (output, cast)
}

#[test]
fn a_script_drives_the_program_and_the_recording_replays() {
    let (output, cast) = record(
        "echo",
        "size 20 3\nrun sh -c \"printf 'ready\\\\n'; read line; printf 'got %s\\\\n' \\\"$line\\\"\"\nwait ready\nmark typed\nsend \"hello\\r\"\nexit\n",
        &[],
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cast = cast.expect("a recording");
    assert!(
        cast.starts_with(r#"{"env":{"TERM":"xterm-256color"},"height":3,"version":2,"width":20}"#)
    );
    assert!(cast.contains(r#","i","hello\r"]"#), "{cast}");
    let output = replay(&["-"], Some(cast.as_bytes()));
    // The last line break scrolls the first line away.
    assert_eq!(stdout(&output), "hello\ngot hello\n\n");
}

#[test]
fn a_recording_that_shows_the_user_is_not_written() {
    let (output, cast) = record(
        "private",
        "run sh -c \"echo zz-someone-zz; sleep 5\"\nwait zz\nexit\n",
        &[("USER", "zz-someone-zz")],
    );
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("showed `zz-someone-zz`"));
    assert_eq!(cast, None);
}

#[test]
fn the_program_gets_the_terminals_replies() {
    // In raw mode, the reply to DA1 comes without a newline; ESC shows as E.
    let (output, cast) = record(
        "reply",
        "run sh -c \"stty raw -echo; printf '\\\\033[c'; r=$(dd bs=1 count=9 2>/dev/null); stty sane; printf 'reply:%s\\\\n' \\\"$r\\\" | tr '\\\\033' E\"\nwait \"reply:E[?62;22c\"\nexit\n",
        &[],
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(cast.unwrap().contains(r#","i","\u001b[?62;22c"]"#));
}

#[test]
fn a_step_that_cannot_end_fails_the_recording() {
    for (name, script, error) in [
        (
            "never",
            "run sh -c \"sleep 5\"\nwait nothing 300\n",
            "`nothing` did not show in 300ms",
        ),
        (
            "busy",
            "run yes\nquiet 1000 3000\n",
            "did not go quiet in 3s",
        ),
    ] {
        let (output, cast) = record(name, script, &[]);
        assert!(!output.status.success(), "{name}");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains(error), "{name}: {stderr}");
        assert_eq!(cast, None, "{name}");
    }
}

#[test]
fn a_name_that_two_reads_split_or_an_escape_breaks_is_caught() {
    let user = [("USER", "zz-someone-zz")];
    for (name, program) in [
        // Split between two reads, and scrolled away before the second ends.
        (
            "split",
            r"printf 'zz-some'; sleep 0.3; printf 'one-zz\\n\\n\\n\\n'; sleep 0.3",
        ),
        // Broken by an escape, and erased before the read ends.
        (
            "escape",
            r"printf 'zz-some\\033[mone-zz\\r\\033[K'; sleep 0.3",
        ),
    ] {
        let script = format!("size 20 3\nrun sh -c \"{program}\"\nexit\n");
        let (output, cast) = record(name, &script, &user);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("showed `zz-someone-zz`"),
            "{name}: {stderr}"
        );
        assert_eq!(cast, None, "{name}");
    }
}

#[test]
fn the_programs_own_home_is_private_too() {
    // As the system resolves it, and as the program's `HOME` names it.
    for (name, program) in [("pwd", "pwd"), ("home", "echo $HOME")] {
        let script = format!("run sh -c \"{program}; sleep 0.3\"\nexit\n");
        let (output, cast) = record(name, &script, &[]);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains(&format!("bolt-record-{name}-")),
            "{name}: {stderr}"
        );
        assert_eq!(cast, None, "{name}");
    }
    // A home whose path holds the user's name keeps the name private.
    let tmp = std::env::temp_dir().join(format!("zz-someone-zz-{}", std::process::id()));
    std::fs::create_dir_all(&tmp).unwrap();
    let (output, cast) = record(
        "named",
        "run sh -c \"echo zz-someone-zz; sleep 0.3\"\nexit\n",
        &[("USER", "zz-someone-zz"), ("TMPDIR", tmp.to_str().unwrap())],
    );
    let _ = std::fs::remove_dir_all(&tmp);
    assert!(String::from_utf8_lossy(&output.stderr).contains("showed"));
    assert_eq!(cast, None);
}

#[test]
fn typing_waits_for_the_program_for_a_while_and_reads_meanwhile() {
    let keys = "x".repeat(20_000);
    // A program that reads, and echoes what it reads: its output has to be
    // read for it to read on.
    let (output, cast) = record(
        "echo-keys",
        &format!(
            "run sh -c \"stty raw -echo; exec cat\"\nsleep 200\nsend \"{keys}\"\nwait xxxxxxxxxx\n"
        ),
        &[],
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(cast.is_some());
    // A program that never reads.
    let (output, cast) = record(
        "deaf",
        &format!(
            "run sh -c \"stty raw -echo; sleep 30\"\nsleep 200\nsend \"{}\" 500\n",
            "x".repeat(70_000)
        ),
        &[],
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("did not take what was typed in 500ms")
    );
    assert_eq!(cast, None);
}

#[test]
fn a_killed_recorders_home_goes_with_the_next_run() {
    let stale = std::env::temp_dir().join("bolt-record-stale-2147483646");
    std::fs::create_dir_all(&stale).unwrap();
    let (output, _) = record("next", "run true\nexit\n", &[]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!stale.exists());
}

#[test]
fn a_wait_a_quiet_and_a_mark_do_not_end_inside_a_synchronized_update() {
    // An update that shows `ready` at once, and ends half a second later.
    let program = "run sh -c \"printf '\\\\033[?2026hready'; sleep 0.5; printf ' done\\\\033[?2026l'; read x\"";
    for (name, steps, after) in [
        ("wait", "wait ready\nsend \"x\\r\"\n", r#","i","x\r"]"#),
        ("quiet", "quiet 200\nsend \"x\\r\"\n", r#","i","x\r"]"#),
        (
            "mark",
            "sleep 100\nmark here\nsend \"x\\r\"\n",
            r#","m","here"]"#,
        ),
    ] {
        let (output, cast) = record(name, &format!("{program}\n{steps}exit\n"), &[]);
        assert!(
            output.status.success(),
            "{name}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let cast = cast.unwrap();
        let end = cast.find("\\u001b[?2026l").expect("the update's end");
        let next = cast.find(after).expect("the step after");
        assert!(end < next, "{name}: {cast}");
    }
}
