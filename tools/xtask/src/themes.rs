//! The themes that the bundle holds, of a pinned commit of the
//! iTerm2-Color-Schemes collection
//! (<https://github.com/mbadolato/iTerm2-Color-Schemes>): every scheme of
//! the collection read as the app reads an `.itermcolors` file; the colors
//! of its sRGB and P3 schemes checked against the files of the same
//! schemes that it ships for Ghostty, which its own tools convert; and
//! those of a permissive license, as its `licenses.yml` tells, or whose
//! license is checked at their source, written in the native format into
//! the bundle's `Contents/Resources/themes`, each with its license and
//! source, with the collection's notices and the licenses' texts in
//! `Contents/Resources/licenses/themes`.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};

use anyhow::{Context, Result, bail, ensure};
use lokyr_kernel::themes::{self, Credits, Theme};
use plist::Value;

/// The commit of the collection whose schemes the bundle holds.
const COMMIT: &str = "b84cd1e68c9033d967fccd9febbac3072d127b7b";

/// The SHA-256 digest of the collection's files that the bundle takes, of
/// [`PARTS`]: of the lines `<digest>  <path>` that `shasum -a 256` tells
/// of each file, by their paths in order.
const DIGEST: &str = "1217144cdcc086d5eeb78fd1e1a3371136abe11e49e89c29af78f90db62a3a9a";

/// The parts of the collection that the bundle takes: its schemes, the
/// same schemes for Ghostty, which the colors are checked against, the
/// sources of the schemes that its tools write, the schemes' licenses,
/// and its own license and credits.
const PARTS: [&str; 6] = [
    "schemes",
    "ghostty",
    "yaml",
    "licenses.yml",
    "LICENSE",
    "CREDITS.md",
];

/// The licenses of `licenses.yml` that let the app hold a scheme: SPDX
/// expressions of permissive licenses alone.
const PERMISSIVE: [&str; 9] = [
    "MIT",
    "MIT OR Apache-2.0",
    "Apache-2.0",
    "ISC",
    "CC0-1.0",
    "WTFPL",
    "Zlib",
    "Unlicense",
    "0BSD",
];

/// The schemes whose license `licenses.yml` does not tell, checked at
/// their source: each one's name, license and source, whose notice is
/// `themes/<name>.txt` of [`TEXTS`].
const CHECKED: [(&str, &str, &str); 1] = [("Dracula", "MIT", "https://github.com/dracula/iterm")];

/// The licenses' texts, `<SPDX identifier>.txt`, of SPDX's list, and the
/// notices of the schemes checked at their source, `themes/<name>.txt`,
/// relative to the workspace root.
const TEXTS: &str = "tools/xtask/licenses";

/// The bundle's folders of the held themes and of their notices, within
/// its resources.
const THEMES: &str = "themes";
const NOTICES: &str = "licenses/themes";

/// `cargo xtask themes [--out DIR]`: checks the collection and writes the
/// themes that the bundle holds into `DIR`, `target/themes` by default.
pub(crate) fn run(mut args: impl Iterator<Item = String>) -> Result<ExitCode> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .context("finding the workspace's root")?;
    let mut out = root.join("target/themes");
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--out" => out = PathBuf::from(args.next().context("--out takes a directory")?),
            _ => bail!("unknown argument `{arg}`"),
        }
    }
    let held = write(&root, &out)?;
    println!("themes: {held} themes in {}", out.display());
    Ok(ExitCode::SUCCESS)
}

/// Checks the collection, downloaded into `root`'s `target` the first
/// time, and writes the themes that the bundle holds into `resources`, the
/// bundle's, or any folder, with their notices. Returns how many.
pub(crate) fn write(root: &Path, resources: &Path) -> Result<usize> {
    let collection = collection(&root.join("target"))?;
    let checked = check(&collection)?;
    println!(
        "themes: {} schemes read, {} checked against their Ghostty files, {} held",
        checked.read,
        checked.compared,
        checked.held.len()
    );
    let folder = resources.join(THEMES);
    let notices = resources.join(NOTICES);
    for directory in [&folder, &notices] {
        if directory.exists() {
            fs::remove_dir_all(directory)
                .with_context(|| format!("removing {}", directory.display()))?;
        }
    }
    fs::create_dir_all(&folder).with_context(|| format!("making {}", folder.display()))?;
    for held in &checked.held {
        let credits = Credits {
            author: None,
            license: Some(&held.license),
            source: Some(&held.source),
        };
        let text = format!(
            "# {}.itermcolors of the iTerm2-Color-Schemes collection, converted.\n{}",
            held.scheme,
            themes::native(&held.theme, credits)
        );
        let path = folder.join(format!("{}.toml", held.theme.name));
        fs::write(&path, text).with_context(|| format!("writing {}", path.display()))?;
        // As the app reads it back, the same theme.
        let name = &held.theme.name;
        let read = themes::resolve(name, None, Some(&folder), &[])
            .with_context(|| format!("reading back {}", path.display()))?;
        ensure!(
            read.theme == held.theme,
            "{} reads back as another theme",
            path.display()
        );
    }
    write_notices(&root.join(TEXTS), &collection, &checked.held, &notices)?;
    Ok(checked.held.len())
}

