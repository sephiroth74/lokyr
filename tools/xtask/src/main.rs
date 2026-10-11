//! Build automation for the Saetta workspace, run as `cargo xtask <task>`.

use std::process::ExitCode;

fn main() -> ExitCode {
    match std::env::args().nth(1).as_deref() {
        None | Some("help" | "-h" | "--help") => {
            print_help();
            ExitCode::SUCCESS
        }
        Some(task) => {
            eprintln!("error: unknown task `{task}`\n");
            print_help();
            ExitCode::FAILURE
        }
    }
}

fn print_help() {
    println!("Usage: cargo xtask <task>\n\nNo tasks are available yet.");
}
