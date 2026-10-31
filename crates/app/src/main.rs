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
mod latency;
#[cfg(target_os = "macos")]
mod mouse;
#[cfg(target_os = "macos")]
mod notice;
#[cfg(target_os = "macos")]
mod output_bench;
#[cfg(target_os = "macos")]
mod performance;
#[cfg(target_os = "macos")]
mod quick;
#[cfg(target_os = "macos")]
mod size_label;
#[cfg(target_os = "macos")]
mod watch;

use std::path::PathBuf;
use std::process::ExitCode;

use lokyr_kernel::config::{self, Config};

/// How the app is started, which a wrong command line tells.
const USAGE: &str = "\
Usage: lokyr [--version]
       lokyr --bench startup
       lokyr --bench latency [--keys <n>] [--flood]
       lokyr --bench output [--window <width>x<height>] -- <program> [args]
       lokyr +show-config [--default] [--docs]
       lokyr +validate-config [path]";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    match args.as_slice() {
        [] => run(Bench::None),
        // From the process's start to the first frame on the display, as
        // JSON, and quits.
        ["--bench", "startup"] => run(Bench::Startup),
        // From each key to the frame that shows it, as JSON, as it quits.
        // Each frame's time on the CPU as a program's output draws, as JSON,
        // as the program ends.
        ["--bench", "output", rest @ ..] => match output_options(rest) {
            Some((size, program)) => run(Bench::Output { size, program }),
            None => {
                eprintln!("{USAGE}");
                ExitCode::FAILURE
            }
        },
        ["--bench", "latency", options @ ..] => match latency_options(options) {
            Some((keys, flood)) => run(Bench::Latency { keys, flood }),
            None => {
                eprintln!("{USAGE}");
                ExitCode::FAILURE
            }
        },
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

/// What the app measures of itself, and then quits.
enum Bench {
    /// Nothing: it runs as it does.
    None,
    /// Its startup, to its first frame.
    Startup,
    /// Its keys' latency: of `keys` that it types itself, or, with none,
    /// of the user's, until they quit; with `flood`, beside a window that
    /// floods its output.
    Latency { keys: usize, flood: bool },
    /// Its frames' time as `program`'s output draws, in a window of `size`
    /// points, 1920 by 1080 without it.
    Output {
        size: (f64, f64),
        program: Vec<String>,
    },
}

/// The output's bench's options, `[--window <width>x<height>] -- <program>
/// [args]`.
fn output_options(options: &[&str]) -> Option<((f64, f64), Vec<String>)> {
    let (mut size, mut rest) = ((1920.0, 1080.0), options);
    if let ["--window", window, others @ ..] = rest {
        let (width, height) = window.split_once('x')?;
        size = (width.parse().ok()?, height.parse().ok()?);
        rest = others;
    }
    match rest {
        ["--", program @ ..] if !program.is_empty() => {
            Some((size, program.iter().map(ToString::to_string).collect()))
        }
        _ => None,
    }
}

/// The latency's bench's options, `--keys <n>` and `--flood`.
fn latency_options(options: &[&str]) -> Option<(usize, bool)> {
    let (mut keys, mut flood) = (0, false);
    let mut options = options.iter();
    while let Some(option) = options.next() {
        match *option {
            "--keys" => keys = options.next()?.parse().ok()?,
            "--flood" => flood = true,
            _ => return None,
        }
    }
    Some((keys, flood))
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

/// Runs the application until it quits: started through a link to its
/// bundle's binary, from the binary's own path, as its bundle; with
/// `bench`, until it measured what that names.
#[cfg(target_os = "macos")]
fn run(bench: Bench) -> ExitCode {
    use std::os::unix::process::CommandExt;

    if let Some(executable) = environment::bundle_executable() {
        // Returns only if the binary does not start.
        let error = std::process::Command::new(&executable)
            .args(std::env::args_os().skip(1))
            .exec();
        eprintln!(
            "lokyr: {} does not start, {error}; the app runs outside its bundle",
            executable.display()
        );
    }
    use lokyr_kernel::commands::Code;
    use lokyr_kernel::diagnostics::{About, Diagnostic, Diagnostics};

    let (mut config, path, error) = config();
    // The GPU and the font take tens of milliseconds the first time in a
    // process, the next nearly nothing: a thread makes them, and lets them
    // go, as AppKit launches, and the app makes its own once it has.
    let warming = {
        let font = config.font.clone();
        std::thread::Builder::new()
            .name("drawing-warmup".into())
            .spawn(move || drop(app::Drawing::new(font)))
            .ok()
    };
    let make = move || {
        if let Some(warming) = warming {
            let _ = warming.join();
        }
        let mut diagnostics = Diagnostics::default();
        let invalid = |error: &str| {
            let text =
                format!("{error}; none of the config applies, and the app takes the defaults");
            Diagnostic::new(Code::InvalidConfig, text, About::Config)
        };
        if let Some(error) = error {
            diagnostics.report(invalid(&error));
        }
        let drawing = match app::Drawing::new(config.font.clone()) {
            Ok(drawing) => Some(drawing),
            // A config whose font fails is as invalid as one that does not
            // read.
            Err(error) if config.font != Config::default().font => {
                diagnostics.report(invalid(&error));
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
        let app = app::App::new(drawing, config, path)
            .with_state(lokyr_kernel::state::directory())
            .with_diagnostics(diagnostics);
        match bench {
            Bench::None => app,
            Bench::Startup => app.with_startup_bench(),
            Bench::Latency { keys, flood } => app.with_latency_bench(keys, flood),
            Bench::Output { size, program } => {
                let (command, args) = program.split_first().expect("a program");
                app.with_output_bench(
                    lokyr_pty::Program::command(command, args),
                    lokyr_platform_core::Size {
                        width: size.0,
                        height: size.1,
                    },
                )
            }
        }
    };
    match lokyr_platform_macos::run_with(make) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("lokyr: {error}");
            ExitCode::FAILURE
        }
    }
}

/// The config file's, or its defaults when there is none or it is invalid,
/// with the error; and the file's path.
#[cfg(target_os = "macos")]
fn config() -> (Config, Option<PathBuf>, Option<String>) {
    let Some(path) = config::path() else {
        return (Config::default(), None, None);
    };
    match Config::read(&path) {
        Ok(config) => (config, Some(path), None),
        Err(error) => (Config::default(), Some(path), Some(error.to_string())),
    }
}

/// Fails: the target has no platform backend yet.
#[cfg(not(target_os = "macos"))]
fn run(_: Bench) -> ExitCode {
    eprintln!("lokyr: this platform has no backend yet");
    ExitCode::FAILURE
}
