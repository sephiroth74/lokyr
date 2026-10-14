//! `cargo xtask ucd`: generates the Unicode tables of `vt-core` from the
//! Unicode Character Database.
//!
//! The UCD files are downloaded with `curl` into `target/ucd/<version>/`,
//! and kept there for the next run. The outputs are
//! `crates/vt-core/src/unicode/tables.rs`, and a copy of the UCD's
//! `GraphemeBreakTest.txt` in `crates/vt-core/testdata/`, so that the
//! conformance test follows the version of the tables. Both are committed.
//! With `--check`, the task regenerates them in memory and fails if they
//! differ from the committed files.

use std::collections::{BTreeMap, HashMap};
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

/// The generated tables, relative to the workspace root.
const OUTPUT: &str = "crates/vt-core/src/unicode/tables.rs";

/// The copy of the grapheme conformance test, relative to the workspace
/// root.
const BREAK_TEST: &str = "crates/vt-core/testdata/GraphemeBreakTest.txt";

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
    let outputs = [
        (OUTPUT, generate(&root)?),
        (
            BREAK_TEST,
            fetch(&root, VERSION, "ucd/auxiliary/GraphemeBreakTest.txt")?.text,
        ),
    ];
    let mut failed = false;
    for (path, contents) in outputs {
        let output = root.join(path);
        if check {
            let committed = fs::read_to_string(&output)
                .with_context(|| format!("reading {}", output.display()))?;
            if committed == contents {
                println!("ucd: {path} matches Unicode {VERSION}");
            } else {
                eprintln!("error: {path} differs from what Unicode {VERSION} gives");
                failed = true;
            }
        } else {
            fs::write(&output, &contents)
                .with_context(|| format!("writing {}", output.display()))?;
            println!("ucd: wrote {path} from Unicode {VERSION}");
        }
    }
    Ok(if failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    })
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
    writeln!(
        out,
        "use super::EastAsianWidth::{{self, Ambiguous, Narrow, Wide}};"
    )?;
    writeln!(out, "use super::GraphemeBreak::{{self, *}};")?;
    writeln!(out, "use super::IndicConjunctBreak;")?;
    writeln!(
        out,
        "use super::Mark::{{self, Enclosing, Nonspacing, Spacing}};"
    )?;
    writeln!(out, "use super::Properties;")?;
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

    let east_asian = east_asian_width(&widths)?;
    let legacy_east_asian = east_asian_width(&legacy_widths)?;
    let marks = select(
        &categories,
        &[("Mn", "Nonspacing"), ("Me", "Enclosing"), ("Mc", "Spacing")],
    )?;
    let format = select(&categories, &[("Cf", "")])?;
    let default_ignorable = select(&core, &[("Default_Ignorable_Code_Point", "")])?;
    let emoji_presentation = select(&emoji, &[("Emoji_Presentation", "")])?;
    let pictographic = select(&emoji, &[("Extended_Pictographic", "")])?;
    let grapheme_break = select(
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
    )?;
    let conjunct = indic_conjunct_break(&core)?;

    table(
        &mut out,
        "East_Asian_Width of the wide and the ambiguous code points; the others are narrow.",
        "EAST_ASIAN_WIDTH",
        "EastAsianWidth",
        &east_asian,
    )?;
    table(
        &mut out,
        "East_Asian_Width as Unicode 8 gave it, before most emoji became wide.",
        "LEGACY_EAST_ASIAN_WIDTH",
        "EastAsianWidth",
        &legacy_east_asian,
    )?;
    table(
        &mut out,
        "The marks: the code points whose General_Category is Mn, Me or Mc.",
        "MARKS",
        "Mark",
        &marks,
    )?;
    set(
        &mut out,
        "The code points whose General_Category is Cf.",
        "FORMAT",
        &format,
    )?;
    set(
        &mut out,
        "The code points with Default_Ignorable_Code_Point.",
        "DEFAULT_IGNORABLE",
        &default_ignorable,
    )?;
    set(
        &mut out,
        "The code points with Emoji_Presentation.",
        "EMOJI_PRESENTATION",
        &emoji_presentation,
    )?;
    set(
        &mut out,
        "The code points with Extended_Pictographic.",
        "EXTENDED_PICTOGRAPHIC",
        &pictographic,
    )?;
    table(
        &mut out,
        "Grapheme_Cluster_Break, for the code points whose value is not Other.",
        "GRAPHEME_BREAK",
        "GraphemeBreak",
        &grapheme_break,
    )?;
    table(
        &mut out,
        "Indic_Conjunct_Break, for the code points whose value is not None.",
        "INDIC_CONJUNCT_BREAK",
        "IndicConjunctBreak",
        &conjunct,
    )?;

    const WIDTHS: &[(&str, &str)] = &[("Wide", "Wide"), ("Ambiguous", "Ambiguous")];
    const PRESENT: &[(&str, &str)] = &[("", "true")];
    properties(
        &mut out,
        &[
            Field {
                name: "east_asian_width",
                ranges: &east_asian,
                absent: "Narrow",
                values: WIDTHS,
            },
            Field {
                name: "legacy_east_asian_width",
                ranges: &legacy_east_asian,
                absent: "Narrow",
                values: WIDTHS,
            },
            Field {
                name: "grapheme_break",
                ranges: &grapheme_break,
                absent: "Other",
                values: &[
                    ("CR", "CR"),
                    ("LF", "LF"),
                    ("Control", "Control"),
                    ("Extend", "Extend"),
                    ("ZWJ", "ZWJ"),
                    ("RegionalIndicator", "RegionalIndicator"),
                    ("Prepend", "Prepend"),
                    ("SpacingMark", "SpacingMark"),
                    ("L", "L"),
                    ("V", "V"),
                    ("T", "T"),
                    ("LV", "LV"),
                    ("LVT", "LVT"),
                ],
            },
            Field {
                name: "indic_conjunct_break",
                ranges: &conjunct,
                absent: "IndicConjunctBreak::None",
                values: &[
                    ("Linker", "IndicConjunctBreak::Linker"),
                    ("Consonant", "IndicConjunctBreak::Consonant"),
                    ("Extend", "IndicConjunctBreak::Extend"),
                ],
            },
            Field {
                name: "mark",
                ranges: &marks,
                absent: "None",
                values: &[
                    ("Nonspacing", "Some(Nonspacing)"),
                    ("Enclosing", "Some(Enclosing)"),
                    ("Spacing", "Some(Spacing)"),
                ],
            },
            Field {
                name: "format",
                ranges: &format,
                absent: "false",
                values: PRESENT,
            },
            Field {
                name: "default_ignorable",
                ranges: &default_ignorable,
                absent: "false",
                values: PRESENT,
            },
            Field {
                name: "extended_pictographic",
                ranges: &pictographic,
                absent: "false",
                values: PRESENT,
            },
        ],
    )?;
    Ok(out)
}

