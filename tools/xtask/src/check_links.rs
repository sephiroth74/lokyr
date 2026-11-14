//! `cargo xtask check-links`: finds the internal links of the Markdown files
//! that git tracks which do not resolve: to a file that does not exist, or
//! to an anchor that no heading of the file makes, as GitHub makes its
//! anchors. Links outside the repository, to the web or by mail, are left
//! out, as are those in code blocks and code spans, and the config guide's
//! template, whose links resolve in the guide that it makes, which the
//! check reads.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use anyhow::{Context, Result, bail};
use regex::Regex;

pub fn run(mut args: impl Iterator<Item = String>) -> Result<ExitCode> {
    if let Some(arg) = args.next() {
        bail!("check-links takes no argument, not `{arg}`");
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .context("the workspace's root")?;
    let listed = Command::new("git")
        .args(["ls-files", "--", "*.md"])
        .current_dir(&root)
        .output()
        .context("listing the files that git tracks")?;
    if !listed.status.success() {
        bail!("git ls-files failed");
    }
    let files = String::from_utf8(listed.stdout).context("git's list of files")?;
    let files: Vec<&str> = files
        .lines()
        .filter(|file| *file != crate::config_guide::TEMPLATE)
        .collect();
    let link = Regex::new(r"\[[^\]]*\]\(([^)\s]+)(?:\s+[^)]*)?\)|^\s*\[[^\]]+\]:\s*(\S+)")?;
    let mut anchors: HashMap<PathBuf, HashSet<String>> = HashMap::new();
    let mut findings = Vec::new();
    let mut links = 0;
    for file in files {
        let path = root.join(file);
        let text = std::fs::read_to_string(&path).with_context(|| format!("reading {file}"))?;
        for (line, prose) in prose_lines(&text) {
            for found in link.captures_iter(&prose) {
                let Some(target) = found.get(1).or_else(|| found.get(2)) else {
                    continue;
                };
                let target = target.as_str();
                if is_external(target) {
                    continue;
                }
                links += 1;
                let (to, anchor) = target.split_once('#').unwrap_or((target, ""));
                let destination = if to.is_empty() {
                    path.clone()
                } else {
                    path.parent().unwrap_or(&root).join(percent_decoded(to))
                };
                if !destination.exists() {
                    findings.push(format!("{file}:{line}: `{target}`: no such file"));
                    continue;
                }
                if anchor.is_empty() || destination.is_dir() {
                    continue;
                }
                let known = match anchors.get(&destination) {
                    Some(known) => known,
                    None => {
                        let text = std::fs::read_to_string(&destination)
                            .with_context(|| format!("reading {}", destination.display()))?;
                        anchors
                            .entry(destination.clone())
                            .or_insert(headings(&text))
                    }
                };
                if !known.contains(&percent_decoded(anchor)) {
                    findings.push(format!("{file}:{line}: `{target}`: no such anchor"));
                }
            }
        }
    }
    if findings.is_empty() {
        println!("check-links: {links} internal links resolve");
        Ok(ExitCode::SUCCESS)
    } else {
        for finding in &findings {
            eprintln!("{finding}");
        }
        eprintln!("check-links: links that do not resolve: {}", findings.len());
        Ok(ExitCode::FAILURE)
    }
}

/// Whether `target` leads out of the repository.
fn is_external(target: &str) -> bool {
    target.contains("://") || target.starts_with("mailto:")
}

/// `text` with its `%` escapes decoded, as a link writes a space `%20`.
fn percent_decoded(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] == b'%'
            && let Some(byte) = text
                .get(at + 1..at + 3)
                .and_then(|hex| u8::from_str_radix(hex, 16).ok())
        {
            decoded.push(byte);
            at += 3;
        } else {
            decoded.push(bytes[at]);
            at += 1;
        }
    }
    String::from_utf8_lossy(&decoded).into_owned()
}

