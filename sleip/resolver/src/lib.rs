//! Version resolution for Sleip.
//!
//! Picks one version of each plugin that satisfies every semver requirement,
//! including the host API version required by the installed app, and explains
//! conflicts in readable terms, of PubGrub.
//!
//! Each plugin's versions are its candidates, each of the constraint of the
//! plugin API that it takes and of the plugins that it needs; the user's
//! plugins are a root's needs, of their constraints. A constraint is of
//! Cargo's grammar, caret by default, as `^0.1`, `~1.2`, `=1.2.3`, `>=1, <2`
//! or `*`.

#![forbid(unsafe_code)]

use std::cmp::Reverse;
use std::collections::BTreeMap;
use std::convert::Infallible;

use pubgrub::{
    DefaultStringReporter, Dependencies, DependencyConstraints, DependencyProvider,
    PackageResolutionStatistics, PubGrubError, Ranges, Reporter,
};
use semver::{Comparator, Op, Prerelease, Version, VersionReq};

/// The name of the root, the user's plugins, as a conflict tells it.
const ROOT: &str = "your plugins";

/// The name of the app's plugin API, as a conflict tells it.
const API: &str = "Lokyr's plugin API";

/// A version of a plugin that the resolution may take.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Candidate {
    /// The plugin's name, `owner/name`.
    pub name: String,
    /// Its version.
    pub version: Version,
    /// The constraint of the plugin API that it takes, if it says one.
    pub api: Option<String>,
    /// The plugins that it needs, by their names, of their constraints.
    pub dependencies: BTreeMap<String, String>,
}

/// The versions that `range` holds, of a constraint of Cargo's grammar.
///
/// # Errors
///
/// Why `constraint` is no constraint.
pub fn ranges_of(constraint: &str) -> Result<Ranges<Version>, String> {
    let requirement = VersionReq::parse(constraint.trim())
        .map_err(|error| format!("`{constraint}` is no version's constraint: {error}"))?;
    Ok(requirement
        .comparators
        .iter()
        .fold(Ranges::full(), |held, comparator| {
            held.intersection(&comparator_ranges(comparator))
        }))
}

/// The version `major.minor.patch`, of no pre-release.
fn version(major: u64, minor: u64, patch: u64) -> Version {
    Version::new(major, minor, patch)
}

/// The least version of `major.minor.patch`, its first pre-release, below
/// which a bound leaves that version's pre-releases out, as Cargo does.
fn least(major: u64, minor: u64, patch: u64) -> Version {
    Version {
        pre: Prerelease::new("0").unwrap_or(Prerelease::EMPTY),
        ..version(major, minor, patch)
    }
}

