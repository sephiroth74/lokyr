//! The notices of the licenses of the crates of other projects that the
//! app's binary holds, which a distribution of the binary carries, in the
//! bundle's `Contents/Resources/licenses/crates`, a directory a crate and
//! an index: of each crate, the license that the app takes of its
//! expression, the first of `deny.toml`'s allowed ones by [`PREFERENCE`]
//! in each choice, and its own files of that license, and those of no
//! license's name, as its `COPYRIGHT` and its `NOTICE`; for a license that
//! no file of the crate's tells, its text of SPDX's list, kept in
//! `tools/xtask/licenses`, under its authors' copyright. A crate whose
//! expression allows none of `deny.toml`'s licenses fails the bundle.

use std::fmt::Write as _;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail};

use crate::sbom::{self, Component};

/// The texts of the licenses, of SPDX's list, relative to the workspace
/// root.
const TEXTS: &str = "tools/xtask/licenses";

/// The licenses that the app takes first, of a choice that allows several.
const PREFERENCE: [&str; 9] = [
    "MIT",
    "Apache-2.0",
    "BSD-3-Clause",
    "BSD-2-Clause",
    "ISC",
    "Zlib",
    "Unicode-3.0",
    "CC0-1.0",
    "Apache-2.0 WITH LLVM-exception",
];

/// The names of a crate's files of its license, by their start, in capitals.
const FILES: [&str; 6] = [
    "LICENSE",
    "LICENCE",
    "COPYING",
    "COPYRIGHT",
    "NOTICE",
    "UNLICENSE",
];

/// The licenses that a file's name tells, by a word of it, in capitals,
/// and the start of the license's name: `UNLICENSE` first, whose name holds
/// `LICENSE`.
const NAMED: [(&str, &str); 10] = [
    ("UNLICENSE", "Unlicense"),
    ("APACHE", "Apache-2.0"),
    ("MIT", "MIT"),
    ("ZLIB", "Zlib"),
    ("UNICODE", "Unicode-3.0"),
    ("BSD", "BSD-"),
    ("ISC", "ISC"),
    ("CC0", "CC0-1.0"),
    ("GPL", "GPL-"),
    ("MPL", "MPL-"),
];

/// Writes the notices of the crates that the app's binary holds into
/// `out`, a directory a crate and `README.md`, the index; how many crates.
pub(crate) fn write(root: &Path, out: &Path) -> Result<usize> {
    let allowed = allowed(root)?;
    let crates = sbom::crates(root)?;
    if out.exists() {
        fs::remove_dir_all(out).with_context(|| format!("removing {}", out.display()))?;
    }
    let mut index = String::from(concat!(
        "# The crates of other projects in Lokyr's binary\n\n",
        "Each directory holds a crate's notices of the license that Lokyr takes of its own: ",
        "its files, or the license's text under its authors' copyright where its package has none.\n\n",
        "| Crate | Version | License | Lokyr takes |\n|---|---|---|---|\n",
    ));
    for component in &crates {
        let expression = component.license.as_deref().with_context(|| {
            format!(
                "{} {} tells no license in its manifest",
                component.name, component.version
            )
        })?;
        let Some(taken) = chosen(expression, &allowed) else {
            bail!(
                "{} {} is under `{expression}`, which allows none of deny.toml's licenses",
                component.name,
                component.version
            );
        };
        let directory = out.join(format!("{}-{}", component.name, component.version));
        fs::create_dir_all(&directory)
            .with_context(|| format!("making {}", directory.display()))?;
        notices(root, component, &taken, &directory)?;
        let _ = writeln!(
            index,
            "| {} | {} | {expression} | {} |",
            component.name,
            component.version,
            taken.join(" AND ")
        );
    }
    fs::write(out.join("README.md"), index).context("writing the index of the notices")?;
    Ok(crates.len())
}

