//! `cargo xtask symbolicate <report.ips> [--dsym <dSYM>]`: the crashed
//! thread of a crash report of macOS, and the main thread, with the frames
//! of Lokyr's binary told by their functions, files and lines, through the
//! dSYM of the binary's UUID, which `just release` keeps for each release
//! in `target/releases/<version>`, or the one beside the last build's
//! binary, or the one that `--dsym` names, a `.dSYM` or its zip.
//!
//! A report is a line of JSON, its header, then a document of JSON, its
//! body: the images that the process had loaded, each with its UUID and
//! its base's address, and its threads, each frame an offset in an image.

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use anyhow::{Context, Result, bail, ensure};
use serde_json::Value;

use crate::bundle;

/// A frame of a thread, as the report tells it.
#[derive(Debug, PartialEq, Eq)]
struct Frame {
    /// The index of its image among the report's.
    image: usize,
    /// Its offset in the image.
    offset: u64,
    /// Its symbol, as the report names it, if it does.
    symbol: Option<String>,
}

/// A thread of the report.
#[derive(Debug, PartialEq, Eq)]
struct Thread {
    /// Its index among the report's.
    index: usize,
    /// Whether it crashed.
    crashed: bool,
    /// Its name or its queue's, if any.
    name: Option<String>,
    frames: Vec<Frame>,
}

/// An image that the process had loaded.
#[derive(Debug, PartialEq, Eq)]
struct Image {
    name: String,
    uuid: String,
    base: u64,
}

pub fn run(mut args: impl Iterator<Item = String>) -> Result<ExitCode> {
    let mut report = None;
    let mut dsym = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--dsym" => dsym = Some(PathBuf::from(args.next().context("--dsym names a dSYM")?)),
            _ if report.is_none() => report = Some(PathBuf::from(arg)),
            _ => bail!("symbolicate takes a report and `--dsym <dSYM>`, not `{arg}`"),
        }
    }
    let report = report.context("symbolicate takes the crash report, a `.ips` file")?;
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .context("the workspace's root")?;
    let text = std::fs::read_to_string(&report)
        .with_context(|| format!("reading {}", report.display()))?;
    let (images, threads) = parse(&text)?;
    let (index, binary) = images
        .iter()
        .enumerate()
        .find(|(_, image)| image.name == bundle::EXECUTABLE)
        .context("the report has no image of Lokyr's binary")?;
    let dsym = match dsym {
        Some(dsym) => unpacked(&dsym)?,
        None => found(&root, &binary.uuid)?,
    };
    ensure!(
        uuids(&dsym)?
            .iter()
            .any(|uuid| same_uuid(uuid, &binary.uuid)),
        "{} is not of the binary's UUID, {}",
        dsym.display(),
        binary.uuid
    );
    println!(
        "symbolicate: Lokyr's binary {} with {}",
        binary.uuid,
        dsym.display()
    );
    for thread in threads
        .iter()
        .filter(|thread| thread.crashed || thread.index == 0)
    {
        let addresses: Vec<u64> = thread
            .frames
            .iter()
            .filter(|frame| frame.image == index)
            .map(|frame| binary.base + frame.offset)
            .collect();
        let mut symbols = symbolicated(&dsym, binary.base, &addresses)?.into_iter();
        println!(
            "\nThread {}{}{}:",
            thread.index,
            if thread.crashed { ", crashed" } else { "" },
            thread
                .name
                .as_deref()
                .map(|name| format!(", {name}"))
                .unwrap_or_default()
        );
        for (number, frame) in thread.frames.iter().enumerate() {
            let image = images
                .get(frame.image)
                .map_or("?", |image| image.name.as_str());
            let symbol = if frame.image == index {
                symbols.next().unwrap_or_default()
            } else {
                frame
                    .symbol
                    .clone()
                    .unwrap_or_else(|| format!("+ {:#x}", frame.offset))
            };
            println!("{number:>3}  {image:<24} {symbol}");
        }
    }
    Ok(ExitCode::SUCCESS)
}

