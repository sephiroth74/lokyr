//! Spike: a signed, notarized AppKit app that hosts WebAssembly components.
//!
//! Without arguments it opens the app window. From a terminal:
//! - `spike --imports <component.wasm>` lists what the component imports;
//! - `spike --bench <component.wasm> [--cached]` runs it and prints metrics
//!   as JSON; `--cached` reuses the compiled artifact of a previous run.

mod app;
mod bench;
mod footprint;

use std::path::Path;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.as_slice() {
        [flag, path] if flag == "--imports" => bench::print_imports(Path::new(path)),
        [flag, path] if flag == "--bench" => bench::run(Path::new(path), false),
        [flag, path, cached] if flag == "--bench" && cached == "--cached" => {
            bench::run(Path::new(path), true)
        }
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
