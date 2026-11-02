//! The software bill of materials of the app's bundle, in CycloneDX's JSON
//! (<https://cyclonedx.org/docs/1.6/json/>): the crates of other projects
//! that the app's binary holds, as `cargo tree` gives them, of Cargo's own
//! resolution of the app's default features for Apple Silicon, through
//! normal dependencies and without the procedural macros, whose code runs
//! as it builds; each with its version, its license and its package's URL,
//! of `cargo metadata`; and what the bundle holds besides, Sparkle's
//! framework and the symbols' font.

use std::collections::HashSet;
use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, ensure};
use cargo_metadata::{MetadataCommand, PackageId};
use serde_json::{Value, json};

use crate::bundle;

/// The package of the app's binary.
const APP: &str = "lokyr-app";

/// The platform that the bundle runs on.
const PLATFORM: &str = "aarch64-apple-darwin";

/// The release of Nerd Fonts whose symbols' font the binary holds, as the
/// font's README tells it.
const FONT_RELEASE: &str = "3.5.1";

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

/// The crates of other projects that the app's binary holds, by name and
/// version.
pub(crate) fn crates(root: &Path) -> Result<Vec<Component>> {
    let tree = Command::new(env!("CARGO"))
        .args(["tree", "--quiet", "--locked", "-p", APP])
        .args(["-e", "normal,no-proc-macro", "--target", PLATFORM])
        .args(["--prefix", "none", "--format", "{p}"])
        .current_dir(root)
        .output()
        .context("running `cargo tree`")?;
    ensure!(
        tree.status.success(),
        "`cargo tree` failed: {}",
        String::from_utf8_lossy(&tree.stderr)
    );
    let built = built(&String::from_utf8_lossy(&tree.stdout));
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
    components.sort();
    components.dedup_by(|a, b| a.name == b.name && a.version == b.version);
    Ok(components)
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

/// The bill of materials of the app at `version`, whose binary holds
/// `crates`, and whose bundle holds Sparkle of `sparkle`.
pub(crate) fn bill(version: &str, crates: &[Component], sparkle: &str) -> Value {
    let mut components: Vec<Value> = crates
        .iter()
        .map(|component| {
            let purl = format!("pkg:cargo/{}@{}", component.name, component.version);
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
        "type": "framework",
        "bom-ref": format!("pkg:github/sparkle-project/Sparkle@{sparkle}"),
        "name": "Sparkle",
        "version": sparkle,
        "purl": format!("pkg:github/sparkle-project/Sparkle@{sparkle}"),
        "description": "The updater, in the bundle's Contents/Frameworks.",
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
        assert!(!names.contains(&"serde_derive"));
        assert!(!names.contains(&"insta"));
        assert!(!names.contains(&"cc"));
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
    fn the_bill_names_the_app_its_crates_sparkle_and_the_font() {
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
        assert_eq!(components[1]["name"], "Sparkle");
        assert_eq!(components[1]["version"], "2.10.1");
        assert_eq!(components[2]["name"], "Symbols Nerd Font Mono");
    }

    #[test]
    fn the_font_s_release_is_the_one_that_its_readme_tells() {
        let readme =
            std::fs::read_to_string(root().join("crates/text-core/fonts/README.md")).unwrap();
        assert!(readme.contains(&format!("release v{FONT_RELEASE}")));
    }
}
