//! Measurement tools for the Saetta performance budgets.
//!
//! Probes for memory footprint, idle wakeups and startup time, usable on
//! Saetta and on other terminals for comparison.

#![forbid(unsafe_code)]

use std::process::ExitCode;

fn main() -> ExitCode {
    match std::env::args().nth(1).as_deref() {
        None | Some("help" | "-h" | "--help") => {
            print_help();
            ExitCode::SUCCESS
        }
        Some(probe) => {
            eprintln!("error: unknown probe `{probe}`\n");
            print_help();
            ExitCode::FAILURE
        }
    }
}

fn print_help() {
    println!("Usage: saetta-bench <probe>\n\nNo probes are available yet.");
}
