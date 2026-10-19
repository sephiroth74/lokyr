//! The throughput of the VT core and of `alacritty_terminal` over the same
//! inputs, each fed as a PTY hands output over, in reads of 64 KiB, to a
//! terminal of 120 by 40 with its default options:
//! - vtebench's scenarios (<https://github.com/alacritty/vtebench>), whose
//!   payloads its scripts print, rebuilt here as they print them at that
//!   size, with the data that they read from the checkout that `--vtebench`
//!   names, and the line feeds that a PTY turns into CR LF. As vtebench
//!   does, each sample resets the terminal and runs the setup, which the
//!   time leaves out, then the payload, repeated to 1 MiB at least;
//! - files, as `cat` would print them, their line ends already CR LF.
//!
//! It prints a table of the median of each core's runs, in MiB/s, and of
//! their ratio.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Duration, Instant};

use alacritty_terminal::event::VoidListener;
use alacritty_terminal::term::test::TermSize;
use alacritty_terminal::term::{Config, Term};
use alacritty_terminal::vte::ansi::Processor;
use anyhow::{Context, Result, bail};
use bolt_vt_core::parser::Parser;
use bolt_vt_core::terminal::{Options, Terminal};

/// The terminal's size.
const COLS: usize = 120;
const ROWS: usize = 40;

/// The size of a read from a PTY.
const READ: usize = 64 * 1024;

/// How many bytes a sample's payload has at least, as vtebench's default.
const MIN_BYTES: usize = 1024 * 1024;

/// How many samples a run of a scenario times.
const SAMPLES: usize = 8;

/// A scenario of vtebench: the output of its setup and of its benchmark.
struct Scenario {
    name: &'static str,
    setup: Vec<u8>,
    payload: Vec<u8>,
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error:#}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let (mut vtebench, mut runs, mut files, mut write_cat) = (None, 5, Vec::new(), None);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--vtebench" => {
                vtebench = Some(PathBuf::from(
                    args.next().context("--vtebench needs a directory")?,
                ));
            }
            "--runs" => {
                runs = args
                    .next()
                    .context("--runs needs a number")?
                    .parse()
                    .context("--runs")?;
            }
            "--write-cat" => {
                write_cat = Some(PathBuf::from(
                    args.next().context("--write-cat needs a path")?,
                ));
            }
            "-h" | "--help" => {
                println!(
                    "usage: bolt-throughput [--vtebench <checkout>] [--runs <n>] [<file>...]\n       \
                     bolt-throughput --write-cat <path>\n\n  \
                     --vtebench <checkout>  measure vtebench's scenarios, with the data of that checkout\n  \
                     --runs <n>             how many runs each median takes, 5 by default\n  \
                     --write-cat <path>     write a file of 1 GiB to measure: the repository's Rust\n                         \
                     sources, repeated, their lines ended by CR LF"
                );
                return Ok(());
            }
            _ if arg.starts_with('-') => bail!("unknown option {arg}"),
            _ => files.push(PathBuf::from(arg)),
        }
    }
    if let Some(path) = write_cat {
        return write_cat_file(&path);
    }
    if runs == 0 {
        bail!("--runs needs one run at least");
    }
    let mut table = String::from(
        "| Input | Bytes timed | VT core | alacritty_terminal | Ratio |\n|---|---|---|---|---|\n",
    );
    if let Some(dir) = vtebench {
        for scenario in scenarios(&dir)? {
            let samples = samples(&scenario);
            let bytes = SAMPLES * samples.len();
            let bolt = median(runs, || {
                time_scenario(&mut BoltCore::new(), &scenario, &samples)
            });
            let alacritty = median(runs, || {
                time_scenario(&mut AlacrittyCore::new(), &scenario, &samples)
            });
            row(&mut table, scenario.name, bytes, bolt, alacritty);
        }
    }
    for file in files {
        let bytes = std::fs::read(&file).with_context(|| format!("reading {}", file.display()))?;
        let bolt = median(runs, || time_bytes(&mut BoltCore::new(), &bytes));
        let alacritty = median(runs, || time_bytes(&mut AlacrittyCore::new(), &bytes));
        let name = file.file_name().map_or_else(
            || file.display().to_string(),
            |name| name.to_string_lossy().into_owned(),
        );
        row(&mut table, &name, bytes.len(), bolt, alacritty);
    }
    print!("{table}");
    Ok(())
}

/// A core to feed.
trait Core {
    fn feed(&mut self, bytes: &[u8]);
}

struct BoltCore {
    parser: Parser,
    terminal: Terminal,
}

impl BoltCore {
    fn new() -> Self {
        Self {
            parser: Parser::new(),
            terminal: Terminal::new(COLS as u16, ROWS as u16, Options::default()),
        }
    }
}

