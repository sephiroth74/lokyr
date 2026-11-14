//! `cargo xtask config-guide`: writes `docs/CONFIGURATION.md`, the config's
//! guide, of the config's schema, `crates/kernel/config.schema.json`, of
//! the plugins' schemas of the sections that it names, and of the guide's
//! template, `crates/kernel/config-guide.md`, its parts written by hand,
//! whose `{{example}}` takes the complete example; with `--check`, tells
//! whether the guide there is the schemas', without writing it.
//!
//! The schemas are read in their files' order, which the guide's sections
//! and keys follow: each section's text, then each key's heading, of the
//! keys that share it, its platforms and its text; the example of each
//! key's value and comment, the comments in a column.

use std::fs;
use std::path::Path;
use std::process::ExitCode;

use anyhow::{Context, Result, bail};
use lokyr_kernel::config::schema::Ordered;

use crate::luau_types::workspace_root;

/// The guide, relative to the workspace root.
const GUIDE: &str = "docs/CONFIGURATION.md";

/// The config's schema.
const SCHEMA: &str = "crates/kernel/config.schema.json";

/// The guide's template.
pub(crate) const TEMPLATE: &str = "crates/kernel/config-guide.md";

/// The place in the template of the complete example.
const EXAMPLE: &str = "{{example}}";

/// The column of the example's comments, past a key and its value.
const COMMENTS: usize = 37;

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
    let made = guide(&root)?;
    let path = root.join(GUIDE);
    if check {
        let held = fs::read_to_string(&path).with_context(|| format!("reading {GUIDE}"))?;
        if held != made {
            eprintln!("{GUIDE} is not the schemas': run `cargo xtask config-guide`");
            return Ok(ExitCode::FAILURE);
        }
        println!("{GUIDE} is the schemas'");
        return Ok(ExitCode::SUCCESS);
    }
    fs::write(&path, made).with_context(|| format!("writing {GUIDE}"))?;
    println!("wrote {GUIDE}");
    Ok(ExitCode::SUCCESS)
}

/// The guide of the schemas and the template under `root`.
pub fn guide(root: &Path) -> Result<String> {
    let read = |path: &str| -> Result<Ordered> {
        let text =
            fs::read_to_string(root.join(path)).with_context(|| format!("reading {path}"))?;
        serde_json::from_str(&text).with_context(|| format!("reading {path}"))
    };
    let schema = read(SCHEMA)?;
    let template =
        fs::read_to_string(root.join(TEMPLATE)).with_context(|| format!("reading {TEMPLATE}"))?;
    if !template.contains(EXAMPLE) {
        bail!("{TEMPLATE} has no `{EXAMPLE}`");
    }
    let plugins: Vec<(String, Ordered)> = schema
        .get("x-plugin-sections")
        .map(Ordered::items)
        .unwrap_or_default()
        .iter()
        .filter_map(Ordered::text)
        .map(|namespace| {
            Ok((
                namespace.to_owned(),
                read(&format!("plugins/{namespace}/{namespace}.schema.json"))?,
            ))
        })
        .collect::<Result<_>>()?;
    let made = Made {
        schema: &schema,
        plugins: &plugins,
    };
    Ok(template.replace(EXAMPLE, &made.example()?) + "\n" + &made.body()?)
}

/// The schemas that the guide is made of.
struct Made<'s> {
    schema: &'s Ordered,
    plugins: &'s [(String, Ordered)],
}

impl Made<'_> {
    /// The complete example, of every key's example and of the lines of
    /// the sections of their own, as `[keys]`.
    fn example(&self) -> Result<String> {
        let sections = self
            .schema
            .properties()
            .context("a schema of no `properties`")?;
        let mut out = Vec::new();
        for (name, node) in sections {
            match name.as_str() {
                "config_version" => out.extend(entry(name, node)?),
                "keys" => {
                    out.extend([String::new(), "[keys]".to_owned()]);
                    out.extend(lines(node));
                }
                "plugins" => {
                    out.push(String::new());
                    out.extend(lines(node));
                    for (namespace, plugin) in self.plugins {
                        out.extend([String::new(), format!("[plugins.{namespace}]")]);
                        for (key, node) in
                            plugin.properties().context("a schema of no `properties`")?
                        {
                            out.extend(entry(key, node)?);
                        }
                    }
                }
                _ => {
                    out.extend([String::new(), format!("[{name}]")]);
                    for (key, node) in node.properties().context("a schema of no `properties`")? {
                        out.extend(entry(key, node)?);
                    }
                }
            }
        }
        Ok(out.join("\n"))
    }

    /// The guide's sections: `config_version`, each section's, `[keys]`,
    /// `[plugins.<namespace>]` and each plugin's of the schema's names.
    fn body(&self) -> Result<String> {
        let mut out = Vec::new();
        for (name, node) in self
            .schema
            .properties()
            .context("a schema of no `properties`")?
        {
            match name.as_str() {
                "config_version" => {
                    out.push(format!("## `{name}`"));
                    out.extend(key_text(node)?);
                }
                "keys" => out.extend(section(name, node, None)?),
                "plugins" => {
                    let own = node
                        .get("additionalProperties")
                        .context("the plugins' section has no keys")?;
                    out.extend(section("plugins.<namespace>", node, Some(own))?);
                    for (namespace, plugin) in self.plugins {
                        out.extend(section(
                            &format!("plugins.{namespace}"),
                            plugin,
                            Some(plugin),
                        )?);
                    }
                }
                _ => out.extend(section(name, node, Some(node))?),
            }
        }
        // The guide starts its sections after a blank line.
        Ok(out.join("\n").trim_start_matches('\n').to_owned() + "\n")
    }
}

