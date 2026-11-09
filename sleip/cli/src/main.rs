//! Sleip, the plugin package manager for Lokyr.
//!
//! Scaffolds, builds, tests and publishes plugins, and installs them for the
//! app: it resolves versions, downloads and verifies packages, and writes
//! `plugins.lock`. The app itself never touches the network.

#![forbid(unsafe_code)]

mod args;
mod commands;
mod output;
mod paths;
mod store_lock;

use std::process::ExitCode;

use args::Parsed;

/// The exit code of a command line that is wrong.
const USAGE: u8 = 2;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args::parse(&args) {
        Ok(Parsed::Help(text)) => {
            output::print(text.strip_suffix('\n').unwrap_or(&text));
            ExitCode::SUCCESS
        }
        Ok(Parsed::Version) => {
            output::print(&format!("sleip {}", env!("CARGO_PKG_VERSION")));
            ExitCode::SUCCESS
        }
        Ok(Parsed::Run(options, command)) => commands::run(options, &command),
        Err(text) => {
            eprintln!("sleip: {text}");
            ExitCode::from(USAGE)
        }
    }
}
