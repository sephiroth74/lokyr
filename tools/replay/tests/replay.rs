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
    for name in ["vim", "htop", "tmux", "less", "fzf", "vttest"] {
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
        // Broken by an escape with an intermediate byte, which chooses a
        // character set.
        (
            "charset",
            r"printf 'zz-some\\033(B\\033[mone-zz\\r\\033[K'; sleep 0.3",
        ),
        // After an OSC string that an escape ends, or that CAN cancels.
        (
            "osc-escape",
            r"printf '\\033]0;t\\033[mzz-someone-zz\\r\\033[K'; sleep 0.3",
        ),
        (
            "osc-cancel",
            r"printf '\\033]0;t\\030zz-someone-zz\\r\\033[K'; sleep 0.3",
        ),
        // In a title, a link's URI, a working directory that percent-encoding
        // hides, and a PM string, which the terminal drops.
        (
            "title",
            r"printf '\\033]0;zz-someone-zz\\007ok\\n'; sleep 0.3",
        ),
        // In a title that a control character breaks, which the string
        // leaves out.
        (
            "title-control",
            r"printf '\\033]0;zz-some\\001one-zz\\007ok\\n'; sleep 0.3",
        ),
        (
            "link",
            r"printf '\\033]8;;file://zz-someone-zz/x\\033\\\\link\\033]8;;\\033\\\\\\n'; sleep 0.3",
        ),
        (
            "directory",
            r"printf '\\033]7;file:///%%7Az-someone-zz\\007ok\\n'; sleep 0.3",
        ),
        (
            "pm",
            r"printf '\\033^zz-someone-zz\\033\\\\ok\\n'; sleep 0.3",
        ),
        // Put together by moving the cursor back, then scrolled away: only
        // the rows show it.
        (
            "moved",
            r"printf '\\033[8Gone-zz\\rzz-some\\n\\n\\n\\n'; sleep 0.3",
        ),
        // Written to the clipboard, in base64.
        (
            "clipboard",
            r"printf '\\033]52;c;%s\\007' $(printf zz-someone-zz | base64); sleep 0.3",
        ),
        // Broken by a control character in a DCS or a PM string, which
        // keep them.
        (
            "dcs-control",
            r"printf '\\033Ppzz-some\\001one-zz\\033\\\\ok\\n'; sleep 0.3",
        ),
        (
            "pm-control",
            r"printf '\\033^zz-some\\001one-zz\\033\\\\ok\\n'; sleep 0.3",
        ),
        // Percent-encoded past the limit of a link's string.
        (
            "long-link",
            r"printf '\\033]8;;'; head -c 70000 /dev/zero | tr '\\000' x; printf '%%7Az-someone-zz\\033\\\\link\\n'; sleep 0.3",
        ),
        // In base64 in kitty's graphics, as a path, in OSC 1337's file, and
        // after a space in OSC 52.
        (
            "kitty",
            r"printf '\\033_Gt=f;%s\\033\\\\' $(printf /home/zz-someone-zz/a.png | base64); sleep 0.3",
        ),
        (
            "iterm-file",
            r"printf '\\033]1337;File=name=%s:\\007' $(printf zz-someone-zz | base64); sleep 0.3",
        ),
        (
            "clipboard-space",
            r"printf '\\033]52;c;%s %s\\007' $(printf abc | base64) $(printf zz-someone-zz | base64); sleep 0.3",
        ),
        // Broken by a line break in a DCS string, and by DEL in a PM one.
        (
            "dcs-line",
            r"printf '\\033Ppzz-some\\none-zz\\033\\\\ok\\n'; sleep 0.3",
        ),
        (
            "pm-delete",
            r"printf '\\033^zz-some\\177one-zz\\033\\\\ok\\n'; sleep 0.3",
        ),
        // The same, with the alternate screen on at the end, so that the
        // primary screen's rows, inactive, show it.
        (
            "alternate",
            r"printf '\\033[8Gone-zz\\rzz-some\\n\\n\\n\\n\\033[?1049h'; sleep 0.3",
        ),
        // In a DCS and a PM string that a control after the ESC hides from a
        // scan for `ESC P` and `ESC ^`, and that a line break breaks; and in
        // a DCS string, percent-encoded.
        (
            "dcs-after-control",
            r"printf '\\033\\001Ppzz-some\\none-zz\\033\\\\ok\\n'; sleep 0.3",
        ),
        (
            "pm-after-control",
            r"printf '\\033\\001^zz-some\\none-zz\\033\\\\ok\\n'; sleep 0.3",
        ),
        (
            "dcs-percent",
            r"printf '\\033Pp%%7Az-some\\none-zz\\033\\\\ok\\n'; sleep 0.3",
        ),
        // In an SOS string after a byte that the escape passes over, and
        // put together by a tab at the right margin, or by the cursor's
        // moves, then erased or written over by the same read.
        (
            "sos-high",
            r"printf '\\033\\303Xzz-some\\none-zz\\033\\\\ok\\n'; sleep 0.3",
        ),
        (
            "tab-wrap",
            r"printf '\\033[13Gzz-some\\tone-zz\\033[1;1H\\033[2K\\033[2;1H\\033[2K'; sleep 0.3",
        ),
        (
            "moved-erased",
            r"printf '\\033[8Gone-zz\\rzz-some\\033[2K'; sleep 0.3",
        ),
        (
            "moved-overwritten",
            r"printf '\\033[8Gone-zz\\rzz-some\\rxxxxxxxxxxxxx'; sleep 0.3",
        ),
        (
            "moved-reset",
            r"printf '\\033[8Gone-zz\\rzz-some\\033c'; sleep 0.3",
        ),
        // In base64 split between the chunks of a transfer: kitty's
        // graphics, and iTerm2's file in parts.
        (
            "kitty-chunks",
            r"b=$(printf /home/zz-someone-zz/a.png | base64); printf '\\033_Ga=T,f=100,m=1;%s\\033\\\\' $(echo $b | cut -c1-16); printf '\\033_Gm=0;%s\\033\\\\' $(echo $b | cut -c17-); sleep 0.3",
        ),
        (
            "iterm-chunks",
            r"b=$(printf zz-someone-zz | base64); printf '\\033]1337;MultipartFile=size=13\\007\\033]1337;FilePart=%s\\007\\033]1337;FilePart=%s\\007\\033]1337;FileEnd\\007' $(echo $b | cut -c1-8) $(echo $b | cut -c9-); sleep 0.3",
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
fn a_name_drawn_then_erased_is_caught_while_it_shows() {
    let user = [("USER", "zz-someone-zz")];
    // The program draws, shows `ready`, and erases once a key comes, so
    // that a read sees the name on the screen, however busy the machine.
    for (name, program) in [
        // Across a soft wrap, as the cursor's moves put it together, so
        // that only the check after each read sees it.
        (
            "wrapped",
            r"stty -echo; printf '\\033[2;2Hne-zz\\033[1;14Hzz-someo\\033[3;1Hready'; read line; printf '\\033[1;1H\\033[2K\\033[2;1H\\033[2K'",
        ),
        // In base64 that a line break splits.
        (
            "base64-wrapped",
            r"stty -echo; printf 'enotc29t\\nZW9uZS16eg==\\nready'; read line; printf '\\033[2J'",
        ),
    ] {
        let script =
            format!("size 20 3\nrun sh -c \"{program}\"\nwait ready\nsend \"k\\r\"\nexit\n");
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
fn a_short_name_one_in_base64_for_urls_and_one_of_wide_characters_are_caught() {
    for (name, user, program) in [
        // A name of 3 characters, whose base64 takes 4, in the clipboard.
        (
            "short",
            "zzq",
            r"printf '\\033]52;c;%s\\007' $(printf zzq | base64); sleep 0.3",
        ),
        // In base64 with `-` and `_` for `+` and `/`.
        (
            "url",
            "zz~someone~zz",
            r"printf '\\033]52;c;%s\\007' $(printf 'zz~someone~zz' | base64 | tr '+/' '-_'); sleep 0.3",
        ),
        // Put together by moving the cursor back, of wide characters, whose
        // second halves are no spaces.
        (
            "wide",
            "zz-\u{65e5}\u{672c}-zz",
            "printf '\\\\033[4G\u{65e5}\u{672c}-zz\\\\rzz-\\\\n\\\\n\\\\n\\\\n'; sleep 0.3",
        ),
    ] {
        let script = format!("size 20 3\nrun sh -c \"{program}\"\nexit\n");
        let (output, cast) = record(name, &script, &[("USER", user)]);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains(&format!("showed `{user}`")),
            "{name}: {stderr}"
        );
        assert_eq!(cast, None, "{name}");
    }
}

#[test]
fn text_that_a_tab_separates_shows_no_name() {
    let (output, cast) = record(
        "tab",
        "size 20 3\nrun sh -c \"printf 'zz-some\\\\tone-zz\\\\n'\"\nexit\n",
        &[("USER", "zz-someone-zz")],
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(cast.is_some_and(|cast| cast.contains(r"zz-some\tone-zz")));
}

/// A new directory of the user's own, named for the test, for a `TMPDIR`
/// that does not depend on the machine's.
/// It goes as it drops, however the test ends, read-only folders in it too.
struct OwnTemp(PathBuf);

impl Drop for OwnTemp {
    fn drop(&mut self) {
        let _ = Command::new("chmod")
            .arg("-R")
            .arg("u+rwx")
            .arg(&self.0)
            .status();
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn own_temp(name: &str) -> OwnTemp {
    let dir = std::env::temp_dir().join(format!("bolt-replay-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    OwnTemp(dir.canonicalize().unwrap())
}

#[test]
fn the_temporary_directory_and_the_users_folders_above_it_are_private() {
    let temp = own_temp("temp");
    let base = &temp.0;
    let inner = base.join("inner");
    std::fs::create_dir(&inner).unwrap();
    // The temporary directory, and the folder above it, which is the
    // user's too.
    for (name, up, shown) in [("temp", "../..", &inner), ("above", "../../..", base)] {
        let (output, cast) = record(
            name,
            &format!("run sh -c \"cd {up}; pwd; sleep 0.3\"\nexit\n"),
            &[("TMPDIR", inner.to_str().unwrap())],
        );
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains(&format!("showed `{}`", shown.display())),
            "{name}: {stderr}"
        );
        assert_eq!(cast, None, "{name}");
    }
    // The folder's name alone, which is long, as macOS's per-user folder's
    // is.
    let (output, cast) = record(
        "folder-name",
        "run sh -c \"cd ../../..; basename $(pwd); sleep 0.3\"\nexit\n",
        &[("TMPDIR", inner.to_str().unwrap())],
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    let name = base.file_name().unwrap().to_str().unwrap();
    assert!(stderr.contains(&format!("showed `{name}`")), "{stderr}");
    assert_eq!(cast, None);
}

#[test]
fn a_name_that_two_writes_split_as_it_is_typed_is_caught() {
    let keys = format!("{}zz-someone-zz", "x".repeat(1015));
    let (output, cast) = record(
        "typed",
        &format!(
            "run sh -c \"stty raw -echo; echo armed; exec cat >/dev/null\"\nwait armed\nsend \"{keys}\"\nsleep 300\n"
        ),
        &[("USER", "zz-someone-zz")],
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("showed `zz-someone-zz`"), "{stderr}");
    assert_eq!(cast, None);
}

#[test]
fn a_home_that_the_program_made_read_only_goes_too() {
    let temp = own_temp("read-only");
    let (output, cast) = record(
        "ro",
        "run sh -c \"mkdir -p d/e; touch d/e/f; chmod 555 d/e d\"\nexit\n",
        &[("TMPDIR", temp.0.to_str().unwrap())],
    );
    let left: Vec<_> = std::fs::read_dir(&temp.0)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(cast.is_some());
    assert!(left.is_empty(), "the home stays: {left:?}");
}

#[test]
fn a_space_at_a_soft_wrap_stays_in_the_check_after_each_read() {
    // `zz some`, its space in the last column of a row that wraps, then
    // erased.
    let program = r"printf '\\033[2;2Home\\033[1;18Hzz s'; sleep 0.3; printf '\\033[1;1H\\033[2K\\033[2;1H\\033[2K'; sleep 0.3";
    let (output, cast) = record(
        "space",
        &format!("size 20 3\nrun sh -c \"{program}\"\nexit\n"),
        &[("USER", "zz some")],
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("showed `zz some`"), "{stderr}");
    assert_eq!(cast, None);
}

#[test]
fn a_name_that_cannot_be_looked_up_is_said() {
    let (output, cast) = record("lookup", "run true\nexit\n", &[("PATH", "/nonexistent")]);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("warning: `hostname -s` failed"), "{stderr}");
    assert!(cast.is_some(), "{stderr}");
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
            "run sh -c \"stty raw -echo; echo armed; exec cat\"\nwait armed\nsend \"{keys}\"\nwait xxxxxxxxxx\n"
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
            "run sh -c \"stty raw -echo; echo armed; sleep 30\"\nwait armed\nsend \"{}\" 500\n",
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
    let temp = std::env::temp_dir().join(format!("bolt-replay-stale-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&temp);
    // A home that a recorder marked, whose process has gone; one of a
    // process that runs, as another user; and a directory that is not a
    // home, though its name looks like one.
    let stale = temp.join("bolt-record-stale-2147483646");
    let running = temp.join("bolt-record-root-1");
    let other = temp.join("bolt-record-notes-2024");
    // Marked, but with a sign or leading zeros, which a recorder never
    // writes.
    let signed = temp.join("bolt-record-signed-+2147483646");
    let padded = temp.join("bolt-record-padded-002147483646");
    for dir in [&stale, &running, &other, &signed, &padded] {
        std::fs::create_dir_all(dir.join("home")).unwrap();
    }
    for dir in [&stale, &running, &signed, &padded] {
        std::fs::write(dir.join(".bolt-record"), b"").unwrap();
    }
    let (output, _) = record(
        "next",
        "run true\nexit\n",
        &[("TMPDIR", temp.to_str().unwrap())],
    );
    let kept = [&running, &other, &signed, &padded].map(|dir| dir.exists());
    let removed = !stale.exists();
    let _ = std::fs::remove_dir_all(&temp);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(removed);
    assert_eq!(kept, [true; 4]);
}

#[test]
fn a_wait_a_quiet_and_a_mark_do_not_end_inside_a_synchronized_update() {
    // An update that shows `ready` in the program's first write, and ends
    // half a second later; and, for the mark, `started`, then an update
    // that ends more than a second later, inside which the mark falls.
    let first = "run sh -c \"printf '\\\\033[?2026hready'; sleep 0.5; printf ' done\\\\033[?2026l'; read x\"";
    let later = "run sh -c \"echo started; sleep 0.1; printf '\\\\033[?2026hready'; sleep 1.5; printf ' done\\\\033[?2026l'; read x\"";
    for (name, program, steps, after) in [
        (
            "wait",
            first,
            "wait ready\nsend \"x\\r\"\n",
            r#","i","x\r"]"#,
        ),
        (
            "quiet",
            first,
            "quiet 200\nsend \"x\\r\"\n",
            r#","i","x\r"]"#,
        ),
        (
            "mark",
            later,
            "wait started\nsleep 800\nmark here\nsend \"x\\r\"\n",
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