/// The lines of a section of `title`, its own text and, of `keys`' keys,
/// each heading and its text.
fn section(title: &str, node: &Ordered, keys: Option<&Ordered>) -> Result<Vec<String>> {
    let mut out = vec![String::new(), format!("## `[{title}]`")];
    if let Some(text) = node.get("x-guide").and_then(Ordered::text)
        && !text.is_empty()
    {
        out.extend([String::new(), text.to_owned()]);
    }
    let Some(keys) = keys else {
        return Ok(out);
    };
    for (key, node) in keys.properties().context("a schema of no `properties`")? {
        let group: Vec<&str> = node
            .get("x-group")
            .map(Ordered::items)
            .unwrap_or_default()
            .iter()
            .filter_map(Ordered::text)
            .collect();
        // The keys that share a heading show under their first.
        if group.first().is_some_and(|first| first != key) {
            continue;
        }
        let names = if group.is_empty() {
            vec![key.as_str()]
        } else {
            group
        };
        let heading: Vec<String> = names.iter().map(|name| format!("`{name}`")).collect();
        out.extend([String::new(), format!("### {}", heading.join(", "))]);
        out.extend(key_text(node)?);
    }
    Ok(out)
}

/// A key's lines under its heading: its platforms, then its text.
fn key_text(node: &Ordered) -> Result<Vec<String>> {
    let platforms: Vec<&str> = node
        .get("x-platforms")
        .map(Ordered::items)
        .unwrap_or_default()
        .iter()
        .filter_map(Ordered::text)
        .map(|name| match name {
            "macos" => Ok("macOS"),
            "linux" => Ok("Linux"),
            "windows" => Ok("Windows"),
            other => bail!("`{other}` is no platform"),
        })
        .collect::<Result<_>>()?;
    let mut out = vec![
        String::new(),
        format!("<sub>Platforms: {}</sub>", platforms.join(", ")),
    ];
    if let Some(text) = node.get("x-guide").and_then(Ordered::text) {
        out.extend([String::new(), text.to_owned()]);
    }
    Ok(out)
}

/// The example's lines of `key`: its value, of its comment in the
/// comments' column, or three spaces past a longer line; none of a key of
/// no example.
fn entry(key: &str, node: &Ordered) -> Result<Vec<String>> {
    let Some(value) = node.get("x-example").and_then(Ordered::text) else {
        return Ok(Vec::new());
    };
    let mut values = value.split('\n');
    let mut first = format!("{key} = {}", values.next().unwrap_or_default());
    if let Some(comment) = node.get("x-example-comment").and_then(Ordered::text) {
        let width = first.chars().count();
        if width < COMMENTS {
            first.push_str(&" ".repeat(COMMENTS - width));
        } else {
            first.push_str("   ");
        }
        first.push_str("# ");
        first.push_str(comment);
    }
    Ok(std::iter::once(first)
        .chain(values.map(str::to_owned))
        .collect())
}

/// The lines of a section's own example, of its `x-example-lines`.
fn lines(node: &Ordered) -> Vec<String> {
    node.get("x-example-lines")
        .map(Ordered::items)
        .unwrap_or_default()
        .iter()
        .filter_map(Ordered::text)
        .map(str::to_owned)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_example_s_comment_takes_its_column_or_three_spaces_past_a_long_line() {
        let node = |value: &str, comment: &str| {
            Ordered::Object(vec![
                ("x-example".to_owned(), Ordered::Scalar(value.into())),
                (
                    "x-example-comment".to_owned(),
                    Ordered::Scalar(comment.into()),
                ),
            ])
        };
        assert_eq!(
            entry("size", &node("13", "points")).unwrap(),
            [format!("size = 13{}# points", " ".repeat(COMMENTS - 9))]
        );
        let long = "\"a very long value that goes past the column\"";
        assert_eq!(
            entry("key", &node(long, "note")).unwrap(),
            [format!("key = {long}   # note")]
        );
        assert_eq!(
            entry("list", &node("[\n  \"a\",\n]", "items")).unwrap(),
            [
                format!("list = [{}# items", " ".repeat(COMMENTS - 8)),
                "  \"a\",".to_owned(),
                "]".to_owned()
            ]
        );
    }

    #[test]
    fn the_guide_is_the_schemas() {
        let root = workspace_root();
        let made = guide(&root).unwrap();
        let held = fs::read_to_string(root.join(GUIDE)).unwrap();
        assert!(
            made == held,
            "{GUIDE} is not the schemas': run `cargo xtask config-guide`"
        );
    }
}
