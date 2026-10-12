//! Spike: a signed, notarized AppKit app that hosts WebAssembly components.
//!
//! Without arguments it opens the app window. From a terminal:
//! - `spike --imports <component.wasm>` lists what the component imports;
//! - `spike --workloads <component.wasm>` times plugin-like work on 5000
//!   lines, in the guest and natively;
//! - `spike --bench <component.wasm> [--cached] [--from-memory]` runs it and
//!   prints metrics as JSON. `--cached` reuses the compiled artifact of a
//!   previous run; `--from-memory` loads it into memory instead of mapping
//!   the file.

mod app;
mod bench;
mod footprint;

use std::path::Path;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.as_slice() {
        [flag, path] if flag == "--imports" => bench::print_imports(Path::new(path)),
        [flag, path] if flag == "--workloads" => bench::workloads(Path::new(path)),
        [flag, path, options @ ..] if flag == "--bench" => match bench_options(options) {
            Some(options) => bench::run(Path::new(path), options),
            None => {
                eprintln!("usage: spike --bench <component.wasm> [--cached] [--from-memory]");
                return ExitCode::FAILURE;
            }
        },
        // Anything else, including whatever the system passes on launch,
        // opens the app.
        _ => {
            app::run();
            return ExitCode::SUCCESS;
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error:#}");
            ExitCode::FAILURE
        }
    }
}

/// Parses the flags that follow `--bench <component.wasm>`.
fn bench_options(flags: &[String]) -> Option<bench::Options> {
    let mut options = bench::Options::default();
    for flag in flags {
        match flag.as_str() {
            "--cached" => options.cached = true,
            "--from-memory" => options.from_memory = true,
            _ => return None,
        }
    }
    Some(options)
}
