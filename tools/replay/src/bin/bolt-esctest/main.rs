//! `bolt-esctest`: runs esctest, the suite of terminal tests of iTerm2 and
//! xterm (<https://github.com/ThomasDickey/esctest2>), in a headless
//! terminal, and tells its pass rate and its failures, grouped by the
//! sequence that each test's class is named for.
//!
//! `bolt-esctest DIR [ARG...]` runs `DIR/esctest.py` with `python3`, from a
//! checkout that the repository does not keep, and passes it the ARGs, as
//! `--include=REGEX`. The terminal is 80 by 25, the size that esctest asks
//! for, with cells of 8 by 16 pixels and xterm's default colors, black on
//! white. It reports titles and the checksums of rectangles, through which
//! esctest reads the screen, and lets programs operate on the window and
//! read the clipboards back, as xterm does when it is allowed to; the
//! harness plays the window, and the runner the user who lets the reads
//! through.
//!
//! The suite must run to its end: esctest exits with status 0, and its
//! log ends with the closing line, whose counts must be those of the tests
//! read. Otherwise the runner says why, prints the screen and keeps the
//! log.

use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow, bail, ensure};
use bolt_pty::Exit;
use bolt_replay::harness::{Harness, HarnessOptions};
use bolt_vt_core::page::Rgb;
use bolt_vt_core::terminal::{Colors, Event, Options, SPECIAL_COLORS};

/// How long esctest waits for each byte of a reply, in seconds: shorter
/// than its own default of a second, as a reply that a headless terminal
/// makes comes at once, or never.
const TIMEOUT: &str = "0.5";

/// How long the whole suite may take.
const LIMIT: Duration = Duration::from_secs(3 * 60 * 60);

/// How long the suite runs between two looks at the terminal's events.
const TURN: Duration = Duration::from_millis(10);

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let Some(dir) = args.next().filter(|arg| arg != "-h" && arg != "--help") else {
        println!(
            "Usage: bolt-esctest DIR [ARG...]\n\n\
             Runs DIR/esctest.py in a headless terminal, with the ARGs, and tells the pass\n\
             rate and the failures, by sequence."
        );
        return ExitCode::FAILURE;
    };
    match run(Path::new(&dir), args.collect()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error:#}");
            ExitCode::FAILURE
        }
    }
}

/// Runs the suite in `dir` with `extra` arguments, and prints what it
/// found.
fn run(dir: &Path, extra: Vec<String>) -> Result<()> {
    let mut temp = Temp::new()?;
    let log = temp.dir.join("esctest.log");
    let mut program = vec![
        "python3".to_owned(),
        "esctest.py".to_owned(),
        "--expected-terminal=xterm".to_owned(),
        // Reverse wraparound as xterm does it since 2023, its mode 45 for
        // the rows that wrap and 1045 for the others.
        "--xterm-reverse-wrap=383".to_owned(),
        "--no-print-logs".to_owned(),
        format!("--timeout={TIMEOUT}"),
        format!("--logfile={}", log.display()),
    ];
    program.extend(arguments(extra)?);
    let options = HarnessOptions {
        program,
        env: vec![
            ("PATH".to_owned(), "/usr/bin:/bin".to_owned()),
            ("LANG".to_owned(), "en_US.UTF-8".to_owned()),
            ("HOME".to_owned(), temp.dir.display().to_string()),
        ],
        terminal: Options {
            title_reports: true,
            clipboard_reads: true,
            rectangle_checksums: true,
            window_ops: true,
            ..Options::default()
        },
        dir: dir.to_owned(),
        cols: 80,
        rows: 25,
    };
    let tests = suite(&options, &mut temp, &log)?;
    print!("{}", report(&tests));
    Ok(())
}