/// The lines of `text` outside its code blocks, with their numbers from 1,
/// and their code spans left out.
fn prose_lines(text: &str) -> Vec<(usize, String)> {
    let mut lines = Vec::new();
    let mut fence: Option<&str> = None;
    for (index, line) in text.lines().enumerate() {
        let trimmed = line.trim_start();
        let marker = ["```", "~~~"]
            .into_iter()
            .find(|marker| trimmed.starts_with(marker));
        match (fence, marker) {
            (None, Some(marker)) => fence = Some(marker),
            (Some(open), Some(marker)) if open == marker => fence = None,
            (None, None) => lines.push((index + 1, without_code_spans(line))),
            _ => {}
        }
    }
    lines
}

/// `line` with its code spans, between backticks, left out.
fn without_code_spans(line: &str) -> String {
    let mut kept = String::with_capacity(line.len());
    let mut in_span = false;
    for c in line.chars() {
        if c == '`' {
            in_span = !in_span;
        } else if !in_span {
            kept.push(c);
        }
    }
    kept
}

/// The anchors that the headings of `text` make, as GitHub makes them: the
/// heading's text in lowercase, without its punctuation but `-` and `_`,
/// its spaces as `-`, and a number after a repeated one.
fn headings(text: &str) -> HashSet<String> {
    let mut seen: HashMap<String, usize> = HashMap::new();
    let mut anchors = HashSet::new();
    for (_, line) in prose_lines_keeping_spans(text) {
        let trimmed = line.trim_start();
        let level = trimmed.chars().take_while(|&c| c == '#').count();
        if !(1..=6).contains(&level) || !trimmed[level..].starts_with(' ') {
            continue;
        }
        let heading = trimmed[level..].trim().trim_end_matches('#').trim();
        let base = slug(heading);
        let count = seen.entry(base.clone()).or_insert(0);
        let anchor = if *count == 0 {
            base.clone()
        } else {
            format!("{base}-{count}")
        };
        *count += 1;
        anchors.insert(anchor);
    }
    anchors
}

/// The lines of `text` outside its code blocks, code spans kept, as a
/// heading's anchor keeps a code span's text.
fn prose_lines_keeping_spans(text: &str) -> Vec<(usize, String)> {
    let mut lines = Vec::new();
    let mut fence: Option<&str> = None;
    for (index, line) in text.lines().enumerate() {
        let trimmed = line.trim_start();
        let marker = ["```", "~~~"]
            .into_iter()
            .find(|marker| trimmed.starts_with(marker));
        match (fence, marker) {
            (None, Some(marker)) => fence = Some(marker),
            (Some(open), Some(marker)) if open == marker => fence = None,
            (None, None) => lines.push((index + 1, line.to_owned())),
            _ => {}
        }
    }
    lines
}

/// A heading's anchor, as GitHub makes it.
pub(crate) fn slug(heading: &str) -> String {
    heading
        .chars()
        .flat_map(char::to_lowercase)
        .filter(|&c| c.is_alphanumeric() || c == '-' || c == '_' || c == ' ')
        .map(|c| if c == ' ' { '-' } else { c })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anchors_are_github_s() {
        assert_eq!(slug("F2-26 — The input method"), "f2-26--the-input-method");
        assert_eq!(
            slug("F2-32 — The config and its errors"),
            "f2-32--the-config-and-its-errors"
        );
        assert_eq!(slug("`[font]`"), "font");
        assert_eq!(slug("Requirements → solution"), "requirements--solution");
        let anchors = headings("# A\n\n## A\n\n```\n# not a heading\n```\n### Ünïcode words\n");
        let mut anchors: Vec<_> = anchors.into_iter().collect();
        anchors.sort();
        assert_eq!(anchors, ["a", "a-1", "ünïcode-words"]);
    }

    #[test]
    fn links_in_code_are_left_out() {
        let text = "a [link](x.md) and `[code](y.md)`\n```\n[block](z.md)\n```\n[ref]: w.md\n";
        let lines: Vec<String> = prose_lines(text)
            .into_iter()
            .map(|(_, line)| line)
            .collect();
        assert_eq!(lines, ["a [link](x.md) and ", "[ref]: w.md"]);
        assert_eq!(percent_decoded("a%20b.md"), "a b.md");
        assert!(is_external("https://example.com/#a") && !is_external("../docs/a.md#b"));
    }
}
