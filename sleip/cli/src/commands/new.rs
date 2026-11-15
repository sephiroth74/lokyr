//! `sleip new <plugin>`: a plugin of Luau that works, of the template that
//! Sleip holds, in a folder of its namespace, its name after the owner, in
//! the current folder: its manifest, `src/init.luau` of a command that
//! shows a notice, its texts in English, a test, the plugin API's types,
//! `lokyr.d.luau`, and luau-lsp's settings. The template's plugin is
//! `acme/hello`, of the namespace `hello`, which the plugin's name takes
//! the place of.

use std::path::{Path, PathBuf};

use lokyr_pkg_model::manifest::is_plugin_name;
use serde_json::json;

use crate::output::Out;

/// The template's files, by their paths in a plugin's folder, each whether
/// the plugin's name takes the place of the template's in it.
const TEMPLATE: [(&str, &str, bool); 8] = [
    (
        "plugin.toml",
        include_str!("../../../../crates/plugin-host-luau/luau/template/plugin.toml"),
        true,
    ),
    (
        "src/init.luau",
        include_str!("../../../../crates/plugin-host-luau/luau/template/src/init.luau"),
        true,
    ),
    (
        "locales/en/hello.ftl",
        include_str!("../../../../crates/plugin-host-luau/luau/template/locales/en/hello.ftl"),
        true,
    ),
    (
        "tests/hello.luau",
        include_str!("../../../../crates/plugin-host-luau/luau/template/tests/hello.luau"),
        true,
    ),
    (
        "README.md",
        include_str!("../../../../crates/plugin-host-luau/luau/template/README.md"),
        true,
    ),
    (
        ".luaurc",
        include_str!("../../../../crates/plugin-host-luau/luau/template/.luaurc"),
        false,
    ),
    (
        ".vscode/settings.json",
        include_str!("../../../../crates/plugin-host-luau/luau/template/.vscode/settings.json"),
        false,
    ),
    ("lokyr.d.luau", super::check::DEFINITIONS, false),
];

/// The template's plugin's name.
const TEMPLATE_NAME: &str = "acme/hello";

/// The template's plugin's namespace.
const TEMPLATE_NAMESPACE: &str = "hello";

/// The template's plugin API's constraint, which the plugin takes of the
/// API that this Sleip knows.
const TEMPLATE_API: &str = "api = \"^0.1\"";

/// Makes the plugin `plugin`, `owner/name`, in the current folder, as `out`
/// tells.
pub fn new(out: Out, plugin: &str) -> Result<(), String> {
    if !is_plugin_name(plugin) {
        return Err(format!(
            "`{plugin}` is not a plugin's name, `owner/name`, of lowercase letters, digits and `-`, each from a letter"
        ));
    }
    let (owner, namespace) = plugin.split_once('/').unwrap_or_default();
    if owner == "lokyr" {
        return Err("the owner `lokyr` is the first-party plugins'".to_owned());
    }
    let folder = std::env::current_dir()
        .map_err(|error| format!("the current folder cannot be read: {error}"))?
        .join(namespace);
    let empty = std::fs::read_dir(&folder).is_ok_and(|mut entries| entries.next().is_none());
    if folder.exists() && !empty {
        return Err(format!(
            "{} is there already, and not empty",
            folder.display()
        ));
    }
    let mut files = Vec::new();
    for (path, text, named) in TEMPLATE {
        let path = path.replace(
            &format!("/{TEMPLATE_NAMESPACE}."),
            &format!("/{namespace}."),
        );
        let text = if named {
            of_name(text, plugin, namespace)
        } else {
            text.to_owned()
        };
        write(&folder, &path, &text)?;
        files.push(path);
    }
    out.tell(
        "created",
        json!({ "name": plugin, "folder": folder.display().to_string(), "files": files }),
        &format!(
            "Made the plugin {plugin} in {}: its command, `{namespace}.say`, shows a notice.",
            folder.display()
        ),
    );
    Ok(())
}

/// `text`, a template's file, of the plugin `plugin` of `namespace`: its
/// namespace in place of each word `hello`, then its name in place of the
/// template's, so that a name of the word `hello`, as `acme/say-hello`,
/// stays as it is, and the plugin API's constraint of this Sleip's whole
/// version, as its types that the plugin is written against.
fn of_name(text: &str, plugin: &str, namespace: &str) -> String {
    let text = text.replace(
        TEMPLATE_API,
        &format!("api = \"^{}\"", lokyr_plugin_sdk::API_VERSION),
    );
    let mut named = String::with_capacity(text.len());
    let mut rest = text.as_str();
    let word =
        |char: Option<char>| char.is_some_and(|char| char.is_ascii_alphanumeric() || char == '_');
    while let Some(at) = rest.find(TEMPLATE_NAMESPACE) {
        let before = rest[..at].chars().next_back();
        let after = rest[at + TEMPLATE_NAMESPACE.len()..].chars().next();
        named.push_str(&rest[..at]);
        // A word of its own, as `hello.say`, not part of another.
        if word(before) || word(after) {
            named.push_str(TEMPLATE_NAMESPACE);
        } else {
            named.push_str(namespace);
        }
        rest = &rest[at + TEMPLATE_NAMESPACE.len()..];
    }
    named.push_str(rest);
    // The template's name, of the plugin's namespace now.
    named.replace(
        &TEMPLATE_NAME.replace(TEMPLATE_NAMESPACE, namespace),
        plugin,
    )
}

/// Writes `text` to `path` in `folder`, its folders made.
fn write(folder: &Path, path: &str, text: &str) -> Result<(), String> {
    let file: PathBuf = folder.join(path);
    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("{} cannot be made: {error}", parent.display()))?;
    }
    std::fs::write(&file, text)
        .map_err(|error| format!("{} cannot be written: {error}", file.display()))
}
