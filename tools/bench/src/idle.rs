//! `lokyr-bench idle`: counts how often a process wakes up.
//!
//! Reads the process's wakeup counters at the start and at every interval,
//! then prints JSON on standard output: the wakeups of each interval, and the
//! rates over the whole duration. An idle process should not wake up at all.

use std::io;
use std::process::ExitCode;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use serde::Serialize;

use crate::process::{self, Wakeups};
use crate::sampling::{self, Options, Stats};

/// How long the probe watches the process unless told otherwise.
const DEFAULT_DURATION: Duration = Duration::from_secs(60);

/// The counters as read at one point in time.
#[derive(Clone, Copy, Debug)]
struct Reading {
    /// Time since the first reading.
    elapsed: Duration,
    wakeups: Wakeups,
}

/// The wakeups between two consecutive readings.
#[derive(Debug, PartialEq, Serialize)]
struct Interval {
    /// Time since the first reading, at the end of the interval.
    elapsed_ms: u128,
    package_idle_wakeups: u64,
    interrupt_wakeups: u64,
}

#[derive(Debug, Serialize)]
struct Summary {
    /// Wakeups per second, over the whole duration.
    package_idle_wakeups_per_s: f64,
    interrupt_wakeups_per_s: f64,
    /// Wakeups per interval.
    package_idle_wakeups: Stats,
    interrupt_wakeups: Stats,
}

/// The counters at the first and at the last reading. They count from the
/// start of the process, as `top` shows them.
#[derive(Debug, Serialize)]
struct Counters {
    first: Wakeups,
    last: Wakeups,
}

#[derive(Debug, Serialize)]
struct Report {
    pid: libc::pid_t,
    name: String,
    interval_ms: u128,
    duration_s: f64,
    counters: Counters,
    intervals: Vec<Interval>,
    summary: Summary,
}

/// Runs the probe with the arguments that follow `idle`.
pub fn run(args: impl Iterator<Item = String>) -> Result<ExitCode> {
    let options = Options::parse(args, DEFAULT_DURATION)?;
    if options.duration < options.interval {
        bail!("the duration must cover at least one interval");
    }
    let name = process::name(options.pid)
        .with_context(|| format!("cannot read the name of process {}", options.pid))?;
    let readings = read(&options, process::wakeups)?;
    let intervals = intervals(&readings);
    let summary = summarize(&readings, &intervals).context("no complete interval")?;
    let counters = Counters {
        first: readings.first().context("no readings")?.wakeups,
        last: readings.last().context("no readings")?.wakeups,
    };
    let report = Report {
        pid: options.pid,
        name,
        interval_ms: options.interval.as_millis(),
        duration_s: options.duration.as_secs_f64(),
        counters,
        intervals,
        summary,
    };
    serde_json::to_writer_pretty(std::io::stdout().lock(), &report)?;
    println!();
    Ok(ExitCode::SUCCESS)
}

/// Reads the counters of the target at every interval, with `wakeups`.
fn read(
    options: &Options,
    mut wakeups: impl FnMut(libc::pid_t) -> io::Result<Wakeups>,
) -> Result<Vec<Reading>> {
    let mut readings = Vec::new();
    sampling::every_interval(options, |elapsed| {
        let wakeups = wakeups(options.pid)
            .with_context(|| format!("cannot read the wakeups of process {}", options.pid))?;
        readings.push(Reading { elapsed, wakeups });
        Ok(())
    })?;
    Ok(readings)
}

/// The wakeups between each pair of consecutive readings.
fn intervals(readings: &[Reading]) -> Vec<Interval> {
    readings
        .windows(2)
        .map(|pair| Interval {
            elapsed_ms: pair[1].elapsed.as_millis(),
            package_idle_wakeups: pair[1]
                .wakeups
                .package_idle
                .saturating_sub(pair[0].wakeups.package_idle),
            interrupt_wakeups: pair[1]
                .wakeups
                .interrupt
                .saturating_sub(pair[0].wakeups.interrupt),
        })
        .collect()
}

/// The rates from the first to the last reading, and the statistics of the
/// intervals. `None` without a complete interval.
fn summarize(readings: &[Reading], intervals: &[Interval]) -> Option<Summary> {
    let (first, last) = (readings.first()?, readings.last()?);
    let seconds = last.elapsed.checked_sub(first.elapsed)?.as_secs_f64();
    if seconds == 0.0 {
        return None;
    }
    let per_second = |count: u64| count as f64 / seconds;
    Some(Summary {
        package_idle_wakeups_per_s: per_second(
            last.wakeups
                .package_idle
                .saturating_sub(first.wakeups.package_idle),
        ),
        interrupt_wakeups_per_s: per_second(
            last.wakeups
                .interrupt
                .saturating_sub(first.wakeups.interrupt),
        ),
        package_idle_wakeups: Stats::of(intervals.iter().map(|i| i.package_idle_wakeups))?,
        interrupt_wakeups: Stats::of(intervals.iter().map(|i| i.interrupt_wakeups))?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reading(ms: u64, package_idle: u64, interrupt: u64) -> Reading {
        Reading {
            elapsed: Duration::from_millis(ms),
            wakeups: Wakeups {
                package_idle,
                interrupt,
            },
        }
    }

    #[test]
    fn counts_the_wakeups_of_each_interval() {
        let readings = [
            reading(0, 100, 500),
            reading(1000, 100, 502),
            reading(2000, 103, 510),
        ];
        assert_eq!(
            intervals(&readings),
            [
                Interval {
                    elapsed_ms: 1000,
                    package_idle_wakeups: 0,
                    interrupt_wakeups: 2,
                },
                Interval {
                    elapsed_ms: 2000,
                    package_idle_wakeups: 3,
                    interrupt_wakeups: 8,
                },
            ]
        );
    }

    #[test]
    fn rates_cover_the_whole_duration() {
        let readings = [
            reading(0, 100, 500),
            reading(1000, 100, 502),
            reading(2000, 103, 510),
        ];
        let summary = summarize(&readings, &intervals(&readings)).unwrap();
        assert!((summary.package_idle_wakeups_per_s - 1.5).abs() < 1e-9);
        assert!((summary.interrupt_wakeups_per_s - 5.0).abs() < 1e-9);
        assert_eq!(summary.interrupt_wakeups.max, 8);
    }

    #[test]
    fn needs_a_complete_interval() {
        let readings = [reading(0, 100, 500)];
        assert!(summarize(&readings, &intervals(&readings)).is_none());
    }

    #[test]
    fn reads_at_every_interval() {
        let mut counter = 0;
        let fake = |_| {
            counter += 1;
            Ok(Wakeups {
                package_idle: counter,
                interrupt: 2 * counter,
            })
        };
        let options = Options {
            pid: 1,
            interval: Duration::from_millis(20),
            duration: Duration::from_millis(40),
        };
        let readings = read(&options, fake).unwrap();
        assert_eq!(readings.len(), 3);
        assert_eq!(intervals(&readings)[1].interrupt_wakeups, 2);
    }
}
