//! Build automation for the Saetta workspace, run as `cargo xtask <task>`.

mod check_deps;

use std::process::ExitCode;

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let result = match args.next().as_deref() {
        None | Some("help" | "-h" | "--help") => {
            print_help();
            return ExitCode::SUCCESS;
        }
        Some("check-deps") => check_deps::run(args),
        Some(task) => {
            eprintln!("error: unknown task `{task}`\n");
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
        "Usage: cargo xtask <task>\n\n\
         Tasks:\n  \
         check-deps [--print-graph]  check the dependency rules of the workspace"
    );
}
