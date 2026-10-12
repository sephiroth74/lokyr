//! `saetta-bench memory`: samples the memory footprint of a process, with
//! and without its GPU memory.
//!
//! The footprint comes from the kernel, and the GPU memory from Apple's
//! `footprint` tool, which each sample runs once. Prints JSON on standard
//! output: every sample, then a summary. Sizes are in bytes.

use std::collections::BTreeMap;
use std::process::ExitCode;
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use serde::Serialize;

use crate::footprint::Breakdown;
use crate::rusage;

/// What the probe samples, and for how long.
#[derive(Debug, PartialEq)]
struct Options {
    pid: libc::pid_t,
    interval: Duration,
    duration: Duration,
}

/// One reading of the target's memory.
#[derive(Debug, Serialize)]
struct Sample {
    /// Time since the first sample.
    elapsed_ms: u128,
    footprint: u64,
    /// The GPU memory counted in the footprint.
    gpu: u64,
    footprint_without_gpu: u64,
    lifetime_peak: u64,
}

/// The samples, and the breakdown of the last one.
struct Readings {
    samples: Vec<Sample>,
    last: Breakdown,
}

/// Minimum, median, maximum and last value of a series.
#[derive(Debug, PartialEq, Serialize)]
struct Stats {
    min: u64,
    median: u64,
    max: u64,
    last: u64,
}

#[derive(Debug, Serialize)]
struct Summary {
    footprint: Stats,
    footprint_without_gpu: Stats,
    gpu: Stats,
    /// The GPU memory of the last sample, by `footprint` category.
    gpu_categories: BTreeMap<String, u64>,
    /// The highest footprint over the life of the process, as of the last
    /// sample.
    lifetime_peak: u64,
}

#[derive(Debug, Serialize)]
struct Report {
    pid: libc::pid_t,
    name: String,
    interval_ms: u128,
    duration_s: f64,
    samples: Vec<Sample>,
    summary: Summary,
}

/// Runs the probe with the arguments that follow `memory`.
pub fn run(args: impl Iterator<Item = String>) -> Result<ExitCode> {
    let options = parse(args)?;
    let Readings { samples, last } = sample(&options, Breakdown::of)?;
    let stats = |value: fn(&Sample) -> u64| Stats::of(samples.iter().map(value));
    let summary = Summary {
        footprint: stats(|sample| sample.footprint).context("no samples")?,
        footprint_without_gpu: stats(|sample| sample.footprint_without_gpu)
            .context("no samples")?,
        gpu: stats(|sample| sample.gpu).context("no samples")?,
        gpu_categories: last.gpu(),
        lifetime_peak: samples.last().map_or(0, |sample| sample.lifetime_peak),
    };
    let report = Report {
        pid: options.pid,
        name: last.name,
        interval_ms: options.interval.as_millis(),
        duration_s: options.duration.as_secs_f64(),
        samples,
        summary,
    };
    serde_json::to_writer_pretty(std::io::stdout().lock(), &report)?;
    println!();
    Ok(ExitCode::SUCCESS)
}

/// Parses `--pid <pid> [--interval <ms>] [--duration <s>]`. By default the
/// probe takes one sample; with a duration, it takes one every interval
/// (1000 ms by default), from the start to the end of the duration.
fn parse(mut args: impl Iterator<Item = String>) -> Result<Options> {
    let mut pid = None;
    let mut interval = Duration::from_millis(1000);
    let mut duration = Duration::ZERO;
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
    Ok(Options {
        pid: pid.context("`--pid <pid>` is required")?,
        interval,
        duration,
    })
}

/// Samples the target at the start, then at every interval that fits in the
/// duration. Each sample is scheduled from the start, so slow readings do not
/// make the next ones drift. `breakdown` splits the footprint into
/// categories, to find the GPU memory.
fn sample(
    options: &Options,
    mut breakdown: impl FnMut(libc::pid_t) -> Result<Breakdown>,
) -> Result<Readings> {
    let start = Instant::now();
    let mut samples = Vec::new();
    loop {
        let elapsed = start.elapsed();
        let memory = rusage::memory(options.pid)
            .with_context(|| format!("cannot read the memory of process {}", options.pid))?;
        let last = breakdown(options.pid)?;
        let gpu = last.gpu().values().sum();
        samples.push(Sample {
            elapsed_ms: elapsed.as_millis(),
            footprint: memory.footprint,
            gpu,
            footprint_without_gpu: memory.footprint.saturating_sub(gpu),
            lifetime_peak: memory.lifetime_peak,
        });
        let next = options.interval * u32::try_from(samples.len()).context("too many samples")?;
        if next > options.duration {
            return Ok(Readings { samples, last });
        }
        if let Some(wait) = next.checked_sub(start.elapsed()) {
            thread::sleep(wait);
        }
    }
}

impl Stats {
    /// The statistics of `values`, or `None` if there are none. With an even
    /// number of values, the median is the mean of the two middle ones.
    fn of(values: impl Iterator<Item = u64>) -> Option<Self> {
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
        parse(args.iter().map(ToString::to_string))
    }

    #[test]
    fn one_sample_by_default() {
        let parsed = options(&["--pid", "42"]).unwrap();
        assert_eq!(
            parsed,
            Options {
                pid: 42,
                interval: Duration::from_millis(1000),
                duration: Duration::ZERO,
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

    #[test]
    fn samples_cover_the_duration() {
        let pid = libc::pid_t::try_from(std::process::id()).unwrap();
        let fake = |_| {
            Ok(Breakdown {
                name: "fake".to_owned(),
                categories: BTreeMap::from([
                    ("IOSurface".to_owned(), 4096),
                    ("Malloc Small".to_owned(), 1 << 20),
                ]),
            })
        };
        let options = Options {
            pid,
            interval: Duration::from_millis(20),
            duration: Duration::from_millis(60),
        };
        let Readings { samples, last } = sample(&options, fake).unwrap();
        // At 0, 20, 40 and 60 ms.
        assert_eq!(samples.len(), 4);
        assert!(
            samples
                .windows(2)
                .all(|pair| pair[0].elapsed_ms <= pair[1].elapsed_ms)
        );
        for sample in &samples {
            assert_eq!(sample.gpu, 4096);
            assert_eq!(sample.footprint_without_gpu, sample.footprint - 4096);
        }
        assert_eq!(last.name, "fake");
    }
}