/// The versions of `comparator`, as Cargo reads it.
fn comparator_ranges(comparator: &Comparator) -> Ranges<Version> {
    let major = comparator.major;
    let full = |minor: u64, patch: u64| Version {
        pre: comparator.pre.clone(),
        ..version(major, minor, patch)
    };
    let between = |low: Version, high: Version| Ranges::between(low, high);
    match (comparator.op, comparator.minor, comparator.patch) {
        (Op::Exact | Op::Wildcard, None, _) => {
            between(version(major, 0, 0), least(major + 1, 0, 0))
        }
        (Op::Exact | Op::Wildcard, Some(minor), None) => {
            between(version(major, minor, 0), least(major, minor + 1, 0))
        }
        (Op::Exact | Op::Wildcard, Some(minor), Some(patch)) => {
            Ranges::singleton(full(minor, patch))
        }
        (Op::Greater, None, _) => Ranges::higher_than(least(major + 1, 0, 0)),
        (Op::Greater, Some(minor), None) => Ranges::higher_than(least(major, minor + 1, 0)),
        (Op::Greater, Some(minor), Some(patch)) => Ranges::strictly_higher_than(full(minor, patch)),
        (Op::GreaterEq, None, _) => Ranges::higher_than(version(major, 0, 0)),
        (Op::GreaterEq, Some(minor), None) => Ranges::higher_than(version(major, minor, 0)),
        (Op::GreaterEq, Some(minor), Some(patch)) => Ranges::higher_than(full(minor, patch)),
        (Op::Less, None, _) => Ranges::strictly_lower_than(least(major, 0, 0)),
        (Op::Less, Some(minor), None) => Ranges::strictly_lower_than(least(major, minor, 0)),
        (Op::Less, Some(minor), Some(patch)) => Ranges::strictly_lower_than(full(minor, patch)),
        (Op::LessEq, None, _) => Ranges::strictly_lower_than(least(major + 1, 0, 0)),
        (Op::LessEq, Some(minor), None) => Ranges::strictly_lower_than(least(major, minor + 1, 0)),
        (Op::LessEq, Some(minor), Some(patch)) => Ranges::lower_than(full(minor, patch)),
        (Op::Tilde, None, _) => between(version(major, 0, 0), least(major + 1, 0, 0)),
        (Op::Tilde, Some(minor), None) => {
            between(version(major, minor, 0), least(major, minor + 1, 0))
        }
        (Op::Tilde, Some(minor), Some(patch)) => {
            between(full(minor, patch), least(major, minor + 1, 0))
        }
        (Op::Caret, None, _) => between(version(major, 0, 0), least(major + 1, 0, 0)),
        (Op::Caret, Some(minor), None) if major > 0 => {
            between(version(major, minor, 0), least(major + 1, 0, 0))
        }
        (Op::Caret, Some(minor), None) => between(version(0, minor, 0), least(0, minor + 1, 0)),
        (Op::Caret, Some(minor), Some(patch)) if major > 0 => {
            between(full(minor, patch), least(major + 1, 0, 0))
        }
        (Op::Caret, Some(minor), Some(patch)) if minor > 0 => {
            between(full(minor, patch), least(0, minor + 1, 0))
        }
        (Op::Caret, Some(_), Some(patch)) => between(full(0, patch), least(0, 0, patch + 1)),
        // An operator that a later semver adds holds nothing it does not
        // know.
        _ => Ranges::empty(),
    }
}

/// The plugins' candidates, the user's needs and the app's plugin API, as
/// PubGrub asks for them.
struct Provider {
    /// Each plugin's candidates, by its name, then by their versions.
    candidates: BTreeMap<String, BTreeMap<Version, Candidate>>,
    /// The user's plugins, of their constraints.
    wanted: Vec<(String, Ranges<Version>)>,
    /// The app's plugin API's version.
    api: Version,
    /// The versions preferred, by the plugins' names, as a lockfile holds
    /// them: each taken while it holds, another of the newest only of a
    /// conflict that needs it.
    preferred: BTreeMap<String, Version>,
}

impl DependencyProvider for Provider {
    type P = String;
    type V = Version;
    type VS = Ranges<Version>;
    type M = String;
    type Err = Infallible;
    type Priority = Reverse<usize>;

    fn prioritize(
        &self,
        package: &String,
        range: &Ranges<Version>,
        _: &PackageResolutionStatistics,
    ) -> Reverse<usize> {
        // The plugins of the fewest versions first.
        let count = self.candidates.get(package).map_or(0, |versions| {
            versions
                .keys()
                .filter(|version| range.contains(version))
                .count()
        });
        Reverse(count)
    }

    fn choose_version(
        &self,
        package: &String,
        range: &Ranges<Version>,
    ) -> Result<Option<Version>, Infallible> {
        Ok(match package.as_str() {
            ROOT => Some(version(0, 0, 0)),
            API => Some(self.api.clone()).filter(|api| range.contains(api)),
            _ => self.candidates.get(package).and_then(|versions| {
                self.preferred
                    .get(package)
                    .filter(|preferred| {
                        range.contains(preferred) && versions.contains_key(preferred)
                    })
                    .or_else(|| {
                        versions
                            .keys()
                            .rev()
                            .find(|version| range.contains(version))
                    })
                    .cloned()
            }),
        })
    }

