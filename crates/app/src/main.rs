//! The Lokyr terminal emulator.
//!
//! Wires the kernel to the platform backends of the target OS and to the
//! built-in plugins.

#![forbid(unsafe_code)]

#[cfg(target_os = "macos")]
mod mac;

use std::path::PathBuf;
use std::process::ExitCode;
#[cfg(target_os = "macos")]
use std::sync::atomic::Ordering;

use lokyr_kernel::config::{self, Config};
use lokyr_text_core::catalog::{self, Filter};

/// Lokyr's version: the crate's, with the date of the commit that it was
/// built from, as `0.2.0-20261101`.
const VERSION: &str = env!("LOKYR_VERSION");

/// How the app is started, which a wrong command line tells.
const USAGE: &str = "\
Usage: lokyr [--version | --api-version]
       lokyr --bench startup
       lokyr --bench latency [--keys <n>] [--flood]
       lokyr --bench output [--window <width>x<height>] -- <program> [args]
       lokyr +show-config [--default] [--docs]
       lokyr +validate-config [path]
       lokyr +list-fonts [--monospace] [--styles] [--family <name>]";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    match args.as_slice() {
        [] => run(Bench::None),
        // From the process's start to the first frame on the display, as
        // JSON, and quits.
        ["--bench", "startup"] => run(Bench::Startup),
        // Each frame's time on the CPU as a program's output draws, as JSON,
        // as the program ends.
        ["--bench", "output", rest @ ..] => match output_options(rest) {
            Some((size, program)) => run(Bench::Output { size, program }),
            None => {
                eprintln!("{USAGE}");
                ExitCode::FAILURE
            }
        },
        // From each key to the frame that shows it, as JSON, as it quits.
        ["--bench", "latency", options @ ..] => match latency_options(options) {
            Some((keys, flood)) => run(Bench::Latency { keys, flood }),
            None => {
                eprintln!("{USAGE}");
                ExitCode::FAILURE
            }
        },
        ["--version" | "-V"] => {
            println!("lokyr {VERSION}");
            ExitCode::SUCCESS
        }
        // The plugin API's version, which Sleip resolves the plugins
        // against.
        ["--api-version"] => {
            println!("{}", lokyr_kernel::plugins::API_VERSION);
            ExitCode::SUCCESS
        }
        ["+show-config", flags @ ..] => show_config(flags),
        ["+validate-config"] => validate_config(None),
        ["+validate-config", path] if !path.starts_with('-') => {
            validate_config(Some(PathBuf::from(path)))
        }
        ["+list-fonts", options @ ..] => match list_fonts_options(options) {
            Some((filter, styles)) => list_fonts(&filter, styles),
            None => {
                eprintln!("{USAGE}");
                ExitCode::FAILURE
            }
        },
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
    /// points, or of the config's columns and rows without it.
    Output {
        size: Option<Points>,
        program: Vec<String>,
    },
}

/// A window's size, in points.
type Points = (f64, f64);

/// The output's bench's options, `[--window <width>x<height>] -- <program>
/// [args]`.
fn output_options(options: &[&str]) -> Option<(Option<Points>, Vec<String>)> {
    let (mut size, mut rest) = (None, options);
    if let ["--window", window, others @ ..] = rest {
        let (width, height) = window.split_once('x')?;
        size = Some((width.parse().ok()?, height.parse().ok()?));
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

/// The list of fonts' options, `--monospace`, `--styles` and `--family
/// <name>`: the families that it keeps, and whether it shows their styles,
/// as it does of a family named.
fn list_fonts_options<'a>(options: &[&'a str]) -> Option<(Filter<'a>, bool)> {
    let (mut filter, mut styles) = (Filter::default(), false);
    let mut options = options.iter();
    while let Some(option) = options.next() {
        match *option {
            "--monospace" => filter.monospaced = true,
            "--styles" => styles = true,
            "--family" => filter.family = Some(options.next()?),
            _ => return None,
        }
    }
    Some((filter, styles || filter.family.is_some()))
}

/// Prints the families of fonts that the system has, by the names that the
/// config's `family` takes, those that `filter` keeps, the config's marked;
/// with `styles`, each with its styles, by the names that `style` takes.
fn list_fonts(filter: &Filter<'_>, styles: bool) -> ExitCode {
    // A config that does not read leaves its family unmarked.
    let configured =
        config::path()
            .filter(|path| path.exists())
            .and_then(|path| match Config::read(&path) {
                Ok(config) => config.font.family.map(|name| name.as_str().to_owned()),
                Err(error) => {
                    eprintln!("lokyr: {error}; the list does not mark the config's family");
                    None
                }
            });
    let Some(families) = fonts(configured.as_deref(), filter) else {
        eprintln!("lokyr: this platform has no font backend yet");
        return ExitCode::FAILURE;
    };
    if let Some(family) = filter.family
        && families.is_empty()
    {
        let kind = if filter.monospaced {
            "monospaced family"
        } else {
            "family"
        };
        eprintln!(
            "lokyr: no {kind} `{family}` is installed; `lokyr +list-fonts` lists those that are"
        );
        return ExitCode::FAILURE;
    }
    match catalog::write(&families, styles, &mut std::io::stdout().lock()) {
        // A reader that stops early, as `head` does, ends the list.
        Err(error) if error.kind() != std::io::ErrorKind::BrokenPipe => {
            eprintln!("lokyr: the list does not write, {error}");
            ExitCode::FAILURE
        }
        _ => ExitCode::SUCCESS,
    }
}

