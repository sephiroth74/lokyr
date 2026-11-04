//! A plugin's manifest, `plugin.toml`: its name, kind and version, the
//! plugin API that it needs, the thread that it runs on, when it activates,
//! the capabilities that it asks for, the commands of others that it runs,
//! what it contributes and the plugins that it needs. A manifest that does
//! not read tells where, by its line and its column.

use std::collections::BTreeMap;
use std::fmt;

use lokyr_plugin_sdk::command::Capability;
use serde::Deserialize;

/// A plugin's manifest, as it reads.
#[derive(Clone, Debug, PartialEq)]
pub struct Manifest {
    /// `owner/name`, as `lokyr/links`.
    pub name: String,
    /// What the package holds.
    pub kind: Kind,
    /// Its version, which a first-party plugin takes from the app's.
    pub version: Option<String>,
    /// The constraint on the plugin API's version, as `^0.1`.
    pub api: Option<String>,
    /// Its license, an SPDX expression.
    pub license: String,
    /// What it does, for the registry.
    pub description: Option<String>,
    /// Its repository's address.
    pub repository: Option<String>,
    /// Its home page's address.
    pub homepage: Option<String>,
    /// Words that find it in the registry.
    pub keywords: Vec<String>,
    /// Its authors.
    pub authors: Vec<String>,
    /// The systems that it runs on, as `macos`; every one when empty.
    pub platforms: Vec<String>,
    /// The thread that its calls run on.
    pub thread: Thread,
    /// When it activates: `startup`, `command:<glob>`, `event:<name>`,
    /// `slot:<name>` or `osc:<number>`.
    pub activation: Vec<String>,
    /// The capabilities that it asks for.
    pub capabilities: Vec<Capability>,
    /// The commands of other plugins that it runs, by their names or their
    /// namespaces, as `tabs.*`.
    pub uses: Vec<String>,
    /// What it contributes, which the palette, the registry and the
    /// validation read without running it.
    pub contributes: Contributes,
    /// The plugins that it needs, by their names, with their versions'
    /// constraints.
    pub dependencies: BTreeMap<String, String>,
}

/// What a package holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Kind {
    /// Luau's sources and their assets.
    Luau,
    /// Themes alone, which hold no code.
    Themes,
    /// A first-party plugin of the repository, linked in the app.
    Builtin,
}

/// The thread that a plugin's calls run on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Thread {
    /// The main thread's, for a plugin that answers at once.
    #[default]
    Main,
    /// A thread of the host's for the plugin alone.
    Worker,
}

/// Why a manifest does not read: where, and what.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ManifestError {
    /// The line and the column, both from 1, if the error has a place.
    pub at: Option<(usize, usize)>,
    /// What is wrong.
    pub message: String,
}

impl fmt::Display for ManifestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.at {
            Some((line, column)) => write!(f, "{line}:{column}: {}", self.message),
            None => f.write_str(&self.message),
        }
    }
}

impl std::error::Error for ManifestError {}

/// The file as serde reads it.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    plugin: Plugin,
    #[serde(default)]
    activation: Activation,
    #[serde(default)]
    capabilities: BTreeMap<String, toml::Spanned<toml::Value>>,
    #[serde(default)]
    uses: Uses,
    #[serde(default)]
    contributes: Contributes,
    #[serde(default)]
    dependencies: BTreeMap<String, String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Plugin {
    name: toml::Spanned<String>,
    version: Option<toml::Spanned<String>>,
    kind: toml::Spanned<Kind>,
    api: Option<String>,
    license: String,
    #[serde(default)]
    thread: Thread,
    description: Option<String>,
    repository: Option<String>,
    homepage: Option<String>,
    #[serde(default)]
    keywords: Vec<String>,
    #[serde(default)]
    authors: Vec<String>,
    #[serde(default)]
    platforms: Vec<String>,
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Activation {
    #[serde(default)]
    on: Vec<toml::Spanned<String>>,
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Uses {
    #[serde(default)]
    commands: Vec<toml::Spanned<String>>,
}

/// What a plugin contributes, as its manifest tells it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct Contributes {
    /// The commands that it registers.
    #[serde(default)]
    pub commands: Vec<Contributed>,
    /// The keys that it binds.
    #[serde(default)]
    pub keybindings: Vec<Keybinding>,
    /// The slots of the UI that it fills.
    #[serde(default)]
    pub slots: Vec<String>,
    /// The file of its config's schema, in the package.
    pub config_schema: Option<String>,
}

/// A command that a plugin registers.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Contributed {
    /// Its name, `namespace.action`.
    pub name: String,
    /// Its title.
    pub title: String,
}