/// The folder of the collection's [`PARTS`] in `target`: downloaded and
/// unpacked the first time, of [`COMMIT`], their digest checked against
/// [`DIGEST`].
fn collection(target: &Path) -> Result<PathBuf> {
    let directory = target.join("themes-collection").join(COMMIT);
    if PARTS.iter().all(|part| directory.join(part).exists()) {
        return Ok(directory);
    }
    let parent = directory.parent().context("the collection's folder")?;
    fs::create_dir_all(parent).with_context(|| format!("making {}", parent.display()))?;
    let archive = parent.join(format!("{COMMIT}.tar.gz"));
    let url = format!("https://codeload.github.com/mbadolato/iTerm2-Color-Schemes/tar.gz/{COMMIT}");
    println!("themes: downloading the iTerm2-Color-Schemes collection, {COMMIT}");
    run_command(
        Command::new("curl")
            .args(["--fail", "--silent", "--show-error", "--location"])
            .args(["--retry", "3", "--output"])
            .arg(&archive)
            .arg(&url),
        "downloading the collection",
    )?;
    let unpacked = parent.join(format!("{COMMIT}.partial"));
    if unpacked.exists() {
        fs::remove_dir_all(&unpacked)
            .with_context(|| format!("removing {}", unpacked.display()))?;
    }
    fs::create_dir_all(&unpacked).with_context(|| format!("making {}", unpacked.display()))?;
    let prefix = format!("iTerm2-Color-Schemes-{COMMIT}");
    run_command(
        Command::new("tar")
            .arg("-xzf")
            .arg(&archive)
            .arg("-C")
            .arg(&unpacked)
            .args(["--strip-components", "1"])
            .args(PARTS.map(|part| format!("{prefix}/{part}"))),
        "unpacking the collection",
    )?;
    fs::remove_file(&archive).with_context(|| format!("removing {}", archive.display()))?;
    let found = digest(&unpacked)?;
    ensure!(
        found == DIGEST,
        "the collection of {url} has the digest {found}, not {DIGEST}"
    );
    if directory.exists() {
        fs::remove_dir_all(&directory)
            .with_context(|| format!("removing {}", directory.display()))?;
    }
    fs::rename(&unpacked, &directory)
        .with_context(|| format!("keeping {}", directory.display()))?;
    Ok(directory)
}

