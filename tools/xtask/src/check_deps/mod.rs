//! `cargo xtask check-deps`: checks the dependency rules of the workspace.
//!
//! The rules live in `tools/xtask/deps-rules.toml`, which documents their
//! format.

mod graph;
mod rules;

use std::collections::BTreeMap;
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

    let graph = Graph::load_workspace(None)?;
    if print_graph {
        print!("{}", graph.render_members());
    }

    let rules = rules::parse(RULES)?;
    // The graphs of the platforms that rules name, each loaded once.
    let mut platforms = BTreeMap::new();
    for platform in rules.iter().filter_map(|rule| rule.platform.as_deref()) {
        if !platforms.contains_key(platform) {
            platforms.insert(platform, Graph::load_workspace(Some(platform))?);
        }
    }
    let mut failed = false;
    for rule in &rules {
        let graph = rule
            .platform
            .as_deref()
            .and_then(|platform| platforms.get(platform))
            .unwrap_or(&graph);
        let unmatched = rule.unmatched_selectors(graph);
        if !unmatched.is_empty() {
            failed = true;
            eprintln!(
                "error[{}]: `crates` selectors match no crate: {}",
                rule.name,
                unmatched.join(", "),
            );
        }
        for violation in rule.check(graph) {
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
