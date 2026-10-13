//! `cargo xtask ucd`: generates the Unicode tables of `vt-core` from the
//! Unicode Character Database.
//!
//! The UCD files are downloaded with `curl` into `target/ucd/<version>/`,
//! and kept there for the next run. The output is
//! `crates/vt-core/src/unicode/tables.rs`, which is committed. With
//! `--check`, the task regenerates the tables in memory and fails if they
//! differ from the committed file.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use anyhow::{Context, Result, bail};

/// The Unicode version of the tables.
const VERSION: &str = "18.0.0";

/// The version whose East_Asian_Width serves programs that predate
/// Unicode 9, which gave most emoji two cells.
const LEGACY_WIDTHS_VERSION: &str = "8.0.0";

/// The generated file, relative to the workspace root.
const OUTPUT: &str = "crates/vt-core/src/unicode/tables.rs";

/// The unassigned code points that East_Asian_Width gives "W" rather than
/// the "N" of its `@missing` line, as the header of `EastAsianWidth.txt`
/// says, in both versions.
const WIDE_BY_DEFAULT: [(u32, u32); 5] = [
    (0x3400, 0x4DBF),
    (0x4E00, 0x9FFF),
    (0xF900, 0xFAFF),
    (0x2_0000, 0x2_FFFD),
    (0x3_0000, 0x3_FFFD),
];

/// Runs the task with the remaining command-line arguments.
pub fn run(args: impl Iterator<Item = String>) -> Result<ExitCode> {
    let mut check = false;
    for arg in args {
        match arg.as_str() {
            "--check" => check = true,
            other => bail!("unknown argument `{other}`"),
        }
    }
    let root = workspace_root();
    let tables = generate(&root)?;
    let output = root.join(OUTPUT);
    if check {
        let committed =
            fs::read_to_string(&output).with_context(|| format!("reading {}", output.display()))?;
        if committed != tables {
            eprintln!("error: {OUTPUT} differs from what Unicode {VERSION} generates");
            return Ok(ExitCode::FAILURE);
        }
        println!("ucd: {OUTPUT} matches Unicode {VERSION}");
    } else {
        fs::write(&output, &tables).with_context(|| format!("writing {}", output.display()))?;
        println!("ucd: wrote {OUTPUT} from Unicode {VERSION}");
    }
    Ok(ExitCode::SUCCESS)
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("xtask lives two levels below the workspace root")
        .to_path_buf()
}

/// A UCD file, downloaded once.
struct Source {
    /// Its first two lines: its name and its date.
    header: String,
    text: String,
}

fn fetch(root: &Path, version: &str, path: &'static str) -> Result<Source> {
    let local = root.join("target/ucd").join(version).join(path);
    if !local.exists() {
        let url = format!("https://www.unicode.org/Public/{version}/{path}");
        let partial = local.with_extension("part");
        let status = Command::new("curl")
            .args([
                "--fail",
                "--silent",
                "--show-error",
                "--location",
                "--create-dirs",
            ])
            .arg("--output")
            .arg(&partial)
            .arg(&url)
            .status()
            .context("running curl")?;
        if !status.success() {
            bail!("downloading {url} failed");
        }
        fs::rename(&partial, &local)?;
    }
    let text =
        fs::read_to_string(&local).with_context(|| format!("reading {}", local.display()))?;
    let header = text.lines().take(2).collect::<Vec<_>>().join("\n");
    Ok(Source { header, text })
}

/// The data lines of a UCD file: a range of code points, and the fields
/// after it, with comments and spaces taken out.
fn records(text: &str) -> impl Iterator<Item = Result<(u32, u32, Vec<&str>)>> + '_ {
    text.lines().filter_map(|line| {
        let data = line.split('#').next().unwrap_or_default().trim();
        if data.is_empty() {
            return None;
        }
        Some(record(data).with_context(|| format!("in line `{line}`")))
    })
}

