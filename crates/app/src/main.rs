//! The Saetta terminal emulator.
//!
//! Wires the kernel to the platform backends of the target OS and to the
//! built-in plugins.

#![forbid(unsafe_code)]

use std::process::ExitCode;

fn main() -> ExitCode {
    match std::env::args().nth(1).as_deref() {
        Some("--version" | "-V") => {
            println!("saetta {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        _ => {
            eprintln!("Usage: saetta --version");
            ExitCode::FAILURE
        }
    }
}
