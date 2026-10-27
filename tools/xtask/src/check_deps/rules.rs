//! The dependency rules: their file format, and how they are checked against
//! a [`Graph`].

use anyhow::{Context, Result};
use serde::Deserialize;

use super::graph::{Crate, CrateId, Graph};

/// A dependency rule.
#[derive(Debug)]
pub struct Rule {
    /// Short identifier, shown in error messages.
    pub name: String,
    /// What the rule guarantees, in one sentence.
    pub description: String,
    crates: Vec<Selector>,
    check: Check,
}

#[derive(Debug)]
enum Check {
    /// No crate may reach a forbidden crate, directly or transitively.
    Forbid(Vec<Selector>),
    /// Every direct dependency must be allowed. When `internal_only` is set,
    /// dependencies outside the workspace are not checked.
    AllowOnly {
        allowed: Vec<Selector>,
        internal_only: bool,
    },
    /// Only the listed crates may depend directly on these crates.
    DependentsOnly(Vec<Selector>),
}

/// A broken rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Violation {
    /// Name of the broken rule.
    pub rule: String,
    /// What is wrong, including the offending dependency path.
    pub message: String,
}

impl Rule {
    /// Checks the rule against `graph` and returns every violation.
    pub fn check(&self, graph: &Graph) -> Vec<Violation> {
        let mut messages = Vec::new();
        for id in self.subjects(graph) {
            let krate = graph.krate(id);
            match &self.check {
                Check::Forbid(forbidden) => {
                    if let Some(path) = graph.find_path(id, |c| any_matches(forbidden, c)) {
                        let target = graph.krate(path[path.len() - 1]);
                        messages.push(format!(
                            "`{}` depends on `{}`: {}",
                            krate.name,
                            target.name,
                            graph.render_path(&path),
                        ));
                    }
                }
                Check::AllowOnly {
                    allowed,
                    internal_only,
                } => {
                    for &dep in graph.deps(id) {
                        let dep = graph.krate(dep);
                        let in_scope = !internal_only || dep.path.is_some();
                        if in_scope && !any_matches(allowed, dep) {
                            messages.push(format!(
                                "`{}` depends on `{}`, which is not allowed",
                                krate.name, dep.name,
                            ));
                        }
                    }
                }
                Check::DependentsOnly(dependents) => {
                    for parent in graph.ids() {
                        let parent_crate = graph.krate(parent);
                        if graph.deps(parent).contains(&id)
                            && !any_matches(dependents, parent_crate)
                        {
                            messages.push(format!(
                                "`{}` depends on `{}`, which is not allowed",
                                parent_crate.name, krate.name,
                            ));
                        }
                    }
                }
            }
        }
        messages
            .into_iter()
            .map(|message| Violation {
                rule: self.name.clone(),
                message,
            })
            .collect()
    }

    /// Selectors in `crates` that match no crate of `graph`.
    pub fn unmatched_selectors(&self, graph: &Graph) -> Vec<&str> {
        self.crates
            .iter()
            .filter(|selector| !graph.ids().any(|id| selector.matches(graph.krate(id))))
            .map(|selector| selector.pattern.as_str())
            .collect()
    }

    /// The crates the rule applies to.
    fn subjects<'a>(&'a self, graph: &'a Graph) -> impl Iterator<Item = CrateId> + 'a {
        graph
            .ids()
            .filter(|&id| any_matches(&self.crates, graph.krate(id)))
    }
}

