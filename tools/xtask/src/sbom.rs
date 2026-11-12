//! The software bill of materials of the app's bundle, in CycloneDX's JSON
//! (<https://cyclonedx.org/docs/1.6/json/>): the crates of other projects
//! that the bundle's binaries hold, the app's and Sleip's, as `cargo tree`
//! gives them, of Cargo's own resolution of their default features for
//! Apple Silicon, through normal dependencies and without the procedural
//! macros, whose code runs as it builds; each with its version, its license
//! and its package's URL, of `cargo metadata`; and what the bundle holds
//! besides, Sparkle's framework, the symbols' font and Luau's type checker,
//! whose sources Sleip holds.

use std::collections::HashSet;
use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, ensure};
use cargo_metadata::{MetadataCommand, PackageId};
use serde_json::{Value, json};

use crate::bundle;

/// The packages of the bundle's binaries, the app's, Sleip's and the Dock
/// tile plug-in's.
const BINARIES: [&str; 3] = ["lokyr-app", "sleip-cli", "lokyr-platform-macos-docktile"];

/// The release of Luau whose type checker, its `Analysis` library, Sleip's
/// `sleip-analyzer` holds of its vendored sources.
const LUAU_RELEASE: &str = "0.740";

/// The platform that the bundle runs on.
const PLATFORM: &str = "aarch64-apple-darwin";

/// The release of Nerd Fonts whose symbols' font the binary holds, as the
/// font's README tells it.
const FONT_RELEASE: &str = "3.5.1";

/// The build-dependencies whose builds compile their sources into the
/// binary, as Luau's through its sources' crate, which the tree of the
/// normal dependencies leaves out.
const COMPILED_IN: [&str; 1] = ["luau0-src"];

/// The name of the component of Rust's standard library, which every Rust
/// binary holds.
pub(crate) const STD: &str = "rust-std";

/// A crate of another project that the binary holds.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Component {
    /// Its name.
    pub(crate) name: String,
    /// Its version.
    pub(crate) version: String,
    /// Its license, an SPDX expression, if its manifest tells one.
    pub(crate) license: Option<String>,
    /// Its authors, as its manifest names them.
    pub(crate) authors: Vec<String>,
    /// The directory of its source, which holds its files of its license.
    pub(crate) source: std::path::PathBuf,
}

/// The crates of other projects that the bundle's binaries hold, by name
/// and version: those of their normal dependencies, those of
/// [`COMPILED_IN`] of their build-dependencies, and Rust's standard
/// library, [`STD`].
pub(crate) fn crates(root: &Path) -> Result<Vec<Component>> {
    let tree = |edges: &str| -> Result<String> {
        let tree = Command::new(env!("CARGO"))
            .args(["tree", "--quiet", "--locked"])
            .args(BINARIES.iter().flat_map(|binary| ["-p", binary]))
            .args(["-e", edges, "--target", PLATFORM])
            .args(["--prefix", "none", "--format", "{p}"])
            .current_dir(root)
            .output()
            .context("running `cargo tree`")?;
        ensure!(
            tree.status.success(),
            "`cargo tree` failed: {}",
            String::from_utf8_lossy(&tree.stderr)
        );
        Ok(String::from_utf8_lossy(&tree.stdout).into_owned())
    };
    let mut built = built(&tree("normal,no-proc-macro")?);
    built.extend(
        self::built(&tree("normal,build,no-proc-macro")?)
            .into_iter()
            .filter(|(name, _)| COMPILED_IN.contains(&name.as_str())),
    );
    let metadata = MetadataCommand::new()
        .manifest_path(root.join("Cargo.toml"))
        .other_options(vec!["--locked".to_owned()])
        .exec()
        .context("running `cargo metadata`")?;
    let members: HashSet<&PackageId> = metadata.workspace_members.iter().collect();
    let mut components: Vec<Component> = metadata
        .packages
        .iter()
        .filter(|package| {
            !members.contains(&package.id)
                && built.contains(&(package.name.to_string(), package.version.to_string()))
        })
        .map(|package| Component {
            name: package.name.to_string(),
            version: package.version.to_string(),
            license: package.license.clone(),
            authors: package.authors.clone(),
            source: package
                .manifest_path
                .parent()
                .map(|directory| directory.as_std_path().to_owned())
                .unwrap_or_default(),
        })
        .collect();
    components.push(standard_library()?);
    components.sort();
    components.dedup_by(|a, b| a.name == b.name && a.version == b.version);
    Ok(components)
}

