//! `cargo xtask since <X.Y.Z>`: what no release has yet, released. The
//! public pages mark a feature of `main` that no release has with
//! `· not released yet`, and the config's schemas a key of `"x-since":
//! "next"`, or the same text in their guides; in the commit that prepares
//! a release, this turns each mark into the release's, `· Since X.Y`, or
//! `· Since X.Y.Z` of a patch, and `"x-since": "X.Y"`, then makes the config
//! guide anew. `cargo xtask release` refuses to release while a mark is
//! left. A mark in code, a code block or a span of backticks, as a guide
//! that shows the mark, is no mark.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use anyhow::{Context, Result, bail, ensure};

/// The mark of what no release has yet, in a page or a schema's text.
pub const MARK: &str = "· not released yet";

/// A schema's key that no release has yet.
const NEXT: &str = r#""x-since": "next""#;

/// The words that make a mark, which a mark of another form holds too.
const WORDS: &str = "not released yet";

/// The root's public pages beside `docs/`.
const ROOT_PAGES: [&str; 4] = [
    "README.md",
    "CHANGELOG.md",
    "CONTRIBUTING.md",
    "TRANSLATING.md",
];

pub fn run(mut args: impl Iterator<Item = String>) -> Result<ExitCode> {
    let version = args
        .next()
        .context("since takes the release's version, X.Y.Z")?;
    if let Some(arg) = args.next() {
        bail!("since takes the release's version alone, not `{arg}`");
    }
    let since = since(&version)?;
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .context("the workspace's root")?;
    let mut changed = 0;
    for file in files(&root)? {
        let path = root.join(&file);
        let text =
            fs::read_to_string(&path).with_context(|| format!("reading {}", file.display()))?;
        let (made, marks) = released(&text, &since, Kind::of(&file));
        if marks > 0 {
            fs::write(&path, made).with_context(|| format!("writing {}", file.display()))?;
            println!("since: {} mark(s) of {}", marks, file.display());
            changed += marks;
        }
    }
    crate::config_guide::run(std::iter::empty()).context("making the config guide anew")?;
    let left = left(&root)?;
    ensure!(
        left.is_empty(),
        "marks of another form are left, which a release refuses: {}",
        left.join(", ")
    );
    println!("since: {changed} mark(s) now of {since}");
    Ok(ExitCode::SUCCESS)
}

/// The version that a release's marks name: `X.Y` of `X.Y.0`, `X.Y.Z` of a
/// patch; none of a candidate's or a build's, which releases nothing.
pub(crate) fn since(version: &str) -> Result<String> {
    let parts: Vec<&str> = version.split('.').collect();
    let numbers = parts.len() == 3
        && parts
            .iter()
            .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()));
    ensure!(
        numbers,
        "`{version}` is no release's version, X.Y.Z: a candidate's marks stay"
    );
    Ok(if parts[2] == "0" {
        format!("{}.{}", parts[0], parts[1])
    } else {
        version.to_owned()
    })
}

/// The files that may hold marks, relative to `root`: the public pages
/// that git tracks, and the config's schemas, the kernel's and the
/// plugins'.
fn files(root: &Path) -> Result<Vec<PathBuf>> {
    let listed = Command::new("git")
        .args([
            "ls-files",
            "--",
            "docs/*.md",
            "crates/kernel/config.schema.json",
            "plugins/*/*.schema.json",
        ])
        .args(ROOT_PAGES)
        .current_dir(root)
        .output()
        .context("listing the files that git tracks")?;
    ensure!(listed.status.success(), "git ls-files failed");
    let files = String::from_utf8(listed.stdout).context("git's list of files")?;
    Ok(files.lines().map(PathBuf::from).collect())
}

/// The marks left under `root`, each `file:line`: the marks, a schema's
/// key of no release, and the words of a mark in any other form, outside
/// a page's code.
pub(crate) fn left(root: &Path) -> Result<Vec<String>> {
    let mut found = Vec::new();
    for file in files(root)? {
        let text = fs::read_to_string(root.join(&file))
            .with_context(|| format!("reading {}", file.display()))?;
        for line in marked(&text, Kind::of(&file)) {
            found.push(format!("{}:{line}", file.display()));
        }
    }
    Ok(found)
}

/// What a file of marks is: a page, of Markdown, whose code is left
/// alone, or a schema, of JSON, whose texts are the guide's whole, as one
/// line holds a key's guide, code and all.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Page,
    Schema,
}

impl Kind {
    fn of(file: &Path) -> Self {
        if file
            .extension()
            .is_some_and(|extension| extension == "json")
        {
            Self::Schema
        } else {
            Self::Page
        }
    }
}

