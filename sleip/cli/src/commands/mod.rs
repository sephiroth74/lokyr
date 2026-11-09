//! Sleip's commands, each of the installation's places and of where it
//! tells what it does.

mod check;
mod dev;
mod doctor;
mod gc;
mod install;
mod list;
mod new;
mod registry;
mod remove;
mod test;

use std::path::Path;
use std::process::{Command as Process, ExitCode};

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
        Command::New(plugin) => new::new(out, plugin),
        Command::Check => check::check(out),
        Command::Test => test::test(out),
        Command::Dev => dev::dev(&paths, out),
        Command::Add(plugin, from) => install::add(&paths, options, out, plugin.as_deref(), from),
        Command::Update(plugin) => install::update(&paths, options, out, plugin.as_deref()),
        Command::List => list::list(&paths, out),
        Command::Remove(plugin) => remove::remove(&paths, options, out, plugin),
        Command::Gc => gc::gc(&paths, out),
        Command::Doctor => doctor::doctor(&paths, out),
        Command::Registry(tool) => registry::run(out, tool),
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

/// Runs git of `args`, in `folder` if given, of no question at the
/// terminal: its output's first line, or its error.
pub(super) fn git(folder: Option<&Path>, args: &[&str]) -> Result<String, String> {
    let mut command = Process::new("git");
    if let Some(folder) = folder {
        command.arg("-C").arg(folder);
    }
    let output = command
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .map_err(|error| format!("git cannot run: {error}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_owned());
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .next()
        .unwrap_or_default()
        .trim()
        .to_owned())
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

/// Files by their paths in a plugin's folder, each of its text or of why it
/// cannot be read.
type Files<T> = Vec<(String, T)>;

/// The sources of Luau of the plugin in `folder`, `src/**/*.luau` and
/// `tests/**/*.luau`, by their paths in it, of `/` between their folders, in
/// their order, links left out, as its package leaves them; and those that
/// cannot be read, each of why.
pub(crate) fn luau_sources(folder: &std::path::Path) -> (Files<String>, Files<std::io::Error>) {
    let (mut sources, mut unread) = (Vec::new(), Vec::new());
    let mut left = vec![folder.join("src"), folder.join("tests")];
    while let Some(at) = left.pop() {
        let Ok(entries) = std::fs::read_dir(&at) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_dir() {
                left.push(path);
            } else if kind.is_file()
                && path
                    .extension()
                    .is_some_and(|extension| extension == "luau")
            {
                let name = path
                    .strip_prefix(folder)
                    .map(|inner| {
                        inner
                            .iter()
                            .map(|part| part.to_string_lossy().into_owned())
                            .collect::<Vec<_>>()
                            .join("/")
                    })
                    .unwrap_or_default();
                match std::fs::read_to_string(&path) {
                    Ok(text) => sources.push((name, text)),
                    Err(error) => unread.push((name, error)),
                }
            }
        }
    }
    sources.sort();
    (sources, unread)
}