fn record(data: &str) -> Result<(u32, u32, Vec<&str>)> {
    let mut fields = data.split(';').map(str::trim);
    let range = fields.next().unwrap_or_default();
    let (start, end) = match range.split_once("..") {
        Some((start, end)) => (start, end),
        None => (range, range),
    };
    let start = u32::from_str_radix(start, 16).context("a code point")?;
    let end = u32::from_str_radix(end, 16).context("a code point")?;
    if start > end || end > 0x10_FFFF {
        bail!("a range of code points out of order or out of range");
    }
    Ok((start, end, fields.collect()))
}

/// A property with values for ranges of code points, which merges ranges
/// that touch and share a value.
#[derive(Default)]
struct Ranges {
    values: BTreeMap<u32, (u32, &'static str)>,
}

impl Ranges {
    /// Gives `value` to every code point from `start` to `end`, over what
    /// they had.
    fn set(&mut self, start: u32, end: u32, value: &'static str) {
        let mut cut: Vec<(u32, u32, &'static str)> = Vec::new();
        // The ranges are apart and in order, so the ones that overlap come
        // last among those that start by `end`.
        let overlapping: Vec<u32> = self
            .values
            .range(..=end)
            .rev()
            .take_while(|&(_, &(last, _))| last >= start)
            .map(|(&first, _)| first)
            .collect();
        for first in overlapping {
            let (last, old) = self.values.remove(&first).expect("the range is there");
            if first < start {
                cut.push((first, start - 1, old));
            }
            if last > end {
                cut.push((end + 1, last, old));
            }
        }
        for (first, last, old) in cut {
            self.values.insert(first, (last, old));
        }
        self.values.insert(start, (end, value));
    }

    /// The ranges, in order, with the ones that touch and share a value
    /// merged.
    fn merged(&self) -> Vec<(u32, u32, &'static str)> {
        let mut merged: Vec<(u32, u32, &'static str)> = Vec::new();
        for (&first, &(last, value)) in &self.values {
            match merged.last_mut() {
                Some((_, end, previous)) if *end + 1 == first && *previous == value => *end = last,
                _ => merged.push((first, last, value)),
            }
        }
        merged
    }
}

/// East_Asian_Width, keeping only the wide and the ambiguous code points.
fn east_asian_width(source: &Source) -> Result<Ranges> {
    let mut ranges = Ranges::default();
    for &(start, end) in &WIDE_BY_DEFAULT {
        ranges.set(start, end, "Wide");
    }
    for record in records(&source.text) {
        let (start, end, fields) = record?;
        let value = match fields.first().copied() {
            Some("W" | "F") => "Wide",
            Some("A") => "Ambiguous",
            Some("N" | "Na" | "H") => "Narrow",
            other => bail!("unknown East_Asian_Width {other:?}"),
        };
        ranges.set(start, end, value);
    }
    Ok(without(&ranges, "Narrow"))
}

/// The ranges of `ranges`, without those whose value is `default`.
fn without(ranges: &Ranges, default: &str) -> Ranges {
    let mut kept = Ranges::default();
    for (start, end, value) in ranges.merged() {
        if value != default {
            kept.set(start, end, value);
        }
    }
    kept
}

/// The ranges of `source` whose first field is one of `values`, under the
/// names that `values` maps them to.
fn select(source: &Source, values: &[(&str, &'static str)]) -> Result<Ranges> {
    let mut ranges = Ranges::default();
    for record in records(&source.text) {
        let (start, end, fields) = record?;
        let Some(&field) = fields.first() else {
            bail!("a line without a value");
        };
        if let Some(&(_, name)) = values.iter().find(|&&(value, _)| value == field) {
            ranges.set(start, end, name);
        }
    }
    Ok(ranges)
}

/// Indic_Conjunct_Break, whose lines in `DerivedCoreProperties.txt` read
/// `range; InCB; value`.
fn indic_conjunct_break(source: &Source) -> Result<Ranges> {
    let mut ranges = Ranges::default();
    for record in records(&source.text) {
        let (start, end, fields) = record?;
        if let ["InCB", value] = fields[..] {
            let name = match value {
                "Linker" => "Linker",
                "Consonant" => "Consonant",
                "Extend" => "Extend",
                other => bail!("unknown Indic_Conjunct_Break {other}"),
            };
            ranges.set(start, end, name);
        }
    }
    Ok(ranges)
}

/// Generates the tables, downloading the files that are missing.
fn generate(root: &Path) -> Result<String> {
    let widths = fetch(root, VERSION, "ucd/EastAsianWidth.txt")?;
    let legacy_widths = fetch(root, LEGACY_WIDTHS_VERSION, "ucd/EastAsianWidth.txt")?;
    let categories = fetch(root, VERSION, "ucd/extracted/DerivedGeneralCategory.txt")?;
    let core = fetch(root, VERSION, "ucd/DerivedCoreProperties.txt")?;
    let emoji = fetch(root, VERSION, "ucd/emoji/emoji-data.txt")?;
    let breaks = fetch(root, VERSION, "ucd/auxiliary/GraphemeBreakProperty.txt")?;

    let mut out = String::new();
    writeln!(
        out,
        "//! The Unicode Character Database tables, generated by `cargo xtask ucd`"
    )?;
    writeln!(out, "//! from these files. Do not edit by hand.")?;
    for source in [&widths, &legacy_widths, &categories, &core, &emoji, &breaks] {
        writeln!(out, "//!")?;
        for line in source.header.lines() {
            writeln!(out, "//! {}", line.trim_start_matches("# "))?;
        }
    }
    writeln!(out)?;
    writeln!(out, "use super::EastAsianWidth::{{self, Ambiguous, Wide}};")?;
    writeln!(out, "use super::GraphemeBreak::{{self, *}};")?;
    writeln!(out, "use super::IndicConjunctBreak;")?;
    writeln!(
        out,
        "use super::Mark::{{self, Enclosing, Nonspacing, Spacing}};"
    )?;
    writeln!(out)?;
    let [major, minor, update] = version_parts(VERSION)?;
    writeln!(
        out,
        "/// The Unicode version of the tables, except for the legacy widths, which come from {LEGACY_WIDTHS_VERSION}."
    )?;
    writeln!(
        out,
        "pub const UNICODE_VERSION: (u8, u8, u8) = ({major}, {minor}, {update});"
    )?;

    table(
        &mut out,
        "East_Asian_Width of the wide and the ambiguous code points; the others are narrow.",
        "EAST_ASIAN_WIDTH",
        "EastAsianWidth",
        &east_asian_width(&widths)?,
    )?;
    table(
        &mut out,
        "East_Asian_Width as Unicode 8 gave it, before most emoji became wide.",
        "LEGACY_EAST_ASIAN_WIDTH",
        "EastAsianWidth",
        &east_asian_width(&legacy_widths)?,
    )?;
    table(
        &mut out,
        "The marks: the code points whose General_Category is Mn, Me or Mc.",
        "MARKS",
        "Mark",
        &select(
            &categories,
            &[("Mn", "Nonspacing"), ("Me", "Enclosing"), ("Mc", "Spacing")],
        )?,
    )?;
    set(
        &mut out,
        "The code points whose General_Category is Cf.",
        "FORMAT",
        &select(&categories, &[("Cf", "")])?,
    )?;
    set(
        &mut out,
        "The code points with Default_Ignorable_Code_Point.",
        "DEFAULT_IGNORABLE",
        &select(&core, &[("Default_Ignorable_Code_Point", "")])?,
    )?;
    set(
        &mut out,
        "The code points with Emoji_Presentation.",
        "EMOJI_PRESENTATION",
        &select(&emoji, &[("Emoji_Presentation", "")])?,
    )?;
    set(
        &mut out,
        "The code points with Extended_Pictographic.",
        "EXTENDED_PICTOGRAPHIC",
        &select(&emoji, &[("Extended_Pictographic", "")])?,
    )?;
    table(
        &mut out,
        "Grapheme_Cluster_Break, for the code points whose value is not Other.",
        "GRAPHEME_BREAK",
        "GraphemeBreak",
        &select(
            &breaks,
            &[
                ("CR", "CR"),
                ("LF", "LF"),
                ("Control", "Control"),
                ("Extend", "Extend"),
                ("ZWJ", "ZWJ"),
                ("Regional_Indicator", "RegionalIndicator"),
                ("Prepend", "Prepend"),
                ("SpacingMark", "SpacingMark"),
                ("L", "L"),
                ("V", "V"),
                ("T", "T"),
                ("LV", "LV"),
                ("LVT", "LVT"),
            ],
        )?,
    )?;
    table(
        &mut out,
        "Indic_Conjunct_Break, for the code points whose value is not None.",
        "INDIC_CONJUNCT_BREAK",
        "IndicConjunctBreak",
        &indic_conjunct_break(&core)?,
    )?;
    Ok(out)
}

fn version_parts(version: &str) -> Result<[u8; 3]> {
    let parts: Vec<u8> = version
        .split('.')
        .map(str::parse)
        .collect::<Result<_, _>>()
        .context("a version as major.minor.update")?;
    parts
        .try_into()
        .map_err(|_| anyhow::anyhow!("a version has three parts"))
}

/// Writes a table of ranges with a value each. The values are written as
/// they are, so the type's variants must be in scope, or `ty` must be the
/// path to them.
fn table(out: &mut String, doc: &str, name: &str, ty: &str, ranges: &Ranges) -> Result<()> {
    writeln!(out)?;
    writeln!(out, "/// {doc}")?;
    writeln!(out, "pub static {name}: &[(u32, u32, {ty})] = &[")?;
    // The variants of Indic_Conjunct_Break share names with those of
    // Grapheme_Cluster_Break, so they keep their path.
    let path = if ty == "IndicConjunctBreak" {
        "IndicConjunctBreak::"
    } else {
        ""
    };
    for (start, end, value) in ranges.merged() {
        writeln!(out, "    (0x{start:04X}, 0x{end:04X}, {path}{value}),")?;
    }
    writeln!(out, "];")?;
    Ok(())
}

/// Writes a table of ranges, for a property that a code point has or not.
fn set(out: &mut String, doc: &str, name: &str, ranges: &Ranges) -> Result<()> {
    writeln!(out)?;
    writeln!(out, "/// {doc}")?;
    writeln!(out, "pub static {name}: &[(u32, u32)] = &[")?;
    for (start, end, _) in ranges.merged() {
        writeln!(out, "    (0x{start:04X}, 0x{end:04X}),")?;
    }
    writeln!(out, "];")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_take_ranges_single_code_points_and_extra_fields() {
        let text = "# a comment\n\n0000..001F;N     # Cc\n0020           ; Na # Zs\n094D ; InCB; Linker # Mn\n1F600..1F64F  ; Extended_Pictographic#\n";
        let records: Vec<(u32, u32, Vec<&str>)> = records(text).collect::<Result<_>>().unwrap();
        assert_eq!(
            records,
            [
                (0x00, 0x1F, vec!["N"]),
                (0x20, 0x20, vec!["Na"]),
                (0x94D, 0x94D, vec!["InCB", "Linker"]),
                (0x1F600, 0x1F64F, vec!["Extended_Pictographic"]),
            ]
        );
    }

    #[test]
    fn a_bad_range_is_an_error() {
        assert!(record("0020..001F; N").is_err());
        assert!(record("110000; N").is_err());
        assert!(record("XYZ; N").is_err());
    }

    #[test]
    fn later_ranges_win_and_touching_ones_merge() {
        let mut ranges = Ranges::default();
        ranges.set(0x10, 0x2F, "Wide");
        ranges.set(0x18, 0x1F, "Ambiguous");
        ranges.set(0x30, 0x3F, "Wide");
        assert_eq!(
            ranges.merged(),
            [
                (0x10, 0x17, "Wide"),
                (0x18, 0x1F, "Ambiguous"),
                (0x20, 0x3F, "Wide")
            ]
        );
    }

    #[test]
    fn listed_code_points_override_the_wide_defaults() {
        let source = Source {
            header: String::new(),
            text: "3400..3401;N\n00A1;A\n".to_owned(),
        };
        let ranges = east_asian_width(&source).unwrap().merged();
        assert_eq!(ranges[0], (0xA1, 0xA1, "Ambiguous"));
        assert_eq!(ranges[1], (0x3402, 0x4DBF, "Wide"));
    }
}
