//! `bolt-esctest`: runs esctest, the suite of terminal tests of iTerm2 and
//! xterm (<https://github.com/ThomasDickey/esctest2>), in a headless
//! terminal, and tells its pass rate and its failures, grouped by the
//! sequence that each test's class is named for.
//!
//! `bolt-esctest DIR [ARG...]` runs `DIR/esctest.py` with `python3`, from a
//! checkout that the repository does not keep, and passes it the ARGs, as
//! `--include=REGEX`. The terminal is 80 by 25, the size that esctest asks
//! for, with cells of 8 by 16 pixels, and reports titles and the checksums
//! of rectangles, through which esctest reads the screen, as xterm does
//! when it is allowed to.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

use anyhow::{Context, Result};
use bolt_replay::harness::{Harness, HarnessOptions};
use bolt_vt_core::terminal::Options;

/// How long esctest waits for each byte of a reply, in seconds: shorter
/// than its own default of a second, as a reply that a headless terminal
/// makes comes at once, or never.
const TIMEOUT: &str = "0.5";

/// How long the whole suite may take.
const LIMIT: Duration = Duration::from_secs(3 * 60 * 60);

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
    let temp = Temp::new()?;
    let log = temp.0.join("esctest.log");
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
    program.extend(extra);
    let options = HarnessOptions {
        program,
        env: vec![
            ("PATH".to_owned(), "/usr/bin:/bin".to_owned()),
            ("LANG".to_owned(), "en_US.UTF-8".to_owned()),
            ("HOME".to_owned(), temp.0.display().to_string()),
        ],
        terminal: Options {
            title_reports: true,
            rectangle_checksums: true,
            ..Options::default()
        },
        dir: dir.to_owned(),
        cols: 80,
        rows: 25,
    };
    let mut harness = Harness::spawn(&options)?;
    // Cells of a font, so that the sizes in pixels are told, as a window's.
    harness.set_cell_size(8, 16)?;
    harness.exit(LIMIT)?;
    let text = fs::read_to_string(&log).with_context(|| format!("reading {}", log.display()))?;
    print!("{}", report(&parse(&text)));
    Ok(())
}

/// A directory of the temporary one, for esctest's log, removed as it
/// drops.
struct Temp(PathBuf);

impl Temp {
    fn new() -> Result<Self> {
        let dir = std::env::temp_dir().join(format!("bolt-esctest-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
        Ok(Self(dir))
    }
}

impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
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
    /// It failed, with the last line of its traceback, and, for a reply that
    /// did not come, the function of esctest's that waited for it.
    Failed(String),
}

/// Each test that the log tells of, in order, with its outcome.
fn parse(log: &str) -> Vec<(String, Outcome)> {
    let mut tests = Vec::new();
    let mut current: Option<String> = None;
    // The failing test, the last line of its traceback, and the function of
    // its innermost frame outside the reading of replies.
    let mut failing: Option<(String, String, String)> = None;
    for line in log.lines() {
        if let Some((name, reason, function)) = &mut failing {
            // A failure's traceback goes on to the blank line after it.
            if line.is_empty() {
                tests.push((name.clone(), failed(reason, function)));
                failing = None;
            } else if let Some(frame) = line.trim().strip_prefix("File \"") {
                if !frame.contains("escio.py\"")
                    && let Some((_, called)) = frame.rsplit_once(", in ")
                {
                    called.clone_into(function);
                }
            } else {
                line.trim().clone_into(reason);
            }
            continue;
        }
        if let Some(name) = line.strip_prefix("Run test: ") {
            current = Some(name.to_owned());
        } else if let Some(name) = current.take() {
            if line == "Passed." {
                tests.push((name, Outcome::Passed));
            } else if line.starts_with("Fails as expected") {
                tests.push((name, Outcome::KnownBug));
            } else if line.starts_with("Skipped because") {
                tests.push((name, Outcome::Skipped));
            } else if line.starts_with("*** TEST ") && line.ends_with(" FAILED:") {
                failing = Some((name, String::new(), String::new()));
            } else {
                current = Some(name);
            }
        }
    }
    if let Some((name, reason, function)) = failing {
        tests.push((name, failed(&reason, &function)));
    }
    tests
}

/// A failure for `reason`, which names `function` when a reply did not
/// come, so that the replies missing show.
fn failed(reason: &str, function: &str) -> Outcome {
    if reason.ends_with("Timeout waiting to read.") && !function.is_empty() {
        Outcome::Failed(format!("{reason} (in {function})"))
    } else {
        Outcome::Failed(reason.to_owned())
    }
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
                   esctypes.InternalError: Timeout waiting to read.\n\n\
                   Run test: DSRTests.test_c\nFails as expected: bug\n\n\
                   Run test: DSRTests.test_d\nSkipped because terminal lacks requisite capability: VT 5\n\n";
        let tests = parse(log);
        assert_eq!(
            tests,
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
        let report = report(&tests);
        assert!(
            report.starts_with(
                "1 of 2 applicable tests passed (50.0%); 1 known bugs of xterm and 1 skipped\n"
            ),
            "{report}"
        );
        assert!(report.contains("CUPTests: 1 failed\n  1 × esctypes.InternalError: Timeout waiting to read. (in GetChecksumOfRect)\n    test_b\n"), "{report}");
    }
}