impl Core for BoltCore {
    fn feed(&mut self, bytes: &[u8]) {
        self.parser.advance(&mut self.terminal, bytes);
    }
}

struct AlacrittyCore {
    processor: Processor,
    term: Term<VoidListener>,
}

impl AlacrittyCore {
    fn new() -> Self {
        Self {
            processor: Processor::new(),
            term: Term::new(Config::default(), &TermSize::new(COLS, ROWS), VoidListener),
        }
    }
}

impl Core for AlacrittyCore {
    fn feed(&mut self, bytes: &[u8]) {
        self.processor.advance(&mut self.term, bytes);
    }
}

/// Feeds `bytes` to `core` in reads, and returns how long it took.
fn time_bytes(core: &mut impl Core, bytes: &[u8]) -> (usize, Duration) {
    let start = Instant::now();
    for read in bytes.chunks(READ) {
        core.feed(read);
    }
    (bytes.len(), start.elapsed())
}

/// Runs [`SAMPLES`] samples of `scenario` on `core`, each a reset and the
/// setup, untimed, then `samples`, timed, and returns the time of those.
fn time_scenario(core: &mut impl Core, scenario: &Scenario, samples: &[u8]) -> (usize, Duration) {
    let mut total = Duration::ZERO;
    for _ in 0..SAMPLES {
        core.feed(b"\x1bc");
        for read in scenario.setup.chunks(READ) {
            core.feed(read);
        }
        total += time_bytes(core, samples).1;
    }
    (SAMPLES * samples.len(), total)
}

/// The median throughput of `runs` runs, in MiB/s.
fn median(runs: u32, mut run: impl FnMut() -> (usize, Duration)) -> f64 {
    let mut rates: Vec<f64> = (0..runs)
        .map(|_| {
            let (bytes, time) = run();
            bytes as f64 / time.as_secs_f64() / (1024.0 * 1024.0)
        })
        .collect();
    rates.sort_by(f64::total_cmp);
    rates[rates.len() / 2]
}

fn row(table: &mut String, name: &str, bytes: usize, bolt: f64, alacritty: f64) {
    let mib = bytes as f64 / (1024.0 * 1024.0);
    let _ = writeln!(
        table,
        "| {name} | {mib:.1} MiB | {bolt:.1} MiB/s | {alacritty:.1} MiB/s | {:.2} |",
        bolt / alacritty
    );
}

/// The payload of a sample: the scenario's, repeated to [`MIN_BYTES`] at
/// least, as vtebench repeats it.
fn samples(scenario: &Scenario) -> Vec<u8> {
    let count = (MIN_BYTES - 1) / scenario.payload.len() + 1;
    scenario.payload.repeat(count)
}

/// `text` as a PTY hands it over: each line feed after a carriage return.
fn onlcr(text: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len() + text.len() / 16);
    for &byte in text {
        if byte == b'\n' {
            out.push(b'\r');
        }
        out.push(byte);
    }
    out
}

/// The letters that several scenarios draw with, one screen each.
const LETTERS: std::ops::RangeInclusive<u8> = b'A'..=b'Z';

/// vtebench's scenarios, as its scripts print them at 120 by 40, with the
/// data files of the checkout in `dir`.
fn scenarios(dir: &Path) -> Result<Vec<Scenario>> {
    let read = |path: &str| {
        let path = dir.join("benchmarks").join(path);
        std::fs::read(&path).with_context(|| format!("reading {}", path.display()))
    };
    let alternate = b"\x1b[?1049h".to_vec();
    // The setup of the scrolling scenarios, which fills the scrollback: a
    // shell that expands `{0..100000}`, as macOS's does.
    let filled = b"y\n".repeat(100_001);
    let region =
        |top: usize, bottom: usize| format!("\x1b[?1049h\x1b[{top};{bottom}r").into_bytes();
    let scenarios = vec![
        Scenario {
            name: "cursor_motion",
            setup: Vec::new(),
            payload: cursor_motion(),
        },
        Scenario {
            name: "dense_cells",
            setup: alternate.clone(),
            payload: dense_cells(),
        },
        Scenario {
            name: "light_cells",
            setup: alternate.clone(),
            payload: light_cells(),
        },
        Scenario {
            name: "medium_cells",
            setup: Vec::new(),
            payload: read("medium_cells/vim_session")?,
        },
        Scenario {
            name: "scrolling",
            setup: filled.clone(),
            payload: b"y\n".to_vec(),
        },
        Scenario {
            name: "scrolling_bottom_region",
            setup: region(1, ROWS - 1),
            payload: b"y\n".to_vec(),
        },
        Scenario {
            name: "scrolling_bottom_small_region",
            setup: region(1, ROWS / 2),
            payload: b"y\n".to_vec(),
        },
        Scenario {
            name: "scrolling_fullscreen",
            setup: filled,
            payload: scrolling_fullscreen(),
        },
        Scenario {
            name: "scrolling_top_region",
            setup: region(2, ROWS),
            payload: b"y\n".to_vec(),
        },
        Scenario {
            name: "scrolling_top_small_region",
            setup: region(ROWS / 2, ROWS),
            payload: b"y\n".to_vec(),
        },
        Scenario {
            name: "sync_medium_cells",
            setup: Vec::new(),
            payload: read("sync_medium_cells/vim_session")?,
        },
        Scenario {
            name: "unicode",
            setup: alternate,
            payload: read("unicode/symbols")?,
        },
    ];
    Ok(scenarios
        .into_iter()
        .map(|scenario| Scenario {
            setup: onlcr(&scenario.setup),
            payload: onlcr(&scenario.payload),
            ..scenario
        })
        .collect())
}