/// The ARGs for esctest, `extra`, with the options that it is told merged
/// into one `--options`, after the window's operations, which the terminal
/// allows, as xterm's `allowWindowOps` does: esctest keeps the last
/// `--options` alone. `--logfile` is refused, as the runner reads the log
/// that it names itself.
fn arguments(extra: Vec<String>) -> Result<Vec<String>> {
    let mut options = vec!["xtermWinopsEnabled".to_owned()];
    let mut rest = Vec::new();
    let mut extra = extra.into_iter().peekable();
    while let Some(arg) = extra.next() {
        if arg == "--logfile" || arg.starts_with("--logfile=") {
            bail!("the runner names esctest's log itself, so --logfile cannot be given");
        }
        if let Some(value) = arg.strip_prefix("--options=") {
            options.extend(value.split_whitespace().map(str::to_owned));
        } else if arg == "--options" {
            // Its values, up to the next argument that starts another.
            while let Some(value) = extra.next_if(|next| !next.starts_with('-')) {
                options.push(value);
            }
        } else {
            rest.push(arg);
        }
    }
    rest.push("--options".to_owned());
    rest.extend(options);
    Ok(rest)
}

/// Runs the suite as `options` tell, and reads the tests from its `log`,
/// in `temp`. If the suite did not run to its end, the error says why,
/// with the screen, and `temp` is kept, for the log to be looked at.
fn suite(options: &HarnessOptions, temp: &mut Temp, log: &Path) -> Result<Vec<(String, Outcome)>> {
    let mut harness = Harness::spawn(options)?;
    // Cells of a font, so that the sizes in pixels are told, as a window's,
    // on a display of 1920 by 1080 pixels.
    harness.set_cell_size(8, 16)?;
    harness.terminal_mut().set_display_size(1920, 1080);
    // xterm's default colors, black on white, which esctest sets before
    // each test, and expects the resets to bring back.
    let (black, white) = (Rgb::new(0, 0, 0), Rgb::new(255, 255, 255));
    harness.terminal_mut().set_default_colors(Colors {
        foreground: black,
        background: white,
        cursor: black,
        special: [black; SPECIAL_COLORS],
        ..Colors::default()
    });
    ended(&mut harness, log).map_err(|error| {
        temp.kept = true;
        anyhow!(
            "{error:#}\n\nThe screen:\n{}\n\nThe log is kept in {}",
            harness.screen(),
            log.display()
        )
    })
}

/// The tests that the log tells of, once the suite ends with status 0
/// and a log whose closing line counts them.
fn ended(harness: &mut Harness, log: &Path) -> Result<Vec<(String, Outcome)>> {
    serve(harness)?;
    match harness.exit_status(LIMIT)? {
        Exit::Code(0) => {}
        Exit::Code(code) => bail!("esctest exited with status {code}"),
        Exit::Signal(signal) => bail!("esctest was ended by signal {signal}"),
    }
    let text = fs::read_to_string(log)
        .with_context(|| format!("esctest left no log to read at {}", log.display()))?;
    let log = parse(&text);
    check(&log)?;
    Ok(log.tests)
}

/// Runs the suite to its end, as a window whose user lets the suite read
/// the clipboards, as xterm's window operations do: each read gets what the
/// suite wrote last.
fn serve(harness: &mut Harness) -> Result<()> {
    let deadline = Instant::now() + LIMIT;
    let mut clipboard = Vec::new();
    while harness.try_exit_status()?.is_none() {
        ensure!(
            Instant::now() < deadline,
            "esctest did not end in {LIMIT:?}"
        );
        harness.sleep(TURN)?;
        for event in harness.take_terminal_events() {
            match event {
                Event::ClipboardWrite { data, .. } => clipboard = data,
                Event::ClipboardRead(request) => {
                    harness.terminal_mut().reply_clipboard(request, &clipboard);
                }
                _ => {}
            }
        }
    }
    // What it wrote before it ended.
    harness.exit(LIMIT)
}

/// A directory of the temporary one, for esctest's log and home, removed
/// as it drops unless it is kept.
struct Temp {
    dir: PathBuf,
    kept: bool,
}

impl Temp {
    fn new() -> Result<Self> {
        // A number for each, as tests make several at once.
        static MADE: AtomicUsize = AtomicUsize::new(0);
        let made = MADE.fetch_add(1, Ordering::Relaxed);
        let name = format!("bolt-esctest-{}-{made}", std::process::id());
        let dir = std::env::temp_dir().join(name);
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
        Ok(Self { dir, kept: false })
    }
}

impl Drop for Temp {
    fn drop(&mut self) {
        if !self.kept {
            let _ = fs::remove_dir_all(&self.dir);
        }
    }
}

