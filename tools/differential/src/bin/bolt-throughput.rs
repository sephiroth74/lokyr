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
use bolt_differential::vtebench::{COLS, READ, ROWS, Scenario, onlcr, samples, scenarios};
use bolt_vt_core::parser::Parser;
use bolt_vt_core::terminal::{Options, Terminal};

/// How many samples a run of a scenario times.
const SAMPLES: usize = 8;

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
            let samples = samples(&scenario.payload);
            let bytes = SAMPLES * samples.len();
            let (bolt, alacritty) = medians(
                runs,
                || time_scenario(&mut BoltCore::new(), &scenario, &samples),
                || time_scenario(&mut AlacrittyCore::new(), &scenario, &samples),
            );
            row(&mut table, scenario.name, bytes, bolt, alacritty);
        }
    }
    for file in files {
        let bytes = std::fs::read(&file).with_context(|| format!("reading {}", file.display()))?;
        let (bolt, alacritty) = medians(
            runs,
            || time_bytes(&mut BoltCore::new(), &bytes),
            || time_bytes(&mut AlacrittyCore::new(), &bytes),
        );
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

/// Runs a sample of `scenario` on `core` as a warm-up, as vtebench does,
/// then [`SAMPLES`] samples, each a reset and the setup, untimed, then
/// `samples`, timed, and returns the time of those.
fn time_scenario(core: &mut impl Core, scenario: &Scenario, samples: &[u8]) -> (usize, Duration) {
    let mut total = Duration::ZERO;
    for sample in 0..=SAMPLES {
        core.feed(b"\x1bc");
        for read in scenario.setup.chunks(READ) {
            core.feed(read);
        }
        let time = time_bytes(core, samples).1;
        if sample > 0 {
            total += time;
        }
    }
    (SAMPLES * samples.len(), total)
}

/// The median throughput of `runs` runs of each core, in MiB/s, their runs
/// taken in turn, the first core's first and then the second's, so that a
/// machine that grows faster or slower favours neither.
fn medians(
    runs: u32,
    mut first: impl FnMut() -> (usize, Duration),
    mut second: impl FnMut() -> (usize, Duration),
) -> (f64, f64) {
    let rate =
        |(bytes, time): (usize, Duration)| bytes as f64 / time.as_secs_f64() / (1024.0 * 1024.0);
    let (mut firsts, mut seconds) = (Vec::new(), Vec::new());
    for run in 0..runs {
        if run % 2 == 0 {
            firsts.push(rate(first()));
            seconds.push(rate(second()));
        } else {
            seconds.push(rate(second()));
            firsts.push(rate(first()));
        }
    }
    (median(firsts), median(seconds))
}

/// The median of `rates`.
fn median(mut rates: Vec<f64>) -> f64 {
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