/// The digest of the [`PARTS`] in `directory`, as [`DIGEST`] is.
fn digest(directory: &Path) -> Result<String> {
    let mut files = Vec::new();
    for part in PARTS {
        let path = directory.join(part);
        if path.is_dir() {
            for entry in
                fs::read_dir(&path).with_context(|| format!("reading {}", path.display()))?
            {
                let name = entry?.file_name();
                let name = name.to_str().context("a file's name of no UTF-8")?;
                files.push(format!("{part}/{name}"));
            }
        } else {
            files.push(part.to_owned());
        }
    }
    files.sort();
    let mut lines = Vec::new();
    for chunk in files.chunks(256) {
        let output = Command::new("shasum")
            .args(["-a", "256"])
            .args(chunk)
            .current_dir(directory)
            .output()
            .context("hashing the collection")?;
        ensure!(output.status.success(), "hashing the collection failed");
        lines.extend_from_slice(&output.stdout);
    }
    let mut shasum = Command::new("shasum")
        .args(["-a", "256"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .context("hashing the collection's digests")?;
    shasum
        .stdin
        .take()
        .context("shasum's input")?
        .write_all(&lines)
        .context("hashing the collection's digests")?;
    let output = shasum.wait_with_output()?;
    ensure!(
        output.status.success(),
        "hashing the collection's digests failed"
    );
    String::from_utf8_lossy(&output.stdout)
        .split_whitespace()
        .next()
        .map(str::to_owned)
        .context("shasum told no digest")
}

/// A theme that the bundle holds: the theme, the scheme of its file, its
/// license and its source.
#[derive(Debug)]
struct Held {
    theme: Theme,
    scheme: String,
    license: String,
    source: String,
}

/// What the check of the collection found: how many schemes it read, how
/// many it compared with their Ghostty files, and the themes held.
#[derive(Debug)]
struct Checked {
    read: usize,
    compared: usize,
    held: Vec<Held>,
}

/// Reads every scheme of `collection`, which must all read and have an
/// entry of `licenses.yml`; compares those of sRGB and P3 colors with
/// their Ghostty files; and keeps the themes of the schemes that the
/// bundle may hold, by their names.
fn check(collection: &Path) -> Result<Checked> {
    let licenses = collection.join("licenses.yml");
    let text =
        fs::read_to_string(&licenses).with_context(|| format!("reading {}", licenses.display()))?;
    let entries = entries(&text).with_context(|| licenses.display().to_string())?;
    let written = written(&collection.join("yaml"))?;
    let schemes = collection.join("schemes");
    let mut paths: Vec<PathBuf> = fs::read_dir(&schemes)
        .with_context(|| format!("reading {}", schemes.display()))?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<Result<_, _>>()?;
    paths.retain(|path| {
        path.extension()
            .is_some_and(|extension| extension == "itermcolors")
    });
    paths.sort();
    let mut errors = Vec::new();
    let mut held = Vec::new();
    let mut compared = 0;
    for path in &paths {
        let name = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .context("a scheme's name of no UTF-8")?;
        let bytes = fs::read(path).with_context(|| format!("reading {}", path.display()))?;
        let read = match themes::itermcolors::read(&bytes, name, path) {
            Ok(read) => read,
            Err(error) => {
                errors.push(error.to_string());
                continue;
            }
        };
        let Some(entry) = entries.get(name) else {
            errors.push(format!("{name}: no entry of licenses.yml"));
            continue;
        };
        let ghostty = collection.join("ghostty").join(name);
        if let [theme] = read.as_slice()
            && of_srgb_or_p3(&bytes)
            && let Ok(text) = fs::read_to_string(&ghostty)
        {
            compared += 1;
            errors.extend(
                differences(theme, &text, !written.contains(name))
                    .into_iter()
                    .map(|difference| format!("{name}: {difference}")),
            );
        }
        let checked = CHECKED.iter().find(|(checked, _, _)| *checked == name);
        let (license, source) = match checked {
            Some((_, license, source)) => ((*license).to_owned(), (*source).to_owned()),
            None if PERMISSIVE.contains(&entry.license.as_str()) => {
                (entry.license.clone(), entry.source.clone())
            }
            None => continue,
        };
        for theme in read {
            held.push(Held {
                theme,
                scheme: name.to_owned(),
                license: license.clone(),
                source: source.clone(),
            });
        }
    }
    if !errors.is_empty() {
        bail!(
            "the collection does not check, {} errors:\n{}",
            errors.len(),
            errors.join("\n")
        );
    }
    let mut names = BTreeSet::new();
    for theme in &held {
        let name = &theme.theme.name;
        ensure!(
            !name.starts_with('.') && !name.contains(['/', '\\']),
            "`{name}` is no theme's name"
        );
        ensure!(names.insert(name.clone()), "two themes are named `{name}`");
    }
    Ok(Checked {
        read: paths.len(),
        compared,
        held,
    })
}

/// Whether every color of the scheme of `bytes` is of sRGB or of Display
/// P3, which the collection's own tools convert as the app does.
fn of_srgb_or_p3(bytes: &[u8]) -> bool {
    let Ok(value) = Value::from_reader(std::io::Cursor::new(bytes)) else {
        return false;
    };
    let Some(dictionary) = value.as_dictionary() else {
        return false;
    };
    dictionary.values().all(|color| {
        let space = color
            .as_dictionary()
            .and_then(|color| color.get("Color Space"))
            .and_then(Value::as_string);
        matches!(space, Some("sRGB" | "P3"))
    })
}

/// How `theme` differs from the colors of the Ghostty file of `text`, of
/// the colors that both give, by more than a unit of a byte, which the
/// digits of the collection's matrix of Display P3 and its rounding make.
/// The collection's tools raise the contrast of the colors below its
/// threshold before they write the file, which iTerm2 does not, nor the
/// app's themes: of a scheme read from its `.itermcolors` file, `raise`,
/// the check raises the theme's colors as they do first; a scheme that
/// they write of its source has its colors raised already, or kept as its
/// source asks.
fn differences(theme: &Theme, text: &str, raise: bool) -> Vec<String> {
    let mut told = BTreeMap::new();
    for line in text.lines() {
        let Some((key, value)) = line.split_once(" = ") else {
            continue;
        };
        let (key, value) = match key {
            "palette" => match value.split_once('=') {
                Some((index, value)) => (format!("palette {index}"), value),
                None => continue,
            },
            key => (key.to_owned(), value),
        };
        if let Some(rgb) = hex(value.trim()) {
            told.insert(key, rgb);
        }
    }
    ghostty_colors(theme, raise)
        .into_iter()
        .filter_map(|(key, rgb)| {
            let theirs = told.get(&key)?;
            let near = rgb
                .iter()
                .zip(theirs)
                .all(|(ours, theirs)| ours.abs_diff(*theirs) <= 1);
            (!near).then(|| {
                format!(
                    "{key} is {}, where Ghostty's file has {}",
                    shown(rgb),
                    shown(*theirs)
                )
            })
        })
        .collect()
}

/// The colors of `theme` as the collection's tools write its Ghostty
/// file: the keys that it lacks of their fallbacks, and, of `raise`, the
/// colors of too little contrast raised to the threshold, as they raise
/// them.
fn ghostty_colors(theme: &Theme, raise: bool) -> Vec<(String, [u8; 3])> {
    let rgb = |color: lokyr_kernel::themes::ThemeColor| [color.red, color.green, color.blue];
    let terminal = &theme.terminal;
    let background = rgb(terminal.background);
    let mut foreground = rgb(terminal.foreground);
    let mut palette = terminal.palette.map(rgb);
    let mut cursor = terminal.cursor.map(rgb);
    let mut cursor_text = terminal.cursor_text.map_or(background, rgb);
    let selection = terminal.selection.map_or(foreground, rgb);
    let mut selection_text = terminal.selection_text.map_or(background, rgb);
    let raised = |color: [u8; 3], background: [u8; 3]| {
        if raise {
            raised(color, background)
        } else {
            color
        }
    };
    foreground = raised(foreground, background);
    for color in &mut palette[1..15] {
        *color = raised(*color, background);
    }
    cursor = cursor.map(|cursor| raised(cursor, background));
    if let Some(cursor) = cursor {
        cursor_text = raised(cursor_text, cursor);
    }
    selection_text = raised(selection_text, selection);
    let mut colors = vec![
        ("background".to_owned(), background),
        ("foreground".to_owned(), foreground),
        ("cursor-text".to_owned(), cursor_text),
        ("selection-background".to_owned(), selection),
        ("selection-foreground".to_owned(), selection_text),
    ];
    if let Some(cursor) = cursor {
        colors.push(("cursor-color".to_owned(), cursor));
    }
    for (index, color) in palette.iter().enumerate() {
        colors.push((format!("palette {index}"), *color));
    }
    colors
}

/// The contrast that the collection's tools raise a color to against its
/// background.
const THRESHOLD: f64 = 1.75;

/// `color` against `background`, raised to [`THRESHOLD`] where it is
/// below it as the collection's tools raise it: by steps of 0.05 towards
/// white over a dark background and towards black over a light one, 20 at
/// most, then white or black.
fn raised(color: [u8; 3], background: [u8; 3]) -> [u8; 3] {
    let unit = |rgb: [u8; 3]| rgb.map(|value| f64::from(value) / 255.0);
    let (color_unit, background_unit) = (unit(color), unit(background));
    if contrast(color_unit, background_unit) >= THRESHOLD {
        return color;
    }
    let dark = luminance(background_unit) < 0.5;
    let mut raised = color_unit;
    for _ in 0..20 {
        for value in &mut raised {
            *value = if dark {
                (*value + 0.05).min(1.0)
            } else {
                (*value - 0.05).max(0.0)
            };
        }
        if contrast(raised, background_unit) >= THRESHOLD {
            return raised.map(|value| (value * 255.0).round() as u8);
        }
    }
    if dark { [255; 3] } else { [0; 3] }
}

/// The relative luminance of the sRGB components `rgb`, as WCAG's.
fn luminance(rgb: [f64; 3]) -> f64 {
    let linear = |value: f64| {
        if value <= 0.040_45 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * linear(rgb[0]) + 0.7152 * linear(rgb[1]) + 0.0722 * linear(rgb[2])
}

/// The contrast of two colors, as WCAG's.
fn contrast(one: [f64; 3], other: [f64; 3]) -> f64 {
    let (one, other) = (luminance(one), luminance(other));
    (one.max(other) + 0.05) / (one.min(other) + 0.05)
}

/// The color of `text`, `#rrggbb` of either case.
fn hex(text: &str) -> Option<[u8; 3]> {
    let digits = text.strip_prefix('#').filter(|digits| digits.len() == 6)?;
    let byte = |at: usize| u8::from_str_radix(digits.get(at..at + 2)?, 16).ok();
    Some([byte(0)?, byte(2)?, byte(4)?])
}

/// `rgb` as `#rrggbb`.
fn shown(rgb: [u8; 3]) -> String {
    format!("#{:02x}{:02x}{:02x}", rgb[0], rgb[1], rgb[2])
}

/// The names of the schemes that the collection's tools write of their
/// sources in `yaml`, its `.itermcolors` file among their files: each
/// source's `name`, or its file's name without one.
fn written(yaml: &Path) -> Result<BTreeSet<String>> {
    let mut names = BTreeSet::new();
    for entry in fs::read_dir(yaml).with_context(|| format!("reading {}", yaml.display()))? {
        let path = entry?.path();
        if path.extension().is_none_or(|extension| extension != "yml") {
            continue;
        }
        let text =
            fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
        let named = text
            .lines()
            .find_map(|line| line.strip_prefix("name:"))
            .map(|name| {
                let name = name.trim();
                match name
                    .strip_prefix('"')
                    .and_then(|name| name.strip_suffix('"'))
                {
                    Some(name) => Ok(name.to_owned()),
                    None => scalar(name),
                }
            })
            .transpose()
            .with_context(|| path.display().to_string())?;
        let name = match named {
            Some(name) => name,
            None => path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .context("a source's name of no UTF-8")?
                .to_owned(),
        };
        names.insert(name);
    }
    Ok(names)
}

/// A scheme's entry of `licenses.yml`: its license, an SPDX expression,
/// `unknown` or `nonfree`, and its source.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Entry {
    license: String,
    source: String,
}

/// The entries of `text`, a `licenses.yml`, by their schemes' names: a
/// mapping of each scheme's name to its `license`, `source` and `note`,
/// plain scalars or single-quoted ones, of several lines too, as the
/// collection writes them.
fn entries(text: &str) -> Result<BTreeMap<String, Entry>> {
    let mut entries = BTreeMap::new();
    let mut current: Option<(String, BTreeMap<String, String>)> = None;
    let mut lines = text.lines().enumerate();
    let mut finish = |current: Option<(String, BTreeMap<String, String>)>| -> Result<()> {
        if let Some((name, mut fields)) = current {
            let license = fields
                .remove("license")
                .with_context(|| format!("`{name}` has no license"))?;
            let source = fields.remove("source").unwrap_or_default();
            ensure!(
                entries
                    .insert(name.clone(), Entry { license, source })
                    .is_none(),
                "`{name}` has two entries"
            );
        }
        Ok(())
    };
    while let Some((number, line)) = lines.next() {
        let at = || format!("line {}: `{line}`", number + 1);
        if line.trim().is_empty() || line.starts_with('#') {
            continue;
        }
        if !line.starts_with(' ') {
            let name = line.strip_suffix(':').with_context(at)?;
            finish(current.take())?;
            current = Some((scalar(name).with_context(at)?, BTreeMap::new()));
            continue;
        }
        let field = line
            .strip_prefix("  ")
            .filter(|field| !field.starts_with(' '));
        let (key, value) = field
            .and_then(|field| field.split_once(':'))
            .with_context(at)?;
        let (_, fields) = current.as_mut().with_context(at)?;
        let value = value.trim_start();
        let value = if value.strip_prefix('\'').is_some_and(|inner| !ends(inner)) {
            // A single-quoted scalar of several lines, folded: a line
            // break a space, an empty line a line feed.
            let mut parts = vec![value.to_owned()];
            loop {
                let (_, next) = lines.next().with_context(at)?;
                parts.push(next.trim().to_owned());
                if ends(next) {
                    break;
                }
            }
            let mut folded = String::new();
            let mut empty = 0;
            for (index, part) in parts.iter().enumerate() {
                if part.is_empty() {
                    empty += 1;
                    continue;
                }
                if index > 0 {
                    if empty == 0 {
                        folded.push(' ');
                    } else {
                        folded.push_str(&"\n".repeat(empty));
                    }
                }
                empty = 0;
                folded.push_str(part);
            }
            folded
        } else {
            value.to_owned()
        };
        fields.insert(key.to_owned(), scalar(&value).with_context(at)?);
    }
    finish(current)?;
    Ok(entries)
}

/// Whether `text`, the inside of a single-quoted scalar from its start or
/// from one of its lines' start, holds its closing quote: a quote that is
/// not doubled.
fn ends(text: &str) -> bool {
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\'' {
            if chars.peek() == Some(&'\'') {
                chars.next();
            } else {
                return true;
            }
        }
    }
    false
}

/// The text of the scalar `text`: plain as it is, or single-quoted, of its
/// doubled quotes; a double-quoted one is not the collection's.
fn scalar(text: &str) -> Result<String> {
    let text = text.trim();
    if let Some(inner) = text.strip_prefix('\'') {
        let inner = inner
            .strip_suffix('\'')
            .context("a single-quoted scalar that does not end")?;
        return Ok(inner.replace("''", "'"));
    }
    ensure!(!text.starts_with('"'), "a double-quoted scalar");
    Ok(text.to_owned())
}

/// Writes the notices of the `held` themes into `notices`: the collection's
/// license and credits, of `collection`; a list of the themes with their
/// licenses and sources; and the texts of their licenses and of the
/// checked schemes' notices, of `texts`.
fn write_notices(texts: &Path, collection: &Path, held: &[Held], notices: &Path) -> Result<()> {
    let licenses = notices.join("licenses");
    fs::create_dir_all(&licenses).with_context(|| format!("making {}", licenses.display()))?;
    for part in ["LICENSE", "CREDITS.md"] {
        fs::copy(collection.join(part), notices.join(part))
            .with_context(|| format!("copying the collection's {part}"))?;
    }
    let mut kept = BTreeSet::new();
    for theme in held {
        if CHECKED.iter().any(|(name, _, _)| *name == theme.scheme) {
            kept.insert(Path::new("themes").join(format!("{}.txt", theme.scheme)));
        }
        kept.extend(
            theme
                .license
                .split(" OR ")
                .map(|license| PathBuf::from(format!("{license}.txt"))),
        );
    }
    for path in &kept {
        let name = path.file_name().context("a license's file")?;
        fs::copy(texts.join(path), licenses.join(name))
            .with_context(|| format!("copying {}", path.display()))?;
    }
    fs::write(notices.join("README.md"), readme(held)).context("writing the themes' notices")
}

/// The list of the `held` themes, with their licenses and sources, and
/// what the notices beside it are.
fn readme(held: &[Held]) -> String {
    let mut text = format!(
        "# Themes\n\n\
         The themes that Lokyr holds come from the iTerm2-Color-Schemes collection, \
         <https://github.com/mbadolato/iTerm2-Color-Schemes>, of its commit {COMMIT}, \
         whose license is in `LICENSE` and whose credits are in `CREDITS.md`: \
         its schemes of a permissive license, as its `licenses.yml` tells, and those \
         whose license was checked at their source, converted into Lokyr's format.\n\n\
         Each theme's copyright is its author's, as its source tells. `licenses/` holds \
         the text of each license, of SPDX's templates, and the notices of the themes \
         checked at their source, of their names.\n\n\
         | Theme | License | Source |\n|---|---|---|\n"
    );
    for theme in held {
        text.push_str(&format!(
            "| {} | {} | {} |\n",
            theme.theme.name.replace('|', "\\|"),
            theme.license,
            theme.source.replace('\n', "<br>").replace('|', "\\|")
        ));
    }
    text
}

/// Runs `command`, for `what`, whose failure is an error.
fn run_command(command: &mut Command, what: &str) -> Result<()> {
    let status = command.status().with_context(|| what.to_owned())?;
    ensure!(status.success(), "{what} failed: {status}");
    Ok(())
}

#[cfg(test)]
mod tests;
