//! `lokyr-bench startup`: times how long an app takes to show its first
//! window.
//!
//! Each run launches the app through LaunchServices, as the Dock does, then
//! polls the window server until the app's process has a window on screen.
//! The probe then quits the app, and waits for it to go away before the next
//! run. Warmup runs come first and are not counted. Prints JSON on standard
//! output, every run then a summary, and progress on standard error.

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime};

use anyhow::{Context, Result, bail};
use serde::Serialize;

use crate::{process, windows};

/// Runs counted by default.
const DEFAULT_RUNS: u32 = 10;
/// Runs made first, and not counted, by default.
const DEFAULT_WARMUP: u32 = 2;
/// How often the probe looks for the app's process and for its window.
const POLL: Duration = Duration::from_millis(2);
/// How long a launch, or a quit, may take before the probe gives up.
const TIMEOUT: Duration = Duration::from_secs(20);
/// The pause after each run, which lets the system settle.
const SETTLE: Duration = Duration::from_secs(1);

/// Which app to launch, and how many times.
#[derive(Debug, PartialEq)]
struct Options {
    app: PathBuf,
    runs: u32,
    warmup: u32,
}

/// One launch of the app.
#[derive(Debug, Serialize)]
struct Run {
    pid: libc::pid_t,
    /// From the start of the process, as the kernel recorded it, to the first
    /// window.
    from_start_ms: f64,
    /// From the launch request to the first window, LaunchServices included.
    from_request_ms: f64,
}

/// Minimum, 10th percentile, median, 90th percentile and maximum of a
/// series.
#[derive(Debug, PartialEq, Serialize)]
struct Distribution {
    min: f64,
    p10: f64,
    median: f64,
    p90: f64,
    max: f64,
}

#[derive(Debug, Serialize)]
struct Summary {
    from_start_ms: Distribution,
    from_request_ms: Distribution,
    /// The range of the central 80% of `from_start_ms` (90th minus 10th
    /// percentile), as a fraction of its median. Unlike the full range, a
    /// single stray run barely moves it.
    spread: f64,
}

#[derive(Debug, Serialize)]
struct Report {
    app: PathBuf,
    executable: PathBuf,
    warmup: u32,
    runs: Vec<Run>,
    summary: Summary,
}

/// Runs the probe with the arguments that follow `startup`.
pub fn run(args: impl Iterator<Item = String>) -> Result<ExitCode> {
    let options = parse(args)?;
    let (app, executable) = bundle(&options.app)?;
    if !process::pids_of(&executable)?.is_empty() {
        bail!("{} is already running: quit it first", app.display());
    }
    for n in 1..=options.warmup {
        let run = launch(&app, &executable)?;
        eprintln!("warmup {n}/{}: {:.1} ms", options.warmup, run.from_start_ms);
    }
    let mut runs = Vec::new();
    for n in 1..=options.runs {
        let run = launch(&app, &executable)?;
        eprintln!("run {n}/{}: {:.1} ms", options.runs, run.from_start_ms);
        runs.push(run);
    }
    let summary = summarize(&runs).context("no runs")?;
    let report = Report {
        app,
        executable,
        warmup: options.warmup,
        runs,
        summary,
    };
    serde_json::to_writer_pretty(std::io::stdout().lock(), &report)?;
    println!();
    Ok(ExitCode::SUCCESS)
}

/// Parses `--app <path> [--runs <n>] [--warmup <n>]`.
fn parse(mut args: impl Iterator<Item = String>) -> Result<Options> {
    let mut app = None;
    let mut runs = DEFAULT_RUNS;
    let mut warmup = DEFAULT_WARMUP;
    while let Some(arg) = args.next() {
        let mut value = || {
            args.next()
                .with_context(|| format!("`{arg}` needs a value"))
        };
        let count = |value: String| -> Result<u32> {
            value
                .parse()
                .with_context(|| format!("`{value}` is not a number of runs"))
        };
        match arg.as_str() {
            "--app" => app = Some(PathBuf::from(value()?)),
            "--runs" => {
                runs = count(value()?)?;
                if runs == 0 {
                    bail!("at least one run is needed");
                }
            }
            "--warmup" => warmup = count(value()?)?,
            other => bail!("unknown argument `{other}`"),
        }
    }
    Ok(Options {
        app: app.context("`--app <path>` is required")?,
        runs,
        warmup,
    })
}

