//! `cargo xtask check-deps`: checks the dependency rules of the workspace.
//!
//! The rules live in `tools/xtask/deps-rules.toml`, which documents their
//! format.

mod graph;
mod rules;

use std::process::ExitCode;

use anyhow::{Result, bail};

use graph::Graph;

/// The rules file, embedded at build time.
const RULES: &str = include_str!("../../deps-rules.toml");

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

    let rules = rules::parse(RULES)?;
    let mut failed = false;
    for rule in &rules {
        let unmatched = rule.unmatched_selectors(&graph);
        if !unmatched.is_empty() {
            failed = true;
            eprintln!(
                "error[{}]: `crates` selectors match no crate: {}",
                rule.name,
                unmatched.join(", "),
            );
        }
        for violation in rule.check(&graph) {
            failed = true;
            eprintln!("error[{}]: {}", violation.rule, violation.message);
            eprintln!("  = rule: {}", rule.description);
        }
    }

    if failed {
        eprintln!("check-deps: the dependency rules are broken");
        return Ok(ExitCode::FAILURE);
    }
    println!(
        "check-deps: {} rules hold for the {} workspace crates",
        rules.len(),
        graph.member_count(),
    );
    Ok(ExitCode::SUCCESS)
}