/// The lines of `text`, from 1, that hold a mark, outside code of a page,
/// or a schema's key of no release.
fn marked(text: &str, kind: Kind) -> Vec<usize> {
    let mut lines = Vec::new();
    let mut fence = Fence::default();
    for (at, line) in text.lines().enumerate() {
        let found = match kind {
            Kind::Schema => line.contains(NEXT) || line.contains(WORDS),
            Kind::Page => {
                !fence.takes(line) && outside_code(line).iter().any(|part| part.contains(WORDS))
            }
        };
        if found {
            lines.push(at + 1);
        }
    }
    lines
}

/// `text` of its marks made the release's, `· Since <since>`, outside code
/// of a page, and of a schema's keys of no release of `since`: and how
/// many it made.
fn released(text: &str, since: &str, kind: Kind) -> (String, usize) {
    let mark = format!("· Since {since}");
    if kind == Kind::Schema {
        let marks = text.matches(MARK).count() + text.matches(NEXT).count();
        let made = text
            .replace(MARK, &mark)
            .replace(NEXT, &format!(r#""x-since": "{since}""#));
        return (made, marks);
    }
    let mut made = String::with_capacity(text.len());
    let mut marks = 0;
    let mut fence = Fence::default();
    for line in text.split_inclusive('\n') {
        if fence.takes(line.trim_end_matches(['\n', '\r'])) {
            made.push_str(line);
            continue;
        }
        let mut at = 0;
        for span in code_spans(line) {
            let part = &line[at..span.start];
            marks += part.matches(MARK).count();
            made.push_str(&part.replace(MARK, &mark));
            made.push_str(&line[span.clone()]);
            at = span.end;
        }
        let part = &line[at..];
        marks += part.matches(MARK).count();
        made.push_str(&part.replace(MARK, &mark));
    }
    (made, marks)
}

/// The parts of `line` outside its spans of code.
fn outside_code(line: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut at = 0;
    for span in code_spans(line) {
        parts.push(&line[at..span.start]);
        at = span.end;
    }
    parts.push(&line[at..]);
    parts
}

/// The spans of code of `line`, backticks and all, as CommonMark reads
/// them: a run of backticks opens one that the next run of as many closes;
/// a run that none closes is plain text.
fn code_spans(line: &str) -> Vec<std::ops::Range<usize>> {
    let bytes = line.as_bytes();
    let run_at = |from: usize| {
        let end = bytes[from..]
            .iter()
            .position(|byte| *byte != b'`')
            .map_or(bytes.len(), |length| from + length);
        end - from
    };
    let mut spans = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] != b'`' {
            at += 1;
            continue;
        }
        let opening = run_at(at);
        // The next run of as many backticks, which closes it.
        let mut look = at + opening;
        let mut closed = None;
        while look < bytes.len() {
            if bytes[look] == b'`' {
                let run = run_at(look);
                if run == opening {
                    closed = Some(look + run);
                    break;
                }
                look += run;
            } else {
                look += 1;
            }
        }
        match closed {
            Some(end) => {
                spans.push(at..end);
                at = end;
            }
            None => at += opening,
        }
    }
    spans
}

/// Where a document stands of its fenced code blocks: the fence that
/// opened the block that it is in, if it is in one.
#[derive(Default)]
struct Fence {
    open: Option<(char, usize)>,
}

