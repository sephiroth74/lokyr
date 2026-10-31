//! `lokyr-bench energy`: how much energy processes spend over a while, as
//! the kernel bills it to each, `ri_energy_nj` of `proc_pid_rusage`, which
//! needs no administrator, where `powermetrics` does.
//!
//! Reads each process's counter at the start and at the end of the
//! duration, then prints JSON: each process's energy, in joules, and its
//! mean power, in milliwatts, and their sum, as for an app of several
//! processes. The window server's composition of the processes' windows is
//! billed to the window server, not to them.

use std::process::ExitCode;
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use serde::Serialize;

use crate::process;

/// How long the probe measures by default.
const DEFAULT_DURATION: Duration = Duration::from_secs(10);

#[derive(Debug, PartialEq)]
struct Options {
    pids: Vec<libc::pid_t>,
    duration: Duration,
}

/// One process's energy over the duration.
#[derive(Debug, Serialize)]
struct Spent {
    pid: libc::pid_t,
    name: String,
    joules: f64,
    milliwatts: f64,
}

#[derive(Debug, Serialize)]
struct Report {
    duration_s: f64,
    processes: Vec<Spent>,
    joules: f64,
    milliwatts: f64,
}

/// Runs the probe with the arguments that follow `energy`.
pub fn run(args: impl Iterator<Item = String>) -> Result<ExitCode> {
    let options = parse(args)?;
    let read = |pid| {
        process::energy(pid).with_context(|| format!("cannot read the energy of process {pid}"))
    };
    let first: Vec<u64> = options
        .pids
        .iter()
        .map(|&pid| read(pid))
        .collect::<Result<_>>()?;
    let start = Instant::now();
    thread::sleep(options.duration);
    let last: Vec<u64> = options
        .pids
        .iter()
        .map(|&pid| read(pid))
        .collect::<Result<_>>()?;
    let elapsed = start.elapsed().as_secs_f64();
    let processes: Vec<Spent> = options
        .pids
        .iter()
        .zip(first.iter().zip(&last))
        .map(|(&pid, (&first, &last))| {
            let joules = last.saturating_sub(first) as f64 / 1e9;
            Spent {
                pid,
                name: process::name(pid).unwrap_or_default(),
                joules,
                milliwatts: joules / elapsed * 1e3,
            }
        })
        .collect();
    let joules = processes.iter().map(|spent| spent.joules).sum();
    let report = Report {
        duration_s: elapsed,
        milliwatts: joules / elapsed * 1e3,
        joules,
        processes,
    };
    serde_json::to_writer_pretty(std::io::stdout().lock(), &report)?;
    println!();
    Ok(ExitCode::SUCCESS)
}

/// Parses `--pid <pid> [--pid <pid>]... [--duration <s>]`.
fn parse(mut args: impl Iterator<Item = String>) -> Result<Options> {
    let mut pids = Vec::new();
    let mut duration = DEFAULT_DURATION;
    while let Some(arg) = args.next() {
        let value = args
            .next()
            .with_context(|| format!("`{arg}` needs a value"))?;
        match arg.as_str() {
            "--pid" => pids.push(
                value
                    .parse()
                    .with_context(|| format!("`{value}` is not a process id"))?,
            ),
            "--duration" => {
                let seconds: f64 = value
                    .parse()
                    .with_context(|| format!("`{value}` is not a number of seconds"))?;
                // NaN is not positive either.
                if seconds.partial_cmp(&0.0) != Some(std::cmp::Ordering::Greater) {
                    bail!("the duration must be positive");
                }
                duration = Duration::from_secs_f64(seconds);
            }
            other => bail!("unknown argument `{other}`"),
        }
    }
    if pids.is_empty() {
        bail!("`--pid <pid>` is required");
    }
    Ok(Options { pids, duration })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn options(args: &[&str]) -> Result<Options> {
        parse(args.iter().map(ToString::to_string))
    }

    #[test]
    fn takes_several_processes_and_a_duration() {
        assert_eq!(
            options(&["--pid", "12", "--pid", "34", "--duration", "2.5"]).unwrap(),
            Options {
                pids: vec![12, 34],
                duration: Duration::from_millis(2500),
            }
        );
        assert_eq!(options(&["--pid", "1"]).unwrap().duration, DEFAULT_DURATION);
    }

    #[test]
    fn rejects_bad_arguments() {
        for args in [
            &[][..],
            &["--pid"],
            &["--pid", "x"],
            &["--pid", "1", "--duration", "0"],
            &["--pid", "1", "--watts", "2"],
        ] {
            assert!(options(args).is_err(), "{args:?}");
        }
    }
}