/// Keys that a plugin binds to a command.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Keybinding {
    /// The keys, as the config writes them.
    pub key: String,
    /// The command.
    pub command: String,
}

/// The capabilities that a manifest asks for with `true`.
const PLAIN: [(&str, Capability); 12] = [
    ("terminal-read", Capability::TerminalRead),
    ("pty-write", Capability::PtyWrite),
    ("clipboard-read", Capability::ClipboardRead),
    ("clipboard-write", Capability::ClipboardWrite),
    ("notifications", Capability::Notifications),
    ("open", Capability::Open),
    ("process-spawn", Capability::ProcessSpawn),
    ("global-hotkey", Capability::GlobalHotkey),
    ("input-read", Capability::InputRead),
    ("history-read", Capability::HistoryRead),
    ("process-read", Capability::ProcessRead),
    ("user-commands", Capability::UserCommands),
];

impl Manifest {
    /// The manifest of `text`, or why it does not read, and where.
    pub fn parse(text: &str) -> Result<Self, ManifestError> {
        let at = |span: std::ops::Range<usize>| Some(line_and_column(text, span.start));
        let file: File = toml::from_str(text).map_err(|error| ManifestError {
            at: error.span().and_then(at),
            message: error.message().to_owned(),
        })?;
        let fail = |span: std::ops::Range<usize>, message: String| ManifestError {
            at: at(span),
            message,
        };
        let plugin = file.plugin;
        let name = plugin.name.get_ref();
        if !is_name(name) {
            return Err(fail(
                plugin.name.span(),
                format!("`{name}` is not `owner/name`, of lowercase letters, digits and `-`"),
            ));
        }
        let kind = *plugin.kind.get_ref();
        if let Some(version) = &plugin.version
            && !is_version(version.get_ref())
        {
            return Err(fail(
                version.span(),
                format!("`{}` is not a version, as `1.2.3`", version.get_ref()),
            ));
        }
        if plugin.version.is_none() && kind != Kind::Builtin {
            return Err(fail(
                plugin.kind.span(),
                "a plugin that is not built in needs its `version`".to_owned(),
            ));
        }
        if plugin.api.is_none() && kind == Kind::Luau {
            return Err(fail(
                plugin.kind.span(),
                "a Luau plugin needs the plugin API's version that it takes, `api`".to_owned(),
            ));
        }
        for on in &file.activation.on {
            if !is_activation(on.get_ref()) {
                return Err(fail(
                    on.span(),
                    format!(
                        "`{}` is not an activation: `startup`, `command:<glob>`, `event:<name>`, `slot:<name>` or `osc:<number>`",
                        on.get_ref()
                    ),
                ));
            }
        }
        let mut capabilities = Vec::new();
        for (key, value) in &file.capabilities {
            capabilities
                .extend(capability(key, value).map_err(|message| fail(value.span(), message))?);
        }
        for pattern in &file.uses.commands {
            if !is_pattern(pattern.get_ref()) {
                return Err(fail(
                    pattern.span(),
                    format!(
                        "`{}` is not a command's name, `namespace.action`, nor a namespace's, `namespace.*`",
                        pattern.get_ref()
                    ),
                ));
            }
        }
        let activation: Vec<String> = file
            .activation
            .on
            .into_iter()
            .map(toml::Spanned::into_inner)
            .collect();
        Ok(Self {
            name: plugin.name.into_inner(),
            kind,
            version: plugin.version.map(toml::Spanned::into_inner),
            api: plugin.api,
            license: plugin.license,
            description: plugin.description,
            repository: plugin.repository,
            homepage: plugin.homepage,
            keywords: plugin.keywords,
            authors: plugin.authors,
            platforms: plugin.platforms,
            thread: plugin.thread,
            activation: if activation.is_empty() && kind == Kind::Builtin {
                vec!["startup".to_owned()]
            } else {
                activation
            },
            capabilities,
            uses: file
                .uses
                .commands
                .into_iter()
                .map(toml::Spanned::into_inner)
                .collect(),
            contributes: file.contributes,
            dependencies: file.dependencies,
        })
    }

