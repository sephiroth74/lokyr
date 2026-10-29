//! The config written out whole: every key with its value, in the order of
//! the configuration's guide, and with the guide's own text of each when
//! asked, so that what it writes reads back as the same config.

use std::fmt::Write as _;

use serde::Serialize;
use toml_edit::{InlineTable, Key, Value};

use super::{Config, keys, sections};

/// The configuration's guide, whose headings give the order of the keys,
/// and whose text tells what each does.
const GUIDE: &str = include_str!("../../../../docs/CONFIGURATION.md");

/// Where the guide is, for the rest of its text.
const GUIDE_URL: &str = "https://github.com/sephiroth74/lokyr/blob/main/docs/CONFIGURATION.md";

/// The widest line of the text, its `# ` included.
const WIDTH: usize = 78;

/// What a heading of the guide is about.
#[derive(Debug, PartialEq)]
enum Topic<'a> {
    /// A section, of its own text.
    Section,
    /// Keys, which share their text.
    Keys(Vec<&'a str>),
    /// The bindings of `[keys]`.
    Bindings,
    /// Something else, as where the file is.
    Other,
}

/// A heading of the guide, with the lines of its text.
#[derive(Debug)]
struct Heading<'a> {
    /// The section that it is in; `None` at the top of the file.
    section: Option<&'a str>,
    topic: Topic<'a>,
    text: Vec<&'a str>,
}

/// The names that `title` gives in backquotes, as `bold` and `italic` of
/// "`bold`, `italic`".
fn quoted(title: &str) -> Vec<&str> {
    title.split('`').skip(1).step_by(2).collect()
}

/// The headings of `guide`, a section's `##` and a key's `###`, in order.
fn headings(guide: &str) -> Vec<Heading<'_>> {
    let mut headings: Vec<Heading<'_>> = Vec::new();
    let mut section = None;
    let mut fenced = false;
    for line in guide.lines() {
        if line.starts_with("```") {
            fenced = !fenced;
        }
        let heading = if fenced {
            None
        } else if let Some(title) = line.strip_prefix("## ") {
            let names = quoted(title);
            match names.as_slice() {
                [name] if name.starts_with('[') => {
                    section = Some(name.trim_matches(['[', ']']));
                    Some(Topic::Section)
                }
                [] => {
                    section = None;
                    Some(Topic::Other)
                }
                _ => {
                    section = None;
                    Some(Topic::Keys(names))
                }
            }
        } else if let Some(title) = line.strip_prefix("### ") {
            let names = quoted(title);
            Some(if !names.is_empty() {
                Topic::Keys(names)
            } else if section == Some("keys") {
                Topic::Bindings
            } else {
                Topic::Other
            })
        } else {
            None
        };
        match heading {
            Some(topic) => headings.push(Heading {
                section,
                topic,
                text: Vec::new(),
            }),
            None => {
                if let Some(heading) = headings.last_mut() {
                    heading.text.push(line);
                }
            }
        }
    }
    headings
}

/// The cells of a row of a table of the guide.
fn cells(row: &str) -> Vec<&str> {
    row.trim_matches('|').split('|').map(str::trim).collect()
}

/// What the lines of a heading's `text` tell, briefly: its first
/// paragraph, with the list that it opens, before any table; its values
/// and its default; and the defaults of the keys' bindings. The rest stays
/// in the guide.
fn described(text: &[&str]) -> Vec<String> {
    let mut paragraph = Vec::new();
    let mut items = Vec::new();
    let mut defaults = Vec::new();
    let mut table = Vec::new();
    let mut ended = false;
    let mut fenced = false;
    for &line in text {
        if line.starts_with("```") {
            fenced = !fenced;
        } else if fenced || line.starts_with("<sub>") {
        } else if line.starts_with('|') {
            table.push(cells(line));
            ended = true;
        } else if let Some(default) = line.strip_prefix("- **Default") {
            defaults.push(format!("Default{default}"));
        } else if line.is_empty() {
            ended |= !paragraph.is_empty();
        } else if !ended && !line.starts_with(['-', ' ']) {
            paragraph.push(line);
        } else if table.is_empty()
            && line.starts_with("- ")
            && paragraph.last().is_some_and(|last| last.ends_with(':'))
        {
            items.push(line.to_owned());
        }
    }
    let mut lines = vec![paragraph.join(" ")];
    lines.extend(items);
    // A table's header, its rule, and its rows.
    match table.first().map(Vec::as_slice) {
        Some([values, "Default"]) => {
            for row in table.iter().skip(2) {
                if let [taken, default] = row.as_slice() {
                    lines.push(format!("{values}: {taken}"));
                    lines.push(format!("Default: {default}"));
                }
            }
        }
        Some(["Keys", ..]) => {
            lines.push("The defaults:".to_owned());
            for row in table.iter().skip(2) {
                if let [keys, command, does] = row.as_slice() {
                    let unquoted = |cell: &str| cell.trim_matches('`').to_owned();
                    lines.push(format!(
                        "  \"{}\" = \"{}\"   ({does})",
                        unquoted(keys),
                        unquoted(command)
                    ));
                }
            }
        }
        _ => {}
    }
    lines.extend(defaults);
    lines.retain(|line| !line.is_empty());
    lines.iter().map(|line| line.replace("**", "")).collect()
}

