//! The software bill of materials of the app's bundle, in CycloneDX's JSON
//! (<https://cyclonedx.org/docs/1.6/json/>): the crates of other projects
//! that the app's binary holds, as Cargo resolves them for Apple Silicon
//! with the app's default features, through normal dependencies, without
//! the procedural macros, whose code runs as it builds; each with its
//! version, its license and its package's URL; and what the bundle holds
//! besides, Sparkle's framework and the symbols' font.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::Path;

use anyhow::{Context, Result};
use cargo_metadata::{DependencyKind, Metadata, MetadataCommand, Package, PackageId, TargetKind};
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
}

/// The crates of other projects that the app's binary holds, by name and
/// version.
pub(crate) fn crates(root: &Path) -> Result<Vec<Component>> {
    let metadata = MetadataCommand::new()
        .manifest_path(root.join("Cargo.toml"))
        .other_options(vec![
            "--filter-platform".to_owned(),
            PLATFORM.to_owned(),
            "--locked".to_owned(),
        ])
        .exec()
        .context("running `cargo metadata`")?;
    shipped(&metadata)
}

/// The packages of `metadata` that the app reaches through normal
/// dependencies, but the workspace's own and the procedural macros.
fn shipped(metadata: &Metadata) -> Result<Vec<Component>> {
    let packages: HashMap<&PackageId, &Package> = metadata
        .packages
        .iter()
        .map(|package| (&package.id, package))
        .collect();
    let members: HashSet<&PackageId> = metadata.workspace_members.iter().collect();
    let resolve = metadata
        .resolve
        .as_ref()
        .context("`cargo metadata` returned no dependency resolution")?;
    let nodes: HashMap<&PackageId, _> = resolve.nodes.iter().map(|node| (&node.id, node)).collect();
    let app = metadata
        .packages
        .iter()
        .find(|package| package.name.as_str() == APP)
        .with_context(|| format!("no package {APP}"))?;
    let mut seen = HashSet::from([&app.id]);
    let mut queue = VecDeque::from([&app.id]);
    let mut components = Vec::new();
    while let Some(id) = queue.pop_front() {
        let node = nodes
            .get(id)
            .with_context(|| format!("{id} is not in the resolution"))?;
        for dep in &node.deps {
            let normal = dep
                .dep_kinds
                .iter()
                .any(|info| info.kind == DependencyKind::Normal);
            let package = packages
                .get(&dep.pkg)
                .with_context(|| format!("{} is not a package", dep.pkg))?;
            let macro_only = package
                .targets
                .iter()
                .all(|target| target.kind.contains(&TargetKind::ProcMacro));
            if !normal || macro_only || !seen.insert(&dep.pkg) {
                continue;
            }
            queue.push_back(&dep.pkg);
            if !members.contains(&dep.pkg) {
                components.push(Component {
                    name: package.name.to_string(),
                    version: package.version.to_string(),
                    license: package.license.clone(),
                });
            }
        }
    }
    components.sort();
    Ok(components)
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
            if let Some(license) = &component.license {
                value["licenses"] = json!([{ "expression": license }]);
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
        // Sorted, once each.
        assert!(crates.windows(2).all(|pair| pair[0] < pair[1]));
    }

    #[test]
    fn the_bill_names_the_app_its_crates_sparkle_and_the_font() {
        let crates = vec![Component {
            name: "objc2".to_owned(),
            version: "0.6.5".to_owned(),
            license: Some("MIT".to_owned()),
        }];
        let bill = bill("0.3.0-rc.1", &crates, "2.10.1");
        assert_eq!(bill["bomFormat"], "CycloneDX");
        assert_eq!(bill["metadata"]["component"]["name"], "Lokyr");
        assert_eq!(bill["metadata"]["component"]["version"], "0.3.0-rc.1");
        let components = bill["components"].as_array().unwrap();
        assert_eq!(components[0]["purl"], "pkg:cargo/objc2@0.6.5");
        assert_eq!(components[0]["licenses"][0]["expression"], "MIT");
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
