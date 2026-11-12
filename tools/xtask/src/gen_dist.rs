//! `cargo xtask gen-dist [--check]`: the app's distributions, of
//! `dist/*.toml`, as the app's features and its plugins' registrations.
//! Each distribution is a feature of `lokyr-app-core`, `dist-<name>`, of a
//! feature of each of its plugins, `plugin-<name>`, which takes the
//! plugin's crate, and a feature of `lokyr-app` of the same name; the
//! default distribution is the crates' default. `crates/app-core/src/
//! dist.rs` gives the build's plugins in the order of the files, `default`
//! first, the others by their names. What it writes in the manifests lies
//! between its markers; with `--check`, it writes nothing, and fails if
//! what it would write differs.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, Result, bail};

/// The line that opens a part of a manifest that the task writes.
const BEGIN: &str = "# What `cargo xtask gen-dist` writes of dist/*.toml, which it rewrites.";

/// The line that ends it.
const END: &str = "# The end of what `cargo xtask gen-dist` writes.";

/// A plugin of a distribution.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Plugin {
    /// Its name in the distributions, of its feature.
    key: String,
    /// Its crate's name.
    krate: String,
    /// The system that it is of alone, as `macos`, if it is.
    target: Option<String>,
}

/// A distribution: its name, and its plugins' keys, in their order.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Distribution {
    name: String,
    plugins: Vec<String>,
}

pub fn run(mut args: impl Iterator<Item = String>) -> Result<ExitCode> {
    let check = match args.next().as_deref() {
        None => false,
        Some("--check") => true,
        Some(arg) => bail!("gen-dist takes `--check` alone, not `{arg}`"),
    };
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let (distributions, plugins) = read(&root)?;
    let workspace =
        std::fs::read_to_string(root.join("Cargo.toml")).context("reading Cargo.toml")?;
    for plugin in &plugins {
        if !workspace.contains(&format!("\n{} = ", plugin.krate)) {
            bail!(
                "the plugin `{}`'s crate, `{}`, is not in the workspace's dependencies",
                plugin.key,
                plugin.krate
            );
        }
    }
    let files: [(PathBuf, Vec<String>); 3] = [
        (
            root.join("crates/app-core/Cargo.toml"),
            vec![
                core_features(&distributions, &plugins),
                dependencies(&plugins),
            ],
        ),
        (
            root.join("crates/app/Cargo.toml"),
            vec![app_features(&distributions)],
        ),
        (root.join("crates/app-core/src/dist.rs"), Vec::new()),
    ];
    let mut stale = Vec::new();
    for (path, parts) in files {
        let old = std::fs::read_to_string(&path)
            .with_context(|| format!("reading {}", path.display()))?;
        let new = if parts.is_empty() {
            registrations(&plugins)
        } else {
            replaced(&old, &parts).with_context(|| format!("in {}", path.display()))?
        };
        if new == old {
            continue;
        }
        if check {
            stale.push(path.display().to_string());
        } else {
            std::fs::write(&path, new).with_context(|| format!("writing {}", path.display()))?;
            println!("gen-dist: wrote {}", path.display());
        }
    }
    if !stale.is_empty() {
        eprintln!(
            "gen-dist: {} differ from dist/*.toml; run `cargo xtask gen-dist`",
            stale.join(", ")
        );
        return Ok(ExitCode::FAILURE);
    }
    if check {
        println!(
            "gen-dist: the {} distributions and their {} plugins are as written",
            distributions.len(),
            plugins.len()
        );
    }
    Ok(ExitCode::SUCCESS)
}

