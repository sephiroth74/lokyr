//! `lokyr-replay`: feeds a byte stream, raw or an asciicast recording, to
//! the VT core, headless, and prints the screen it leaves.

use std::io::Read as _;
use std::process::ExitCode;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use lokyr_replay::recording::Recording;
use lokyr_replay::screen;
use lokyr_vt_core::parser::Parser;
use lokyr_vt_core::terminal::{Options, Terminal};

/// The size of the screen when neither the command line nor the recording
/// gives one.
const DEFAULT_SIZE: (u16, u16) = (80, 24);

/// What the command line asks for.
#[derive(Debug, Default, PartialEq, Eq)]
struct Args {
    /// The file to replay; `-` is standard input.
    path: String,
    cols: Option<u16>,
    rows: Option<u16>,
    attributes: bool,
    /// How many runs to time, if the replay is a benchmark.
    bench: Option<u32>,
}

impl Args {
    fn parse(args: impl Iterator<Item = String>) -> Result<Option<Self>> {
        let mut parsed = Self::default();
        let mut path = None;
        let mut args = args.peekable();
        while let Some(arg) = args.next() {
            let mut value =
                |name: &str| args.next().with_context(|| format!("{name} needs a value"));
            match arg.as_str() {
                "-h" | "--help" => return Ok(None),
                "--cols" => parsed.cols = Some(value("--cols")?.parse().context("--cols")?),
                "--rows" => parsed.rows = Some(value("--rows")?.parse().context("--rows")?),
                "--attributes" => parsed.attributes = true,
                "--bench" => parsed.bench = Some(5),
                "--runs" => parsed.bench = Some(value("--runs")?.parse().context("--runs")?),
                other if other.starts_with("--") => bail!("unknown option `{other}`"),
                other => {
                    if path.replace(other.to_owned()).is_some() {
                        bail!("one file at a time");
                    }
                }
            }
        }
        parsed.path = path.context("which file to replay?")?;
        if parsed.cols == Some(0) || parsed.rows == Some(0) {
            bail!("the screen needs a row and a column at least");
        }
        if parsed.bench == Some(0) {
            bail!("--runs needs one run at least");
        }
        Ok(Some(parsed))
    }
}

fn main() -> ExitCode {
    match Args::parse(std::env::args().skip(1)) {
        Ok(Some(args)) => run(&args).unwrap_or_else(|error| {
            eprintln!("error: {error:#}");
            ExitCode::FAILURE
        }),
        Ok(None) => {
            print_help();
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("error: {error:#}\n");
            print_help();
            ExitCode::FAILURE
        }
    }
}

fn print_help() {
    println!(
        "Usage: lokyr-replay [options] <file>\n\n\
         Feeds <file>, a raw byte stream or an asciicast recording (v2 or v3), to the VT core,\n\
         headless, and prints the screen it leaves. `-` reads standard input.\n\n\
         Options:\n  \
         --cols <n>, --rows <n>  the size of the screen; by default the recording's, or 80x24\n  \
         --attributes            print the attributes of the cells and the cursor too\n  \
         --bench                 time the replay instead, over 5 runs\n  \
         --runs <n>              time the replay over <n> runs"
    );
}

fn run(args: &Args) -> Result<ExitCode> {
    let bytes = if args.path == "-" {
        let mut bytes = Vec::new();
        std::io::stdin()
            .read_to_end(&mut bytes)
            .context("reading standard input")?;
        bytes
    } else {
        std::fs::read(&args.path).with_context(|| format!("reading {}", args.path))?
    };
    let recording = Recording::parse(&bytes)?;
    if recording.resizes > 0 {
        eprintln!(
            "warning: the recording resizes the terminal {} times, which the replay leaves out",
            recording.resizes
        );
    }
    let (cols, rows) = recording.size.unwrap_or(DEFAULT_SIZE);
    let size = (args.cols.unwrap_or(cols), args.rows.unwrap_or(rows));

    if let Some(runs) = args.bench {
        bench(&recording, size, runs);
        return Ok(ExitCode::SUCCESS);
    }
    let terminal = replay(&recording, size);
    if args.attributes {
        print!("{}", screen::with_attributes(&terminal));
    } else {
        print!("{}", screen::text(&terminal));
    }
    Ok(ExitCode::SUCCESS)
}

/// The terminal's options for `recording`: the defaults, with the window's
/// operations if the recording was made with them.
fn options(recording: &Recording) -> Options {
    Options {
        window_ops: recording.window_ops,
        ..Options::default()
    }
}

/// A terminal of `size` that the recording's output went through.
fn replay(recording: &Recording, (cols, rows): (u16, u16)) -> Terminal {
    let mut terminal = Terminal::new(cols, rows, options(recording));
    feed(&mut terminal, recording);
    terminal
}

fn feed(terminal: &mut Terminal, recording: &Recording) {
    let mut parser = Parser::new();
    for chunk in &recording.chunks {
        parser.advance(terminal, chunk);
    }
}

/// Times `runs` replays, each into a new terminal, and prints the median.
/// Only the parsing counts: making the terminal does not.
fn bench(recording: &Recording, (cols, rows): (u16, u16), runs: u32) {
    let bytes: usize = recording.chunks.iter().map(Vec::len).sum();
    let mut times: Vec<Duration> = (0..runs)
        .map(|_| {
            let mut terminal = Terminal::new(cols, rows, options(recording));
            let start = Instant::now();
            feed(&mut terminal, recording);
            start.elapsed()
        })
        .collect();
    times.sort_unstable();
    let median = times[times.len() / 2];
    let mib_per_second = bytes as f64 / median.as_secs_f64() / (1024.0 * 1024.0);
    println!(
        "{bytes} bytes in {:.3} ms, the median of {runs} runs: {mib_per_second:.1} MiB/s",
        median.as_secs_f64() * 1000.0
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Option<Args>> {
        Args::parse(args.iter().map(|&arg| arg.to_owned()))
    }

    #[test]
    fn options_and_a_file() {
        let args = parse(&["--cols", "100", "--rows", "30", "--attributes", "out.cast"])
            .unwrap()
            .unwrap();
        assert_eq!(
            args,
            Args {
                path: "out.cast".to_owned(),
                cols: Some(100),
                rows: Some(30),
                attributes: true,
                bench: None,
            }
        );
        assert_eq!(parse(&["--bench", "-"]).unwrap().unwrap().bench, Some(5));
        assert_eq!(
            parse(&["--runs", "9", "x"]).unwrap().unwrap().bench,
            Some(9)
        );
        assert!(parse(&["--help"]).unwrap().is_none());
    }

    #[test]
    fn bad_command_lines_are_errors() {
        for args in [
            &[][..],
            &["a", "b"],
            &["--cols"],
            &["--cols", "x", "f"],
            &["--cols", "0", "f"],
            &["--runs", "0", "f"],
            &["--wat", "f"],
        ] {
            assert!(parse(args).is_err(), "{args:?}");
        }
    }
}
