//! A new language of Lokyr's texts, by one command, and the check of a
//! language's catalogs against English's.
//!
//! `new-language <code>` copies every English catalog, each `locales/en`
//! folder of the kernel, the app and the built-in plugins, to the language's
//! folder, adds the language to each list of catalogs that the binary holds,
//! of its array's size, and to the configuration guide's `language` key.
//! `check-language <code>` tells each catalog's messages missing, those that
//! English has not, those of other variables, and those left as English has
//! them, which a catalog copied and not translated yet has: the first three
//! fail it.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context as _, bail};
use lokyr_kernel::l10n;

/// The folders of the workspace that hold catalogs.
const ROOTS: [&str; 2] = ["crates", "plugins"];

/// The configuration guide, whose `language` key lists the languages.
const GUIDE: &str = "docs/CONFIGURATION.md";

/// The languages of the guide's `language` key, on the row of its values.
const GUIDE_ROW: &str = "| `\"system\"`, ";

/// `cargo xtask new-language <code>`.
pub fn run_new(mut args: impl Iterator<Item = String>) -> anyhow::Result<ExitCode> {
    let Some(code) = args.next() else {
        bail!("usage: cargo xtask new-language <code>, as `fr`");
    };
    let made = new_language(&root(), &code)?;
    println!(
        "new-language: `{code}` made, of {} catalogs copied and {} lists of catalogs; translate the files of its folders, then `cargo xtask check-language {code}`",
        made.copied.len(),
        made.lists.len()
    );
    Ok(ExitCode::SUCCESS)
}

/// `cargo xtask check-language <code>`.
pub fn run_check(mut args: impl Iterator<Item = String>) -> anyhow::Result<ExitCode> {
    let Some(code) = args.next() else {
        bail!("usage: cargo xtask check-language <code>, as `fr`");
    };
    let report = check_language(&root(), &code)?;
    print!("{}", report.text);
    Ok(if report.holds {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}

/// The workspace's root.
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// What a new language made: the catalogs copied, and the source files of
/// the lists of catalogs that took it.
#[derive(Debug, Default)]
pub struct Made {
    pub copied: Vec<PathBuf>,
    pub lists: Vec<PathBuf>,
}

/// Whether `code` is a language's code as Lokyr takes it: two or three
/// lowercase letters, of no region.
fn is_code(code: &str) -> bool {
    (2..=3).contains(&code.len()) && code.chars().all(|c| c.is_ascii_lowercase())
}

/// The `locales/en` folders of the workspace at `root` whose catalogs a
/// list of catalogs holds, in their paths' order: Lokyr's own, not a
/// template's of a plugin of an author's.
fn english_folders(root: &Path) -> anyhow::Result<Vec<PathBuf>> {
    let mut found = Vec::new();
    for list in catalog_lists(root)? {
        let text = std::fs::read_to_string(&list)?;
        let folder = list.parent().unwrap_or(root);
        for (prefix, _) in text.lines().filter_map(english_entry) {
            let english = folder.join(&prefix).join("locales/en");
            let english = std::fs::canonicalize(&english).unwrap_or(english);
            if !found.contains(&english) {
                found.push(english);
            }
        }
    }
    found.sort();
    Ok(found)
}

/// The Rust sources of the workspace at `root` that hold a list of
/// catalogs: an array of `[(&str, &str); N]` of an English catalog's
/// `include_str!`.
fn catalog_lists(root: &Path) -> anyhow::Result<Vec<PathBuf>> {
    let mut found = Vec::new();
    let mut left: Vec<PathBuf> = ROOTS.iter().map(|folder| root.join(folder)).collect();
    while let Some(folder) = left.pop() {
        let Ok(entries) = std::fs::read_dir(&folder) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if path.file_name().is_some_and(|name| name != "target") {
                    left.push(path);
                }
                continue;
            }
            if path.extension().is_none_or(|extension| extension != "rs") {
                continue;
            }
            let text = std::fs::read_to_string(&path)
                .with_context(|| format!("{} cannot be read", path.display()))?;
            if text.contains("[(&str, &str); ") && text.contains("/locales/en/") {
                found.push(path);
            }
        }
    }
    found.sort();
    Ok(found)
}