/// The distributions of `root`'s `dist/*.toml`, `default` first, and their
/// plugins, each once, in the order of the files.
fn read(root: &Path) -> Result<(Vec<Distribution>, Vec<Plugin>)> {
    let directory = root.join("dist");
    let mut files: Vec<PathBuf> = std::fs::read_dir(&directory)
        .with_context(|| format!("reading {}", directory.display()))?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "toml")
        })
        .collect();
    files.sort_by_key(|path| {
        let stem = path
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned());
        (stem.as_deref() != Some("default"), stem)
    });
    let (mut distributions, mut plugins) = (Vec::new(), Vec::<Plugin>::new());
    for file in files {
        let text = std::fs::read_to_string(&file)
            .with_context(|| format!("reading {}", file.display()))?;
        let (distribution, found) = parse(&text, &|path| crate_name(&directory.join(path)))
            .with_context(|| format!("in {}", file.display()))?;
        for plugin in found {
            match plugins.iter().find(|known| known.key == plugin.key) {
                Some(known) if *known != plugin => bail!(
                    "{}: the plugin `{}` is another crate, or of another system, than another distribution's",
                    file.display(),
                    plugin.key
                ),
                Some(_) => {}
                None => plugins.push(plugin),
            }
        }
        if distributions
            .iter()
            .any(|known: &Distribution| known.name == distribution.name)
        {
            bail!(
                "{}: a second distribution `{}`",
                file.display(),
                distribution.name
            );
        }
        distributions.push(distribution);
    }
    if !distributions
        .iter()
        .any(|distribution| distribution.name == "default")
    {
        bail!("dist/ has no distribution `default`");
    }
    Ok((distributions, plugins))
}

/// The distribution of `text`, a file of `dist/`, and its plugins, whose
/// crates' names `crate_of` gives of their paths.
fn parse(
    text: &str,
    crate_of: &dyn Fn(&str) -> Result<String>,
) -> Result<(Distribution, Vec<Plugin>)> {
    let document: toml_edit::Document<&str> = toml_edit::Document::parse(text)?;
    let root = document.as_table();
    let name = root
        .get("distribution")
        .and_then(|distribution| distribution.get("name"))
        .and_then(toml_edit::Item::as_str)
        .context("a distribution has `[distribution] name`")?;
    if !is_name(name) {
        bail!("`{name}` is not a distribution's name: lowercase letters, digits and `-`");
    }
    let table = root
        .get("plugins")
        .and_then(toml_edit::Item::as_table_like)
        .context("a distribution has `[plugins]`")?;
    let mut plugins = Vec::new();
    for (key, item) in table.iter() {
        if !is_name(key) {
            bail!("`{key}` is not a plugin's name: lowercase letters, digits and `-`");
        }
        let path = item
            .get("path")
            .and_then(toml_edit::Item::as_str)
            .with_context(|| format!("the plugin `{key}` has a `path`"))?;
        let target = match item.get("target") {
            None => None,
            Some(target) => match target.as_str() {
                Some(system @ ("macos" | "linux")) => Some(system.to_owned()),
                _ => bail!("the plugin `{key}`'s `target` is `\"macos\"` or `\"linux\"`"),
            },
        };
        plugins.push(Plugin {
            key: key.to_owned(),
            krate: crate_of(path)?,
            target,
        });
    }
    Ok((
        Distribution {
            name: name.to_owned(),
            plugins: plugins.iter().map(|plugin| plugin.key.clone()).collect(),
        },
        plugins,
    ))
}