impl Fence {
    /// Whether `line` is of a fenced code block, its fences too: a fence
    /// of three backticks or tildes or more opens one, and a line of the
    /// same character, as many or more and nothing else, closes it.
    fn takes(&mut self, line: &str) -> bool {
        let trimmed = line.trim_start();
        let fence = |line: &str| {
            let first = line
                .chars()
                .next()
                .filter(|first| matches!(first, '`' | '~'))?;
            let count = line.chars().take_while(|char| *char == first).count();
            (count >= 3).then_some((first, count))
        };
        match self.open {
            Some((char, count)) => {
                if let Some((closing, closing_count)) = fence(trimmed)
                    && closing == char
                    && closing_count >= count
                    && trimmed.trim_start_matches(char).trim().is_empty()
                {
                    self.open = None;
                }
                true
            }
            None => match fence(trimmed) {
                Some(opened) => {
                    self.open = Some(opened);
                    true
                }
                None => false,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_release_names_its_minor_or_its_patch_and_a_candidate_none() {
        assert_eq!(since("1.3.0").unwrap(), "1.3");
        assert_eq!(since("1.3.2").unwrap(), "1.3.2");
        assert_eq!(since("10.0.0").unwrap(), "10.0");
        for refused in ["1.3.0-rc.1", "1.3", "1.3.0-20261117", "v1.3.0", "1..0"] {
            assert!(since(refused).is_err(), "{refused}");
        }
    }

    #[test]
    fn the_marks_outside_code_become_the_release_s() {
        let text = "# Tabs\n\n## Hints\n\n<sub>Platforms: macOS · not released yet</sub>\n\n- **Pinned tabs** stay at the start · not released yet\n| `double-tap` | `\"none\"` | The modifier · not released yet |\n\nMark it with `· not released yet`, in code.\n\n```markdown\n<sub>Platforms: macOS · not released yet</sub>\n```\n\n````markdown\n```sh\nls · not released yet\n```\n````\n\nAfter the blocks · not released yet\n";
        let (made, marks) = released(text, "1.3", Kind::Page);
        assert_eq!(marks, 4, "{made}");
        assert!(made.contains("<sub>Platforms: macOS · Since 1.3</sub>\n\n- **Pinned tabs**"));
        assert!(made.contains("stay at the start · Since 1.3\n"));
        assert!(made.contains("The modifier · Since 1.3 |"));
        assert!(made.contains("After the blocks · Since 1.3\n"));
        // In code, in a block, in a block of a block, as they were.
        assert!(made.contains("Mark it with `· not released yet`, in code."));
        assert!(made.contains("```markdown\n<sub>Platforms: macOS · not released yet</sub>\n```"));
        assert!(made.contains("ls · not released yet\n```\n````"));
        // What is left: none outside code.
        assert!(marked(&made, Kind::Page).is_empty());
        assert_eq!(marked(text, Kind::Page), [5, 7, 8, 22]);
        // A span of two backticks holds one, and a backtick that no other
        // closes is plain: the mark after it is one.
        let text = "Bind ``\"global:ctrl+`\"`` · not released yet, and `x-since`: `\"x-since\": \"next\"`\nA lone ` tick · not released yet\n";
        let (made, marks) = released(text, "1.3", Kind::Page);
        assert_eq!(marks, 2, "{made}");
        assert_eq!(
            made,
            "Bind ``\"global:ctrl+`\"`` · Since 1.3, and `x-since`: `\"x-since\": \"next\"`\nA lone ` tick · Since 1.3\n"
        );
    }

    #[test]
    fn a_schema_s_key_of_no_release_takes_the_release_s_version() {
        let schema = "{\n  \"double-tap\": {\n    \"x-platforms\": [\"macos\"],\n    \"x-since\": \"next\",\n    \"x-guide\": \"| `cmd+u` | `window.toggle-opacity` | Shows it opaque · not released yet |\"\n  }\n}\n";
        assert_eq!(marked(schema, Kind::Schema), [4, 5]);
        let (made, marks) = released(schema, "1.3", Kind::Schema);
        assert_eq!(marks, 2);
        assert!(made.contains("\"x-since\": \"1.3\","));
        assert!(made.contains("Shows it opaque · Since 1.3 |"));
        assert!(marked(&made, Kind::Schema).is_empty());
        // A guide's one line of an odd number of backticks before the mark.
        let guide = r#"{"x-guide": "| `global:ctrl+`` | a key | its text · not released yet |"}"#;
        let (made, marks) = released(guide, "1.3", Kind::Schema);
        assert_eq!(marks, 1);
        assert!(made.contains("its text · Since 1.3 |"));
        // A page shows a schema's key of no release as it is.
        assert!(marked(r#"Write `"x-since": "next"` beside it."#, Kind::Page).is_empty());
    }

    #[test]
    fn a_mark_of_another_form_is_left_and_told() {
        let text = "Hints (not released yet)\n\nNot released yet, in another case\n";
        let (made, marks) = released(text, "1.3", Kind::Page);
        assert_eq!((made.as_str(), marks), (text, 0));
        assert_eq!(marked(text, Kind::Page), [1]);
    }

    #[test]
    fn the_repository_s_marks_are_all_of_their_form() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()
            .unwrap();
        // Each mark left becomes the release's: none of another form, a
        // schema reading still, and a page's code as it was.
        for file in files(&root).unwrap() {
            let text = fs::read_to_string(root.join(&file)).unwrap();
            let kind = Kind::of(&file);
            let (made, _) = released(&text, "9.9", kind);
            let left = marked(&made, kind);
            assert!(left.is_empty(), "{}: {left:?}", file.display());
            if kind == Kind::Schema {
                serde_json::from_str::<serde_json::Value>(&made).unwrap();
            }
        }
        let guide = fs::read_to_string(root.join("docs/WRITING-DOCS.md")).unwrap();
        let (made, _) = released(&guide, "9.9", Kind::Page);
        assert!(made.contains("`\"x-since\": \"next\"`") && made.contains("`· not released yet`"));
    }
}
