//! `cargo xtask since <X.Y.Z>`: what no release has yet, released. The
//! public pages mark a feature of `main` that no release has with
//! `· not released yet`, the config's schemas a key of `"x-since":
//! "next"`, or the same text in their guides, and the themes' rules a
//! token of the same text in its comment; in the commit that prepares a
//! release, this turns each mark into the release's, `· Since X.Y`, or
//! `· Since X.Y.Z` of a patch, and `"x-since": "X.Y"`, then makes the config
//! guide anew, checks the theme guide's tokens against the rules, and writes
//! the released commands' file of the app's, the commands' reference made
//! anew of none marked (on a Mac, as its keys are). A page's mark is of its text
//! as Markdown reads it: in a code block's comment, of the block's language,
//! as the Luau reference's `-- … · not released yet`, it is one too; in other
//! code, a block or a span, as a guide that shows the mark, and in a block
//! whose info string says `example`, it is no mark; in a heading, or in
//! HTML, it is refused. Every other occurrence of the words, in any case, is
//! left and told, and `cargo xtask release` refuses to release while one is
//! left.

use std::fs;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::sync::LazyLock;

use anyhow::{Context, Result, bail, ensure};
use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd};
use regex::Regex;

/// The mark of what no release has yet, in a page or a schema's text.
pub const MARK: &str = "· not released yet";

/// A schema's key that no release has yet, of any spaces around its colon.
static NEXT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#""x-since"\s*:\s*"next""#).expect("the pattern"));

/// The words that make a mark, in any case and of any spaces between them,
/// which a mark of another form holds too.
static WORDS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)not\s+released\s+yet").expect("the pattern"));

/// The root's public pages beside `docs/`.
const ROOT_PAGES: [&str; 4] = [
    "README.md",
    "CHANGELOG.md",
    "CONTRIBUTING.md",
    "TRANSLATING.md",
];