/// `text`, a Rust source, of `code` in each of its lists of catalogs: after
/// each catalog's last language, a line of its file of `code`'s folder, and
/// after a list's last `l10n::KERNEL[i]`, the next, each array's size of
/// the lines that it took; `None` where it holds no list.
fn with_language(text: &str, code: &str) -> Option<String> {
    let lines: Vec<&str> = text.lines().collect();
    let mut out: Vec<String> = Vec::with_capacity(lines.len() + 8);
    let mut changed = false;
    let mut at = 0;
    while at < lines.len() {
        let line = lines[at];
        let Some((head, size)) = array_head(line) else {
            out.push(line.to_owned());
            at += 1;
            continue;
        };
        // The array's entries, to its closing line.
        let end = (at + 1..lines.len()).find(|&end| lines[end].trim_start().starts_with("];"))?;
        let entries = &lines[at + 1..end];
        let mut taken: Vec<String> = entries.iter().map(|entry| (*entry).to_owned()).collect();
        let mut added = 0;
        // Each English catalog's line, the new language after the last line
        // of its file.
        let english: Vec<(String, String)> = entries
            .iter()
            .filter_map(|entry| english_entry(entry))
            .collect();
        for (prefix, file) in english {
            let last = taken.iter().rposition(|entry| {
                entry.contains("/locales/") && entry.ends_with(&format!("/{file}\")),"))
            })?;
            let indent: String = taken[last]
                .chars()
                .take_while(|c| c.is_whitespace())
                .collect();
            taken.insert(
                last + 1,
                format!("{indent}(\"{code}\", include_str!(\"{prefix}/locales/{code}/{file}\")),"),
            );
            added += 1;
        }
        // The kernel's catalogs that a list takes of their places.
        if let Some(last) = taken
            .iter()
            .rposition(|entry| entry.trim_start().starts_with("l10n::KERNEL["))
        {
            let place: usize = taken[last]
                .trim()
                .trim_start_matches("l10n::KERNEL[")
                .trim_end_matches("],")
                .parse()
                .ok()?;
            let indent: String = taken[last]
                .chars()
                .take_while(|c| c.is_whitespace())
                .collect();
            taken.insert(last + 1, format!("{indent}l10n::KERNEL[{}],", place + 1));
            added += 1;
        }
        if added == 0 {
            out.push(line.to_owned());
            at += 1;
            continue;
        }
        changed = true;
        out.push(format!("{head}[(&str, &str); {}] = [", size + added));
        out.extend(taken);
        out.push(lines[end].to_owned());
        at = end + 1;
    }
    let mut joined = out.join("\n");
    if text.ends_with('\n') {
        joined.push('\n');
    }
    changed.then_some(joined)
}

/// The head of a list of catalogs' declaration, before its type, and its
/// size, as `pub const KERNEL: [(&str, &str); 2] = [`.
fn array_head(line: &str) -> Option<(&str, usize)> {
    let at = line.find("[(&str, &str); ")?;
    let rest = &line[at + "[(&str, &str); ".len()..];
    let size = rest.split(']').next()?.parse().ok()?;
    rest.ends_with("] = [").then(|| (&line[..at], size))
}

/// The prefix of the path of an English catalog's entry, before
/// `/locales/en/`, and its file's name, as `("en",
/// include_str!("../locales/en/kernel.ftl")),`.
fn english_entry(entry: &str) -> Option<(String, String)> {
    let entry = entry.trim();
    let path = entry
        .strip_prefix("(\"en\", include_str!(\"")?
        .strip_suffix("\")),")?;
    let (prefix, file) = path.split_once("/locales/en/")?;
    Some((prefix.to_owned(), file.to_owned()))
}

/// The guide's `language` row of `text`, of `code` among its values.
fn guide_with_language(text: &str, code: &str) -> Option<String> {
    let start = text.find("### `language`")?;
    let row_start = start + text[start..].find(GUIDE_ROW)?;
    let row_end = row_start + text[row_start..].find(" | ")?;
    let row = &text[row_start..row_end];
    Some(format!(
        "{}{row}, `\"{code}\"`{}",
        &text[..row_start],
        &text[row_end..]
    ))
}

/// Makes the language `code` in the workspace at `root`: its catalogs,
/// copies of English's, its lists of catalogs and the guide's key.
///
/// # Errors
///
/// A code that is none, a language that is there, or a file that cannot be
/// read or written.
pub fn new_language(root: &Path, code: &str) -> anyhow::Result<Made> {
    if !is_code(code) {
        bail!("`{code}` is no language's code: two or three lowercase letters, as `fr`");
    }
    let folders = english_folders(root)?;
    if folders.is_empty() {
        bail!("no English catalog under {}", root.display());
    }
    if let Some(there) = folders
        .iter()
        .map(|english| english.with_file_name(code))
        .find(|folder| folder.exists())
    {
        bail!(
            "the language `{code}` is there already: {}",
            there.display()
        );
    }
    // Each list first, which a language that one names already refuses.
    let mut edits = Vec::new();
    for list in catalog_lists(root)? {
        let text = std::fs::read_to_string(&list)?;
        if text.contains(&format!("(\"{code}\", include_str!(")) {
            bail!("the language `{code}` is there already: {}", list.display());
        }
        if let Some(edited) = with_language(&text, code) {
            edits.push((list, edited));
        }
    }
    let mut made = Made::default();
    for english in &folders {
        let folder = english.with_file_name(code);
        std::fs::create_dir_all(&folder)?;
        for entry in std::fs::read_dir(english)?.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|extension| extension == "ftl") {
                let copy = folder.join(entry.file_name());
                std::fs::copy(&path, &copy)?;
                made.copied.push(copy);
            }
        }
    }
    for (list, edited) in edits {
        std::fs::write(&list, edited)?;
        made.lists.push(list);
    }
    let guide = root.join(GUIDE);
    if let Ok(text) = std::fs::read_to_string(&guide)
        && let Some(edited) = guide_with_language(&text, code)
    {
        std::fs::write(&guide, edited)?;
    }
    made.copied.sort();
    Ok(made)
}

