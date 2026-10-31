//! Measurement tools for the Lokyr performance budgets.
//!
//! Probes for memory footprint, idle wakeups and startup time, usable on
//! Lokyr and on other terminals for comparison.

// `deny` rather than `forbid`: reading other processes' statistics and
// listing windows go through C APIs, which are `unsafe`, and each of those
// call sites is allowed explicitly.
#![deny(unsafe_code)]

#[cfg(target_os = "macos")]
mod footprint;
#[cfg(target_os = "macos")]
mod idle;
#[cfg(target_os = "macos")]
mod memory;
#[cfg(target_os = "macos")]
mod process;
#[cfg(target_os = "macos")]
mod sampling;
#[cfg(target_os = "macos")]
mod startup;
#[cfg(target_os = "macos")]
mod windows;

use std::process::ExitCode;

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let result = match args.next().as_deref() {
        None | Some("help" | "-h" | "--help") => {
            print_help();
            return ExitCode::SUCCESS;
        }
        #[cfg(target_os = "macos")]
        Some("memory") => memory::run(args),
        #[cfg(target_os = "macos")]
        Some("idle") => idle::run(args),
        #[cfg(target_os = "macos")]
        Some("startup") => startup::run(args),
        Some(probe) => {
            eprintln!("error: unknown probe `{probe}`\n");
            print_help();
            return ExitCode::FAILURE;
        }
    };
    result.unwrap_or_else(|error| {
        eprintln!("error: {error:#}");
        ExitCode::FAILURE
    })
}

fn print_help() {
    println!(
        "Usage: lokyr-bench <probe> [options]\n\n\
         Probes (macOS):\n  \
         memory --pid <pid> [--interval <ms>] [--duration <s>]\n      \
         sample the memory footprint of a process, with and without its GPU\n      \
         memory; prints JSON, sizes in bytes\n  \
         idle --pid <pid> [--interval <ms>] [--duration <s>]\n      \
         count how often a process wakes up, over 60 s by default; prints JSON\n  \
         startup --app <path> [--runs <n>] [--warmup <n>] [--first-frame] [--cold]\n      \
         time an app's first window over 10 runs, after 2 warmup runs; the app\n      \
         must not be running, and opens and quits once per run; prints JSON"
    );
}
