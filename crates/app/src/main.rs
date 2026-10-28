//! The Lokyr terminal emulator.
//!
//! Wires the kernel to the platform backends of the target OS and to the
//! built-in plugins.

#![forbid(unsafe_code)]

#[cfg(target_os = "macos")]
mod app;
#[cfg(target_os = "macos")]
mod environment;
#[cfg(target_os = "macos")]
mod icon;
#[cfg(target_os = "macos")]
mod keys;
#[cfg(target_os = "macos")]
mod mouse;
#[cfg(target_os = "macos")]
mod watch;

use std::path::PathBuf;
use std::process::ExitCode;

use lokyr_kernel::config::{self, Config};

/// How the app is started, which a wrong command line tells.
const USAGE: &str = "\
Usage: lokyr [--version]
       lokyr +show-config [--default] [--docs]
       lokyr +validate-config [path]";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    match args.as_slice() {
        [] => run(),
        ["--version" | "-V"] => {
            println!("lokyr {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        ["+show-config", flags @ ..] => show_config(flags),
        ["+validate-config"] => validate_config(None),
        ["+validate-config", path] if !path.starts_with('-') => {
            validate_config(Some(PathBuf::from(path)))
        }
        _ => {
            eprintln!("{USAGE}");
            ExitCode::FAILURE
        }
    }
}

/// Prints every key of the config, with its value: the config file's, or
/// the defaults' with `--default`; with `--docs`, after the text of each.
fn show_config(flags: &[&str]) -> ExitCode {
    let (mut defaults, mut docs) = (false, false);
    for flag in flags {
        match *flag {
            "--default" => defaults = true,
            "--docs" => docs = true,
            _ => {
                eprintln!("{USAGE}");
                return ExitCode::FAILURE;
            }
        }
    }
    let config = if defaults {
        Config::default()
    } else {
        let Some(path) = config::path() else {
            eprintln!("lokyr: no home directory, and so no config file: the defaults apply");
            return ExitCode::FAILURE;
        };
        match Config::read(&path) {
            Ok(config) => config,
            Err(error) => {
                eprintln!("lokyr: {error}");
                return ExitCode::FAILURE;
            }
        }
    };
    print!("{}", config.written(docs));
    ExitCode::SUCCESS
}

/// Checks the config file at `path`, or the app's own without one: the
/// exit code tells whether the app takes it, and the error, with its
/// place in the file, what is wrong. A file that the app's own path does
/// not find is no error, as the defaults then apply; one that `path` names
/// must be there.
fn validate_config(path: Option<PathBuf>) -> ExitCode {
    let path = match path {
        Some(path) if !path.exists() => {
            eprintln!("lokyr: {}: there is no such file", path.display());
            return ExitCode::FAILURE;
        }
        Some(path) => path,
        None => match config::path() {
            Some(path) if !path.exists() => {
                println!("{}: no file, and so the defaults", path.display());
                return ExitCode::SUCCESS;
            }
            Some(path) => path,
            None => {
                eprintln!("lokyr: no home directory, and so no config file");
                return ExitCode::FAILURE;
            }
        },
    };
    let checked = Config::read(&path)
        .map_err(|error| error.to_string())
        .and_then(|config| check(&config).map_err(|error| format!("{}: {error}", path.display())));
    match checked {
        Ok(()) => {
            println!("{}: valid", path.display());
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("lokyr: {error}");
            ExitCode::FAILURE
        }
    }
}

/// Checks `config` as the app takes it.
#[cfg(target_os = "macos")]
fn check(config: &Config) -> Result<(), String> {
    app::check(config)
}

/// Checks nothing more than the file's reading, without a platform.
#[cfg(not(target_os = "macos"))]
fn check(_: &Config) -> Result<(), String> {
    Ok(())
}

/// Runs the application until it quits.
#[cfg(target_os = "macos")]
fn run() -> ExitCode {
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
fn config() -> (Config, Option<PathBuf>) {
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
