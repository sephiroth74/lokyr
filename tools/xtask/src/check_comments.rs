//! `cargo xtask check-comments`: finds the plan in the code's comments,
//! which describe the code alone: the tasks' identifiers, the ADRs' numbers,
//! the phases named as phases, and the files of the tasks. It reads the
//! comments of the Rust and Metal files that git tracks, and knows their
//! strings, characters and raw strings, so that a `//` in a string is no
//! comment. A phase's name alone can be a function key's, and counts only
//! next to the word phase.

use std::path::Path;
use std::process::{Command, ExitCode};

use anyhow::{Context, Result, bail};
use regex::Regex;

/// What the comments must not hold, and how a finding names it.
const PATTERNS: [(&str, &str); 6] = [
    (r"\bF[0-9]b?-[0-9]{2,3}\b", "a task"),
    (r"\bADR[- ]?[0-9]{1,4}\b|\badr/[0-9]{4}", "an ADR"),
    (r"(?i)\bphases? F[0-9]b?\b", "a phase"),
    (r"\bF[0-9]b?(?:'s)? phase\b", "a phase"),
    (
        r"\bF[0-9]b?'s (?:search|exit|criteri\w*|tasks?|plan|blocks?|reviews?|end|work|plugins?|release)\b",
        "a phase",
    ),
    (r"docs/tasks\b", "the tasks' files"),
];

