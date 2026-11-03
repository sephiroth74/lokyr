//! A window of `ui-core`'s nodes and widgets, each in its states, live, as
//! the app draws its chrome: of the platform's backend, the Metal renderer
//! and the interface's font, in the light and the dark appearance, to check
//! them by hand. `ui-gallery --images <directory> [--scale <n>]` draws it
//! into PNGs instead, light and dark, at a scale of 2 or of `n`.

#![forbid(unsafe_code)]

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
mod gallery;
#[cfg(target_os = "macos")]
mod mac;

use std::process::ExitCode;

const USAGE: &str = "Usage: ui-gallery [--images <directory> [--scale <n>]]";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let done = match args.as_slice() {
        [] => run(),
        ["--images", directory] => images(std::path::Path::new(directory), 2.0),
        ["--images", directory, "--scale", scale] => match scale.parse::<f32>() {
            Ok(scale) if (1.0..=4.0).contains(&scale) => {
                images(std::path::Path::new(directory), scale)
            }
            _ => {
                eprintln!("{USAGE}");
                return ExitCode::FAILURE;
            }
        },
        _ => {
            eprintln!("{USAGE}");
            return ExitCode::FAILURE;
        }
    };
    match done {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("ui-gallery: {error}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(target_os = "macos")]
fn run() -> Result<(), String> {
    mac::run()
}

#[cfg(target_os = "macos")]
fn images(directory: &std::path::Path, scale: f32) -> Result<(), String> {
    mac::images(directory, scale)
}

#[cfg(not(target_os = "macos"))]
fn run() -> Result<(), String> {
    Err("the gallery runs on macOS alone, for now".to_owned())
}

#[cfg(not(target_os = "macos"))]
fn images(_: &std::path::Path, _: f32) -> Result<(), String> {
    run()
}