/// How many code points there are.
const CODE_POINTS: usize = 0x11_0000;

/// A field of the properties that printing reads, and the ranges that its
/// values come from.
struct Field<'a> {
    name: &'static str,
    ranges: &'a Ranges,
    /// The field's value, as Rust, for the code points that no range holds.
    absent: &'static str,
    /// The values of the ranges, each with the field's value, as Rust.
    values: &'static [(&'static str, &'static str)],
}

/// Writes the properties of every code point, `fields` together, as a
/// table of three stages: the block of each run of code points, the entry
/// of each code point in a block, and the properties of each entry.
fn properties(out: &mut String, fields: &[Field]) -> Result<()> {
    // Each field, as the index of each code point's value: 0 for absent,
    // then the values in order.
    let mut columns = Vec::new();
    for field in fields {
        let mut column = vec![0_u8; CODE_POINTS];
        for (start, end, value) in field.ranges.merged() {
            let Some(index) = field.values.iter().position(|&(name, _)| name == value) else {
                bail!("{}: no value for {value:?}", field.name);
            };
            column[start as usize..=end as usize].fill(u8::try_from(index + 1)?);
        }
        columns.push(column);
    }

    // The entries: the combinations of values, in the order that the code
    // points first have them.
    let mut entries: Vec<Vec<u8>> = Vec::new();
    let mut known: HashMap<Vec<u8>, u8> = HashMap::new();
    let mut entry_of = Vec::with_capacity(CODE_POINTS);
    for c in 0..CODE_POINTS {
        let entry: Vec<u8> = columns.iter().map(|column| column[c]).collect();
        let id = match known.get(&entry) {
            Some(&id) => id,
            None => {
                let id = u8::try_from(entries.len()).context("more than 256 entries")?;
                known.insert(entry.clone(), id);
                entries.push(entry);
                id
            }
        };
        entry_of.push(id);
    }

    let (shift, index, blocks) = (4..=10)
        .map(|shift| stages(&entry_of, shift))
        .collect::<Result<Vec<_>>>()?
        .into_iter()
        .min_by_key(|(_, index, blocks)| index_bytes(index) + blocks.len())
        .expect("several block sizes were tried");
    let index_type = if index_bytes(&index) == index.len() {
        "u8"
    } else {
        "u16"
    };

    writeln!(out)?;
    writeln!(
        out,
        "/// The properties that printing reads, for every code point, in three stages:"
    )?;
    writeln!(
        out,
        "/// `PROPERTY_INDEX` gives the block of each run of 2^`PROPERTY_SHIFT` code points,"
    )?;
    writeln!(
        out,
        "/// `PROPERTY_BLOCKS` the entry of each code point of a block, and `PROPERTIES` the"
    )?;
    writeln!(out, "/// properties of each entry.")?;
    writeln!(out, "pub const PROPERTY_SHIFT: u32 = {shift};")?;
    numbers(
        out,
        "The block of each run of code points.",
        "PROPERTY_INDEX",
        index_type,
        &index,
    )?;
    numbers(
        out,
        "The entry of each code point, a block after the other.",
        "PROPERTY_BLOCKS",
        "u8",
        &blocks,
    )?;
    writeln!(out)?;
    writeln!(
        out,
        "/// The combinations of properties that code points have."
    )?;
    writeln!(
        out,
        "pub static PROPERTIES: [Properties; {}] = [",
        entries.len()
    )?;
    for entry in &entries {
        let values: Vec<String> = fields
            .iter()
            .zip(entry)
            .map(|(field, &value)| {
                let rust = match value {
                    0 => field.absent,
                    value => field.values[usize::from(value) - 1].1,
                };
                format!("{}: {rust}", field.name)
            })
            .collect();
        writeln!(out, "    Properties {{ {} }},", values.join(", "))?;
    }
    writeln!(out, "];")?;
    Ok(())
}