/// What a language's check found: its text, and whether its catalogs take
/// English's place.
#[derive(Debug)]
pub struct Report {
    pub text: String,
    pub holds: bool,
}

/// Checks the catalogs of the language `code` in the workspace at `root`
/// against English's.
///
/// # Errors
///
/// A code that is none, a language that is not there, or a catalog that
/// cannot be read.
pub fn check_language(root: &Path, code: &str) -> anyhow::Result<Report> {
    if !is_code(code) {
        bail!("`{code}` is no language's code: two or three lowercase letters, as `fr`");
    }
    let mut text = String::new();
    let (mut holds, mut found, mut untranslated) = (true, false, 0);
    for english in english_folders(root)? {
        let folder = english.with_file_name(code);
        let mut files: Vec<PathBuf> = std::fs::read_dir(&english)?
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.extension().is_some_and(|extension| extension == "ftl"))
            .collect();
        files.sort();
        for source in files {
            let other = folder.join(source.file_name().unwrap_or_default());
            let shown = other
                .strip_prefix(root)
                .unwrap_or(&other)
                .display()
                .to_string();
            let Ok(theirs) = std::fs::read_to_string(&other) else {
                holds = false;
                let _ = writeln!(text, "{shown}: missing");
                continue;
            };
            found = true;
            let comparison = l10n::compare(&std::fs::read_to_string(&source)?, &theirs)
                .map_err(|error| anyhow::anyhow!("{shown}: {error}"))?;
            holds &= comparison.holds();
            untranslated += comparison.untranslated.len();
            for (what, ids) in [
                ("missing", &comparison.missing),
                ("not in English", &comparison.extra),
                ("of other variables than English's", &comparison.variables),
                ("as English has it", &comparison.untranslated),
            ] {
                if !ids.is_empty() {
                    let _ = writeln!(text, "{shown}: {what}: {}", ids.join(", "));
                }
            }
        }
    }
    if !found {
        bail!("the language `{code}` has no catalog: `cargo xtask new-language {code}` makes it");
    }
    let _ = writeln!(
        text,
        "check-language: `{code}` {}; {untranslated} messages as English has them",
        if holds {
            "takes English's place"
        } else {
            "lacks messages or holds others"
        }
    );
    Ok(Report { text, holds })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A workspace of a crate and a plugin, each of English's and Italian's
    /// catalogs and its list of them, and of the guide's key.
    fn workspace(name: &str) -> PathBuf {
        let root =
            std::env::temp_dir().join(format!("xtask-languages-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for (folder, file) in [
            ("crates/kernel", "kernel.ftl"),
            ("plugins/tabs", "tabs.ftl"),
        ] {
            for language in ["en", "it"] {
                let locales = root.join(folder).join("locales").join(language);
                std::fs::create_dir_all(&locales).unwrap();
                let text = if language == "en" {
                    "hello = Hello\nbye = Bye, { $name }\n"
                } else {
                    "hello = Ciao\nbye = Ciao, { $name }\n"
                };
                std::fs::write(locales.join(file), text).unwrap();
            }
            std::fs::create_dir_all(root.join(folder).join("src")).unwrap();
            std::fs::write(
                root.join(folder).join("src/lib.rs"),
                format!(
                    "/// The catalogs.\npub const CATALOGS: [(&str, &str); 2] = [\n    (\"en\", include_str!(\"../locales/en/{file}\")),\n    (\"it\", include_str!(\"../locales/it/{file}\")),\n];\n"
                ),
            )
            .unwrap();
        }
        std::fs::create_dir_all(root.join("crates/app/src")).unwrap();
        std::fs::write(
            root.join("crates/app/src/app.rs"),
            "pub(crate) const CATALOGS: [(&str, &str); 4] = [\n    l10n::KERNEL[0],\n    l10n::KERNEL[1],\n    (\"en\", include_str!(\"../locales/en/app.ftl\")),\n    (\"it\", include_str!(\"../locales/it/app.ftl\")),\n];\n",
        )
        .unwrap();
        for language in ["en", "it"] {
            let locales = root.join("crates/app/locales").join(language);
            std::fs::create_dir_all(&locales).unwrap();
            std::fs::write(locales.join("app.ftl"), "quit = Quit\n").unwrap();
        }
        std::fs::create_dir_all(root.join("docs")).unwrap();
        std::fs::write(
            root.join(GUIDE),
            "### `language`\n\n| Values | Default |\n|---|---|\n| `\"system\"`, `\"en\"`, `\"it\"` | `\"system\"` |\n",
        )
        .unwrap();
        root
    }

    #[test]
    fn a_new_language_takes_every_catalog_and_list_and_its_check_tells_what_to_translate() {
        let root = workspace("new");
        let made = new_language(&root, "fr").unwrap();
        assert_eq!(made.copied.len(), 3);
        assert_eq!(made.lists.len(), 3);
        // Its catalogs, English's copied.
        assert_eq!(
            std::fs::read_to_string(root.join("plugins/tabs/locales/fr/tabs.ftl")).unwrap(),
            "hello = Hello\nbye = Bye, { $name }\n"
        );
        // Each list of it, of its size.
        assert_eq!(
            std::fs::read_to_string(root.join("plugins/tabs/src/lib.rs")).unwrap(),
            "/// The catalogs.\npub const CATALOGS: [(&str, &str); 3] = [\n    (\"en\", include_str!(\"../locales/en/tabs.ftl\")),\n    (\"it\", include_str!(\"../locales/it/tabs.ftl\")),\n    (\"fr\", include_str!(\"../locales/fr/tabs.ftl\")),\n];\n"
        );
        let app = std::fs::read_to_string(root.join("crates/app/src/app.rs")).unwrap();
        assert!(app.contains("[(&str, &str); 6] = ["), "{app}");
        assert!(
            app.contains("    l10n::KERNEL[1],\n    l10n::KERNEL[2],\n"),
            "{app}"
        );
        assert!(
            app.contains("(\"fr\", include_str!(\"../locales/fr/app.ftl\")),\n];"),
            "{app}"
        );
        // The guide's key.
        let guide = std::fs::read_to_string(root.join(GUIDE)).unwrap();
        assert!(
            guide.contains("`\"en\"`, `\"it\"`, `\"fr\"` | `\"system\"` |"),
            "{guide}"
        );
        // Its check: every message there, each as English has it.
        let report = check_language(&root, "fr").unwrap();
        assert!(report.holds, "{}", report.text);
        assert!(
            report.text.contains("as English has it: bye, hello"),
            "{}",
            report.text
        );
        assert!(
            report.text.contains("5 messages as English has them"),
            "{}",
            report.text
        );
        // A message translated, one taken away: the check fails.
        std::fs::write(
            root.join("plugins/tabs/locales/fr/tabs.ftl"),
            "hello = Bonjour\n",
        )
        .unwrap();
        let report = check_language(&root, "fr").unwrap();
        assert!(!report.holds);
        assert!(
            report
                .text
                .contains("plugins/tabs/locales/fr/tabs.ftl: missing: bye"),
            "{}",
            report.text
        );
        // A language that is there, or a wrong code, refused.
        assert!(
            new_language(&root, "fr")
                .unwrap_err()
                .to_string()
                .contains("there already")
        );
        assert!(
            new_language(&root, "it")
                .unwrap_err()
                .to_string()
                .contains("there already")
        );
        for wrong in ["FR", "f", "fran", "fr-CA", "../x"] {
            assert!(new_language(&root, wrong).is_err(), "{wrong}");
        }
        assert!(
            check_language(&root, "de")
                .unwrap_err()
                .to_string()
                .contains("no catalog")
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn the_workspace_s_lists_of_catalogs_each_take_a_new_language() {
        // The workspace's own, each list taking the language, not written.
        let root = root();
        let lists = catalog_lists(&root).unwrap();
        assert!(lists.len() >= 12, "{lists:?}");
        for list in &lists {
            let text = std::fs::read_to_string(list).unwrap();
            let edited = with_language(&text, "zz").unwrap_or_else(|| panic!("{}", list.display()));
            assert!(
                edited.contains("(\"zz\", include_str!("),
                "{}",
                list.display()
            );
        }
        // Every folder of English's catalogs, the app's resources too.
        let folders = english_folders(&root).unwrap();
        assert!(
            folders
                .iter()
                .any(|folder| folder.ends_with("app-core/resources/locales/en"))
        );
        assert!(folders.len() >= 12, "{folders:?}");
        let guide = std::fs::read_to_string(root.join(GUIDE)).unwrap();
        assert!(
            guide_with_language(&guide, "zz")
                .unwrap()
                .contains(", `\"zz\"` | `\"system\"` |")
        );
    }
}