/// Rust's standard library of the toolchain that builds the app: its
/// version, `rustc`'s, its license, and the directory of its notices, the
/// toolchain's `share/doc/rust`.
fn standard_library() -> Result<Component> {
    let rustc = std::env::var("RUSTC").unwrap_or_else(|_| "rustc".to_owned());
    let output = |args: &[&str]| -> Result<String> {
        let ran = Command::new(&rustc)
            .args(args)
            .output()
            .with_context(|| format!("running `{rustc}`"))?;
        ensure!(ran.status.success(), "`{rustc} {}` failed", args.join(" "));
        Ok(String::from_utf8_lossy(&ran.stdout).trim().to_owned())
    };
    // `rustc 1.99.0 (b940084d7 2026-09-28)`.
    let version = output(&["--version"])?
        .split_whitespace()
        .nth(1)
        .context("`rustc --version` tells no version")?
        .to_owned();
    let sysroot = output(&["--print", "sysroot"])?;
    Ok(Component {
        name: STD.to_owned(),
        version,
        license: Some("MIT OR Apache-2.0".to_owned()),
        authors: vec!["The Rust Project Developers".to_owned()],
        source: Path::new(&sysroot).join("share/doc/rust"),
    })
}

/// The packages, by name and version, of `cargo tree`'s lines without
/// their prefix, `name v1.2.3`, a path's or a repeat's mark after them.
fn built(tree: &str) -> HashSet<(String, String)> {
    tree.lines()
        .filter_map(|line| {
            let mut words = line.split_whitespace();
            let name = words.next()?;
            let version = words.next()?.strip_prefix('v')?;
            Some((name.to_owned(), version.to_owned()))
        })
        .collect()
}

