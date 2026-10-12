//! Apple's `footprint` tool, which breaks the footprint of a process into
//! categories of memory.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};

use anyhow::{Context, Result, bail};
use serde::Deserialize;

/// The footprint of a process, by category.
#[derive(Debug)]
pub struct Breakdown {
    /// The name of the process.
    pub name: String,
    /// The footprint of each category, in bytes. They add up to the
    /// footprint of the process.
    pub categories: BTreeMap<String, u64>,
}

impl Breakdown {
    /// Runs `footprint` on `pid`.
    pub fn of(pid: libc::pid_t) -> Result<Self> {
        let json = TempFile::new();
        let output = Command::new("/usr/bin/footprint")
            .arg("-j")
            .arg(&json.0)
            .arg(pid.to_string())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .output()
            .context("cannot run /usr/bin/footprint")?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!(
                "footprint failed: {}",
                stderr.lines().next().unwrap_or("no message")
            );
        }
        let text = fs::read_to_string(&json.0).context("cannot read the output of footprint")?;
        Self::parse(&text, pid)
    }

    /// Parses the JSON that `footprint -j` writes, for process `pid`.
    fn parse(json: &str, pid: libc::pid_t) -> Result<Self> {
        let output: Output =
            serde_json::from_str(json).context("unexpected output from footprint")?;
        let process = output
            .processes
            .into_iter()
            .find(|process| process.pid == pid)
            .with_context(|| format!("footprint did not report process {pid}"))?;
        let categories = process
            .categories
            .into_iter()
            .map(|(name, category)| (name, category.dirty * output.bytes_per_unit))
            .collect();
        Ok(Self {
            name: process.name,
            categories,
        })
    }

    /// The categories that hold GPU memory, with their sizes in bytes.
    pub fn gpu(&self) -> BTreeMap<String, u64> {
        self.categories
            .iter()
            .filter(|(name, size)| **size > 0 && is_gpu(name))
            .map(|(name, size)| (name.clone(), *size))
            .collect()
    }
}

/// Whether a `footprint` category holds GPU memory: IOSurface and
/// IOAccelerator memory, the GPU carveout, and whatever the graphics ledger
/// accounts, such as GPU memory the process owns without mapping it.
fn is_gpu(category: &str) -> bool {
    category.starts_with("IOAccelerator")
        || category.starts_with("IOSurface")
        || category.starts_with("GPU ")
        || category.ends_with("(graphics)")
}

/// The parts of `footprint -j` that the probe reads.
#[derive(Deserialize)]
struct Output {
    #[serde(rename = "bytes per unit")]
    bytes_per_unit: u64,
    processes: Vec<Process>,
}

#[derive(Deserialize)]
struct Process {
    name: String,
    pid: libc::pid_t,
    categories: BTreeMap<String, Category>,
}

#[derive(Deserialize)]
struct Category {
    /// The category's share of the footprint, compressed memory included.
    dirty: u64,
}

/// A path for `footprint` to write to, removed when dropped.
struct TempFile(PathBuf);

impl TempFile {
    fn new() -> Self {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let name = format!("saetta-bench-footprint-{}-{n}.json", std::process::id());
        Self(std::env::temp_dir().join(name))
    }
}

impl Drop for TempFile {
    fn drop(&mut self) {
        // The file may not exist if `footprint` failed early.
        let _ = fs::remove_file(&self.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A `footprint -j` output for Finder, cut down to a few categories whose
    /// sizes still add up to the footprint.
    const FINDER: &str = r#"{
      "unit": "byte",
      "bytes per unit": 1,
      "processes": [{
        "name": "Finder",
        "pid": 31308,
        "translated": false,
        "page size": 16384,
        "footprint": 182347776,
        "categories": {
          "Malloc Small": {"dirty": 159072256, "swapped": 0, "clean": 0, "reclaimable": 0, "wired": 0, "regions": 142},
          "CoreAnimation": {"dirty": 13512704, "swapped": 0, "clean": 0, "reclaimable": 3145728, "wired": 0, "regions": 647},
          "IOAccelerator (graphics)": {"dirty": 5750784, "swapped": 4702208, "clean": 0, "reclaimable": 5210112, "wired": 0, "regions": 50},
          "Owned physical footprint (unmapped) (graphics)": {"dirty": 2965504, "swapped": 720896, "clean": 0, "reclaimable": 0, "wired": 98304, "regions": 33},
          "IOSurface": {"dirty": 966656, "swapped": 212992, "clean": 0, "reclaimable": 0, "wired": 425984, "regions": 65},
          "IOAccelerator": {"dirty": 65536, "swapped": 0, "clean": 0, "reclaimable": 0, "wired": 65536, "regions": 2},
          "GPU Carveout (reserved)": {"dirty": 0, "swapped": 0, "clean": 0, "reclaimable": 0, "wired": 0, "regions": 1},
          "IOKit": {"dirty": 14336, "swapped": 0, "clean": 0, "reclaimable": 0, "wired": 0, "regions": 8}
        },
        "auxiliary": {"phys_footprint_peak": 474694928, "phys_footprint": 182347776}
      }],
      "errors": [],
      "warnings": []
    }"#;

    #[test]
    fn parses_the_categories() {
        let breakdown = Breakdown::parse(FINDER, 31308).unwrap();
        assert_eq!(breakdown.name, "Finder");
        assert_eq!(breakdown.categories.len(), 8);
        assert_eq!(breakdown.categories.values().sum::<u64>(), 182_347_776);
    }

    #[test]
    fn finds_the_gpu_categories() {
        let gpu = Breakdown::parse(FINDER, 31308).unwrap().gpu();
        let names: Vec<&str> = gpu.keys().map(String::as_str).collect();
        // Empty categories are left out.
        assert_eq!(
            names,
            [
                "IOAccelerator",
                "IOAccelerator (graphics)",
                "IOSurface",
                "Owned physical footprint (unmapped) (graphics)",
            ]
        );
        assert_eq!(gpu.values().sum::<u64>(), 9_748_480);
    }

    #[test]
    fn scales_by_the_unit() {
        let pages = FINDER.replace(r#""bytes per unit": 1"#, r#""bytes per unit": 16384"#);
        let breakdown = Breakdown::parse(&pages, 31308).unwrap();
        assert_eq!(breakdown.categories["IOAccelerator"], 65_536 * 16_384);
    }

    #[test]
    fn needs_the_requested_process() {
        let error = Breakdown::parse(FINDER, 1).unwrap_err();
        assert_eq!(error.to_string(), "footprint did not report process 1");
    }

    #[test]
    fn tells_gpu_categories_apart() {
        for name in [
            "IOAccelerator",
            "IOAccelerator (graphics)",
            "IOSurface",
            "GPU Carveout (reserved)",
            "Owned physical footprint (unmapped) (graphics)",
        ] {
            assert!(is_gpu(name), "{name}");
        }
        for name in [
            "Malloc Small",
            "CoreAnimation",
            "IOKit",
            "CG Image",
            "mapped file",
        ] {
            assert!(!is_gpu(name), "{name}");
        }
    }

    #[test]
    #[ignore = "runs Apple's footprint tool, which a sandbox may block"]
    fn breaks_down_its_own_process() {
        let pid = libc::pid_t::try_from(std::process::id()).unwrap();
        let breakdown = Breakdown::of(pid).unwrap();
        assert!(breakdown.categories.values().sum::<u64>() > 0);
    }
}