/// How a test ended.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Outcome {
    Passed,
    /// It fails as esctest expects of xterm.
    KnownBug,
    /// It needs a higher VT level than the one asked for.
    Skipped,
    /// It failed, with the line of the exception that its traceback ends
    /// with, and, for a reply that did not come, the function of esctest's
    /// that waited for it.
    Failed(String),
}

/// What esctest's log tells.
#[derive(Debug, PartialEq, Eq)]
struct Log {
    /// Each test, in order, with its outcome.
    tests: Vec<(String, Outcome)>,
    /// The counts of the closing line, if the suite got to it.
    totals: Option<Totals>,
}

/// The counts of esctest's closing line, which counts the tests skipped
/// with the known bugs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Totals {
    passed: usize,
    known: usize,
    failed: usize,
}

impl fmt::Display for Totals {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} passed, {} known bugs or skipped and {} failed",
            self.passed, self.known, self.failed
        )
    }
}

/// A failing test's traceback, read a line at a time.
struct Failure {
    name: String,
    /// The line of the exception, once the frames are read.
    reason: String,
    /// The function of the innermost frame outside the reading of replies.
    function: String,
    /// Whether the lines read are a traceback's frames.
    frames: bool,
}

impl Failure {
    fn new(name: String) -> Self {
        Self {
            name,
            reason: String::new(),
            function: String::new(),
            frames: false,
        }
    }

    /// Reads a line of the traceback. Its frames are indented, and the
    /// exception's line follows them, then the rest of the message, which
    /// can hold blank lines; a chain of exceptions prints a traceback for
    /// each, the one raised last at the end.
    fn read(&mut self, line: &str) {
        if line == "Traceback (most recent call last):" {
            self.reason.clear();
            self.function.clear();
            self.frames = true;
        } else if self.frames {
            if line.starts_with(char::is_whitespace) {
                if let Some(frame) = line.trim_start().strip_prefix("File \"")
                    && !frame.contains("escio.py\"")
                    && let Some((_, called)) = frame.rsplit_once(", in ")
                {
                    called.clone_into(&mut self.function);
                }
            } else if !line.is_empty() {
                line.trim_end().clone_into(&mut self.reason);
                self.frames = false;
            }
        }
    }

    /// The test, failed for the exception's line, which names the function
    /// when a reply did not come, so that the replies missing show.
    fn outcome(self) -> (String, Outcome) {
        let reason =
            if self.reason.ends_with("Timeout waiting to read.") && !self.function.is_empty() {
                format!("{} (in {})", self.reason, self.function)
            } else {
                self.reason
            };
        (self.name, Outcome::Failed(reason))
    }
}

/// The tests that the log tells of, and its closing line's counts.
fn parse(log: &str) -> Log {
    let mut tests = Vec::new();
    let mut totals = None;
    let mut current: Option<String> = None;
    let mut failing: Option<Failure> = None;
    for line in log.lines() {
        let next = line.strip_prefix("Run test: ");
        let closing = closing_line(line);
        // A failure's traceback goes on to the next test, or to the end.
        if let Some(failure) = &mut failing {
            if next.is_none() && closing.is_none() {
                failure.read(line);
                continue;
            }
            tests.extend(failing.take().map(Failure::outcome));
        }
        if let Some(name) = next {
            current = Some(name.to_owned());
        } else if closing.is_some() {
            totals = closing;
            current = None;
        } else if let Some(name) = current.take() {
            if line == "Passed." {
                tests.push((name, Outcome::Passed));
            } else if line.starts_with("Fails as expected") {
                tests.push((name, Outcome::KnownBug));
            } else if line.starts_with("Skipped because") {
                tests.push((name, Outcome::Skipped));
            } else if line.starts_with("*** TEST ") && line.ends_with(" FAILED:") {
                failing = Some(Failure::new(name));
            } else {
                current = Some(name);
            }
        }
    }
    tests.extend(failing.map(Failure::outcome));
    Log { tests, totals }
}