/// The two first stages for blocks of 2^`shift` code points: the block of
/// each run, and the blocks, each told once.
fn stages(entry_of: &[u8], shift: u32) -> Result<(u32, Vec<u16>, Vec<u8>)> {
    let size = 1 << shift;
    let mut blocks = Vec::new();
    let mut known: HashMap<&[u8], u16> = HashMap::new();
    let mut index = Vec::new();
    for run in entry_of.chunks(size) {
        let next = u16::try_from(blocks.len() / size).context("too many blocks")?;
        let block = *known.entry(run).or_insert_with(|| {
            blocks.extend_from_slice(run);
            next
        });
        index.push(block);
    }
    Ok((shift, index, blocks))
}

/// The bytes that `index` takes, as bytes if every value fits in one.
fn index_bytes(index: &[u16]) -> usize {
    if index.iter().all(|&block| block <= u16::from(u8::MAX)) {
        index.len()
    } else {
        2 * index.len()
    }
}

/// Writes an array of numbers, 24 to a line.
fn numbers<T: std::fmt::Display>(
    out: &mut String,
    doc: &str,
    name: &str,
    ty: &str,
    values: &[T],
) -> Result<()> {
    writeln!(out)?;
    writeln!(out, "/// {doc}")?;
    writeln!(out, "pub static {name}: [{ty}; {}] = [", values.len())?;
    for line in values.chunks(24) {
        let line: Vec<String> = line.iter().map(ToString::to_string).collect();
        writeln!(out, "    {},", line.join(", "))?;
    }
    writeln!(out, "];")?;
    Ok(())
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