/// The images and the threads of the report's `text`.
fn parse(text: &str) -> Result<(Vec<Image>, Vec<Thread>)> {
    let (_, body) = text
        .split_once('\n')
        .context("the report has no body after its header's line")?;
    let body: Value = serde_json::from_str(body).context("the report's body is no JSON")?;
    let images = body["usedImages"]
        .as_array()
        .context("the report has no usedImages")?
        .iter()
        .map(|image| Image {
            name: image["name"].as_str().unwrap_or("?").to_owned(),
            uuid: image["uuid"].as_str().unwrap_or_default().to_owned(),
            base: image["base"].as_u64().unwrap_or_default(),
        })
        .collect();
    let faulting = body["faultingThread"].as_u64();
    let threads = body["threads"]
        .as_array()
        .context("the report has no threads")?
        .iter()
        .enumerate()
        .map(|(index, thread)| Thread {
            index,
            crashed: thread["triggered"].as_bool().unwrap_or(false)
                || faulting == u64::try_from(index).ok(),
            name: thread["name"]
                .as_str()
                .or_else(|| thread["queue"].as_str())
                .map(str::to_owned),
            frames: thread["frames"]
                .as_array()
                .map(|frames| {
                    frames
                        .iter()
                        .map(|frame| Frame {
                            image: frame["imageIndex"]
                                .as_u64()
                                .and_then(|index| usize::try_from(index).ok())
                                .unwrap_or(usize::MAX),
                            offset: frame["imageOffset"].as_u64().unwrap_or_default(),
                            symbol: frame["symbol"].as_str().map(str::to_owned),
                        })
                        .collect()
                })
                .unwrap_or_default(),
        })
        .collect();
    Ok((images, threads))
}

/// The dSYM of the binary of `uuid`: one that `just release` kept, its zip
/// unpacked beside it, or the one beside the last build's binary.
fn found(root: &Path, uuid: &str) -> Result<PathBuf> {
    let mut candidates = Vec::new();
    if let Ok(releases) = std::fs::read_dir(root.join("target/releases")) {
        for release in releases.flatten() {
            for file in std::fs::read_dir(release.path())
                .into_iter()
                .flatten()
                .flatten()
            {
                let path = file.path();
                if path.to_string_lossy().ends_with(".dSYM.zip") {
                    candidates.push(unpacked(&path)?);
                }
            }
        }
    }
    candidates.push(
        root.join("target/release")
            .join(format!("{}.dSYM", bundle::EXECUTABLE)),
    );
    for candidate in candidates {
        if candidate.exists() && uuids(&candidate)?.iter().any(|told| same_uuid(told, uuid)) {
            return Ok(candidate);
        }
    }
    bail!("no dSYM of the UUID {uuid} in target/releases or target/release: name one with --dsym")
}

/// The `.dSYM` of `path`, a `.dSYM` or its zip, unpacked beside it once.
fn unpacked(path: &Path) -> Result<PathBuf> {
    let name = path.to_string_lossy();
    let Some(stem) = name.strip_suffix(".zip") else {
        return Ok(path.to_owned());
    };
    let directory = PathBuf::from(format!("{stem}.unpacked"));
    if !directory.exists() {
        let status = Command::new("ditto")
            .args(["-x", "-k"])
            .arg(path)
            .arg(&directory)
            .status()
            .context("unpacking the dSYM")?;
        ensure!(
            status.success(),
            "unpacking {} failed: {status}",
            path.display()
        );
    }
    std::fs::read_dir(&directory)
        .with_context(|| format!("reading {}", directory.display()))?
        .flatten()
        .map(|entry| entry.path())
        .find(|path| {
            path.extension()
                .is_some_and(|extension| extension == "dSYM")
        })
        .with_context(|| format!("{} holds no .dSYM", path.display()))
}

/// The UUIDs of the code that `path` holds, as `dwarfdump --uuid` tells
/// them.
fn uuids(path: &Path) -> Result<Vec<String>> {
    let output = Command::new("dwarfdump")
        .arg("--uuid")
        .arg(path)
        .output()
        .with_context(|| format!("reading the UUIDs of {}", path.display()))?;
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| {
            line.strip_prefix("UUID: ")?
                .split(' ')
                .next()
                .map(str::to_owned)
        })
        .collect())
}

/// Whether two UUIDs are the same, whatever their case and their dashes.
fn same_uuid(a: &str, b: &str) -> bool {
    let plain = |uuid: &str| uuid.replace('-', "").to_uppercase();
    plain(a) == plain(b)
}

/// The functions, files and lines of `addresses` of the binary loaded at
/// `base`, through `atos` and the dSYM, one each.
fn symbolicated(dsym: &Path, base: u64, addresses: &[u64]) -> Result<Vec<String>> {
    if addresses.is_empty() {
        return Ok(Vec::new());
    }
    let output = Command::new("atos")
        .args(atos_arguments(dsym, base, addresses))
        .output()
        .context("running atos")?;
    ensure!(
        output.status.success(),
        "atos failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(demangled)
        .collect())
}