    /// The namespace of the plugin's own commands: its name's after its
    /// owner, as `links` of `lokyr/links`.
    pub fn namespace(&self) -> &str {
        self.name.rsplit('/').next().unwrap_or(&self.name)
    }
}

/// The capabilities of `key`, whose value is `value`: `true` for one that
/// takes nothing, `false` for none, and a list for `net`, `fs-read`,
/// `fs-write` and `env-read`, of hosts, paths and variables.
fn capability(key: &str, value: &toml::Spanned<toml::Value>) -> Result<Vec<Capability>, String> {
    let listed = |of: fn(String) -> Capability| match value.get_ref() {
        toml::Value::Array(items) => items
            .iter()
            .map(|item| match item {
                toml::Value::String(item) if !item.is_empty() => Ok(of(item.clone())),
                _ => Err(format!("`{key}` lists texts, none empty")),
            })
            .collect(),
        _ => Err(format!("`{key}` is a list, as `{key} = [\"…\"]`")),
    };
    match key {
        "net" => listed(Capability::Net),
        "fs-read" => listed(Capability::FsRead),
        "fs-write" => listed(Capability::FsWrite),
        "env-read" => listed(Capability::EnvRead),
        _ => {
            let Some((_, plain)) = PLAIN.iter().find(|(name, _)| *name == key) else {
                return Err(format!("`{key}` is no capability"));
            };
            match value.get_ref() {
                toml::Value::Boolean(true) => Ok(vec![plain.clone()]),
                toml::Value::Boolean(false) => Ok(Vec::new()),
                _ => Err(format!("`{key}` is `true` or `false`")),
            }
        }
    }
}

/// Whether `name` is `owner/name`, each part of lowercase letters, digits
/// and `-`, from a letter.
fn is_name(name: &str) -> bool {
    name.split_once('/')
        .is_some_and(|(owner, rest)| is_part(owner) && is_part(rest))
}

