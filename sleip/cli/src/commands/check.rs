//! `sleip check`: the plugin of the current folder checked, as the
//! registry's CI checks it before it publishes it: its manifest; the
//! capabilities that the app takes; the plugin API's version, this
//! Sleip's; its catalogs, each that parses, and the titles of its commands
//! in English; its package's layout and sizes; and its sources' types,
//! against the plugin API's definitions that this Sleip holds, of Luau's
//! type checker. Each problem of its file, line and column; a note of what
//! is not wrong but worth knowing. A failure of a problem found.

use std::path::Path;

use lokyr_pkg_model::manifest::{Kind, Manifest};
use serde_json::json;
use sleip_analyzer::Kind as ProblemKind;

use crate::output::Out;

/// The plugin API's definitions, this Sleip's.
pub(super) const DEFINITIONS: &str =
    include_str!("../../../../crates/plugin-host-luau/luau/lokyr.d.luau");

/// The problems and the notes told so far.
struct Findings {
    out: Out,
    problems: usize,
}

impl Findings {
    /// Tells a problem of `file`, at `line` and `column` if given, of
    /// `kind`, as `message` says.
    fn problem(&mut self, file: &str, at: Option<(u32, u32)>, kind: &str, message: &str) {
        self.problems += 1;
        let (line, column) = at.unwrap_or((0, 0));
        let place = match at {
            Some((line, column)) => format!("{file}:{line}:{column}"),
            None => file.to_owned(),
        };
        self.out.tell(
            "problem",
            json!({ "file": file, "line": line, "column": column, "kind": kind, "message": message }),
            &format!("{place}: {kind}: {message}"),
        );
    }

    /// Tells a note of `file`, as `message` says.
    fn note(&self, file: &str, message: &str) {
        self.out.tell(
            "note",
            json!({ "file": file, "message": message }),
            &format!("{file}: note: {message}"),
        );
    }
}

/// Checks the plugin of the current folder, as `out` tells.
pub fn check(out: Out) -> Result<(), String> {
    let folder = std::env::current_dir()
        .map_err(|error| format!("the current folder cannot be read: {error}"))?;
    let problems = problems_in(&folder, out);
    out.tell("done", json!({ "problems": problems }), &summary(problems));
    // The summary told the problems already.
    if problems == 0 {
        Ok(())
    } else {
        Err(String::new())
    }
}

/// Checks the plugin in `folder`, each problem and note told as `out`
/// tells: how many problems it found.
pub(super) fn problems_in(folder: &Path, out: Out) -> usize {
    let mut findings = Findings { out, problems: 0 };
    check_folder(folder, &mut findings);
    findings.problems
}

/// The text of `problems` found.
pub(super) fn summary(problems: usize) -> String {
    match problems {
        0 => "No problem found.".to_owned(),
        1 => "1 problem found.".to_owned(),
        count => format!("{count} problems found."),
    }
}

/// Checks the plugin in `folder`, into `findings`.
fn check_folder(folder: &Path, findings: &mut Findings) {
    let text = match std::fs::read_to_string(folder.join("plugin.toml")) {
        Ok(text) => text,
        Err(error) => {
            findings.problem(
                "plugin.toml",
                None,
                "Manifest",
                &format!("it cannot be read, as no plugin is in this folder or the file is not there: {error}"),
            );
            return;
        }
    };
    let manifest = match Manifest::parse(&text) {
        Ok(manifest) => manifest,
        Err(error) => {
            let at = error
                .at
                .map(|(line, column)| (to_u32(line), to_u32(column)));
            findings.problem("plugin.toml", at, "Manifest", &error.message);
            return;
        }
    };
    // What the app refuses, at the line that asks for it.
    for capability in manifest
        .capabilities
        .iter()
        .filter(|capability| capability.is_withheld())
    {
        let written = capability.to_string();
        let key = written.split(':').next().unwrap_or(&written);
        findings.problem(
            "plugin.toml",
            line_of_key(&text, key),
            "Capability",
            &format!("the app refuses `{written}` before Lokyr 1.0.0, as plugins reach no network nor processes yet"),
        );
    }
    let api = lokyr_plugin_sdk::API_VERSION;
    if !manifest.takes_api(api) {
        findings.problem(
            "plugin.toml",
            line_of_key(&text, "api"),
            "Api",
            &format!(
                "it asks for the plugin API `{}`, and this Sleip's is {api}",
                manifest.api.clone().unwrap_or_default()
            ),
        );
    }
    check_catalogs(folder, &manifest, findings);
    if let Err(error) = lokyr_pkg_model::archive::pack(folder) {
        findings.problem(".", None, "Package", &error.to_string());
    }
    if manifest.kind == Kind::Luau {
        check_sources(folder, findings);
    }
    if let Ok(copy) = std::fs::read_to_string(folder.join("lokyr.d.luau"))
        && copy != DEFINITIONS
    {
        findings.note(
            "lokyr.d.luau",
            "it is not this Sleip's, whose types the check takes: the editor's types may differ",
        );
    }
}

