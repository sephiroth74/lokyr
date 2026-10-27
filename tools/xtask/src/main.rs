//! Build automation for the Lokyr workspace, run as `cargo xtask <task>`.

mod bundle;
mod check_comments;
mod check_deps;
mod check_links;
mod terminfo;
mod ucd;

use std::process::ExitCode;

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let result = match args.next().as_deref() {
        None | Some("help" | "-h" | "--help") => {
            print_help();
            return ExitCode::SUCCESS;
        }
        Some("bundle") => bundle::run(args),
        Some("check-comments") => check_comments::run(args),
        Some("check-deps") => check_deps::run(args),
        Some("check-links") => check_links::run(args),
        Some("terminfo") => terminfo::run(args),
        Some("ucd") => ucd::run(args),
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
         bundle                      build target/Lokyr.app from the release binary, signed\n  \
         check-comments              find the plan, tasks, phases and ADRs, in the code's comments\n  \
         check-deps [--print-graph]  check the dependency rules of the workspace\n  \
         check-links                 find the internal links of the Markdown files that do not resolve\n  \
         terminfo [--out DIR]        compile the terminfo entry xterm-lokyr, into target/terminfo by default\n  \
         ucd [--check]               generate the Unicode tables of vt-core, or check them"
    );
}