/// Writes into `directory` the notices of `component` of the licenses
/// `taken`: its files of them, and of no license's name; the texts of
/// SPDX's list of those that none of its files tells, where it has no file
/// of no license's name either.
fn notices(root: &Path, component: &Component, taken: &[String], directory: &Path) -> Result<()> {
    let mut covered = vec![false; taken.len()];
    let mut general = false;
    let entries = fs::read_dir(&component.source)
        .with_context(|| format!("reading {}", component.source.display()))?;
    let mut names: Vec<String> = entries
        .filter_map(|entry| entry.ok()?.file_name().into_string().ok())
        .filter(|name| {
            let upper = name.to_uppercase();
            FILES.iter().any(|start| upper.starts_with(start))
        })
        .collect();
    names.sort();
    for name in names {
        let keep = match named(&name) {
            None => {
                general = true;
                true
            }
            Some(start) => {
                let mut matched = false;
                for (license, covered) in taken.iter().zip(&mut covered) {
                    if license.starts_with(start) {
                        *covered = true;
                        matched = true;
                    }
                }
                matched
            }
        };
        if keep {
            let path = component.source.join(&name);
            if path.is_file() {
                fs::copy(&path, directory.join(&name))
                    .with_context(|| format!("copying {}", path.display()))?;
            }
        }
    }
    if general {
        return Ok(());
    }
    for (license, covered) in taken.iter().zip(covered) {
        if !covered {
            let text = text(root, license, &holders(component))?;
            let name = format!("LICENSE-{}", license.replace(' ', "-"));
            fs::write(directory.join(&name), text)
                .with_context(|| format!("writing {name} of {}", component.name))?;
        }
    }
    Ok(())
}

/// The license that the file `name` of a crate tells, by the start of the
/// license's name, if its name tells one.
fn named(name: &str) -> Option<&'static str> {
    let upper = name.to_uppercase();
    NAMED
        .iter()
        .find(|(word, _)| upper.contains(word))
        .map(|&(_, start)| start)
}

/// Who holds the copyright of `component`, for a text that names them: its
/// authors, or its own authors by its name.
fn holders(component: &Component) -> String {
    if component.authors.is_empty() {
        format!("the authors of {}", component.name)
    } else {
        component.authors.join(", ")
    }
}

/// The text of `license`, of SPDX's list, its copyright's line of
/// `holders`; an exception's after its license's.
fn text(root: &Path, license: &str, holders: &str) -> Result<String> {
    let read = |id: &str| {
        let path = root.join(TEXTS).join(format!("{id}.txt"));
        fs::read_to_string(&path)
            .with_context(|| format!("{license} has no text: {}", path.display()))
    };
    let mut text = match license.split_once(" WITH ") {
        Some((base, exception)) => format!("{}\n\n{}", read(base)?, read(exception)?),
        None => read(license)?,
    };
    if let Some(line) = text.lines().find(|line| line.contains("<year>")) {
        let line = line.to_owned();
        text = text.replace(&line, &format!("Copyright (c) {holders}"));
    }
    Ok(text)
}

/// The licenses that `deny.toml` allows.
fn allowed(root: &Path) -> Result<Vec<String>> {
    let path = root.join("deny.toml");
    let text = fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    let deny: toml::Value =
        toml::from_str(&text).with_context(|| format!("reading {}", path.display()))?;
    deny.get("licenses")
        .and_then(|licenses| licenses.get("allow"))
        .and_then(toml::Value::as_array)
        .context("deny.toml has no [licenses] allow")?
        .iter()
        .map(|license| {
            license
                .as_str()
                .map(str::to_owned)
                .context("a license of deny.toml is no string")
        })
        .collect()
}

/// The licenses that the app takes of `expression`, an SPDX expression as
/// Cargo's manifests write it, `/` for `OR` among them: of each choice the
/// allowed one first by [`PREFERENCE`]; `None` where a part of it allows
/// none of `allowed`.
fn chosen(expression: &str, allowed: &[String]) -> Option<Vec<String>> {
    let spaced = expression
        .replace('(', " ( ")
        .replace(')', " ) ")
        .replace('/', " OR ");
    let tokens: Vec<&str> = spaced.split_whitespace().collect();
    let mut at = 0;
    let taken = either(&tokens, &mut at, allowed)?;
    (at == tokens.len()).then_some(taken)
}

