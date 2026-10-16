//! Crossbow, the plugin package manager for Bolt.
//!
//! Scaffolds, builds, tests and publishes plugins, and installs them for the
//! app: it resolves versions, downloads and verifies packages, and writes
//! `plugins.lock`. The app itself never touches the network.

#![forbid(unsafe_code)]

use std::process::ExitCode;

fn main() -> ExitCode {
    match std::env::args().nth(1).as_deref() {
        Some("--version" | "-V") => {
            println!("crossbow {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        _ => {
            eprintln!("Usage: crossbow --version");
            ExitCode::FAILURE
        }
    }
}
