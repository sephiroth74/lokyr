//! What the probes share: which process to read, how often and for how long,
//! and the statistics of what they read.

use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use serde::Serialize;

/// Which process a probe reads, how often, and for how long.
#[derive(Debug, PartialEq)]
pub struct Options {
    pub pid: libc::pid_t,
    pub interval: Duration,
    pub duration: Duration,
}

impl Options {
    /// Parses `--pid <pid> [--interval <ms>] [--duration <s>]`. The interval
    /// is 1000 ms by default, and the duration is the probe's
    /// `default_duration`.
    pub fn parse(
        mut args: impl Iterator<Item = String>,
        default_duration: Duration,
    ) -> Result<Self> {
        let mut pid = None;
        let mut interval = Duration::from_millis(1000);
        let mut duration = default_duration;
        while let Some(arg) = args.next() {
            let mut value = || {
                args.next()
                    .with_context(|| format!("`{arg}` needs a value"))
            };
            match arg.as_str() {
                "--pid" => {
                    let value = value()?;
                    let parsed: libc::pid_t = value
                        .parse()
                        .with_context(|| format!("`{value}` is not a process ID"))?;
                    if parsed <= 0 {
                        bail!("`{value}` is not a process ID");
                    }
                    pid = Some(parsed);
                }
                "--interval" => {
                    let value = value()?;
                    let ms: u64 = value
                        .parse()
                        .with_context(|| format!("`{value}` is not a number of milliseconds"))?;
                    if ms == 0 {
                        bail!("the interval must be at least 1 ms");
                    }
                    interval = Duration::from_millis(ms);
                }
                "--duration" => {
                    let value = value()?;
                    duration = value
                        .parse()
                        .ok()
                        .and_then(|seconds| Duration::try_from_secs_f64(seconds).ok())
                        .with_context(|| format!("`{value}` is not a number of seconds"))?;
                }
                other => bail!("unknown argument `{other}`"),
            }
        }
        Ok(Self {
            pid: pid.context("`--pid <pid>` is required")?,
            interval,
            duration,
        })
    }
}

/// Calls `read` at the start, then at every interval that fits in the
/// duration, with the time elapsed since the start. Each call is scheduled
/// from the start, so slow readings do not make the next ones drift.
pub fn every_interval(
    options: &Options,
    mut read: impl FnMut(Duration) -> Result<()>,
) -> Result<()> {
    let start = Instant::now();
    for n in 1_u32.. {
        read(start.elapsed())?;
        let next = options.interval * n;
        if next > options.duration {
            break;
        }
        if let Some(wait) = next.checked_sub(start.elapsed()) {
            thread::sleep(wait);
        }
    }
    Ok(())
}

/// Minimum, median, maximum and last value of a series.
#[derive(Debug, PartialEq, Serialize)]
pub struct Stats {
    pub min: u64,
    pub median: u64,
    pub max: u64,
    pub last: u64,
}

impl Stats {
    /// The statistics of `values`, or `None` if there are none. With an even
    /// number of values, the median is the mean of the two middle ones.
    pub fn of(values: impl Iterator<Item = u64>) -> Option<Self> {
        let values: Vec<u64> = values.collect();
        let last = *values.last()?;
        let mut sorted = values;
        sorted.sort_unstable();
        let middle = sorted.len() / 2;
        let median = if sorted.len().is_multiple_of(2) {
            sorted[middle - 1].midpoint(sorted[middle])
        } else {
            sorted[middle]
        };
        Some(Self {
            min: sorted[0],
            median,
            max: sorted[sorted.len() - 1],
            last,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn options(args: &[&str]) -> Result<Options> {
        Options::parse(args.iter().map(ToString::to_string), Duration::from_secs(5))
    }

    #[test]
    fn defaults() {
        let parsed = options(&["--pid", "42"]).unwrap();
        assert_eq!(
            parsed,
            Options {
                pid: 42,
                interval: Duration::from_millis(1000),
                duration: Duration::from_secs(5),
            }
        );
    }

    #[test]
    fn interval_and_duration() {
        let parsed = options(&["--duration", "2.5", "--pid", "7", "--interval", "250"]).unwrap();
        assert_eq!(parsed.interval, Duration::from_millis(250));
        assert_eq!(parsed.duration, Duration::from_millis(2500));
    }

    #[test]
    fn rejects_bad_arguments() {
        for args in [
            &[][..],
            &["--pid"],
            &["--pid", "0"],
            &["--pid", "abc"],
            &["--pid", "1", "--interval", "0"],
            &["--pid", "1", "--duration", "-1"],
            &["--pid", "1", "--verbose"],
        ] {
            assert!(options(args).is_err(), "{args:?}");
        }
    }

    #[test]
    fn reads_at_every_interval() {
        let options = Options {
            pid: 1,
            interval: Duration::from_millis(20),
            duration: Duration::from_millis(60),
        };
        let mut elapsed = Vec::new();
        every_interval(&options, |at| {
            elapsed.push(at);
            Ok(())
        })
        .unwrap();
        // At 0, 20, 40 and 60 ms.
        assert_eq!(elapsed.len(), 4);
        assert!(elapsed.windows(2).all(|pair| pair[0] <= pair[1]));
        assert!(elapsed[3] >= Duration::from_millis(60));
    }

    #[test]
    fn one_reading_without_a_duration() {
        let options = Options {
            pid: 1,
            interval: Duration::from_millis(20),
            duration: Duration::ZERO,
        };
        let mut calls = 0;
        every_interval(&options, |_| {
            calls += 1;
            Ok(())
        })
        .unwrap();
        assert_eq!(calls, 1);
    }

    #[test]
    fn stats_of_an_odd_and_an_even_series() {
        let odd = Stats::of([30, 10, 20].into_iter()).unwrap();
        assert_eq!(
            odd,
            Stats {
                min: 10,
                median: 20,
                max: 30,
                last: 20,
            }
        );
        let even = Stats::of([40, 10, 20, 30].into_iter()).unwrap();
        assert_eq!(even.median, 25);
        assert_eq!(even.last, 30);
        assert_eq!(Stats::of(std::iter::empty()), None);
    }
}