/// `atos`'s line with its Rust symbol demangled, which `atos` leaves as
/// the compiler mangles it: the symbol before ` (in `.
fn demangled(line: &str) -> String {
    let (symbol, rest) = line
        .split_once(" (in ")
        .map_or((line, ""), |(symbol, rest)| (symbol, rest));
    let name = symbol
        .strip_prefix('_')
        .filter(|name| name.starts_with("_R") || name.starts_with("_ZN"))
        .unwrap_or(symbol);
    let readable = rustc_demangle::try_demangle(name)
        .map_or_else(|_| symbol.to_owned(), |demangled| format!("{demangled:#}"));
    if rest.is_empty() {
        readable
    } else {
        format!("{readable} (in {rest}")
    }
}

/// The arguments of `atos` for `addresses` of the binary loaded at `base`.
fn atos_arguments(dsym: &Path, base: u64, addresses: &[u64]) -> Vec<String> {
    let mut arguments = vec![
        "-o".to_owned(),
        dsym.display().to_string(),
        "-arch".to_owned(),
        "arm64".to_owned(),
        "-l".to_owned(),
        format!("{base:#x}"),
    ];
    arguments.extend(addresses.iter().map(|address| format!("{address:#x}")));
    arguments
}

#[cfg(test)]
mod tests {
    use super::*;

    const REPORT: &str = concat!(
        r#"{"app_name":"Lokyr","bug_type":"309","app_version":"0.3.0"}"#,
        "\n",
        r#"{"procName":"lokyr","faultingThread":1,"#,
        r#""usedImages":[{"name":"lokyr","uuid":"23e00f53-3969-3158-8849-7ceec9d032c1","base":4294967296},"#,
        r#"{"name":"AppKit","uuid":"aaaa","base":6442450944}],"#,
        r#""threads":[{"queue":"com.apple.main-thread","frames":[{"imageOffset":4096,"imageIndex":0},"#,
        r#"{"imageOffset":64,"imageIndex":1,"symbol":"-[NSApplication run]"}]},"#,
        r#"{"triggered":true,"name":"lokyr-read","frames":[{"imageOffset":8192,"imageIndex":0}]}]}"#,
    );

    #[test]
    fn a_report_gives_its_images_and_its_threads() {
        let (images, threads) = parse(REPORT).unwrap();
        assert_eq!(images[0].name, "lokyr");
        assert_eq!(images[0].base, 0x1_0000_0000);
        assert_eq!(threads.len(), 2);
        assert!(!threads[0].crashed);
        assert_eq!(threads[0].name.as_deref(), Some("com.apple.main-thread"));
        assert_eq!(
            threads[0].frames[1].symbol.as_deref(),
            Some("-[NSApplication run]")
        );
        assert!(threads[1].crashed);
        assert_eq!(threads[1].name.as_deref(), Some("lokyr-read"));
        assert_eq!(
            threads[1].frames,
            [Frame {
                image: 0,
                offset: 8192,
                symbol: None
            }]
        );
        assert!(parse("no body").is_err());
    }

    #[test]
    fn uuids_match_whatever_their_case_and_dashes() {
        assert!(same_uuid(
            "23e00f53-3969-3158-8849-7ceec9d032c1",
            "23E00F5339693158884 97CEEC9D032C1"
                .replace(' ', "")
                .as_str()
        ));
        assert!(!same_uuid("23e00f53", "23e00f54"));
    }

    #[test]
    fn a_rust_symbol_of_atos_is_demangled() {
        assert_eq!(
            demangled("_RNvCs1234_5lokyr4main (in lokyr) (main.rs:33)"),
            "lokyr::main (in lokyr) (main.rs:33)"
        );
        assert_eq!(
            demangled("__ZN5lokyr4main17h0123456789abcdefE (in lokyr) (main.rs:33)"),
            "lokyr::main (in lokyr) (main.rs:33)"
        );
        assert_eq!(
            demangled("0x100000000 (in lokyr)"),
            "0x100000000 (in lokyr)"
        );
    }

    #[test]
    fn atos_takes_the_dsym_the_base_and_the_addresses() {
        let arguments = atos_arguments(Path::new("x.dSYM"), 0x1_0000_0000, &[0x1_0000_1000]);
        assert_eq!(
            arguments,
            [
                "-o",
                "x.dSYM",
                "-arch",
                "arm64",
                "-l",
                "0x100000000",
                "0x100001000"
            ]
        );
    }
}
