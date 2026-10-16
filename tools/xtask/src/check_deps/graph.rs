//! The dependency graph of the workspace, as seen by the dependency rules.

use std::collections::{HashMap, HashSet, VecDeque};
use std::fmt::Write as _;
use std::path::Path;

use anyhow::{Context, Result};
use cargo_metadata::{CargoOpt, DependencyKind, Metadata, MetadataCommand};

/// A crate in the dependency graph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Crate {
    /// Package name, e.g. `bolt-vt-core`.
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

    /// All crate ids, in insertion order.
    pub fn ids(&self) -> std::ops::Range<CrateId> {
        0..self.crates.len()
    }

    /// The crate with the given id.
    pub fn krate(&self, id: CrateId) -> &Crate {
        &self.crates[id]
    }

    /// The direct dependencies of a crate.
    pub fn deps(&self, id: CrateId) -> &[CrateId] {
        &self.deps[id]
    }

    /// Number of workspace members.
    pub fn member_count(&self) -> usize {
        self.crates.iter().filter(|c| c.path.is_some()).count()
    }

    /// The shortest dependency path from `from` to a crate accepted by
    /// `is_target`, both ends included. `from` itself is never a target.
    pub fn find_path(
        &self,
        from: CrateId,
        is_target: impl Fn(&Crate) -> bool,
    ) -> Option<Vec<CrateId>> {
        let mut parent: Vec<Option<CrateId>> = vec![None; self.crates.len()];
        let mut seen = vec![false; self.crates.len()];
        let mut queue = VecDeque::from([from]);
        seen[from] = true;
        while let Some(current) = queue.pop_front() {
            for &dep in &self.deps[current] {
                if seen[dep] {
                    continue;
                }
                seen[dep] = true;
                parent[dep] = Some(current);
                if is_target(&self.crates[dep]) {
                    let mut path = vec![dep];
                    let mut node = dep;
                    while let Some(prev) = parent[node] {
                        path.push(prev);
                        node = prev;
                    }
                    path.reverse();
                    return Some(path);
                }
                queue.push_back(dep);
            }
        }
        None
    }

    /// Renders a dependency path as `a → b → c`.
    pub fn render_path(&self, path: &[CrateId]) -> String {
        path.iter()
            .map(|&id| self.crates[id].name.as_str())
            .collect::<Vec<_>>()
            .join(" → ")
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A graph of external crates named after their index: `c0`, `c1`, …
    fn chain(edges: &[(CrateId, CrateId)], size: usize) -> Graph {
        let mut graph = Graph::default();
        for id in 0..size {
            graph.add_crate(&format!("c{id}"), None);
        }
        for &(from, to) in edges {
            graph.add_dep(from, to);
        }
        graph
    }

    #[test]
    fn find_path_returns_the_shortest_path() {
        // c0 → c1 → c2 → c3, and a shortcut c0 → c4 → c3.
        let graph = chain(&[(0, 1), (1, 2), (2, 3), (0, 4), (4, 3)], 5);
        let path = graph
            .find_path(0, |c| c.name == "c3")
            .expect("c3 is reachable");
        assert_eq!(graph.render_path(&path), "c0 → c4 → c3");
    }

    #[test]
    fn find_path_never_targets_the_start() {
        let graph = chain(&[(0, 1), (1, 0)], 2);
        assert_eq!(graph.find_path(0, |c| c.name == "c0"), None);
    }

    #[test]
    fn duplicate_edges_are_ignored() {
        let graph = chain(&[(0, 1), (0, 1)], 2);
        assert_eq!(graph.deps(0), [1]);
    }

    #[test]
    fn render_members_shows_only_dependencies_between_members() {
        let mut graph = Graph::default();
        let app = graph.add_crate("app", Some("crates/app".to_owned()));
        let lib = graph.add_crate("lib", Some("crates/lib".to_owned()));
        let external = graph.add_crate("serde", None);
        graph.add_dep(app, lib);
        graph.add_dep(app, external);
        assert_eq!(graph.render_members(), "app → lib\nlib → —\n");
    }
}