/// The app bundle at `app`, made absolute, and its executable, which its
/// Info.plist names.
fn bundle(app: &Path) -> Result<(PathBuf, PathBuf)> {
    let app = app
        .canonicalize()
        .with_context(|| format!("cannot find {}", app.display()))?;
    let output = Command::new("/usr/bin/plutil")
        .args(["-extract", "CFBundleExecutable", "raw", "-o", "-"])
        .arg(app.join("Contents/Info.plist"))
        .stdin(Stdio::null())
        .output()
        .context("cannot run /usr/bin/plutil")?;
    if !output.status.success() {
        bail!(
            "{} has no CFBundleExecutable in its Info.plist",
            app.display()
        );
    }
    let name = String::from_utf8(output.stdout).context("the executable name is not UTF-8")?;
    let executable = app.join("Contents/MacOS").join(name.trim());
    Ok((app, executable))
}

/// Launches the app once, times its first window, then quits it.
fn launch(app: &Path, executable: &Path) -> Result<Run> {
    let requested = Instant::now();
    let status = Command::new("/usr/bin/open")
        .arg("-a")
        .arg(app)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .status()
        .context("cannot run /usr/bin/open")?;
    if !status.success() {
        bail!("open could not launch {}", app.display());
    }
    let pid = poll("the app's process", || {
        Ok(process::pids_of(executable)?.into_iter().next())
    })?;
    let launched = Launched {
        pid,
        executable,
        quit: false,
    };
    let started = process::start_time(pid).context("cannot read when the app started")?;
    poll("the app's first window", || {
        Ok(windows::has_window(pid).then_some(()))
    })?;
    let shown = (Instant::now(), SystemTime::now());
    let from_start = shown
        .1
        .duration_since(started)
        .context("the app started after its window appeared")?;
    launched.quit()?;
    thread::sleep(SETTLE);
    Ok(Run {
        pid,
        from_start_ms: milliseconds(from_start),
        from_request_ms: milliseconds(shown.0 - requested),
    })
}

/// An app launched by the probe. It is quit when dropped, so that a failed
/// run does not leave it open.
struct Launched<'a> {
    pid: libc::pid_t,
    executable: &'a Path,
    quit: bool,
}

impl Launched<'_> {
    /// Asks the app to terminate, kills it if it does not, and waits until its
    /// process is gone.
    fn quit(mut self) -> Result<()> {
        self.quit = true;
        quit(self.pid, self.executable)
    }
}

impl Drop for Launched<'_> {
    fn drop(&mut self) {
        if !self.quit {
            let _ = quit(self.pid, self.executable);
        }
    }
}

fn quit(pid: libc::pid_t, executable: &Path) -> Result<()> {
    let gone = || -> Result<Option<()>> {
        Ok((!process::pids_of(executable)?.contains(&pid)).then_some(()))
    };
    // The process may have ended on its own already.
    let _ = process::terminate(pid);
    if poll("the app to quit", gone).is_ok() {
        return Ok(());
    }
    let _ = process::kill(pid);
    poll("the app to be killed", gone)
}

/// Calls `check` every `POLL` until it returns a value, for at most
/// `TIMEOUT`.
fn poll<T>(what: &str, mut check: impl FnMut() -> Result<Option<T>>) -> Result<T> {
    let start = Instant::now();
    loop {
        if let Some(value) = check()? {
            return Ok(value);
        }
        if start.elapsed() > TIMEOUT {
            bail!("gave up waiting for {what} after {} s", TIMEOUT.as_secs());
        }
        thread::sleep(POLL);
    }
}

/// `duration` in milliseconds, rounded to a tenth.
fn milliseconds(duration: Duration) -> f64 {
    (duration.as_secs_f64() * 10_000.0).round() / 10.0
}

