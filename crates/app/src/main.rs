//! The Bolt terminal emulator.
//!
//! Wires the kernel to the platform backends of the target OS and to the
//! built-in plugins.

#![forbid(unsafe_code)]

#[cfg(target_os = "macos")]
mod app;

use std::process::ExitCode;

fn main() -> ExitCode {
    match std::env::args().nth(1).as_deref() {
        None => run(),
        Some("--version" | "-V") => {
            println!("bolt {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Some(_) => {
            eprintln!("Usage: bolt [--version]");
            ExitCode::FAILURE
        }
    }
}

/// Runs the application until it quits.
#[cfg(target_os = "macos")]
fn run() -> ExitCode {
    let gpu = bolt_render_metal::Gpu::new()
        .inspect_err(|error| eprintln!("bolt: {error}"))
        .ok();
    match bolt_platform_macos::run(app::App::new(gpu)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("bolt: {error}");
            ExitCode::FAILURE
        }
    }
}

/// Fails: the target has no platform backend yet.
#[cfg(not(target_os = "macos"))]
fn run() -> ExitCode {
    eprintln!("bolt: this platform has no backend yet");
    ExitCode::FAILURE
}