    fn get_dependencies(
        &self,
        package: &String,
        version: &Version,
    ) -> Result<Dependencies<String, Ranges<Version>, String>, Infallible> {
        Ok(match package.as_str() {
            ROOT => Dependencies::Available(self.wanted.iter().cloned().collect()),
            API => Dependencies::Available(DependencyConstraints::default()),
            _ => {
                let Some(candidate) = self
                    .candidates
                    .get(package)
                    .and_then(|versions| versions.get(version))
                else {
                    return Ok(Dependencies::Unavailable(format!(
                        "{package} {version} is not there"
                    )));
                };
                let mut needs = Vec::new();
                if let Some(api) = &candidate.api {
                    match ranges_of(api) {
                        Ok(ranges) => needs.push((API.to_owned(), ranges)),
                        Err(error) => return Ok(Dependencies::Unavailable(error)),
                    }
                }
                for (name, constraint) in &candidate.dependencies {
                    match ranges_of(constraint) {
                        Ok(ranges) => needs.push((name.clone(), ranges)),
                        Err(error) => return Ok(Dependencies::Unavailable(error)),
                    }
                }
                Dependencies::Available(needs.into_iter().collect())
            }
        })
    }
}

/// One version of each plugin that `wanted`, the user's plugins of their
/// constraints, need, and of each plugin that those need, of `candidates`,
/// each of a version of the plugin API that takes `api`, the app's: each
/// plugin's version, by its name.
///
/// # Errors
///
/// Why none holds, as PubGrub tells the conflict, or of a constraint that
/// is none.
pub fn resolve(
    api: &Version,
    wanted: &[(String, String)],
    candidates: &[Candidate],
) -> Result<BTreeMap<String, Version>, String> {
    resolve_preferring(api, wanted, candidates, &BTreeMap::new())
}

/// One version of each plugin, as [`resolve`] picks them, of `preferred`,
/// the versions that a lockfile holds, by the plugins' names: each taken
/// while the others hold with it, so that a conflict moves the plugins of
/// the conflict alone, to the newest of theirs that holds.
///
/// # Errors
///
/// As [`resolve`]'s.
pub fn resolve_preferring(
    api: &Version,
    wanted: &[(String, String)],
    candidates: &[Candidate],
    preferred: &BTreeMap<String, Version>,
) -> Result<BTreeMap<String, Version>, String> {
    let mut by_name: BTreeMap<String, BTreeMap<Version, Candidate>> = BTreeMap::new();
    for candidate in candidates {
        by_name
            .entry(candidate.name.clone())
            .or_default()
            .insert(candidate.version.clone(), candidate.clone());
    }
    let mut needs = Vec::new();
    for (name, constraint) in wanted {
        needs.push((name.clone(), ranges_of(constraint)?));
    }
    let provider = Provider {
        candidates: by_name,
        wanted: needs,
        api: api.clone(),
        preferred: preferred.clone(),
    };
    match pubgrub::resolve(&provider, ROOT.to_owned(), version(0, 0, 0)) {
        Ok(selected) => Ok(selected
            .into_iter()
            .filter(|(name, _)| name != ROOT && name != API)
            .collect()),
        Err(PubGrubError::NoSolution(mut tree)) => {
            tree.collapse_no_versions();
            Err(told(&DefaultStringReporter::report(&tree), &provider))
        }
        Err(error) => Err(error.to_string()),
    }
}

/// `report`, PubGrub's of a conflict, as Sleip tells it: the root of no
/// version, the bounds that leave pre-releases out of their own, and a
/// line of each plugin that it names and that no source holds.
fn told(report: &str, provider: &Provider) -> String {
    let mut text = report
        .replace(&format!("{ROOT} 0.0.0"), ROOT)
        .replace("is forbidden", "cannot all be installed")
        .replace("-0,", ",")
        .replace("-0 ", " ");
    if let Some(stripped) = text.strip_suffix("-0") {
        text = stripped.to_owned();
    }
    let needed = provider.wanted.iter().map(|(name, _)| name).chain(
        provider
            .candidates
            .values()
            .flat_map(BTreeMap::values)
            .flat_map(|candidate| candidate.dependencies.keys()),
    );
    let mut missing: Vec<&String> = needed
        .filter(|name| !provider.candidates.contains_key(*name) && report.contains(name.as_str()))
        .collect();
    missing.sort();
    missing.dedup();
    for name in missing {
        text.push_str(&format!("\nNo source holds {name}."));
    }
    text
}

#[cfg(test)]
mod tests;