/// Whether `part` is of lowercase letters, digits and `-`, from a letter.
fn is_part(part: &str) -> bool {
    let mut bytes = part.bytes();
    bytes.next().is_some_and(|first| first.is_ascii_lowercase())
        && bytes.all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

/// Whether `version` is `major.minor.patch`, with a pre-release after a
/// `-` if any.
fn is_version(version: &str) -> bool {
    let core = version.split_once('-').map_or(version, |(core, _)| core);
    let numbers: Vec<&str> = core.split('.').collect();
    numbers.len() == 3
        && numbers
            .iter()
            .all(|number| !number.is_empty() && number.bytes().all(|byte| byte.is_ascii_digit()))
}

/// Whether `on` is an activation event.
fn is_activation(on: &str) -> bool {
    if on == "startup" {
        return true;
    }
    match on.split_once(':') {
        Some(("command" | "event" | "slot", rest)) => !rest.is_empty(),
        Some(("osc", number)) => {
            !number.is_empty() && number.bytes().all(|byte| byte.is_ascii_digit())
        }
        _ => false,
    }
}

/// Whether `pattern` is a command's name, `namespace.action`, or a
/// namespace's, `namespace.*`.
fn is_pattern(pattern: &str) -> bool {
    pattern
        .split_once('.')
        .is_some_and(|(namespace, action)| is_part(namespace) && (action == "*" || is_part(action)))
}

/// The line and the column, both from 1, of the byte `at` of `text`.
fn line_and_column(text: &str, at: usize) -> (usize, usize) {
    let before = &text[..at.min(text.len())];
    let line = before.matches('\n').count() + 1;
    let column = before
        .rsplit('\n')
        .next()
        .map_or(0, |line| line.chars().count())
        + 1;
    (line, column)
}

#[cfg(test)]
mod tests {
    use super::*;

    const LINKS: &str = r#"
[plugin]
name = "lokyr/links"
kind = "builtin"
license = "MIT"

[capabilities]
terminal-read = true
open = true
process-read = true
"#;

    #[test]
    fn a_built_in_plugin_s_manifest_reads_with_its_defaults() {
        let manifest = Manifest::parse(LINKS).unwrap();
        assert_eq!(manifest.name, "lokyr/links");
        assert_eq!(manifest.namespace(), "links");
        assert_eq!(manifest.kind, Kind::Builtin);
        assert_eq!(manifest.version, None);
        assert_eq!(manifest.thread, Thread::Main);
        assert_eq!(manifest.activation, ["startup"]);
        assert_eq!(
            manifest.capabilities,
            [
                Capability::Open,
                Capability::ProcessRead,
                Capability::TerminalRead
            ]
        );
        assert!(manifest.uses.is_empty());
    }

    #[test]
    fn a_luau_plugin_s_manifest_takes_every_field() {
        let manifest = Manifest::parse(
            r#"
[plugin]
name = "sephiroth74/search-plus"
version = "0.1.0"
kind = "luau"
api = "^0.1"
license = "MIT"
thread = "worker"
description = "Advanced search"
keywords = ["search"]
platforms = ["macos"]

[activation]
on = ["command:search-plus.*", "osc:1337"]

[capabilities]
clipboard-write = true
pty-write = false
net = ["api.github.com"]
env-read = ["AWS_*"]

[uses]
commands = ["pane.copy", "tabs.*"]

[contributes]
commands = [{ name = "search-plus.open", title = "Advanced search" }]
keybindings = [{ key = "cmd+shift+f", command = "search-plus.open" }]
slots = ["status-bar"]
config-schema = "schema.json"

[dependencies]
"lokyr/themes" = "^0.3"
"#,
        )
        .unwrap();
        assert_eq!(manifest.thread, Thread::Worker);
        assert_eq!(manifest.activation, ["command:search-plus.*", "osc:1337"]);
        assert_eq!(
            manifest.capabilities,
            [
                Capability::ClipboardWrite,
                Capability::EnvRead("AWS_*".to_owned()),
                Capability::Net("api.github.com".to_owned())
            ]
        );
        assert_eq!(manifest.uses, ["pane.copy", "tabs.*"]);
        assert_eq!(manifest.contributes.commands[0].name, "search-plus.open");
        assert_eq!(manifest.contributes.keybindings[0].key, "cmd+shift+f");
        assert_eq!(manifest.contributes.slots, ["status-bar"]);
        assert_eq!(manifest.platforms, ["macos"]);
        assert_eq!(manifest.dependencies["lokyr/themes"], "^0.3");
    }

    #[test]
    fn a_manifest_that_does_not_read_tells_its_line_and_column() {
        let wrong = |text: &str| Manifest::parse(text).unwrap_err().to_string();
        let plugin = "[plugin]\nname = \"lokyr/x\"\nkind = \"builtin\"\nlicense = \"MIT\"\n";
        // A capability misspelt, its value, a field that the format lacks.
        assert_eq!(
            wrong(&format!("{plugin}[capabilities]\ntermnal-read = true\n")),
            "6:16: `termnal-read` is no capability"
        );
        assert_eq!(
            wrong(&format!("{plugin}[capabilities]\nnet = true\n")),
            "6:7: `net` is a list, as `net = [\"…\"]`"
        );
        assert!(
            wrong(&format!("{plugin}color = \"red\"\n")).starts_with("5:1: unknown field `color`")
        );
        // A name, a version, an activation and a command of `uses`.
        assert_eq!(
            wrong("[plugin]\nname = \"Links\"\nkind = \"builtin\"\nlicense = \"MIT\"\n"),
            "2:8: `Links` is not `owner/name`, of lowercase letters, digits and `-`"
        );
        assert!(
            wrong(&plugin.replace("builtin\"", "builtin\"\nversion = \"1.2\""))
                .starts_with("4:11: `1.2` is not a version")
        );
        assert!(
            wrong(&format!("{plugin}[activation]\non = [\"boot\"]\n"))
                .starts_with("6:7: `boot` is not an activation")
        );
        assert!(
            wrong(&format!("{plugin}[uses]\ncommands = [\"copy\"]\n"))
                .starts_with("6:13: `copy` is not a command's name")
        );
        // What a kind needs.
        assert!(
            wrong(&plugin.replace("builtin", "themes"))
                .starts_with("3:8: a plugin that is not built in needs its `version`")
        );
        assert!(
            wrong(&plugin.replace("builtin\"", "luau\"\nversion = \"1.0.0\""))
                .starts_with("3:8: a Luau plugin needs")
        );
        // A text that is no TOML.
        assert!(wrong("[plugin\n").starts_with("1:8: "));
    }
}