/// The bill of materials of the app at `version`, whose binaries, its own
/// and Sleip's, hold `crates`, and whose bundle holds Sparkle of
/// `sparkle`.
pub(crate) fn bill(version: &str, crates: &[Component], sparkle: &str) -> Value {
    let mut components: Vec<Value> = crates
        .iter()
        .map(|component| {
            // The standard library is no crate of crates.io's.
            let purl = if component.name == STD {
                format!("pkg:generic/{STD}@{}", component.version)
            } else {
                format!("pkg:cargo/{}@{}", component.name, component.version)
            };
            let mut value = json!({
                "type": "library",
                "bom-ref": purl,
                "name": component.name,
                "version": component.version,
                "purl": purl,
            });
            // Cargo's old `/`, which SPDX writes `OR`.
            if let Some(license) = &component.license {
                value["licenses"] = json!([{ "expression": license.replace('/', " OR ") }]);
            }
            value
        })
        .collect();
    components.push(json!({
        "type": "application",
        "bom-ref": format!("{}.sleip@{version}", bundle::IDENTIFIER),
        "name": "Sleip",
        "version": version,
        "description": "The plugin package manager, the bundle's Contents/MacOS/sleip.",
        "licenses": [{ "expression": "MIT" }],
    }));
    components.push(json!({
        "type": "framework",
        "bom-ref": format!("pkg:github/sparkle-project/Sparkle@{sparkle}"),
        "name": "Sparkle",
        "version": sparkle,
        "purl": format!("pkg:github/sparkle-project/Sparkle@{sparkle}"),
        "description": "The updater, in the bundle's Contents/Frameworks.",
        "licenses": [{ "license": { "id": "MIT" } }],
    }));
    components.push(json!({
        "type": "library",
        "bom-ref": format!("pkg:github/luau-lang/luau@{LUAU_RELEASE}#Analysis"),
        "name": "Luau Analysis",
        "version": LUAU_RELEASE,
        "purl": format!("pkg:github/luau-lang/luau@{LUAU_RELEASE}"),
        "description": "Luau's type checker, whose vendored sources Sleip's sleip-analyzer holds; its notice in the bundle's licenses/luau-analysis.",
        "licenses": [{ "license": { "id": "MIT" } }],
    }));
    components.push(json!({
        "type": "file",
        "bom-ref": format!("pkg:github/ryanoasis/nerd-fonts@v{FONT_RELEASE}#SymbolsNerdFontMono-Regular.ttf"),
        "name": "Symbols Nerd Font Mono",
        "version": FONT_RELEASE,
        "purl": format!("pkg:github/ryanoasis/nerd-fonts@v{FONT_RELEASE}"),
        "description": "The font of symbols in the binary; its icons' sets are under their own licenses, in the bundle's licenses/symbols-nerd-font.",
        "licenses": [{ "license": { "id": "MIT" } }],
    }));
    json!({
        "bomFormat": "CycloneDX",
        "specVersion": "1.6",
        "version": 1,
        "metadata": {
            "component": {
                "type": "application",
                "bom-ref": format!("{}@{version}", bundle::IDENTIFIER),
                "name": bundle::NAME,
                "version": version,
                "licenses": [{ "expression": "MIT" }],
            }
        },
        "components": components,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    #[test]
    fn the_app_s_crates_are_others_and_shipped() {
        let crates = crates(&root()).unwrap();
        let names: Vec<&str> = crates
            .iter()
            .map(|component| component.name.as_str())
            .collect();
        // A crate that the binary links, with its license.
        let objc2 = crates
            .iter()
            .find(|component| component.name == "objc2")
            .expect("objc2 is in the binary");
        assert!(objc2.license.is_some());
        // Neither the workspace's own crates, nor a procedural macro, nor
        // a crate of the tests or of the build alone.
        assert!(!names.iter().any(|name| name.starts_with("lokyr-")));
        assert!(!names.iter().any(|name| name.starts_with("sleip-")));
        // Sleip's own, its resolver's among them.
        assert!(names.contains(&"pubgrub"), "{names:?}");
        assert!(!names.contains(&"serde_derive"));
        assert!(!names.contains(&"insta"));
        assert!(!names.contains(&"cc"));
        // A build-dependency's sources that the build compiles into the
        // binary, Luau's, and the standard library.
        assert!(names.contains(&"luau0-src"), "{names:?}");
        let std = crates
            .iter()
            .find(|component| component.name == STD)
            .expect("the standard library");
        assert_eq!(std.license.as_deref(), Some("MIT OR Apache-2.0"));
        assert!(std.source.is_dir(), "{}", std.source.display());
        // A procedural macro of test targets too, and its own dependencies,
        // which build alone; optional dependencies whose features are off,
        // which `cargo metadata`'s resolution names.
        for absent in [
            "displaydoc",
            "proc-macro2",
            "syn",
            "zerovec",
            "zerofrom",
            "objc2-io-surface",
        ] {
            assert!(!names.contains(&absent), "{absent}");
        }
        // Sorted, once each.
        assert!(crates.windows(2).all(|pair| pair[0] < pair[1]));
    }

    #[test]
    fn cargo_tree_s_lines_give_their_packages() {
        let tree = "lokyr-app v0.3.0 (/x/crates/app)\nlibc v0.2.190\nobjc2 v0.6.5 (*)\n\n";
        let built = built(tree);
        assert!(built.contains(&("libc".to_owned(), "0.2.190".to_owned())));
        assert!(built.contains(&("objc2".to_owned(), "0.6.5".to_owned())));
        assert!(built.contains(&("lokyr-app".to_owned(), "0.3.0".to_owned())));
        assert_eq!(built.len(), 3);
    }

    #[test]
    fn the_bill_names_the_app_its_crates_sleip_sparkle_luau_and_the_font() {
        let crates = vec![Component {
            name: "objc2".to_owned(),
            version: "0.6.5".to_owned(),
            license: Some("MIT".to_owned()),
            authors: Vec::new(),
            source: std::path::PathBuf::new(),
        }];
        let bill = bill("0.3.0-rc.1", &crates, "2.10.1");
        assert_eq!(bill["bomFormat"], "CycloneDX");
        assert_eq!(bill["metadata"]["component"]["name"], "Lokyr");
        assert_eq!(bill["metadata"]["component"]["version"], "0.3.0-rc.1");
        let components = bill["components"].as_array().unwrap();
        assert_eq!(components[0]["purl"], "pkg:cargo/objc2@0.6.5");
        assert_eq!(components[0]["licenses"][0]["expression"], "MIT");
        // Cargo's old `/` as SPDX's `OR`.
        let old_form = [Component {
            name: "same-file".to_owned(),
            version: "1.0.6".to_owned(),
            license: Some("Unlicense/MIT".to_owned()),
            authors: Vec::new(),
            source: std::path::PathBuf::new(),
        }];
        let bill_of_old = super::bill("0.3.0", &old_form, "2.10.1");
        assert_eq!(
            bill_of_old["components"][0]["licenses"][0]["expression"],
            "Unlicense OR MIT"
        );
        assert_eq!(components[1]["name"], "Sleip");
        assert_eq!(components[1]["version"], "0.3.0-rc.1");
        assert_eq!(components[2]["name"], "Sparkle");
        assert_eq!(components[2]["version"], "2.10.1");
        assert_eq!(components[3]["name"], "Luau Analysis");
        assert_eq!(components[3]["version"], LUAU_RELEASE);
        assert_eq!(components[4]["name"], "Symbols Nerd Font Mono");
    }

    #[test]
    fn luau_s_release_is_the_one_that_its_vendored_sources_tell() {
        let readme = std::fs::read_to_string(root().join("sleip/analyzer/luau/README.md")).unwrap();
        assert!(
            readme.contains(&format!(
                "Luau](https://github.com/luau-lang/luau) {LUAU_RELEASE}"
            )),
            "{readme}"
        );
    }

    #[test]
    fn the_font_s_release_is_the_one_that_its_readme_tells() {
        let readme =
            std::fs::read_to_string(root().join("crates/text-core/fonts/README.md")).unwrap();
        assert!(readme.contains(&format!("release v{FONT_RELEASE}")));
    }
}
