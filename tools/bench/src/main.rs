//! Measurement tools for the Saetta performance budgets.
//!
//! Probes for memory footprint, idle wakeups and startup time, usable on
//! Saetta and on other terminals for comparison.

// `deny` rather than `forbid`: reading another process's statistics goes
// through `proc_pid_rusage` and `proc_name`, which are `unsafe`, and those
// call sites are allowed explicitly.
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
        "Usage: saetta-bench <probe> [options]\n\n\
         Probes (macOS):\n  \
         memory --pid <pid> [--interval <ms>] [--duration <s>]\n      \
         sample the memory footprint of a process, with and without its GPU\n      \
         memory; prints JSON, sizes in bytes\n  \
         idle --pid <pid> [--interval <ms>] [--duration <s>]\n      \
         count how often a process wakes up, over 60 s by default; prints JSON"
    );
}
