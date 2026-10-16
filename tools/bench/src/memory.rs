//! `bolt-bench memory`: samples the memory footprint of a process, with
//! and without its GPU memory.
//!
//! The footprint comes from the kernel, and the GPU memory from Apple's
//! `footprint` tool, which each sample runs once. Prints JSON on standard
//! output: every sample, then a summary. Sizes are in bytes.

use std::collections::BTreeMap;
use std::process::ExitCode;
use std::time::Duration;

use anyhow::{Context, Result};
use serde::Serialize;

use crate::footprint::Breakdown;
use crate::process;
use crate::sampling::{self, Options, Stats};
use crate::windows::{self, Size};

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
    /// The target's windows on screen after the last sample, which GPU
    /// memory depends on.
    windows: Vec<Size>,
    samples: Vec<Sample>,
    summary: Summary,
}

/// Runs the probe with the arguments that follow `memory`. Without
/// `--duration`, it takes a single sample.
pub fn run(args: impl Iterator<Item = String>) -> Result<ExitCode> {
    let options = Options::parse(args, Duration::ZERO)?;
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
        windows: windows::sizes(options.pid),
        samples,
        summary,
    };
    serde_json::to_writer_pretty(std::io::stdout().lock(), &report)?;
    println!();
    Ok(ExitCode::SUCCESS)
}

/// Samples the target at every interval. `breakdown` splits the footprint
/// into categories, to find the GPU memory.
fn sample(
    options: &Options,
    mut breakdown: impl FnMut(libc::pid_t) -> Result<Breakdown>,
) -> Result<Readings> {
    let mut samples = Vec::new();
    let mut last = None;
    sampling::every_interval(options, |elapsed| {
        let memory = process::memory(options.pid)
            .with_context(|| format!("cannot read the memory of process {}", options.pid))?;
        let categories = breakdown(options.pid)?;
        let gpu = categories.gpu().values().sum();
        samples.push(Sample {
            elapsed_ms: elapsed.as_millis(),
            footprint: memory.footprint,
            gpu,
            footprint_without_gpu: memory.footprint.saturating_sub(gpu),
            lifetime_peak: memory.lifetime_peak,
        });
        last = Some(categories);
        Ok(())
    })?;
    Ok(Readings {
        samples,
        last: last.context("no samples")?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subtracts_the_gpu_memory_from_every_sample() {
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
            duration: Duration::from_millis(40),
        };
        let Readings { samples, last } = sample(&options, fake).unwrap();
        assert_eq!(samples.len(), 3);
        for sample in &samples {
            assert_eq!(sample.gpu, 4096);
            assert_eq!(sample.footprint_without_gpu, sample.footprint - 4096);
        }
        assert_eq!(last.name, "fake");
    }
}