/// The themes' rules, whose comments the theme guide's tokens show.
const RULES: &str = "crates/plugin-sdk/src/theme/rules.toml";

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
    // The commands' reference, made of the app, marks what the released
    // commands' file lacks: the file of the app's commands and keys now,
    // and the reference of none marked.
    crate::docs::release().context("writing the released commands")?;
    // The theme guide's table, whose marks the page's own rewrite took, is
    // checked against the rewritten rules by a build of them, as this
    // binary holds the rules of before.
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let checked = Command::new(cargo)
        .args(["xtask", "theme-guide", "--check"])
        .current_dir(&root)
        .status()
        .context("checking the theme guide of the rules")?;
    ensure!(
        checked.success(),
        "the theme guide's table is not the rewritten rules': run `cargo xtask theme-guide`"
    );
    let left = left(&root)?;
    ensure!(
        left.is_empty(),
        "marks are left that no release takes, of another form, in a heading or in HTML: {}",
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
/// that git tracks, the config's schemas, the kernel's and the plugins',
/// and the themes' rules.
fn files(root: &Path) -> Result<Vec<PathBuf>> {
    let listed = Command::new("git")
        .args([
            "ls-files",
            "--",
            "docs/*.md",
            "crates/kernel/config.schema.json",
            "plugins/*/*.schema.json",
            RULES,
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
/// alone, or a text of a mark wherever it is: a schema, of JSON, whose
/// texts are the guide's whole, as one line holds a key's guide, code and
/// all, and the themes' rules, whose comments the guide shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Page,
    Text,
}

impl Kind {
    fn of(file: &Path) -> Self {
        if file.extension().is_some_and(|extension| extension == "md") {
            Self::Page
        } else {
            Self::Text
        }
    }
}

/// The lines of `text`, from 1, that hold the words of a mark, in any
/// form but in a page's code, or a schema's key of no release.
fn marked(text: &str, kind: Kind) -> Vec<usize> {
    let (code, comments) = match kind {
        Kind::Page => {
            let read = read(text);
            (read.code, read.comments)
        }
        Kind::Text => (Vec::new(), Vec::new()),
    };
    let within =
        |ranges: &[Range<usize>], at: usize| ranges.iter().any(|range| range.contains(&at));
    let mut starts: Vec<usize> = WORDS
        .find_iter(text)
        .map(|found| found.start())
        .filter(|&at| !within(&code, at) || within(&comments, at))
        .collect();
    if kind == Kind::Text {
        starts.extend(NEXT.find_iter(text).map(|found| found.start()));
    }
    let mut lines: Vec<usize> = starts
        .into_iter()
        .map(|at| text[..at].matches('\n').count() + 1)
        .collect();
    lines.sort_unstable();
    lines.dedup();
    lines
}

/// `text` of its marks made the release's, `· Since <since>`, in a page's
/// text outside its code, headings and HTML, and of a schema's keys of no
/// release of `since`: and how many it made.
fn released(text: &str, since: &str, kind: Kind) -> (String, usize) {
    let mark = format!("· Since {since}");
    if kind == Kind::Text {
        let marks = text.matches(MARK).count() + NEXT.find_iter(text).count();
        let marked = text.replace(MARK, &mark);
        let key = format!(r#""x-since": "{since}""#);
        let made = NEXT
            .replace_all(&marked, regex::NoExpand(&key))
            .into_owned();
        return (made, marks);
    }
    let marks = read(text).marks;
    let mut made = String::with_capacity(text.len());
    let mut at = 0;
    for found in &marks {
        made.push_str(&text[at..found.start]);
        // A mark that a table's cell holds alone, as a Since column's, is
        // the version alone, as the other rows write theirs.
        let line_start = text[..found.start].rfind('\n').map_or(0, |at| at + 1);
        let line_end = text[found.end..]
            .find('\n')
            .map_or(text.len(), |at| found.end + at);
        let alone = text[line_start..found.start].trim_end().ends_with('|')
            && text[found.end..line_end].trim_start().starts_with('|');
        made.push_str(if alone { since } else { &mark });
        at = found.end;
    }
    made.push_str(&text[at..]);
    (made, marks.len())
}

/// What a page's Markdown holds, of byte ranges: the marks that a release
/// makes its own, in its text, outside its code, headings and HTML, and in
/// its code blocks' comments; its code, blocks and spans, whose words are no
/// mark but in a comment; and the comments of its code blocks, of the
/// languages that [`comment_of`] knows, but of a block of an example.
#[derive(Debug, Default)]
struct Read {
    marks: Vec<Range<usize>>,
    code: Vec<Range<usize>>,
    comments: Vec<Range<usize>>,
}

/// What starts a line's comment in a code block whose info string is
/// `info`, of its first word, the language; none of a language of no
/// comment that a mark goes in, as Markdown's or a console's, and of a
/// block whose info string says `example`, which shows the mark as it is.
fn comment_of(info: &str) -> Option<&'static str> {
    let mut words = info
        .split([' ', ',', '{', '}'])
        .filter(|word| !word.is_empty());
    let language = words.next()?.to_ascii_lowercase();
    if words.any(|word| word.eq_ignore_ascii_case("example")) {
        return None;
    }
    match language.as_str() {
        "luau" | "lua" | "sql" => Some("--"),
        "toml" | "sh" | "bash" | "zsh" | "fish" | "shell" | "python" | "py" | "ruby" | "yaml"
        | "yml" | "ini" | "just" | "justfile" | "make" | "makefile" => Some("#"),
        "rust" | "rs" | "js" | "javascript" | "ts" | "typescript" | "swift" | "c" | "cpp"
        | "go" | "java" | "kotlin" | "jsonc" | "json5" => Some("//"),
        _ => None,
    }
}

/// The marks and the code of the page `text`, as Markdown reads it.
fn read(text: &str) -> Read {
    let options = Options::ENABLE_TABLES
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_GFM;
    let mut read = Read::default();
    // The runs of text outside code and headings, each of the text events
    // that follow one another.
    let mut runs: Vec<Range<usize>> = Vec::new();
    let (mut blocks, mut headings) = (0usize, 0usize);
    // What starts a comment in the code block under way, if its language
    // has one.
    let mut comment: Option<&str> = None;
    for (event, range) in Parser::new_ext(text, options).into_offset_iter() {
        match event {
            Event::Start(Tag::CodeBlock(kind)) => {
                comment = match &kind {
                    CodeBlockKind::Fenced(info) => comment_of(info),
                    CodeBlockKind::Indented => None,
                };
                read.code.push(range);
                blocks += 1;
            }
            Event::End(TagEnd::CodeBlock) => {
                blocks = blocks.saturating_sub(1);
                comment = None;
            }
            // Each line's comment of the block's text, from what starts it
            // to the line's end.
            Event::Text(_) if blocks > 0 => {
                if let Some(start) = comment {
                    let mut at = range.start;
                    for line in text[range.clone()].split_inclusive('\n') {
                        if let Some(found) = line.find(start) {
                            read.comments
                                .push(at + found..at + line.trim_end_matches('\n').len());
                        }
                        at += line.len();
                    }
                }
            }
            Event::Start(Tag::Heading { .. }) => headings += 1,
            Event::End(TagEnd::Heading(_)) => headings = headings.saturating_sub(1),
            Event::Code(_) => read.code.push(range),
            Event::Text(_) if blocks == 0 && headings == 0 => match runs.last_mut() {
                Some(run) if run.end == range.start => run.end = range.end,
                _ => runs.push(range),
            },
            _ => {}
        }
    }
    runs.extend(read.comments.iter().cloned());
    runs.sort_by_key(|run| run.start);
    for run in runs {
        let mut at = run.start;
        while let Some(found) = text[at..run.end].find(MARK) {
            let start = at + found;
            read.marks.push(start..start + MARK.len());
            at = start + MARK.len();
        }
    }
    read
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
    fn a_mark_that_a_table_s_cell_holds_alone_is_the_version_alone() {
        let text = "| Code | What it does | Since |\n|---|---|---|\n| OSC 9 | Notifications | 0.2 |\n| OSC 99 | Notifications in parts | · not released yet |\n| OSC 7501 | A status · not released yet | |\n";
        let (made, marks) = released(text, "1.3", Kind::Page);
        assert_eq!(marks, 2, "{made}");
        assert!(
            made.contains("| OSC 99 | Notifications in parts | 1.3 |"),
            "{made}"
        );
        assert!(
            made.contains("| OSC 7501 | A status · Since 1.3 | |"),
            "{made}"
        );
    }

    #[test]
    fn a_schema_s_key_of_no_release_takes_the_release_s_version() {
        let schema = "{\n  \"double-tap\": {\n    \"x-platforms\": [\"macos\"],\n    \"x-since\": \"next\",\n    \"x-guide\": \"| `cmd+u` | `window.toggle-opacity` | Shows it opaque · not released yet |\"\n  }\n}\n";
        assert_eq!(marked(schema, Kind::Text), [4, 5]);
        let (made, marks) = released(schema, "1.3", Kind::Text);
        assert_eq!(marks, 2);
        assert!(made.contains("\"x-since\": \"1.3\","));
        assert!(made.contains("Shows it opaque · Since 1.3 |"));
        assert!(marked(&made, Kind::Text).is_empty());
        // A guide's one line of an odd number of backticks before the mark.
        let guide = r#"{"x-guide": "| `global:ctrl+`` | a key | its text · not released yet |"}"#;
        let (made, marks) = released(guide, "1.3", Kind::Text);
        assert_eq!(marks, 1);
        assert!(made.contains("its text · Since 1.3 |"));
        // A page shows a schema's key of no release as it is.
        assert!(marked(r#"Write `"x-since": "next"` beside it."#, Kind::Page).is_empty());
    }

    #[test]
    fn a_page_s_marks_are_of_its_text_as_markdown_reads_it() {
        // A fence opened on a list item's line, a span of three backticks,
        // a quote's fence, an indented block and a span over two lines:
        // code, as they were; the marks after each, the release's.
        let text = "- ```sh\n  ls · not released yet\n  ```\n\nAfter the item · not released yet\n\nUse ```a · not released yet``` here · not released yet\n\n> ```\n> ls · not released yet\n> ```\n\nText.\n\n    ls · not released yet\n\nWrite `x · not\nreleased yet` here · not released yet\n";
        let (made, marks) = released(text, "1.3", Kind::Page);
        assert_eq!(marks, 3, "{made}");
        assert_eq!(
            made,
            "- ```sh\n  ls · not released yet\n  ```\n\nAfter the item · Since 1.3\n\nUse ```a · not released yet``` here · Since 1.3\n\n> ```\n> ls · not released yet\n> ```\n\nText.\n\n    ls · not released yet\n\nWrite `x · not\nreleased yet` here · Since 1.3\n"
        );
        assert!(marked(&made, Kind::Page).is_empty());
        // In a heading or in HTML, a mark is refused: left, and told.
        let text = "# Tabs\n\n## Groups · not released yet\n\n<div>Pinned · not released yet</div>\n\nBody · not released yet\n";
        let (made, marks) = released(text, "1.3", Kind::Page);
        assert_eq!(marks, 1, "{made}");
        assert!(made.ends_with("Body · Since 1.3\n"));
        assert_eq!(marked(&made, Kind::Page), [3, 5]);
    }

    #[test]
    fn a_schema_s_compact_key_and_the_rules_comments_take_the_release() {
        let schema = r#"{"a": {"x-since":"next", "x-guide": "It · not released yet"}}"#;
        assert_eq!(marked(schema, Kind::Text), [1]);
        let (made, marks) = released(schema, "1.3", Kind::Text);
        assert_eq!(marks, 2);
        assert_eq!(
            made,
            r#"{"a": {"x-since": "1.3", "x-guide": "It · Since 1.3"}}"#
        );
        // The themes' rules, whose comments the theme guide shows.
        let rules = "# A hint's label · not released yet.\n\"hints.label.bg\" = \"accent\"\n";
        let (made, marks) = released(rules, "1.3", Kind::of(Path::new(RULES)));
        assert_eq!(marks, 1);
        assert!(made.starts_with("# A hint's label · Since 1.3.\n"));
        // The comment is the token's description, its mark or its release's
        // with it, whichever the rules hold now.
        let token = lokyr_plugin_sdk::theme::rules::rule("hints.label.bg").unwrap();
        assert!(
            token.description.starts_with("A hint's label"),
            "{}",
            token.description
        );
    }

    #[test]
    fn a_code_block_s_comment_takes_the_release_and_an_example_s_stays() {
        // A Luau comment's mark, of its own line and after code, and a
        // TOML one's: the release's; the code before a comment, a string
        // of the words, Markdown's block and an example's comment, as they
        // were.
        let text = "```luau\n-- A popup over the tab · not released yet.\nopen_popup: () -> PaneId -- its pane · not released yet\nlocal s = \"· not released yet\"\n```\n\n```toml\n[window]\n# Its share · not released yet\nwidth = 0.8\n```\n\n```markdown\n- **A key** · not released yet\n```\n\n```luau example\n-- Mark it so · not released yet\n```\n\n> ```lua\n> -- In a quote · not released yet\n> ```\n";
        assert_eq!(marked(text, Kind::Page), [2, 3, 9, 22]);
        let (made, marks) = released(text, "1.5", Kind::Page);
        assert_eq!(marks, 4, "{made}");
        assert!(
            made.contains("-- A popup over the tab · Since 1.5.\n"),
            "{made}"
        );
        assert!(
            made.contains("-> PaneId -- its pane · Since 1.5\n"),
            "{made}"
        );
        assert!(
            made.contains("local s = \"· not released yet\"\n"),
            "{made}"
        );
        assert!(made.contains("# Its share · Since 1.5\n"), "{made}");
        assert!(made.contains("> -- In a quote · Since 1.5\n"), "{made}");
        assert!(
            made.contains("- **A key** · not released yet\n```"),
            "{made}"
        );
        assert!(
            made.contains("-- Mark it so · not released yet\n```"),
            "{made}"
        );
        assert!(marked(&made, Kind::Page).is_empty(), "{made}");
        // A comment's mark of another form is told, and stays.
        let text = "```luau\n-- A popup (not released yet)\n```\n";
        assert_eq!(released(text, "1.5", Kind::Page).1, 0);
        assert_eq!(marked(text, Kind::Page), [2]);
        // The languages: Luau's, a shell's and Rust's comments; none of
        // Markdown, a console, no info, or an example of any language.
        assert_eq!(comment_of("luau"), Some("--"));
        assert_eq!(comment_of("sh"), Some("#"));
        assert_eq!(comment_of("Rust"), Some("//"));
        for none in ["markdown", "console", "", "luau example", "toml,example"] {
            assert_eq!(comment_of(none), None, "{none}");
        }
    }

    #[test]
    fn a_mark_of_another_form_is_left_and_told() {
        let text = "Hints (not released yet)\n\nNot Released\nyet, in another case\n";
        let (made, marks) = released(text, "1.3", Kind::Page);
        assert_eq!((made.as_str(), marks), (text, 0));
        assert_eq!(marked(text, Kind::Page), [1, 3]);
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
            if file
                .extension()
                .is_some_and(|extension| extension == "json")
            {
                serde_json::from_str::<serde_json::Value>(&made).unwrap();
            }
            if file
                .extension()
                .is_some_and(|extension| extension == "toml")
            {
                toml::from_str::<toml::Value>(&made).unwrap();
            }
        }
        let guide = fs::read_to_string(root.join("docs/WRITING-DOCS.md")).unwrap();
        let (made, _) = released(&guide, "9.9", Kind::Page);
        assert!(made.contains("`\"x-since\": \"next\"`") && made.contains("`· not released yet`"));
    }
}
