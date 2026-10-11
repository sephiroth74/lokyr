//! The dependency graph of the workspace, as seen by the dependency rules.

use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;
use std::path::Path;

use anyhow::{Context, Result};
use cargo_metadata::{CargoOpt, DependencyKind, Metadata, MetadataCommand};

/// A crate in the dependency graph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Crate {
    /// Package name, e.g. `saetta-vt-core`.
    pub name: String,
    /// Directory relative to the workspace root, e.g. `crates/vt-core`.
    /// Only workspace members have one.
    pub path: Option<String>,
}

/// Identifies a crate within a [`Graph`].
pub type CrateId = usize;

/// Crates and the dependencies between them.
///
/// Normal and build dependencies are edges; dev-dependencies are not, since
/// they only affect tests.
#[derive(Debug, Default)]
pub struct Graph {
    crates: Vec<Crate>,
    deps: Vec<Vec<CrateId>>,
}

impl Graph {
    /// Loads the graph of the workspace this tool belongs to, with every
    /// feature enabled and dependencies for every target platform.
    pub fn load_workspace() -> Result<Self> {
        let manifest = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Cargo.toml");
        let metadata = MetadataCommand::new()
            .manifest_path(manifest)
            .features(CargoOpt::AllFeatures)
            .exec()
            .context("running `cargo metadata`")?;
        Self::from_metadata(&metadata)
    }

    fn from_metadata(metadata: &Metadata) -> Result<Self> {
        let members: HashSet<_> = metadata.workspace_members.iter().collect();
        let mut graph = Self::default();
        let mut ids = HashMap::new();
        for package in &metadata.packages {
            let path = if members.contains(&package.id) {
                let dir = package
                    .manifest_path
                    .parent()
                    .context("a manifest path has no parent directory")?;
                let relative = dir
                    .strip_prefix(&metadata.workspace_root)
                    .with_context(|| format!("`{dir}` is outside the workspace"))?;
                Some(relative.as_str().to_owned())
            } else {
                None
            };
            ids.insert(&package.id, graph.add_crate(&package.name, path));
        }

        let resolve = metadata
            .resolve
            .as_ref()
            .context("`cargo metadata` returned no dependency resolution")?;
        for node in &resolve.nodes {
            let from = *ids
                .get(&node.id)
                .context("unknown package in the resolution")?;
            for dep in &node.deps {
                let dev_only = !dep.dep_kinds.is_empty()
                    && dep
                        .dep_kinds
                        .iter()
                        .all(|info| info.kind == DependencyKind::Development);
                if !dev_only {
                    let to = *ids
                        .get(&dep.pkg)
                        .context("unknown dependency in the resolution")?;
                    graph.add_dep(from, to);
                }
            }
        }
        Ok(graph)
    }

    /// Adds a crate and returns its id.
    pub fn add_crate(&mut self, name: &str, path: Option<String>) -> CrateId {
        self.crates.push(Crate {
            name: name.to_owned(),
            path,
        });
        self.deps.push(Vec::new());
        self.crates.len() - 1
    }

    /// Records that `from` depends on `to`. Duplicate edges are ignored.
    pub fn add_dep(&mut self, from: CrateId, to: CrateId) {
        if !self.deps[from].contains(&to) {
            self.deps[from].push(to);
        }
    }

    /// Renders the dependencies between workspace members, one member per
    /// line, sorted by name.
    pub fn render_members(&self) -> String {
        let mut members: Vec<CrateId> = (0..self.crates.len())
            .filter(|&id| self.crates[id].path.is_some())
            .collect();
        members.sort_by(|&a, &b| self.crates[a].name.cmp(&self.crates[b].name));

        let mut out = String::new();
        for id in members {
            let mut deps: Vec<&str> = self.deps[id]
                .iter()
                .filter(|&&dep| self.crates[dep].path.is_some())
                .map(|&dep| self.crates[dep].name.as_str())
                .collect();
            deps.sort_unstable();
            let deps = if deps.is_empty() {
                "—".to_owned()
            } else {
                deps.join(", ")
            };
            // Writing to a `String` cannot fail.
            let _ = writeln!(out, "{} → {deps}", self.crates[id].name);
        }
        out
    }
}