/// The counts of esctest's closing line, as `*** 5 tests passed, 1 known
/// bug, 2 TESTS FAILED ***`, or `0 tests failed` when none did.
fn closing_line(line: &str) -> Option<Totals> {
    let counts = line.strip_prefix("*** ")?.strip_suffix(" ***")?;
    let mut parts = counts.split(", ");
    let mut count = |one: &str, many: &str| -> Option<usize> {
        let (number, what) = parts.next()?.split_once(' ')?;
        let what = what.to_ascii_lowercase();
        (what == one || what == many).then(|| number.parse().ok())?
    };
    let totals = Totals {
        passed: count("test passed", "tests passed")?,
        known: count("known bug", "known bugs")?,
        failed: count("test failed", "tests failed")?,
    };
    parts.next().is_none().then_some(totals)
}

/// Checks that the log ends with the suite's closing line, and that the
/// tests read agree with its counts.
fn check(log: &Log) -> Result<()> {
    let Some(totals) = log.totals else {
        bail!("esctest's log ends before its closing line, as a run cut short");
    };
    let count =
        |outcome: fn(&Outcome) -> bool| log.tests.iter().filter(|(_, o)| outcome(o)).count();
    let read = Totals {
        passed: count(|outcome| *outcome == Outcome::Passed),
        known: count(|outcome| matches!(outcome, Outcome::KnownBug | Outcome::Skipped)),
        failed: count(|outcome| matches!(outcome, Outcome::Failed(_))),
    };
    ensure!(
        read == totals,
        "esctest counted {totals}, and the runner read {read} in its log"
    );
    Ok(())
}

