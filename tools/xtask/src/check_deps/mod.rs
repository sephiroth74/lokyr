//! `cargo xtask check-deps`: checks the dependency graph of the workspace.

mod graph;

use std::process::ExitCode;

use anyhow::{Result, bail};

use graph::Graph;

/// Runs the task with the remaining command-line arguments.
pub fn run(args: impl Iterator<Item = String>) -> Result<ExitCode> {
    let mut print_graph = false;
    for arg in args {
        match arg.as_str() {
            "--print-graph" => print_graph = true,
            other => bail!("unknown argument `{other}`"),
        }
    }

    let graph = Graph::load_workspace()?;
    if print_graph {
        print!("{}", graph.render_members());
    }
    Ok(ExitCode::SUCCESS)
}