/// Whether `name` is of lowercase letters, digits and `-`, from a letter.
fn is_name(name: &str) -> bool {
    name.starts_with(|c: char| c.is_ascii_lowercase())
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// The name of the crate at `directory`, of its `Cargo.toml`.
fn crate_name(directory: &Path) -> Result<String> {
    let manifest = directory.join("Cargo.toml");
    let text = std::fs::read_to_string(&manifest)
        .with_context(|| format!("reading {}", manifest.display()))?;
    let document: toml_edit::Document<&str> = toml_edit::Document::parse(text.as_str())?;
    document
        .get("package")
        .and_then(|package| package.get("name"))
        .and_then(toml_edit::Item::as_str)
        .map(str::to_owned)
        .with_context(|| format!("{} has no `[package] name`", manifest.display()))
}

/// The features of `lokyr-app-core`: the default distribution, each
/// distribution of its plugins, and each plugin of its crate.
fn core_features(distributions: &[Distribution], plugins: &[Plugin]) -> String {
    let mut out = String::from("default = [\"dist-default\"]\n");
    for distribution in distributions {
        let features: Vec<String> = distribution
            .plugins
            .iter()
            .map(|key| format!("\"plugin-{key}\""))
            .collect();
        let _ = writeln!(
            out,
            "dist-{} = [{}]",
            distribution.name,
            features.join(", ")
        );
    }
    for plugin in plugins {
        let _ = writeln!(out, "plugin-{} = [\"dep:{}\"]", plugin.key, plugin.krate);
    }
    out
}

/// The features of `lokyr-app`: each distribution, of `lokyr-app-core`'s.
fn app_features(distributions: &[Distribution]) -> String {
    let mut out = String::new();
    for distribution in distributions {
        let _ = writeln!(
            out,
            "dist-{0} = [\"lokyr-app-core/dist-{0}\"]",
            distribution.name
        );
    }
    out
}

/// The plugins' crates, as `lokyr-app-core`'s optional dependencies, of
/// the system that they are of alone, if they are.
fn dependencies(plugins: &[Plugin]) -> String {
    let line = |plugin: &Plugin| {
        format!(
            "{} = {{ workspace = true, optional = true }}\n",
            plugin.krate
        )
    };
    let mut out: String = plugins
        .iter()
        .filter(|plugin| plugin.target.is_none())
        .map(line)
        .collect();
    for system in ["macos", "linux"] {
        let of: Vec<&Plugin> = plugins
            .iter()
            .filter(|plugin| plugin.target.as_deref() == Some(system))
            .collect();
        if !of.is_empty() {
            let _ = writeln!(
                out,
                "\n[target.'cfg(target_os = \"{system}\")'.dependencies]"
            );
            out.extend(of.into_iter().map(line));
        }
    }
    out
}

/// `crates/app-core/src/dist.rs`: the build's plugins' registrations, in
/// their order.
fn registrations(plugins: &[Plugin]) -> String {
    let mut out = String::from(
        "//! The first-party plugins of the app's distributions, which `cargo xtask\n\
         //! gen-dist` writes of `dist/*.toml`: each distribution is a feature of\n\
         //! the crate, `dist-<name>`, of a feature of each of its plugins,\n\
         //! `plugin-<name>`; a build of several takes the plugins of each.\n\
         \n\
         use lokyr_kernel::plugins::Registration;\n\
         \n\
         /// The registrations of the build's plugins, in the order of their\n\
         /// distributions' files, which the app adds and activates in it.\n\
         pub(crate) fn plugins() -> Vec<Registration> {\n    vec![\n",
    );
    for plugin in plugins {
        let condition = match &plugin.target {
            None => format!("feature = \"plugin-{}\"", plugin.key),
            Some(system) => format!(
                "all(feature = \"plugin-{}\", target_os = \"{system}\")",
                plugin.key
            ),
        };
        let _ = writeln!(
            out,
            "        #[cfg({condition})]\n        {}::registration(),",
            plugin.krate.replace('-', "_")
        );
    }
    out.push_str("    ]\n}\n");
    out
}

/// `old`, its parts between the markers, in their order, each in place of
/// the one before.
fn replaced(old: &str, parts: &[String]) -> Result<String> {
    let mut out = String::new();
    let mut rest = old;
    for part in parts {
        let Some(begin) = rest.find(BEGIN) else {
            bail!("a part lacks its line `{BEGIN}`");
        };
        let after = begin + BEGIN.len();
        let Some(end) = rest[after..].find(END) else {
            bail!("a part lacks its line `{END}`");
        };
        out.push_str(&rest[..after]);
        out.push('\n');
        out.push_str(part);
        rest = &rest[after + end..];
        // A part that opens a table, as a system's dependencies, takes the
        // keys after its end into it: the next table starts there.
        let header = |line: &str| line.trim_start().starts_with('[');
        if part.lines().any(header)
            && let Some(key) = rest
                .lines()
                .skip(1)
                .take_while(|line| !header(line))
                .find(|line| !line.trim().is_empty() && !line.trim_start().starts_with('#'))
        {
            bail!(
                "a part opens a table, which `{}` after its end would fall into: move it before the part",
                key.trim()
            );
        }
    }
    if rest.contains(BEGIN) {
        bail!("more parts than the task writes");
    }
    out.push_str(rest);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_distribution_gives_its_features_dependencies_and_registrations() {
        let crate_of = |path: &str| Ok(format!("lokyr-{}", path.rsplit('/').next().unwrap()));
        let (default, mut plugins) = parse(
            "[distribution]\nname = \"default\"\n[plugins]\ncore-actions = { path = \"../plugins/core-actions\" }\nkeymap-macos = { path = \"../plugins/keymap-macos\", target = \"macos\" }\nlinks = { path = \"../plugins/links\" }\n",
            &crate_of,
        )
        .unwrap();
        assert_eq!(default.plugins, ["core-actions", "keymap-macos", "links"]);
        let (minimal, found) = parse(
            "[distribution]\nname = \"minimal\"\n[plugins]\ncore-actions = { path = \"../plugins/core-actions\" }\n",
            &crate_of,
        )
        .unwrap();
        assert_eq!(found[0], plugins[0]);
        let distributions = [default, minimal];
        assert_eq!(
            core_features(&distributions, &plugins),
            "default = [\"dist-default\"]\ndist-default = [\"plugin-core-actions\", \"plugin-keymap-macos\", \"plugin-links\"]\ndist-minimal = [\"plugin-core-actions\"]\nplugin-core-actions = [\"dep:lokyr-core-actions\"]\nplugin-keymap-macos = [\"dep:lokyr-keymap-macos\"]\nplugin-links = [\"dep:lokyr-links\"]\n"
        );
        assert_eq!(
            app_features(&distributions),
            "dist-default = [\"lokyr-app-core/dist-default\"]\ndist-minimal = [\"lokyr-app-core/dist-minimal\"]\n"
        );
        assert_eq!(
            dependencies(&plugins),
            "lokyr-core-actions = { workspace = true, optional = true }\nlokyr-links = { workspace = true, optional = true }\n\n[target.'cfg(target_os = \"macos\")'.dependencies]\nlokyr-keymap-macos = { workspace = true, optional = true }\n"
        );
        plugins.truncate(2);
        let written = registrations(&plugins);
        assert!(written.ends_with(
            "    vec![\n        #[cfg(feature = \"plugin-core-actions\")]\n        lokyr_core_actions::registration(),\n        #[cfg(all(feature = \"plugin-keymap-macos\", target_os = \"macos\"))]\n        lokyr_keymap_macos::registration(),\n    ]\n}\n"
        ), "{written}");
    }

    #[test]
    fn the_parts_go_between_their_markers_and_a_wrong_file_is_refused() {
        let old = format!(
            "[features]\n{BEGIN}\nold = []\n{END}\nkept = []\n\n[dependencies]\n{BEGIN}\n{END}\nlast = 1\n"
        );
        let new = replaced(&old, &["a = []\n".to_owned(), "b = 1\n".to_owned()]).unwrap();
        assert_eq!(
            new,
            format!(
                "[features]\n{BEGIN}\na = []\n{END}\nkept = []\n\n[dependencies]\n{BEGIN}\nb = 1\n{END}\nlast = 1\n"
            )
        );
        assert!(replaced(&old, &["a\n".to_owned()]).is_err());
        assert!(replaced("nothing\n", &["a\n".to_owned()]).is_err());
        // A part that opens a table ends where the next one starts, so that
        // no key after it falls into the table.
        let table = "a = 1\n\n[target.'cfg(unix)'.dependencies]\nb = 1\n".to_owned();
        let ended =
            format!("[dependencies]\n{BEGIN}\n{END}\n# kept\n\n[build-dependencies]\nc = 1\n");
        assert!(
            replaced(&ended, std::slice::from_ref(&table))
                .unwrap()
                .contains(&format!("b = 1\n{END}\n# kept\n\n[build-dependencies]"))
        );
        let falling = format!("[dependencies]\n{BEGIN}\n{END}\nc = 1\n");
        assert_eq!(
            replaced(&falling, &[table]).unwrap_err().to_string(),
            "a part opens a table, which `c = 1` after its end would fall into: move it before the part"
        );
        let crate_of = |_: &str| Ok("x".to_owned());
        for text in [
            "[plugins]\n",
            "[distribution]\nname = \"Big\"\n[plugins]\n",
            "[distribution]\nname = \"d\"\n",
            "[distribution]\nname = \"d\"\n[plugins]\nx = { }\n",
            "[distribution]\nname = \"d\"\n[plugins]\nx = { path = \"p\", target = \"windows\" }\n",
        ] {
            assert!(parse(text, &crate_of).is_err(), "{text}");
        }
    }
}