/// The rank of a license among [`PREFERENCE`], later ones and others last.
fn rank(license: &str) -> usize {
    PREFERENCE
        .iter()
        .position(|preferred| *preferred == license)
        .unwrap_or(PREFERENCE.len())
}

/// A choice of terms, `A OR B`: the term whose licenses rank first.
fn either(tokens: &[&str], at: &mut usize, allowed: &[String]) -> Option<Vec<String>> {
    let mut best: Option<Vec<String>> = None;
    loop {
        let term = both(tokens, at, allowed);
        if let Some(term) = term {
            let key = |licenses: &[String]| {
                (
                    licenses.iter().map(|license| rank(license)).max(),
                    licenses.len(),
                )
            };
            if best.as_ref().is_none_or(|best| key(&term) < key(best)) {
                best = Some(term);
            }
        }
        if tokens.get(*at) == Some(&"OR") {
            *at += 1;
        } else {
            return best;
        }
    }
}

/// A term of factors, `A AND B`: all of them, each allowed.
fn both(tokens: &[&str], at: &mut usize, allowed: &[String]) -> Option<Vec<String>> {
    let mut licenses = Vec::new();
    let mut whole = true;
    loop {
        match factor(tokens, at, allowed) {
            Some(factor) => {
                for license in factor {
                    if !licenses.contains(&license) {
                        licenses.push(license);
                    }
                }
            }
            None => whole = false,
        }
        if tokens.get(*at) == Some(&"AND") {
            *at += 1;
        } else {
            return whole.then_some(licenses);
        }
    }
}