/// Parses the rules file.
pub fn parse(text: &str) -> Result<Vec<Rule>> {
    let file: RulesFile = toml::from_str(text).context("parsing the dependency rules")?;
    Ok(file.rules.into_iter().map(RuleSpec::into_rule).collect())
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RulesFile {
    #[serde(rename = "rule")]
    rules: Vec<RuleSpec>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
enum RuleSpec {
    Forbid {
        name: String,
        description: String,
        crates: Vec<String>,
        forbidden: Vec<String>,
    },
    AllowOnly {
        name: String,
        description: String,
        crates: Vec<String>,
        allowed: Vec<String>,
        #[serde(default)]
        scope: Scope,
    },
    DependentsOnly {
        name: String,
        description: String,
        crates: Vec<String>,
        dependents: Vec<String>,
    },
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum Scope {
    #[default]
    All,
    Internal,
}

impl RuleSpec {
    fn into_rule(self) -> Rule {
        let (name, description, crates, check) = match self {
            Self::Forbid {
                name,
                description,
                crates,
                forbidden,
            } => (
                name,
                description,
                crates,
                Check::Forbid(selectors(forbidden)),
            ),
            Self::AllowOnly {
                name,
                description,
                crates,
                allowed,
                scope,
            } => (
                name,
                description,
                crates,
                Check::AllowOnly {
                    allowed: selectors(allowed),
                    internal_only: scope == Scope::Internal,
                },
            ),
            Self::DependentsOnly {
                name,
                description,
                crates,
                dependents,
            } => (
                name,
                description,
                crates,
                Check::DependentsOnly(selectors(dependents)),
            ),
        };
        Rule {
            name,
            description,
            crates: selectors(crates),
            check,
        }
    }
}

/// Selects crates by workspace directory or by package name.
///
/// A pattern that contains `/` matches the directory of a workspace member,
/// relative to the workspace root; any other pattern matches package names.
/// In both cases `*` matches any run of characters.
#[derive(Debug, Clone)]
struct Selector {
    pattern: String,
    by_path: bool,
}

impl Selector {
    fn new(pattern: String) -> Self {
        let by_path = pattern.contains('/');
        Self { pattern, by_path }
    }

    fn matches(&self, krate: &Crate) -> bool {
        if self.by_path {
            krate
                .path
                .as_deref()
                .is_some_and(|path| glob_match(&self.pattern, path))
        } else {
            glob_match(&self.pattern, &krate.name)
        }
    }
}

fn selectors(patterns: Vec<String>) -> Vec<Selector> {
    patterns.into_iter().map(Selector::new).collect()
}

fn any_matches(selectors: &[Selector], krate: &Crate) -> bool {
    selectors.iter().any(|selector| selector.matches(krate))
}

/// Matches `text` against `pattern`, where `*` stands for any run of
/// characters (possibly empty) and everything else matches literally.
fn glob_match(pattern: &str, text: &str) -> bool {
    let (pattern, text) = (pattern.as_bytes(), text.as_bytes());
    let (mut p, mut t) = (0, 0);
    // Position of the last `*` seen, and the text position it was tried at.
    let mut backtrack: Option<(usize, usize)> = None;
    while t < text.len() {
        if p < pattern.len() && pattern[p] == b'*' {
            backtrack = Some((p, t));
            p += 1;
        } else if p < pattern.len() && pattern[p] == text[t] {
            p += 1;
            t += 1;
        } else if let Some((star, start)) = backtrack {
            // Let the last `*` swallow one more character and retry.
            p = star + 1;
            t = start + 1;
            backtrack = Some((star, start + 1));
        } else {
            return false;
        }
    }
    pattern[p..].iter().all(|&c| c == b'*')
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeSet, HashMap};

    use super::*;

    /// Workspace members of the test graph, shaped like the real workspace.
    const MEMBERS: &[(&str, &str)] = &[
        ("sleip-cli", "sleip/cli"),
        ("sleip-index", "sleip/index"),
        ("sleip-resolver", "sleip/resolver"),
        ("lokyr-app", "crates/app"),
        ("lokyr-bench", "tools/bench"),
        ("lokyr-core-actions", "plugins/core-actions"),
        ("lokyr-differential", "tools/differential"),
        ("lokyr-font-coretext", "crates/font-coretext"),
        ("lokyr-kernel", "crates/kernel"),
        ("lokyr-pkg-model", "crates/pkg-model"),
        ("lokyr-platform-core", "crates/platform-core"),
        ("lokyr-platform-macos", "crates/platform-macos"),
        ("lokyr-plugin-host-luau", "crates/plugin-host-luau"),
        ("lokyr-plugin-sdk", "crates/plugin-sdk"),
        ("lokyr-pty", "crates/pty"),
        ("lokyr-render-core", "crates/render-core"),
        ("lokyr-render-metal", "crates/render-metal"),
        ("lokyr-search", "crates/search"),
        ("lokyr-session", "crates/session"),
        ("lokyr-text-core", "crates/text-core"),
        ("lokyr-ui-core", "crates/ui-core"),
        ("lokyr-vt-core", "crates/vt-core"),
        ("xtask", "tools/xtask"),
    ];

    /// External crates of the test graph.
    const EXTERNALS: &[&str] = &["alacritty_terminal", "hyper", "objc2", "reqwest", "tokio"];

    /// Dependencies of the test graph; they satisfy every rule.
    const EDGES: &[(&str, &str)] = &[
        ("sleip-cli", "sleip-index"),
        ("sleip-cli", "sleip-resolver"),
        ("sleip-cli", "lokyr-pkg-model"),
        ("sleip-index", "reqwest"),
        ("sleip-index", "lokyr-pkg-model"),
        ("sleip-resolver", "lokyr-pkg-model"),
        ("hyper", "tokio"),
        ("reqwest", "hyper"),
        ("reqwest", "tokio"),
        ("lokyr-app", "lokyr-core-actions"),
        ("lokyr-app", "lokyr-font-coretext"),
        ("lokyr-app", "lokyr-kernel"),
        ("lokyr-app", "lokyr-platform-macos"),
        ("lokyr-app", "lokyr-plugin-host-luau"),
        ("lokyr-app", "lokyr-render-metal"),
        ("lokyr-core-actions", "lokyr-plugin-sdk"),
        ("lokyr-differential", "alacritty_terminal"),
        ("lokyr-differential", "lokyr-vt-core"),
        ("lokyr-font-coretext", "lokyr-text-core"),
        ("lokyr-kernel", "lokyr-pkg-model"),
        ("lokyr-kernel", "lokyr-plugin-sdk"),
        ("lokyr-kernel", "lokyr-search"),
        ("lokyr-kernel", "lokyr-session"),
        ("lokyr-kernel", "lokyr-ui-core"),
        ("lokyr-platform-macos", "objc2"),
        ("lokyr-platform-macos", "lokyr-platform-core"),
        ("lokyr-plugin-host-luau", "lokyr-plugin-sdk"),
        ("lokyr-render-metal", "lokyr-render-core"),
        ("lokyr-search", "lokyr-vt-core"),
        ("lokyr-session", "lokyr-pty"),
        ("lokyr-session", "lokyr-vt-core"),
        ("lokyr-ui-core", "lokyr-render-core"),
        ("lokyr-ui-core", "lokyr-text-core"),
    ];

    /// The test graph, with an optional extra dependency.
    fn workspace(extra: Option<(&str, &str)>) -> Graph {
        let mut graph = Graph::default();
        let mut ids = HashMap::new();
        for &(name, path) in MEMBERS {
            ids.insert(name, graph.add_crate(name, Some(path.to_owned())));
        }
        for &name in EXTERNALS {
            ids.insert(name, graph.add_crate(name, None));
        }
        for &(from, to) in EDGES.iter().chain(extra.iter()) {
            graph.add_dep(ids[from], ids[to]);
        }
        graph
    }

    fn real_rules() -> Vec<Rule> {
        parse(crate::check_deps::RULES).expect("the rules file parses")
    }

    fn broken_rules(graph: &Graph) -> BTreeSet<String> {
        real_rules()
            .iter()
            .flat_map(|rule| rule.check(graph))
            .map(|violation| violation.rule)
            .collect()
    }

    #[test]
    fn glob_matches_literals_and_wildcards() {
        assert!(glob_match("tokio", "tokio"));
        assert!(!glob_match("tokio", "tokio-util"));
        assert!(glob_match("objc2-*", "objc2-app-kit"));
        assert!(!glob_match("objc2-*", "objc2"));
        assert!(glob_match("objc2*", "objc2"));
        assert!(glob_match("crates/*-core", "crates/vt-core"));
        assert!(!glob_match("crates/*-core", "crates/kernel"));
        assert!(glob_match("*a*b*", "xxaxxbxx"));
        assert!(!glob_match("*a*b", "xxbxxa"));
        assert!(glob_match("*", ""));
        assert!(!glob_match("", "x"));
    }

    #[test]
    fn selectors_match_by_directory_or_by_name() {
        let member = Crate {
            name: "lokyr-vt-core".to_owned(),
            path: Some("crates/vt-core".to_owned()),
        };
        let external = Crate {
            name: "objc2".to_owned(),
            path: None,
        };
        let by_path = Selector::new("crates/*".to_owned());
        let by_name = Selector::new("objc2*".to_owned());
        assert!(by_path.matches(&member));
        assert!(!by_path.matches(&external));
        assert!(by_name.matches(&external));
        assert!(!by_name.matches(&member));
    }

    #[test]
    fn the_rules_hold_for_the_workspace_shape() {
        let graph = workspace(None);
        for rule in real_rules() {
            assert_eq!(rule.unmatched_selectors(&graph), Vec::<&str>::new());
            assert_eq!(rule.check(&graph), Vec::new(), "rule {}", rule.name);
        }
    }

    #[test]
    fn each_rule_catches_its_own_violation() {
        let cases = [
            ("pure-terminal-core", ("lokyr-vt-core", "lokyr-ui-core")),
            ("sdk-stands-alone", ("lokyr-plugin-sdk", "lokyr-pkg-model")),
            (
                "plugins-use-the-sdk",
                ("lokyr-core-actions", "lokyr-session"),
            ),
            (
                "kernel-only-in-app",
                ("lokyr-plugin-host-luau", "lokyr-kernel"),
            ),
            ("app-without-network", ("lokyr-kernel", "tokio")),
            ("portable-without-macos", ("lokyr-session", "objc2")),
            (
                "alacritty-only-in-the-differential-tests",
                ("lokyr-vt-core", "alacritty_terminal"),
            ),
        ];
        let tested: BTreeSet<&str> = cases.iter().map(|(rule, _)| *rule).collect();
        let all: BTreeSet<String> = real_rules().into_iter().map(|rule| rule.name).collect();
        assert_eq!(
            tested,
            all.iter().map(String::as_str).collect(),
            "every rule needs a case"
        );

        for (rule, edge) in cases {
            let broken = broken_rules(&workspace(Some(edge)));
            assert_eq!(
                broken,
                BTreeSet::from([rule.to_owned()]),
                "adding {edge:?} should break only {rule}"
            );
        }
    }

    #[test]
    fn forbid_reports_the_dependency_path() {
        let graph = workspace(Some(("lokyr-kernel", "tokio")));
        let messages: Vec<String> = real_rules()
            .iter()
            .flat_map(|rule| rule.check(&graph))
            .map(|violation| violation.message)
            .collect();
        assert_eq!(
            messages,
            ["`lokyr-app` depends on `tokio`: lokyr-app → lokyr-kernel → tokio"]
        );
    }

    #[test]
    fn unknown_fields_are_rejected() {
        let text = r#"
            [[rule]]
            name = "typo"
            kind = "forbid"
            description = "A misspelled field must not be ignored."
            crates = ["crates/vt-core"]
            forbiden = ["tokio"]
        "#;
        assert!(parse(text).is_err());
    }

    #[test]
    fn selectors_that_match_nothing_are_reported() {
        let text = r#"
            [[rule]]
            name = "stale"
            kind = "dependents-only"
            description = "Points at a crate that was renamed."
            crates = ["crates/vt-core", "crates/old-name"]
            dependents = ["crates/session"]
        "#;
        let rules = parse(text).expect("the rule parses");
        assert_eq!(
            rules[0].unmatched_selectors(&workspace(None)),
            ["crates/old-name"]
        );
    }
}