/// `text` in lines of `width` characters at most, broken between words.
fn wrapped(text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        if !line.is_empty() && line.chars().count() + 1 + word.chars().count() > width {
            lines.push(std::mem::take(&mut line));
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(word);
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}

/// Writes `lines` as comments, wrapped, an item of a list under its own
/// start, but for those that a space starts, which keep their own.
fn comment(out: &mut String, lines: &[String]) {
    for line in lines {
        if line.starts_with(' ') {
            let _ = writeln!(out, "#{line}");
        } else if let Some(item) = line.strip_prefix("- ") {
            for (i, line) in wrapped(item, WIDTH - 4).iter().enumerate() {
                let _ = writeln!(out, "# {} {line}", if i == 0 { '-' } else { ' ' });
            }
        } else {
            for line in wrapped(line, WIDTH - 2) {
                let _ = writeln!(out, "# {line}");
            }
        }
    }
}

/// Parts the text to come from what `out` has with an empty line, but at
/// the start of the file or of a section, or after one already.
fn part(out: &mut String) {
    let last = out.trim_end_matches('\n').lines().last();
    if !out.is_empty() && !out.ends_with("\n\n") && !last.is_some_and(|line| line.starts_with('['))
    {
        out.push('\n');
    }
}

/// `value` as the file writes it: a number without a fraction as a whole
/// one, as the guide writes them, which reads back as the same.
fn shown(value: &Value) -> String {
    let mut value = value.clone();
    value.decor_mut().clear();
    match value.as_float() {
        Some(number) if number.fract() == 0.0 && number.abs() < 1e15 => {
            // Within the integers that an `f64` holds exactly.
            format!("{}", number as i64)
        }
        _ => value.to_string(),
    }
}

/// Writes the keys `known` of `table`, the values of `section`, in the
/// order of the guide's headings, with their text when `docs` asks for it;
/// a key that has no value, for a default that the file cannot write, as
/// a comment.
fn write_keys(
    out: &mut String,
    table: &InlineTable,
    section: Option<&str>,
    known: &[&str],
    headings: &[Heading<'_>],
    docs: bool,
) {
    let write = |out: &mut String, key: &str| {
        let _ = match table.get(key) {
            Some(value) => writeln!(out, "{key} = {}", shown(value)),
            None => writeln!(out, "# {key}: unset, for its default"),
        };
    };
    let mut written = Vec::new();
    for heading in headings.iter().filter(|heading| heading.section == section) {
        let Topic::Keys(names) = &heading.topic else {
            continue;
        };
        let names: Vec<&str> = names
            .iter()
            .copied()
            .filter(|name| known.contains(name) && !written.contains(name))
            .collect();
        if names.is_empty() {
            continue;
        }
        if docs {
            part(out);
            comment(out, &described(&heading.text));
        }
        for name in names {
            write(out, name);
            written.push(name);
        }
    }
    for &key in known.iter().filter(|key| !written.contains(key)) {
        write(out, key);
    }
}

/// Writes the text of `section`'s own heading, and of its bindings'.
fn write_section_text(out: &mut String, section: &str, headings: &[Heading<'_>]) {
    for heading in headings.iter().filter(|heading| {
        heading.section == Some(section)
            && matches!(heading.topic, Topic::Section | Topic::Bindings)
    }) {
        part(out);
        comment(out, &described(&heading.text));
    }
}

impl Config {
    /// The config as a file writes it: every key, with its value, in the
    /// guide's order; with `docs`, after the text of each, which tells
    /// what it does, its values and its default. It reads back as this
    /// same config.
    pub fn written(&self, docs: bool) -> String {
        let root = self
            .serialize(toml_edit::ser::ValueSerializer::new())
            .expect("the config writes as TOML");
        let root = root.as_inline_table().expect("the config is a table");
        let headings = headings(GUIDE);
        let sections = sections();
        let mut out = String::new();
        if docs {
            comment(
                &mut out,
                &[format!(
                    "Every key of Lokyr's config, with its value; the guide tells more of each: {GUIDE_URL}"
                )],
            );
            out.push('\n');
        }
        let top: Vec<&str> = keys::<Config>()
            .iter()
            .copied()
            .filter(|key| root.get(key).is_some_and(|value| !value.is_inline_table()))
            .collect();
        write_keys(&mut out, root, None, &top, &headings, docs);
        let empty = InlineTable::new();
        for (section, known) in sections {
            let table = root
                .get(section)
                .and_then(Value::as_inline_table)
                .unwrap_or(&empty);
            let _ = writeln!(out, "\n[{section}]");
            if docs {
                write_section_text(&mut out, section, &headings);
            }
            write_keys(&mut out, table, Some(section), known, &headings, docs);
        }
        out.push_str("\n[keys]\n");
        if docs {
            write_section_text(&mut out, "keys", &headings);
        }
        if let Some(bindings) = root.get("keys").and_then(Value::as_inline_table) {
            for (keys, value) in bindings {
                let mut value = value.clone();
                value.decor_mut().clear();
                let _ = writeln!(out, "{} = {value}", Key::new(keys));
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    fn parse(text: &str) -> Config {
        Config::parse(text, Path::new("config.toml"))
            .unwrap_or_else(|error| panic!("{error}\n{text}"))
    }

    #[test]
    fn the_written_defaults_read_back_as_the_defaults() {
        let defaults = Config::default();
        for docs in [false, true] {
            let written = defaults.written(docs);
            assert_eq!(parse(&written), defaults, "{written}");
        }
        let written = defaults.written(false);
        assert!(written.starts_with("config_version = 1\n\n[font]\n"));
        assert!(written.contains("\n[font]\n# family: unset, for its default\nsize = 13\n"));
        assert!(written.contains("\n[window]\nclose-on-exit = \"clean\"\npadding-x = 4\n"));
        assert!(written.contains("\nlimit = 10\nbudget = 100\n"));
        assert!(
            written.ends_with(
                "\n[app]\nicon = \"jormungandr\"\nlanguage = \"system\"\nappearance = \"system\"\n\n[keys]\n"
            )
        );
        // Without the docs, the only comments are of the keys left unset.
        assert!(
            written
                .lines()
                .filter(|line| line.starts_with('#'))
                .all(|line| line.ends_with(": unset, for its default"))
        );
    }

    #[test]
    fn a_written_config_reads_back_as_itself() {
        let guide = GUIDE
            .split_once("### A complete example")
            .and_then(|(_, rest)| rest.split_once("```toml\n"))
            .and_then(|(_, rest)| rest.split_once("```"))
            .map(|(example, _)| example)
            .expect("the guide has a complete example");
        let mut config = parse(guide);
        config.keys.bindings.insert(
            "cmd+t".to_owned(),
            parse(
                "[keys]\n\"cmd+t\" = [{ command = \"tabs.goto\", args = { index = 1, wrap = true, step = 0.5, to = \"x\" } }, { command = \"tabs.new\", when = \"selection\" }]\n",
            )
            .keys
            .bindings["cmd+t"]
            .clone(),
        );
        assert_ne!(config, Config::default());
        for docs in [false, true] {
            let written = config.written(docs);
            assert_eq!(parse(&written), config, "{written}");
        }
        let written = config.written(false);
        assert!(
            written.contains("\nfamily = \"CaskaydiaCove NFM\"\n"),
            "{written}"
        );
        assert!(written.contains("\n\"cmd+q\" = false\n"), "{written}");
        assert!(
            written.contains(
                "\n\"ctrl+a>n\" = { command = \"window.new\", when = \"!alternate-screen\" }\n"
            ),
            "{written}"
        );
    }

    #[test]
    fn the_docs_tell_each_key_from_the_guide() {
        let written = Config::default().written(true);
        assert!(
            written.contains(
                "# The size of the text, in points.\n# Values: from `4` to `200`, whole or not\n# Default: `13`\nsize = 13\n"
            ),
            "{written}"
        );
        // Keys that share a heading share its text, once.
        assert!(written.contains("# Default of `bold-italic-style`: the family's italic face"));
        assert_eq!(
            written
                .matches("# The faces of bold, italic and bold italic text")
                .count(),
            1
        );
        assert!(written.contains("# family: unset, for its default"));
        // The list that a paragraph opens, and only the paragraph before a
        // table.
        assert!(written.contains(
            "rather than taking them from the font:\n# - `builtin-box-drawing`: the box drawing"
        ));
        assert!(written.contains("# The shape of the cursor.\n# Values: `\"block\"`"));
        assert!(!written.contains("Programs can choose among the first three"));
        // Blocks part with one empty line, none under a section's name.
        assert!(!written.contains("\n\n\n"));
        assert!(written.contains("\n\n[window]\n# What a window does when its program ends"));
        assert!(written.contains("# Default: the system's monospaced font, SF Mono.\n"));
        // A section's own text, and the bindings' defaults.
        assert!(written.contains("[terminal]\n# What programs are told of the terminal"));
        assert!(
            written.contains("# The defaults:\n#  \"cmd+n\" = \"window.new\"   (Opens a window)\n")
        );
        assert!(!written.contains("<sub>"));
        assert!(!written.contains("**"));
        for line in written.lines() {
            assert!(
                line.chars().count() <= WIDTH || !line.contains(' ') || line.starts_with("#  "),
                "a line too long: {line}"
            );
        }
    }

    #[test]
    fn every_key_has_its_heading_in_the_guide() {
        let headings = headings(GUIDE);
        let documented = |section: Option<&str>, key: &str| {
            headings.iter().any(|heading| {
                heading.section == section
                    && matches!(&heading.topic, Topic::Keys(names) if names.contains(&key))
            })
        };
        // Each heading of keys opens with what they are.
        for heading in &headings {
            if let Topic::Keys(names) = &heading.topic {
                let first = described(&heading.text).into_iter().next();
                assert!(
                    first.is_some_and(|first| !first.starts_with("Value")),
                    "{names:?} open with what they are"
                );
            }
        }
        assert!(documented(None, "config_version"));
        for (section, keys) in sections() {
            for key in keys {
                assert!(documented(Some(section), key), "`{key}` of `[{section}]`");
            }
        }
        assert!(
            headings
                .iter()
                .any(|heading| heading.section == Some("keys") && heading.topic == Topic::Bindings)
        );
    }

    #[test]
    fn the_text_is_the_paragraph_before_the_table() {
        let text = [
            "<sub>Platforms: macOS</sub>",
            "",
            "| Values | Default |",
            "|---|---|",
            "| `1`, `2` | `1` |",
            "",
            "A paragraph after the table.",
            "",
            "- **Default**: as the table says.",
        ];
        assert_eq!(
            described(&text),
            [
                "Values: `1`, `2`",
                "Default: `1`",
                "Default: as the table says."
            ]
        );
        let text = [
            "Two lines",
            "of one paragraph:",
            "",
            "- an item;",
            "  - not its own",
            "- another.",
            "",
            "Another paragraph.",
        ];
        assert_eq!(
            described(&text),
            ["Two lines of one paragraph:", "- an item;", "- another."]
        );
    }

    #[test]
    fn keys_without_a_heading_come_after_those_with_one() {
        let mut table = InlineTable::new();
        table.insert("b", Value::from(1.5));
        let mut out = String::new();
        write_keys(&mut out, &table, Some("x"), &["a", "b"], &[], true);
        assert_eq!(out, "# a: unset, for its default\nb = 1.5\n");
    }

    #[test]
    fn text_wraps_between_words() {
        assert_eq!(wrapped("one two three", 7), ["one two", "three"]);
        assert_eq!(wrapped("a verylongword b", 4), ["a", "verylongword", "b"]);
        assert!(wrapped("  ", 10).is_empty());
    }
}