/// A factor: an expression between parentheses, or a license, with its
/// exception after `WITH`, if allowed.
fn factor(tokens: &[&str], at: &mut usize, allowed: &[String]) -> Option<Vec<String>> {
    let token = *tokens.get(*at)?;
    *at += 1;
    if token == "(" {
        let inner = either(tokens, at, allowed);
        if tokens.get(*at) == Some(&")") {
            *at += 1;
        }
        return inner;
    }
    let mut license = token.to_owned();
    if tokens.get(*at) == Some(&"WITH") {
        let exception = tokens.get(*at + 1)?;
        license = format!("{license} WITH {exception}");
        *at += 2;
    }
    allowed.contains(&license).then(|| vec![license])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    fn allowed_list() -> Vec<String> {
        allowed(&root()).unwrap()
    }

    #[test]
    fn a_choice_takes_the_allowed_license_first_by_preference() {
        let allowed = allowed_list();
        let taken = |expression| chosen(expression, &allowed);
        assert_eq!(taken("MIT OR Apache-2.0"), Some(vec!["MIT".to_owned()]));
        assert_eq!(taken("Apache-2.0 OR MIT"), Some(vec!["MIT".to_owned()]));
        assert_eq!(
            taken("Zlib OR Apache-2.0 OR MIT"),
            Some(vec!["MIT".to_owned()])
        );
        // Cargo's old form, `/` for `OR`, and a choice of one not allowed.
        assert_eq!(taken("Unlicense/MIT"), Some(vec!["MIT".to_owned()]));
        assert_eq!(
            taken("Apache-2.0 OR GPL-2.0-only"),
            Some(vec!["Apache-2.0".to_owned()])
        );
        // Both of a term, and an exception with its license.
        assert_eq!(
            taken("(MIT OR Apache-2.0) AND Unicode-3.0"),
            Some(vec!["MIT".to_owned(), "Unicode-3.0".to_owned()])
        );
        assert_eq!(
            taken("Apache-2.0 WITH LLVM-exception"),
            Some(vec!["Apache-2.0 WITH LLVM-exception".to_owned()])
        );
        // None allowed, of a term or of the whole.
        assert_eq!(taken("GPL-3.0-only"), None);
        assert_eq!(taken("MIT AND GPL-3.0-only"), None);
    }

    #[test]
    fn a_file_s_name_tells_its_license_or_none() {
        assert_eq!(named("LICENSE-MIT"), Some("MIT"));
        assert_eq!(named("LICENSE-APACHE.md"), Some("Apache-2.0"));
        assert_eq!(named("LICENSE-GPLv2"), Some("GPL-"));
        assert_eq!(named("UNLICENSE"), Some("Unlicense"));
        assert_eq!(named("LICENSE-UNICODE"), Some("Unicode-3.0"));
        assert_eq!(named("LICENSE"), None);
        assert_eq!(named("COPYING"), None);
        assert_eq!(named("NOTICE"), None);
    }

    #[test]
    fn every_allowed_license_has_its_text_with_its_holders() {
        for license in allowed_list() {
            let text = text(&root(), &license, "Someone").unwrap();
            assert!(!text.contains("<year>"), "{license}");
            assert!(text.len() > 300, "{license}");
        }
        let mit = text(&root(), "MIT", "Steven Sheldon, Mads Marquart").unwrap();
        assert!(mit.contains("Copyright (c) Steven Sheldon, Mads Marquart\n"));
    }

    #[test]
    fn a_crate_without_files_takes_the_texts_and_fails_without_one() {
        let base = std::env::temp_dir().join(format!("lokyr-notices-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let (source, out) = (base.join("source"), base.join("out"));
        fs::create_dir_all(&source).unwrap();
        fs::create_dir_all(&out).unwrap();
        let component = Component {
            name: "example".to_owned(),
            version: "1.0.0".to_owned(),
            license: Some("MIT AND Zlib".to_owned()),
            authors: vec!["Someone <someone@example.com>".to_owned()],
            source: source.clone(),
        };
        let taken = ["MIT".to_owned(), "Zlib".to_owned()];
        notices(&root(), &component, &taken, &out).unwrap();
        let mit = fs::read_to_string(out.join("LICENSE-MIT")).unwrap();
        assert!(mit.contains("Copyright (c) Someone <someone@example.com>\n"));
        assert!(out.join("LICENSE-Zlib").is_file());
        // A license that has no text fails, as a crate without a notice.
        let unknown = ["Example-License".to_owned()];
        assert!(notices(&root(), &component, &unknown, &out).is_err());
        fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn every_crate_of_the_binary_has_its_notices() {
        let out = std::env::temp_dir().join(format!("lokyr-licenses-{}", std::process::id()));
        let count = write(&root(), &out).unwrap();
        let directories: Vec<_> = fs::read_dir(&out)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| entry.path().is_dir())
            .collect();
        assert_eq!(directories.len(), count);
        for directory in &directories {
            assert!(
                fs::read_dir(directory.path()).unwrap().next().is_some(),
                "{} has no notice",
                directory.path().display()
            );
        }
        // A crate whose package has no file: MIT's text, its authors' own.
        let objc2 = directories
            .iter()
            .find(|entry| entry.file_name().to_string_lossy().starts_with("objc2-0."))
            .expect("objc2 is in the binary");
        let mit = fs::read_to_string(objc2.path().join("LICENSE-MIT")).unwrap();
        assert!(mit.starts_with("MIT License\n\nCopyright (c) "));
        assert!(!mit.contains("<year>"));
        // A choice of a license not taken leaves its file out.
        let same_file = directories
            .iter()
            .find(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with("same-file-")
            })
            .expect("same-file is in the binary");
        assert!(same_file.path().join("LICENSE-MIT").is_file());
        assert!(!same_file.path().join("UNLICENSE").exists());
        let index = fs::read_to_string(out.join("README.md")).unwrap();
        assert!(index.contains("| objc2 |"));
        fs::remove_dir_all(out).unwrap();
    }
}