/// The distributions of both times, and the spread of the time from the
/// process start. `None` without runs.
fn summarize(runs: &[Run]) -> Option<Summary> {
    let from_start = Distribution::of(runs.iter().map(|run| run.from_start_ms))?;
    let from_request = Distribution::of(runs.iter().map(|run| run.from_request_ms))?;
    let spread = (from_start.p90 - from_start.p10) / from_start.median;
    Some(Summary {
        from_start_ms: from_start,
        from_request_ms: from_request,
        spread: (spread * 1000.0).round() / 1000.0,
    })
}

impl Distribution {
    /// The distribution of `values`, or `None` if there are none. The median
    /// of an even number of values is the mean of the two middle ones; the
    /// percentiles are nearest ranks.
    fn of(values: impl Iterator<Item = f64>) -> Option<Self> {
        let mut sorted: Vec<f64> = values.collect();
        if sorted.is_empty() {
            return None;
        }
        sorted.sort_by(f64::total_cmp);
        let n = sorted.len();
        let median = if n.is_multiple_of(2) {
            // The mean of two tenths can land between hundredths, as
            // 302.54999999999995 for 302.55.
            (f64::midpoint(sorted[n / 2 - 1], sorted[n / 2]) * 100.0).round() / 100.0
        } else {
            sorted[n / 2]
        };
        // Nearest ranks, counted from 1: ceil(0.1 n) and ceil(0.9 n).
        let p10 = sorted[n.div_ceil(10) - 1];
        let p90 = sorted[(9 * n).div_ceil(10) - 1];
        Some(Self {
            min: sorted[0],
            p10,
            median,
            p90,
            max: sorted[n - 1],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn options(args: &[&str]) -> Result<Options> {
        parse(args.iter().map(ToString::to_string))
    }

    #[test]
    fn defaults() {
        assert_eq!(
            options(&["--app", "/Applications/Foo.app"]).unwrap(),
            Options {
                app: PathBuf::from("/Applications/Foo.app"),
                runs: 10,
                warmup: 2,
            }
        );
    }

    #[test]
    fn runs_and_warmup() {
        let parsed = options(&["--runs", "5", "--app", "x.app", "--warmup", "0"]).unwrap();
        assert_eq!((parsed.runs, parsed.warmup), (5, 0));
    }

    #[test]
    fn rejects_bad_arguments() {
        for args in [
            &[][..],
            &["--app"],
            &["--app", "x.app", "--runs", "0"],
            &["--app", "x.app", "--runs", "many"],
            &["--app", "x.app", "--cold"],
        ] {
            assert!(options(args).is_err(), "{args:?}");
        }
    }

    #[test]
    fn distribution_of_ten_runs() {
        let values = [
            310.0, 300.0, 330.0, 320.0, 305.0, 315.0, 325.0, 335.0, 400.0, 312.0,
        ];
        let distribution = Distribution::of(values.into_iter()).unwrap();
        assert_eq!(
            distribution,
            Distribution {
                min: 300.0,
                p10: 300.0,
                median: 317.5,
                p90: 335.0,
                max: 400.0,
            }
        );
        assert_eq!(Distribution::of(std::iter::empty()), None);
        let pair = Distribution::of([302.5, 302.6].into_iter()).unwrap();
        assert_eq!(pair.median, 302.55);
    }

    #[test]
    fn spread_covers_the_central_runs() {
        // Ten runs of Terminal.app, with one slower than the others.
        let runs = [
            283.7, 281.2, 292.0, 281.7, 301.0, 286.9, 281.7, 280.3, 272.1, 279.7,
        ]
        .map(|ms| Run {
            pid: 1,
            from_start_ms: ms,
            from_request_ms: ms + 20.0,
        });
        let summary = summarize(&runs).unwrap();
        assert_eq!(summary.from_start_ms.median, 281.7);
        assert_eq!(summary.from_start_ms.p10, 272.1);
        assert_eq!(summary.from_start_ms.p90, 292.0);
        // The full range would be 10.3% of the median, because of the slow run.
        assert_eq!(summary.spread, 0.071);
        assert_eq!(summary.from_request_ms.min, 292.1);
    }

    #[test]
    fn rounds_to_a_tenth_of_a_millisecond() {
        assert_eq!(milliseconds(Duration::from_micros(412_345)), 412.3);
        assert_eq!(milliseconds(Duration::from_micros(412_355)), 412.4);
    }
}
