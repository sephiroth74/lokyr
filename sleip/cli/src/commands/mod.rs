//! Sleip's commands, each of the installation's places and of where it
//! tells what it does.

mod doctor;
mod gc;
mod list;
mod remove;

use std::process::ExitCode;

use crate::args::{Command, Options};
use crate::output::Out;
use crate::paths::Paths;

/// Runs `command` of `options`: its exit code, 1 of a failure, which it
/// tells, unless the command told it, of an empty error.
pub fn run(options: Options, command: &Command) -> ExitCode {
    let out = Out::new(options.json);
    let paths = match Paths::of_environment() {
        Ok(paths) => paths,
        Err(error) => {
            out.error(&error);
            return ExitCode::FAILURE;
        }
    };
    let ran = match command {
        Command::List => list::list(&paths, out),
        Command::Remove(plugin) => remove::remove(&paths, options, out, plugin),
        Command::Gc => gc::gc(&paths, out),
        Command::Doctor => doctor::doctor(&paths, out),
    };
    match ran {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) if error.is_empty() => ExitCode::FAILURE,
        Err(error) => {
            out.error(&error);
            ExitCode::FAILURE
        }
    }
}

/// The bytes of the files in `folder` and its folders, of a folder that
/// can be read.
fn size_of(folder: &std::path::Path) -> u64 {
    let mut size = 0;
    let mut left = vec![folder.to_path_buf()];
    while let Some(at) = left.pop() {
        let Ok(entries) = std::fs::read_dir(&at) else {
            continue;
        };
        for entry in entries.flatten() {
            match entry.file_type() {
                Ok(kind) if kind.is_dir() => left.push(entry.path()),
                Ok(kind) if kind.is_file() => {
                    size += entry.metadata().map_or(0, |metadata| metadata.len());
                }
                _ => {}
            }
        }
    }
    size
}

/// `bytes` for a person, as `12.3 KB`.
fn bytes_text(bytes: u64) -> String {
    #[allow(clippy::cast_precision_loss)]
    let bytes = bytes as f64;
    if bytes < 1024.0 {
        format!("{bytes} bytes")
    } else if bytes < 1024.0 * 1024.0 {
        format!("{:.1} KB", bytes / 1024.0)
    } else {
        format!("{:.1} MB", bytes / (1024.0 * 1024.0))
    }
}