/// The families that CoreText finds, which `filter` keeps.
#[cfg(target_os = "macos")]
fn fonts(configured: Option<&str>, filter: &Filter<'_>) -> Option<Vec<catalog::Family>> {
    let mut backend = lokyr_font_coretext::CoreText::new();
    Some(catalog::list(&mut backend, configured, filter))
}

/// No families, without a font backend.
#[cfg(not(target_os = "macos"))]
fn fonts(_: Option<&str>, _: &Filter<'_>) -> Option<Vec<catalog::Family>> {
    None
}

/// Checks `config` as the app takes it.
#[cfg(target_os = "macos")]
fn check(config: &Config) -> Result<(), String> {
    lokyr_app_core::check::<mac::Mac>(config)
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

    if let Some(executable) = lokyr_app_core::bundle_executable() {
        // Returns only if the binary does not start.
        let error = std::process::Command::new(&executable)
            .args(std::env::args_os().skip(1))
            .exec();
        eprintln!(
            "lokyr: {} does not start, {error}; the app runs outside its bundle",
            executable.display()
        );
    }
    use lokyr_app_core::{App, Drawing};
    use lokyr_kernel::commands::Code;
    use lokyr_kernel::diagnostics::{About, Diagnostic, Diagnostics};

    // The app's log, where a panic writes its backtrace, and the standard
    // error when no terminal reads it, as from the Finder; the benches keep
    // theirs.
    if matches!(bench, Bench::None)
        && let Some(log) = lokyr_app_core::log::file()
        && let Err(error) = lokyr_app_core::log::start(&log, VERSION, true)
    {
        eprintln!(
            "lokyr: the log at {} does not start, {error}",
            log.display()
        );
    }
    let (mut config, path, error) = config();
    // The plugins that Sleip installed: of the lockfile beside the config
    // file, of their packages in the user's store; none in a bench, which
    // measures the app of its own plugins.
    let installed = path
        .as_ref()
        .and_then(|path| path.parent())
        .zip(std::env::home_dir())
        .filter(|_| matches!(bench, Bench::None))
        .map(|(folder, home)| {
            let data = std::env::var_os("XDG_DATA_HOME").map(PathBuf::from);
            (
                folder.join(lokyr_pkg_model::lock::FILE),
                lokyr_pkg_model::store::Store::of_user(&home, data.as_deref()),
            )
        });
    // The GPU and the font take tens of milliseconds the first time in a
    // process, the next nearly nothing: a thread makes them, and lets them
    // go, as AppKit launches, and the app makes its own once it has.
    let warming = {
        let font = config.font.clone();
        std::thread::Builder::new()
            .name("drawing-warmup".into())
            .spawn(move || drop(Drawing::<mac::Mac>::new(font)))
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
        let drawing = match Drawing::<mac::Mac>::new(config.font.clone()) {
            Ok(drawing) => Some(drawing),
            // A config whose font fails is as invalid as one that does not
            // read.
            Err(error) if config.font != Config::default().font => {
                diagnostics.report(invalid(&error));
                config = Config::default();
                Drawing::new(config.font.clone())
                    .inspect_err(|error| eprintln!("lokyr: {error}"))
                    .ok()
            }
            Err(error) => {
                eprintln!("lokyr: {error}");
                None
            }
        };
        let mut app = App::new(drawing, config, path)
            .with_version(VERSION)
            .with_state(lokyr_kernel::state::directory())
            .with_diagnostics(diagnostics);
        if let Some((lock, store)) = installed {
            app = app.with_installed(lock, store);
        }
        match bench {
            Bench::None => app,
            Bench::Startup => app.with_startup_bench(),
            Bench::Latency { keys, flood } => app.with_latency_bench(keys, flood),
            Bench::Output { size, program } => {
                let (command, args) = program.split_first().expect("a program");
                app.with_output_bench(
                    lokyr_pty::Program::command(command, args),
                    size.map(|(width, height)| lokyr_platform_core::Size { width, height }),
                )
            }
        }
    };
    match lokyr_platform_macos::run_with(make) {
        Ok(()) if lokyr_app_core::BENCH_FAILED.load(Ordering::Relaxed) => ExitCode::FAILURE,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_output_s_bench_takes_a_window_of_points_or_the_config_s() {
        assert_eq!(
            output_options(&["--window", "800x600", "--", "cat", "file"]),
            Some((
                Some((800.0, 600.0)),
                vec!["cat".to_owned(), "file".to_owned()]
            ))
        );
        assert_eq!(
            output_options(&["--", "yes"]),
            Some((None, vec!["yes".to_owned()]))
        );
        for options in [
            &[][..],
            &["--"],
            &["--window", "800", "--", "yes"],
            &["yes"],
        ] {
            assert_eq!(output_options(options), None, "{options:?}");
        }
    }

    #[test]
    fn the_list_of_fonts_takes_the_monospaced_ones_and_a_family() {
        let filter = |options: &[&str]| {
            list_fonts_options(options).map(|(filter, styles)| {
                (filter.monospaced, filter.family.map(str::to_owned), styles)
            })
        };
        // The families' names alone, but with `--styles` or of one family.
        assert_eq!(filter(&[]), Some((false, None, false)));
        assert_eq!(filter(&["--monospace"]), Some((true, None, false)));
        assert_eq!(
            filter(&["--styles", "--monospace"]),
            Some((true, None, true))
        );
        assert_eq!(
            filter(&["--family", "SF Mono", "--monospace"]),
            Some((true, Some("SF Mono".to_owned()), true))
        );
        for options in [&["--family"][..], &["--mono"], &["Menlo"]] {
            assert_eq!(filter(options), None, "{options:?}");
        }
    }
}
