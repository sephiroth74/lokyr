//! `cargo xtask check-changelog`: `CHANGELOG.md`, of the format of Keep a
//! Changelog 1.1.0 (<https://keepachangelog.com/en/1.1.0/>): an
//! `## [Unreleased]` section first, then each version's, `## [X.Y.Z] -
//! YYYY-MM-DD`, the newest first; within each, the sections `### Added`,
//! `Changed`, `Deprecated`, `Removed`, `Fixed` and `Security`, in that
//! order, none empty; and a link at the end for every version. A release
//! needs its version's section.

use std::path::Path;
use std::process::ExitCode;

use anyhow::{Context, Result, bail};

/// The kinds of changes, in their order.
const KINDS: [&str; 6] = [
    "Added",
    "Changed",
    "Deprecated",
    "Removed",
    "Fixed",
    "Security",
];

pub fn run(mut args: impl Iterator<Item = String>) -> Result<ExitCode> {
    if let Some(arg) = args.next() {
        bail!("check-changelog takes no argument, not `{arg}`");
    }
    let path = root().join("CHANGELOG.md");
    let text =
        std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    let versions = check(&text).map_err(anyhow::Error::msg)?;
    println!(
        "check-changelog: {} versions and Unreleased",
        versions.len()
    );
    Ok(ExitCode::SUCCESS)
}

/// The workspace's root.
pub fn root() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
}

/// Whether the changelog `text` has the section of the release `version`,
/// `X.Y.Z`.
pub fn has_release(text: &str, version: &str) -> bool {
    text.lines()
        .any(|line| heading(line).is_some_and(|(name, date)| date.is_some() && name == version))
}

/// The version and the date of a version's heading, `## [X.Y.Z] -
/// YYYY-MM-DD`, or `## [Unreleased]` of no date.
fn heading(line: &str) -> Option<(&str, Option<&str>)> {
    let rest = line.strip_prefix("## [")?;
    let (name, after) = rest.split_once(']')?;
    if name == "Unreleased" && after.is_empty() {
        return Some((name, None));
    }
    Some((name, Some(after.strip_prefix(" - ")?)))
}