pub fn run(mut args: impl Iterator<Item = String>) -> Result<ExitCode> {
    if let Some(arg) = args.next() {
        bail!("check-comments takes no argument, not `{arg}`");
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let listed = Command::new("git")
        .args(["ls-files", "--", "*.rs", "*.metal"])
        .current_dir(&root)
        .output()
        .context("listing the files that git tracks")?;
    if !listed.status.success() {
        bail!("git ls-files failed");
    }
    let patterns: Vec<(Regex, &str)> = PATTERNS
        .iter()
        .map(|&(pattern, what)| Ok((Regex::new(pattern)?, what)))
        .collect::<Result<_>>()?;
    let mut findings = Vec::new();
    let files = String::from_utf8(listed.stdout).context("git's list of files")?;
    for file in files.lines() {
        let text =
            std::fs::read_to_string(root.join(file)).with_context(|| format!("reading {file}"))?;
        for (line, comment) in comments(&text) {
            for (pattern, what) in &patterns {
                if let Some(found) = pattern.find(&comment) {
                    findings.push(format!(
                        "{file}:{line}: a comment names {what}: `{}`",
                        found.as_str()
                    ));
                }
            }
        }
    }
    if findings.is_empty() {
        println!("check-comments: no comment names the plan");
        Ok(ExitCode::SUCCESS)
    } else {
        for finding in &findings {
            eprintln!("{finding}");
        }
        eprintln!(
            "check-comments: comments that name the plan: {}; comments describe the code alone",
            findings.len()
        );
        Ok(ExitCode::FAILURE)
    }
}

/// The comments of `text`, Rust's or Metal's: each line of each, with its
/// line's number, from 1.
pub fn comments(text: &str) -> Vec<(usize, String)> {
    let chars: Vec<char> = text.chars().collect();
    let mut found = Vec::new();
    let mut line = 1;
    let mut at = 0;
    let next = |at: usize| chars.get(at).copied();
    while at < chars.len() {
        let c = chars[at];
        match c {
            '\n' => {
                line += 1;
                at += 1;
            }
            '/' if next(at + 1) == Some('/') => {
                let start = at;
                while at < chars.len() && chars[at] != '\n' {
                    at += 1;
                }
                found.push((line, chars[start..at].iter().collect()));
            }
            '/' if next(at + 1) == Some('*') => {
                // Rust's block comments nest.
                let mut depth = 0;
                let mut text = String::new();
                while at < chars.len() {
                    if chars[at] == '/' && next(at + 1) == Some('*') {
                        depth += 1;
                        at += 2;
                        continue;
                    }
                    if chars[at] == '*' && next(at + 1) == Some('/') {
                        depth -= 1;
                        at += 2;
                        if depth == 0 {
                            break;
                        }
                        continue;
                    }
                    if chars[at] == '\n' {
                        found.push((line, std::mem::take(&mut text)));
                        line += 1;
                    } else {
                        text.push(chars[at]);
                    }
                    at += 1;
                }
                found.push((line, text));
            }
            'r' if matches!(next(at + 1), Some('"' | '#')) && raw_prefix(&chars, at) => {
                // A raw string, of as many `#` around it as it opens with.
                let mut hashes = 0;
                let mut cursor = at + 1;
                while next(cursor) == Some('#') {
                    hashes += 1;
                    cursor += 1;
                }
                if next(cursor) != Some('"') {
                    at += 1;
                    continue;
                }
                cursor += 1;
                loop {
                    match next(cursor) {
                        None => break,
                        Some('"') if (1..=hashes).all(|k| next(cursor + k) == Some('#')) => {
                            cursor += 1 + hashes;
                            break;
                        }
                        Some('\n') => line += 1,
                        Some(_) => {}
                    }
                    cursor += 1;
                }
                at = cursor;
            }
            '"' => {
                at += 1;
                while let Some(c) = next(at) {
                    at += 1;
                    match c {
                        '\\' => at += 1,
                        '"' => break,
                        '\n' => line += 1,
                        _ => {}
                    }
                }
            }
            '\'' => {
                // A character, `'x'` or `'\n'`, or else a lifetime or a
                // label, which goes on as code.
                if next(at + 1) == Some('\\') {
                    at += 2;
                    while let Some(c) = next(at) {
                        at += 1;
                        if c == '\'' {
                            break;
                        }
                    }
                } else if next(at + 2) == Some('\'') {
                    at += 3;
                } else {
                    at += 1;
                }
            }
            _ => at += 1,
        }
    }
    found
}

fn is_identifier(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Whether the `r` at `at` starts a raw string, `r"…"` or `br"…"`, rather
/// than ending an identifier.
fn raw_prefix(chars: &[char], at: usize) -> bool {
    let before = |back: usize| at.checked_sub(back).map(|at| chars[at]);
    match before(1) {
        None => true,
        Some('b') => before(2).is_none_or(|c| !is_identifier(c)),
        Some(c) => !is_identifier(c),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(text: &str) -> Vec<(usize, String)> {
        comments(text)
    }

    #[test]
    fn comments_are_found_and_strings_are_not() {
        let code = concat!(
            "let url = \"https://example.com\"; // a comment\n",
            "let quote = '\"'; let glob = \"crates/*-core\";\n",
            "/// a doc comment\n",
            "let raw = r#\"// not a comment \"# ; /* a block\n",
            "comment */ fn f<'a>(x: &'a str) {}\n",
            "let escaped = \"a \\\" // still a string\"; // after\n",
        );
        let found = texts(code);
        assert_eq!(
            found,
            [
                (1, "// a comment".to_owned()),
                (3, "/// a doc comment".to_owned()),
                (4, " a block".to_owned()),
                (5, "comment ".to_owned()),
                (6, "// after".to_owned()),
            ]
        );
    }

    #[test]
    fn the_plan_is_found_in_comments_but_not_function_keys() {
        let patterns: Vec<Regex> = PATTERNS
            .iter()
            .map(|&(pattern, _)| Regex::new(pattern).unwrap())
            .collect();
        let names = |comment: &str| patterns.iter().any(|pattern| pattern.is_match(comment));
        for planned in [
            "// as F2-71 asks",
            "// see ADR 0012",
            "// docs/adr/0009-toml-config.md",
            "// comes in phase F3",
            "// F3b's phase brings it",
            "// the exit criterion of F3's search",
            "// see docs/tasks/F2.md",
        ] {
            assert!(names(planned), "{planned}");
        }
        for plain in [
            "// F1 to F12, and F13 to F20",
            "// the F3 key",
            "// xterm's F1-F4 send SS3",
            "// F1's sequence of the keypad",
        ] {
            assert!(!names(plain), "{plain}");
        }
    }
}