/// Checks the catalogs of the plugin of `manifest` in `folder`: each that
/// parses, of the problems' lines, and the English one of each command's
/// title.
fn check_catalogs(folder: &Path, manifest: &Manifest, findings: &mut Findings) {
    let locales = folder.join("locales");
    let mut english = String::new();
    let mut languages: Vec<_> = std::fs::read_dir(&locales)
        .map(|entries| entries.flatten().map(|entry| entry.path()).collect())
        .unwrap_or_default();
    languages.sort();
    for language in languages.iter().filter(|path| path.is_dir()) {
        let mut catalogs: Vec<_> = std::fs::read_dir(language)
            .map(|entries| entries.flatten().map(|entry| entry.path()).collect())
            .unwrap_or_default();
        catalogs.sort();
        for catalog in catalogs
            .iter()
            .filter(|path| path.extension().is_some_and(|extension| extension == "ftl"))
        {
            let file = catalog
                .strip_prefix(folder)
                .unwrap_or(catalog)
                .display()
                .to_string();
            let Ok(text) = std::fs::read_to_string(catalog) else {
                findings.problem(&file, None, "Catalog", "it cannot be read as UTF-8");
                continue;
            };
            if let Err((_, errors)) = fluent_syntax::parser::parse(text.as_str()) {
                for error in errors {
                    let at = line_and_column(&text, error.pos.start);
                    findings.problem(&file, Some(at), "Catalog", &format!("{}", error.kind));
                }
            }
            if language.file_name().is_some_and(|name| name == "en") {
                english.push_str(&text);
                english.push('\n');
            }
        }
    }
    if !languages.iter().any(|path| path.ends_with("en")) {
        findings.note(
            "locales",
            "no catalog in English, `locales/en/*.ftl`, which the other languages fall back to",
        );
        return;
    }
    for command in &manifest.contributes.commands {
        let message = format!("command-{}", command.name.replace('.', "-"));
        let held = english.lines().any(|line| {
            line.split('=')
                .next()
                .is_some_and(|id| id.trim() == message)
        });
        if !held {
            findings.note(
                "locales/en",
                &format!(
                    "no `{message}`, the title of `{}`, which takes its manifest's",
                    command.name
                ),
            );
        }
    }
}

/// Checks the types of the sources of the plugin in `folder`, `src/**/*.luau`,
/// and of its tests, `tests/**/*.luau`, against the plugin API's
/// definitions, of `TestHost` among them.
fn check_sources(folder: &Path, findings: &mut Findings) {
    let (sources, unread) = super::luau_sources(folder);
    for (name, error) in unread {
        findings.problem(
            &name,
            None,
            "Source",
            &format!("it cannot be read: {error}"),
        );
    }
    for problem in sleip_analyzer::check(&sources, DEFINITIONS) {
        let kind = match problem.kind {
            ProblemKind::Syntax => "SyntaxError",
            ProblemKind::Type => "TypeError",
            ProblemKind::Internal => "InternalError",
        };
        findings.problem(
            &problem.file,
            Some((problem.line, problem.column)),
            kind,
            &problem.message,
        );
    }
}

/// The line, from 1, and the column 1 of the first line of `text` that sets
/// `key`, as `key = …`.
fn line_of_key(text: &str, key: &str) -> Option<(u32, u32)> {
    text.lines()
        .position(|line| {
            line.trim_start()
                .strip_prefix(key)
                .is_some_and(|rest| rest.trim_start().starts_with('='))
        })
        .map(|line| (to_u32(line + 1), 1))
}

/// The line and the column, both from 1, of the byte `at` of `text`.
fn line_and_column(text: &str, at: usize) -> (u32, u32) {
    let before = &text[..text.floor_char_boundary(at)];
    let line = before.matches('\n').count() + 1;
    let column = before
        .rsplit('\n')
        .next()
        .map_or(0, |last| last.chars().count())
        + 1;
    (to_u32(line), to_u32(column))
}

/// `value` as a `u32`, of the largest that fits.
fn to_u32(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_key_s_line_is_the_line_that_sets_it() {
        let text = "apis = 1\n  api  = \"^0.1\"\n";
        assert_eq!(line_of_key(text, "api"), Some((2, 1)));
        assert_eq!(line_of_key(text, "net"), None);
    }

    #[test]
    fn a_byte_s_line_and_column_count_from_one_of_characters() {
        let text = "ab\nçd\n";
        assert_eq!(line_and_column(text, 0), (1, 1));
        assert_eq!(line_and_column(text, 5), (2, 2));
        assert_eq!(line_and_column(text, 4), (2, 1));
        assert_eq!(line_and_column(text, 99), (3, 1));
    }
}