/// The pass rate, over the tests that passed or failed, and the failures,
/// by the class that each test is in, with how many failed for each reason.
fn report(tests: &[(String, Outcome)]) -> String {
    let count = |outcome: fn(&Outcome) -> bool| tests.iter().filter(|(_, o)| outcome(o)).count();
    let passed = count(|outcome| *outcome == Outcome::Passed);
    let failed = count(|outcome| matches!(outcome, Outcome::Failed(_)));
    let known = count(|outcome| *outcome == Outcome::KnownBug);
    let skipped = count(|outcome| *outcome == Outcome::Skipped);
    let applicable = passed + failed;
    // A share of the tests, as a percentage.
    let rate = if applicable == 0 {
        0.0
    } else {
        100.0 * passed as f64 / applicable as f64
    };
    let mut text = format!(
        "{passed} of {applicable} applicable tests passed ({rate:.1}%); {known} known bugs of xterm and {skipped} skipped\n"
    );
    let mut classes: BTreeMap<&str, BTreeMap<&str, Vec<&str>>> = BTreeMap::new();
    for (name, outcome) in tests {
        if let Outcome::Failed(reason) = outcome {
            let (class, test) = name.split_once('.').unwrap_or((name, ""));
            classes
                .entry(class)
                .or_default()
                .entry(reason)
                .or_default()
                .push(test);
        }
    }
    for (class, reasons) in classes {
        let failures: usize = reasons.values().map(Vec::len).sum();
        text.push_str(&format!("\n{class}: {failures} failed\n"));
        for (reason, tests) in reasons {
            text.push_str(&format!("  {} × {reason}\n", tests.len()));
            for test in tests {
                text.push_str(&format!("    {test}\n"));
            }
        }
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_log_tells_each_tests_outcome() {
        let log = "Run test: CUPTests.test_a\nPassed.\n\n\
                   Run test: CUPTests.test_b\n*** TEST CUPTests.test_b FAILED:\n\
                   Traceback (most recent call last):\n  File \"/e/escutil.py\", line 313, in GetChecksumOfRect\n\
                   \x20   params = escio.ReadDCS()\n  File \"/e/escio.py\", line 243, in read\n\
                   esctypes.InternalError: Timeout waiting to read.\n\n\n\
                   Run test: DSRTests.test_c\nFails as expected: bug\n\n\
                   Run test: DSRTests.test_d\nSkipped because terminal lacks requisite capability: VT 5\n\n\
                   *** 1 test passed, 2 known bugs, 1 TEST FAILED ***\nFailing tests:\nCUPTests.test_b\n";
        let log = parse(log);
        assert_eq!(
            log.tests,
            [
                ("CUPTests.test_a".to_owned(), Outcome::Passed),
                (
                    "CUPTests.test_b".to_owned(),
                    Outcome::Failed(
                        "esctypes.InternalError: Timeout waiting to read. (in GetChecksumOfRect)"
                            .to_owned()
                    )
                ),
                ("DSRTests.test_c".to_owned(), Outcome::KnownBug),
                ("DSRTests.test_d".to_owned(), Outcome::Skipped),
            ]
        );
        check(&log).unwrap();
        let report = report(&log.tests);
        assert!(
            report.starts_with(
                "1 of 2 applicable tests passed (50.0%); 1 known bugs of xterm and 1 skipped\n"
            ),
            "{report}"
        );
        assert!(report.contains("CUPTests: 1 failed\n  1 × esctypes.InternalError: Timeout waiting to read. (in GetChecksumOfRect)\n    test_b\n"), "{report}");
    }

    #[test]
    fn a_failures_reason_is_its_exceptions_line_past_the_blank_lines_of_its_message() {
        // A checksum's message holds a blank line, and a chain of
        // exceptions ends with the one raised last.
        let log = "Run test: EDTests.test_a\n*** TEST EDTests.test_a FAILED:\n\
                   Traceback (most recent call last):\n  File \"/e/tests/ed.py\", line 9, in test_a\n\
                   \x20   AssertScreenCharsInRectEqual(rect, [\"ab\"])\n\
                   esctypes.ChecksumException: Checksum failed at the following locations:\n\
                   Point(x=2, y=1)\nActual:\naX\n\nExpected:\nab\n\n\n\
                   Run test: EDTests.test_b\nPassed.\n\n\
                   Run test: DATests.test_c\n*** TEST DATests.test_c FAILED:\n\
                   Traceback (most recent call last):\n  File \"/e/escio.py\", line 243, in read\n\
                   \x20   raise esctypes.InternalError(\"Timeout waiting to read.\")\n\
                   esctypes.InternalError: Timeout waiting to read.\n\n\
                   During handling of the above exception, another exception occurred:\n\n\
                   Traceback (most recent call last):\n  File \"/e/tests/da.py\", line 20, in test_c\n\
                   \x20   AssertEQ(params, [1])\n\
                   esctypes.TestFailure: Test failed: expected '[1]' but got '[]'\n\n\n\
                   *** 1 test passed, 0 known bugs, 2 TESTS FAILED ***\n";
        let log = parse(log);
        let checksum = "esctypes.ChecksumException: Checksum failed at the following locations:";
        assert_eq!(
            log.tests,
            [
                (
                    "EDTests.test_a".to_owned(),
                    Outcome::Failed(checksum.to_owned())
                ),
                ("EDTests.test_b".to_owned(), Outcome::Passed),
                (
                    "DATests.test_c".to_owned(),
                    Outcome::Failed(
                        "esctypes.TestFailure: Test failed: expected '[1]' but got '[]'".to_owned()
                    )
                ),
            ]
        );
        check(&log).unwrap();
    }

    #[test]
    fn the_closing_line_counts_the_tests() {
        let totals = |passed, known, failed| Totals {
            passed,
            known,
            failed,
        };
        let line = "*** 403 tests passed, 39 known bugs, 125 TESTS FAILED ***";
        assert_eq!(closing_line(line), Some(totals(403, 39, 125)));
        let line = "*** 1 test passed, 1 known bug, 0 tests failed ***";
        assert_eq!(closing_line(line), Some(totals(1, 1, 0)));
        let line = "*** 1 test passed, 0 known bugs, 1 TEST FAILED ***";
        assert_eq!(closing_line(line), Some(totals(1, 0, 1)));
        assert_eq!(closing_line("*** TEST CUPTests.test_a FAILED:"), None);
        assert_eq!(closing_line("*** 1 test passed ***"), None);
        // A log cut short, or one whose tests the runner misread.
        let passed = "Run test: A.test_a\nPassed.\n\n";
        let error = check(&parse(passed)).unwrap_err().to_string();
        assert!(error.contains("before its closing line"), "{error}");
        let log = format!("{passed}*** 2 tests passed, 0 known bugs, 0 tests failed ***\n");
        let error = check(&parse(&log)).unwrap_err().to_string();
        assert_eq!(
            error,
            "esctest counted 2 passed, 0 known bugs or skipped and 0 failed, \
             and the runner read 1 passed, 0 known bugs or skipped and 0 failed in its log"
        );
    }

    /// Runs `script` as the suite, with the log's path as `$1`.
    fn fake(script: &str, temp: &mut Temp) -> Result<Vec<(String, Outcome)>> {
        let log = temp.dir.join("esctest.log");
        let options = HarnessOptions {
            program: vec![
                "/bin/sh".to_owned(),
                "-c".to_owned(),
                script.to_owned(),
                "sh".to_owned(),
                log.display().to_string(),
            ],
            env: vec![("PATH".to_owned(), "/usr/bin:/bin".to_owned())],
            terminal: Options {
                clipboard_reads: true,
                ..Options::default()
            },
            dir: temp.dir.clone(),
            cols: 40,
            rows: 5,
        };
        suite(&options, temp, &log)
    }

    #[test]
    fn a_run_that_does_not_end_well_says_so_and_keeps_its_log() {
        let closing = "*** 1 test passed, 0 known bugs, 0 tests failed ***";
        let passed = format!("printf 'Run test: A.test_a\\nPassed.\\n\\n{closing}\\n' > \"$1\"");
        for (script, says) in [
            (
                format!("echo shown; {passed}; exit 3"),
                "esctest exited with status 3",
            ),
            ("echo shown; kill -TERM $$".to_owned(), "ended by signal 15"),
            ("echo shown".to_owned(), "esctest left no log to read"),
            (
                "echo shown; printf 'Run test: A.test_a\\nPassed.\\n' > \"$1\"".to_owned(),
                "before its closing line",
            ),
        ] {
            let mut temp = Temp::new().unwrap();
            let message = format!("{:#}", fake(&script, &mut temp).unwrap_err());
            assert!(message.contains(says), "{message}");
            assert!(message.contains("The screen:\nshown"), "{message}");
            assert!(message.contains("The log is kept in"), "{message}");
            let dir = temp.dir.clone();
            drop(temp);
            assert!(dir.exists(), "{message}");
            fs::remove_dir_all(dir).unwrap();
        }
        let mut temp = Temp::new().unwrap();
        let tests = fake(&passed, &mut temp).unwrap();
        assert_eq!(tests, [("A.test_a".to_owned(), Outcome::Passed)]);
        let dir = temp.dir.clone();
        drop(temp);
        assert!(!dir.exists());
    }

    #[test]
    fn the_suite_reads_back_what_it_wrote_to_the_clipboard() {
        // The suite writes `hi`, asks for it, and passes if it comes back
        // within 2 s.
        let script = r#"stty raw -echo min 0 time 20; printf '\033]52;;aGk=\033\\\033]52;;?\033\\'; r=$(dd bs=1 count=14 2>/dev/null); [ "$r" = "$(printf '\033]52;s0;aGk=\033\\')" ] && printf 'Run test: A.test_a\nPassed.\n\n*** 1 test passed, 0 known bugs, 0 tests failed ***\n' > "$1""#;
        let mut temp = Temp::new().unwrap();
        let tests = fake(script, &mut temp).unwrap();
        assert_eq!(tests, [("A.test_a".to_owned(), Outcome::Passed)]);
    }

    #[test]
    fn options_given_join_the_runners_and_a_log_is_refused() {
        let args = |list: &[&str]| arguments(list.iter().map(|arg| (*arg).to_owned()).collect());
        assert_eq!(
            args(&["--include=CUP", "--options=disableWideChars"]).unwrap(),
            [
                "--include=CUP",
                "--options",
                "xtermWinopsEnabled",
                "disableWideChars"
            ]
        );
        assert_eq!(
            args(&["--options", "a", "b", "--stop-on-failure"]).unwrap(),
            [
                "--stop-on-failure",
                "--options",
                "xtermWinopsEnabled",
                "a",
                "b"
            ]
        );
        assert_eq!(args(&[]).unwrap(), ["--options", "xtermWinopsEnabled"]);
        for log in [&["--logfile=x"][..], &["--logfile", "x"]] {
            let error = args(log).unwrap_err().to_string();
            assert!(error.contains("--logfile"), "{error}");
        }
    }
}
