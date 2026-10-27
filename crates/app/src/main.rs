//! The Lokyr terminal emulator.
//!
//! Wires the kernel to the platform backends of the target OS and to the
//! built-in plugins.

#![forbid(unsafe_code)]

#[cfg(target_os = "macos")]
mod app;
#[cfg(target_os = "macos")]
mod keys;
#[cfg(target_os = "macos")]
mod pane;
#[cfg(target_os = "macos")]
mod watch;

use std::process::ExitCode;

fn main() -> ExitCode {
    match std::env::args().nth(1).as_deref() {
        None => run(),
        Some("--version" | "-V") => {
            println!("lokyr {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Some(_) => {
            eprintln!("Usage: lokyr [--version]");
            ExitCode::FAILURE
        }
    }
}

/// Runs the application until it quits.
#[cfg(target_os = "macos")]
fn run() -> ExitCode {
    use lokyr_kernel::config::Config;

    let (mut config, path) = config();
    let drawing = match app::Drawing::new(config.font.clone()) {
        Ok(drawing) => Some(drawing),
        // A config whose font fails is as invalid as one that does not read.
        Err(error) if config.font != Config::default().font => {
            eprintln!("lokyr: {error}");
            eprintln!("lokyr: none of the config applies, and the app takes the defaults");
            config = Config::default();
            app::Drawing::new(config.font.clone())
                .inspect_err(|error| eprintln!("lokyr: {error}"))
                .ok()
        }
        Err(error) => {
            eprintln!("lokyr: {error}");
            None
        }
    };
    match lokyr_platform_macos::run(app::App::new(drawing, config, path)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("lokyr: {error}");
            ExitCode::FAILURE
        }
    }
}

/// The config file's, or its defaults when there is none or it is invalid,
/// which the log tells, and the file's path.
#[cfg(target_os = "macos")]
fn config() -> (lokyr_kernel::config::Config, Option<std::path::PathBuf>) {
    use lokyr_kernel::config::{self, Config};

    let Some(path) = config::path() else {
        return (Config::default(), None);
    };
    let config = Config::read(&path).unwrap_or_else(|error| {
        eprintln!("lokyr: {error}");
        eprintln!("lokyr: none of the config applies, and the app takes the defaults");
        Config::default()
    });
    (config, Some(path))
}

/// Fails: the target has no platform backend yet.
#[cfg(not(target_os = "macos"))]
fn run() -> ExitCode {
    eprintln!("lokyr: this platform has no backend yet");
    ExitCode::FAILURE
}