/// A letter at every cell, the cursor moved to each, along the edges of
/// smaller and smaller rectangles, a letter a screen.
fn cursor_motion() -> Vec<u8> {
    let mut out = String::new();
    for letter in LETTERS.map(char::from) {
        let (mut column_start, mut column_end) = (1, COLS);
        let (mut line_start, mut line_end) = (1, ROWS);
        loop {
            let (mut column, mut line) = (column_start, line_start);
            while column < column_end {
                let _ = write!(out, "\x1b[{line};{column}H{letter}");
                column += 1;
            }
            while line < line_end {
                let _ = write!(out, "\x1b[{line};{column}H{letter}");
                line += 1;
            }
            while column > column_start {
                let _ = write!(out, "\x1b[{line};{column}H{letter}");
                column -= 1;
            }
            while line > line_start {
                let _ = write!(out, "\x1b[{line};{column}H{letter}");
                line -= 1;
            }
            column_start += 1;
            line_start += 1;
            column_end -= 1;
            line_end -= 1;
            if column_start > column_end || line_start > line_end {
                break;
            }
        }
    }
    out.into_bytes()
}

/// Every cell in 256 colors, bold, italic and underlined, a letter a
/// screen.
fn dense_cells() -> Vec<u8> {
    let mut out = String::new();
    for (offset, letter) in LETTERS.map(char::from).enumerate() {
        out.push_str("\x1b[H");
        for line in 1..=ROWS {
            for column in 1..=COLS {
                let index = line + column + offset;
                let foreground = index % 156 + 100;
                let background = 255 - index % 156 + 100;
                let _ = write!(
                    out,
                    "\x1b[38;5;{foreground};48;5;{background};1;3;4m{letter}"
                );
            }
        }
    }
    out.into_bytes()
}

/// The screen full of a letter, a letter a screen.
fn light_cells() -> Vec<u8> {
    let mut out = Vec::new();
    for letter in LETTERS {
        out.extend_from_slice(b"\x1b[H");
        out.extend(std::iter::repeat_n(letter, COLS * ROWS));
    }
    out
}

/// Full rows of a letter, each a line, a letter a row.
fn scrolling_fullscreen() -> Vec<u8> {
    let mut out = Vec::new();
    for letter in LETTERS {
        out.extend(std::iter::repeat_n(letter, COLS));
        out.push(b'\n');
    }
    out
}

/// The size of the file that `--write-cat` writes.
const CAT_BYTES: usize = 1 << 30;

/// Writes [`CAT_BYTES`] of the repository's Rust sources, repeated, their
/// lines ended by CR LF, to `path`.
fn write_cat_file(path: &Path) -> Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut sources = Vec::new();
    for top in ["crates", "tools"] {
        collect_sources(&root.join(top), &mut sources)?;
    }
    sources.sort();
    let mut text = Vec::new();
    for source in &sources {
        text.extend(onlcr(
            &std::fs::read(source).with_context(|| format!("reading {}", source.display()))?,
        ));
    }
    if text.is_empty() {
        bail!("no Rust source under {}", root.display());
    }
    let mut out = text.repeat(CAT_BYTES / text.len() + 1);
    out.truncate(CAT_BYTES);
    std::fs::write(path, out).with_context(|| format!("writing {}", path.display()))
}

/// The `.rs` files under `dir`, its build outputs left out.
fn collect_sources(dir: &Path, sources: &mut Vec<PathBuf>) -> Result<()> {
    for entry in std::fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))? {
        let path = entry?.path();
        if path.is_dir() {
            if path.file_name().is_some_and(|name| name != "target") {
                collect_sources(&path, sources)?;
            }
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            sources.push(path);
        }
    }
    Ok(())
}