/// A version's number, `X.Y.Z`, as numbers.
fn number(version: &str) -> Option<(u64, u64, u64)> {
    let mut parts = version.split('.').map(|part| {
        (!part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
            .then(|| part.parse().ok())
            .flatten()
    });
    let number = (parts.next()??, parts.next()??, parts.next()??);
    parts.next().is_none().then_some(number)
}

/// Whether `date` is `YYYY-MM-DD`.
fn is_date(date: &str) -> bool {
    let bytes = date.as_bytes();
    bytes.len() == 10
        && bytes.iter().enumerate().all(|(at, byte)| match at {
            4 | 7 => *byte == b'-',
            _ => byte.is_ascii_digit(),
        })
}

/// Checks the changelog `text`: its versions, the newest first, or what is
/// wrong, of its line.
pub fn check(text: &str) -> Result<Vec<String>, String> {
    let mut versions: Vec<String> = Vec::new();
    let mut unreleased = false;
    // The kind of changes of the section under way, and its entries.
    let mut kind: Option<(usize, usize, usize)> = None;
    let mut kinds_seen: Option<usize> = None;
    let close = |kind: Option<(usize, usize, usize)>| match kind {
        Some((_, line, 0)) => Err(format!("CHANGELOG.md:{line}: a section of no entry")),
        _ => Ok(()),
    };
    for (at, line) in text.lines().enumerate() {
        let line_number = at + 1;
        if line.starts_with("## ") {
            close(kind.take())?;
            kinds_seen = Some(0);
            let (name, date) = heading(line).ok_or_else(|| {
                format!(
                    "CHANGELOG.md:{line_number}: `## [X.Y.Z] - YYYY-MM-DD` or `## [Unreleased]`"
                )
            })?;
            match date {
                None => {
                    if unreleased || !versions.is_empty() {
                        return Err(format!(
                            "CHANGELOG.md:{line_number}: Unreleased comes first, once"
                        ));
                    }
                    unreleased = true;
                }
                Some(date) => {
                    let this = number(name).ok_or_else(|| {
                        format!("CHANGELOG.md:{line_number}: `{name}` is not X.Y.Z")
                    })?;
                    if !is_date(date) {
                        return Err(format!(
                            "CHANGELOG.md:{line_number}: `{date}` is not YYYY-MM-DD"
                        ));
                    }
                    if let Some(last) = versions.last()
                        && number(last).is_some_and(|last| last <= this)
                    {
                        return Err(format!(
                            "CHANGELOG.md:{line_number}: {name} after {last}: the newest first"
                        ));
                    }
                    versions.push(name.to_owned());
                }
            }
        } else if let Some(name) = line.strip_prefix("### ") {
            close(kind.take())?;
            let Some(seen) = kinds_seen else {
                return Err(format!(
                    "CHANGELOG.md:{line_number}: a kind of changes outside a version"
                ));
            };
            let index = KINDS
                .iter()
                .position(|known| *known == name)
                .ok_or_else(|| {
                    format!("CHANGELOG.md:{line_number}: `{name}` is none of {KINDS:?}")
                })?;
            if index < seen {
                return Err(format!(
                    "CHANGELOG.md:{line_number}: `{name}` out of the order {KINDS:?}, or twice"
                ));
            }
            kinds_seen = Some(index + 1);
            kind = Some((index, line_number, 0));
        } else if line.starts_with("- ") {
            match kind.as_mut() {
                Some((_, _, entries)) => *entries += 1,
                None if kinds_seen.is_some() => {
                    return Err(format!(
                        "CHANGELOG.md:{line_number}: an entry outside a kind of changes"
                    ));
                }
                None => {}
            }
        } else if line.starts_with('[') && line.contains("]: ") {
            close(kind.take())?;
            kinds_seen = None;
        }
    }
    close(kind)?;
    if !unreleased {
        return Err("CHANGELOG.md: no `## [Unreleased]` section".to_owned());
    }
    for name in std::iter::once("Unreleased").chain(versions.iter().map(String::as_str)) {
        let link = format!("[{name}]: https://");
        if !text.lines().any(|line| line.starts_with(&link)) {
            return Err(format!("CHANGELOG.md: no link of [{name}] at the end"));
        }
    }
    Ok(versions)
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOOD: &str = "# Changelog\n\n- a list in the preamble\n\n## [Unreleased]\n\n### Added\n\n- a\n\n### Fixed\n\n- b\n\n## [0.2.0] - 2026-11-02\n\n### Changed\n\n- c\n\n## [0.1.0] - 2026-10-01\n\n### Security\n\n- d\n\n[Unreleased]: https://x/compare/v0.2.0...HEAD\n[0.2.0]: https://x/compare/v0.1.0...v0.2.0\n[0.1.0]: https://x/releases/tag/v0.1.0\n";

    #[test]
    fn the_repository_s_changelog_is_of_the_format() {
        let text = std::fs::read_to_string(root().join("CHANGELOG.md")).unwrap();
        let versions = check(&text).unwrap();
        assert!(versions.iter().any(|version| version == "0.9.0"));
    }

    #[test]
    fn a_changelog_of_the_format_tells_its_versions() {
        assert_eq!(
            check(GOOD),
            Ok(vec!["0.2.0".to_owned(), "0.1.0".to_owned()])
        );
        assert!(has_release(GOOD, "0.2.0"));
        assert!(!has_release(GOOD, "0.3.0"));
        assert!(!has_release(GOOD, "Unreleased"));
    }

    #[test]
    fn a_changelog_out_of_the_format_is_told_at_its_line() {
        let wrong = |from: &str, to: &str| check(&GOOD.replacen(from, to, 1)).unwrap_err();
        assert!(wrong("### Fixed", "### Bugs").contains("`Bugs` is none of"));
        assert!(wrong("### Fixed", "### Added").contains("out of the order"));
        assert!(wrong("- c\n", "").contains("a section of no entry"));
        assert!(wrong("2026-11-02", "2 Nov 2026").contains("not YYYY-MM-DD"));
        assert!(wrong("[0.2.0] - ", "[0.0.9] - ").contains("the newest first"));
        assert!(wrong("[0.2.0] - ", "[0.2] - ").contains("is not X.Y.Z"));
        assert!(wrong("## [Unreleased]", "## Unreleased").contains("`## [X.Y.Z]"));
        assert!(wrong("[0.1.0]: https", "[0.1.0]: http").contains("no link of [0.1.0]"));
        let late = GOOD.replacen(
            "[Unreleased]:",
            "## [Unreleased]\n\n### Added\n\n- e\n\n[Unreleased]:",
            1,
        );
        assert!(check(&late).unwrap_err().contains("Unreleased comes first"));
    }
}
