//! A plugin's manifest, `plugin.toml`: its name, kind and version, the
//! plugin API that it needs, the thread that it runs on, when it activates,
//! the capabilities that it asks for, the commands of others that it runs,
//! what it contributes and the plugins that it needs. A manifest that does
//! not read tells where, by its line and its column.

use std::collections::BTreeMap;

use lokyr_plugin_sdk::command::{self, Capability};
use lokyr_plugin_sdk::keys;
use lokyr_plugin_sdk::ui::Slot;
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
pub type ManifestError = crate::FileError;

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
    contributes: RawContributes,
    #[serde(default)]
    dependencies: BTreeMap<toml::Spanned<String>, toml::Spanned<String>>,
}

/// What a plugin contributes, as the file writes it, of the places that
/// its checks tell.
#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
struct RawContributes {
    #[serde(default)]
    commands: Vec<RawContributed>,
    #[serde(default)]
    keybindings: Vec<RawKeybinding>,
    #[serde(default)]
    slots: Vec<toml::Spanned<String>>,
    config_schema: Option<toml::Spanned<String>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawContributed {
    name: toml::Spanned<String>,
    title: toml::Spanned<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawKeybinding {
    key: toml::Spanned<String>,
    command: toml::Spanned<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Plugin {
    name: toml::Spanned<String>,
    version: Option<toml::Spanned<String>>,
    kind: toml::Spanned<Kind>,
    api: Option<toml::Spanned<String>>,
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
        if let Some(api) = &plugin.api
            && !is_requirement(api.get_ref())
        {
            return Err(fail(
                api.span(),
                format!(
                    "`{}` is not a version's constraint, as `^0.1`",
                    api.get_ref()
                ),
            ));
        }
        for on in &file.activation.on {
            if !is_activation(on.get_ref()) {
                return Err(fail(
                    on.span(),
                    format!(
                        "`{}` is not an activation: `startup`, `command:<glob>`, `event:<name>`, `slot:<name>` of a slot's name, or `osc:<number>`",
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
        let namespace = name.rsplit('/').next().unwrap_or(name).to_owned();
        let contributes = contributed(file.contributes, kind, &namespace, &fail)?;
        let mut dependencies = BTreeMap::new();
        for (needed, constraint) in file.dependencies {
            if !is_name(needed.get_ref()) {
                return Err(fail(
                    needed.span(),
                    format!(
                        "`{}` is not a plugin's name, `owner/name`",
                        needed.get_ref()
                    ),
                ));
            }
            if !is_requirement(constraint.get_ref()) {
                return Err(fail(
                    constraint.span(),
                    format!(
                        "`{}` is not a version's constraint, as `^0.3`",
                        constraint.get_ref()
                    ),
                ));
            }
            dependencies.insert(needed.into_inner(), constraint.into_inner());
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
            api: plugin.api.map(toml::Spanned::into_inner),
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
            contributes,
            dependencies,
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
            let Some(plain) = Capability::of_name(key) else {
                return Err(format!("`{key}` is no capability"));
            };
            match value.get_ref() {
                toml::Value::Boolean(true) => Ok(vec![plain]),
                toml::Value::Boolean(false) => Ok(Vec::new()),
                _ => Err(format!("`{key}` is `true` or `false`")),
            }
        }
    }
}

/// The contributions of `raw`, of a plugin of `kind` and `namespace`,
/// checked: its commands of names, of its namespace but a built-in
/// plugin's, and of titles; its keys of the config's grammar, of commands'
/// names; its slots of the UI's; its schema a file of the package, within
/// it; or why not, as `fail` tells, at its place.
fn contributed(
    raw: RawContributes,
    kind: Kind,
    namespace: &str,
    fail: &impl Fn(std::ops::Range<usize>, String) -> ManifestError,
) -> Result<Contributes, ManifestError> {
    let mut contributes = Contributes::default();
    let prefix = format!("{namespace}.");
    for command in raw.commands {
        let name = command.name.get_ref();
        if !command::is_name(name) {
            return Err(fail(
                command.name.span(),
                format!("`{name}` is not a command's name, `namespace.action`"),
            ));
        }
        if kind != Kind::Builtin && !name.starts_with(&prefix) {
            return Err(fail(
                command.name.span(),
                format!("`{name}` is not of the plugin's namespace, `{namespace}`"),
            ));
        }
        if command.title.get_ref().trim().is_empty() {
            return Err(fail(
                command.title.span(),
                "a command's title is empty".to_owned(),
            ));
        }
        contributes.commands.push(Contributed {
            name: command.name.into_inner(),
            title: command.title.into_inner(),
        });
    }
    for binding in raw.keybindings {
        if let Err(why) = keys::parse_keys(binding.key.get_ref()) {
            return Err(fail(binding.key.span(), why));
        }
        if !command::is_name(binding.command.get_ref()) {
            return Err(fail(
                binding.command.span(),
                format!(
                    "`{}` is not a command's name, `namespace.action`",
                    binding.command.get_ref()
                ),
            ));
        }
        contributes.keybindings.push(Keybinding {
            key: binding.key.into_inner(),
            command: binding.command.into_inner(),
        });
    }
    for slot in raw.slots {
        if !is_slot(slot.get_ref()) {
            return Err(fail(
                slot.span(),
                format!("`{}` is no slot: {}", slot.get_ref(), slot_names()),
            ));
        }
        contributes.slots.push(slot.into_inner());
    }
    if let Some(schema) = raw.config_schema {
        use std::path::{Component, Path};
        let path = Path::new(schema.get_ref());
        let within = !schema.get_ref().is_empty()
            && path
                .components()
                .all(|component| matches!(component, Component::Normal(_)));
        if !within {
            return Err(fail(
                schema.span(),
                format!(
                    "`{}` is not a file of the package, a path within it",
                    schema.get_ref()
                ),
            ));
        }
        contributes.config_schema = Some(schema.into_inner());
    }
    Ok(contributes)
}

/// Whether `name` is the name of a slot of the UI, as `tab-bar`.
fn is_slot(name: &str) -> bool {
    Slot::ALL.iter().any(|slot| slot.name() == name)
}

/// The slots' names, as an error lists them.
fn slot_names() -> String {
    Slot::ALL
        .iter()
        .map(|slot| format!("`{}`", slot.name()))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Whether `name` is `owner/name`, each part of lowercase letters, digits
/// and `-`, from a letter.
pub(crate) fn is_name(name: &str) -> bool {
    name.split_once('/')
        .is_some_and(|(owner, rest)| is_part(owner) && is_part(rest))
}

/// Whether `part` is of lowercase letters, digits and `-`, from a letter.
pub(crate) fn is_part(part: &str) -> bool {
    let mut bytes = part.bytes();
    bytes.next().is_some_and(|first| first.is_ascii_lowercase())
        && bytes.all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

/// Whether `version` is a version of Semantic Versioning 2.0.0,
/// `major.minor.patch`, of numbers of no leading zero, with a pre-release
/// after a `-` and a build after a `+` if any, each of identifiers of
/// letters, digits and `-` after dots, a pre-release's numbers of no
/// leading zero: <https://semver.org/#backusnaur-form-grammar-for-valid-semver-versions>.
pub(crate) fn is_version(version: &str) -> bool {
    let (rest, build) = match version.split_once('+') {
        Some((rest, build)) => (rest, Some(build)),
        None => (version, None),
    };
    let (core, pre) = match rest.split_once('-') {
        Some((core, pre)) => (core, Some(pre)),
        None => (rest, None),
    };
    let number = |part: &str| {
        !part.is_empty()
            && part.bytes().all(|byte| byte.is_ascii_digit())
            && (part == "0" || !part.starts_with('0'))
    };
    let identifier = |part: &str| {
        !part.is_empty()
            && part
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    };
    let numbers: Vec<&str> = core.split('.').collect();
    numbers.len() == 3
        && numbers.iter().all(|part| number(part))
        && pre.is_none_or(|pre| {
            pre.split('.').all(|part| {
                identifier(part)
                    && (!part.bytes().all(|byte| byte.is_ascii_digit()) || number(part))
            })
        })
        && build.is_none_or(|build| build.split('.').all(identifier))
}

/// Whether `constraint` is a version's constraint: an operator, `^`, `~`,
/// `=`, `>=`, `<=`, `>` or `<`, or none, of a version or of its first
/// numbers, as `^0.3` or `>=1.2.3-beta.1`; or `*`.
pub(crate) fn is_requirement(constraint: &str) -> bool {
    if constraint == "*" {
        return true;
    }
    let version = [">=", "<=", "^", "~", "=", ">", "<"]
        .iter()
        .find_map(|operator| constraint.strip_prefix(operator))
        .unwrap_or(constraint)
        .trim_start();
    let core = version.split(['-', '+']).next().unwrap_or_default();
    let parts = core.split('.').count();
    match parts {
        3 => is_version(version),
        1 | 2 => {
            core == version
                && core.split('.').all(|part| {
                    !part.is_empty()
                        && part.bytes().all(|byte| byte.is_ascii_digit())
                        && (part == "0" || !part.starts_with('0'))
                })
        }
        _ => false,
    }
}

/// Whether `on` is an activation event.
fn is_activation(on: &str) -> bool {
    if on == "startup" {
        return true;
    }
    match on.split_once(':') {
        Some(("command" | "event", rest)) => !rest.is_empty(),
        Some(("slot", name)) => is_slot(name),
        Some(("osc", number)) => {
            !number.is_empty() && number.bytes().all(|byte| byte.is_ascii_digit())
        }
        _ => false,
    }
}

/// Whether `pattern` is a command's name, `namespace.action`, of as many
/// parts as a command's name may have, or a namespace's, `namespace.*`.
fn is_pattern(pattern: &str) -> bool {
    match pattern.strip_suffix(".*") {
        Some(namespace) => namespace.split('.').all(is_part),
        None => command::is_name(pattern),
    }
}

/// The line and the column, both from 1, of the byte `at` of `text`.
pub(crate) fn line_and_column(text: &str, at: usize) -> (usize, usize) {
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

    #[test]
    fn a_version_is_semantic_versioning_s_and_a_constraint_of_one() {
        for good in [
            "1.2.3",
            "0.0.0",
            "1.2.3-beta.1",
            "1.2.3-0.alpha",
            "1.2.3+build.5",
            "1.2.3-rc.1+20261108",
        ] {
            assert!(is_version(good), "{good}");
        }
        for bad in [
            "1.2",
            "01.2.3",
            "1.02.3",
            "1.2.3-",
            "1.2.3-01",
            "1.2.3-a..b",
            "1.2.3+",
            "1.2.3+a_b",
            "v1.2.3",
            "1.2.3.4",
        ] {
            assert!(!is_version(bad), "{bad}");
        }
        for good in ["^0.1", "~1.2", ">=1.2.3-beta.1", "1", "=2.0.0", "*", "<3"] {
            assert!(is_requirement(good), "{good}");
        }
        for bad in ["^", "0.1.x", "^01.2", ">>1", "1.2-beta", "latest", ""] {
            assert!(!is_requirement(bad), "{bad}");
        }
    }

    #[test]
    fn a_package_s_contributions_dependencies_and_uses_are_checked_at_their_places() {
        let wrong = |text: &str| Manifest::parse(text).unwrap_err().to_string();
        let luau = "[plugin]\nname = \"acme/x\"\nversion = \"1.0.0\"\nkind = \"luau\"\napi = \"^0.1\"\nlicense = \"MIT\"\n";
        // A command of another plugin's namespace, of no title.
        assert_eq!(
            wrong(&format!(
                "{luau}[contributes]\ncommands = [{{ name = \"tabs.new\", title = \"New\" }}]\n"
            )),
            "8:22: `tabs.new` is not of the plugin's namespace, `x`"
        );
        assert!(
            wrong(&format!(
                "{luau}[contributes]\ncommands = [{{ name = \"x.go\", title = \" \" }}]\n"
            ))
            .starts_with("8:38: a command's title is empty")
        );
        // A key that does not read, a slot that is none, a schema out of
        // the package.
        assert!(
            wrong(&format!(
                "{luau}[contributes]\nkeybindings = [{{ key = \"cmd+\", command = \"x.go\" }}]\n"
            ))
            .starts_with("8:24: `` is not a key")
        );
        assert!(
            wrong(&format!("{luau}[contributes]\nslots = [\"tabbar\"]\n"))
                .starts_with("8:10: `tabbar` is no slot: `pane-header`")
        );
        assert!(
            wrong(&format!(
                "{luau}[contributes]\nconfig-schema = \"../x.json\"\n"
            ))
            .starts_with("8:17: `../x.json` is not a file of the package")
        );
        // A dependency's name and constraint, the API's constraint.
        assert!(
            wrong(&format!("{luau}[dependencies]\nThemes = \"^0.3\"\n"))
                .starts_with("8:1: `Themes` is not a plugin's name")
        );
        assert!(
            wrong(&format!(
                "{luau}[dependencies]\n\"lokyr/themes\" = \"newest\"\n"
            ))
            .starts_with("8:18: `newest` is not a version's constraint")
        );
        assert!(
            wrong(&luau.replace("^0.1", "one"))
                .starts_with("5:7: `one` is not a version's constraint")
        );
        // An activation of a slot that is none.
        assert!(
            wrong(&format!("{luau}[activation]\non = [\"slot:sidebar\"]\n"))
                .starts_with("8:7: `slot:sidebar` is not an activation")
        );
        // Commands of three parts, and namespaces of two, in `uses`.
        let manifest = Manifest::parse(&format!(
            "{luau}[uses]\ncommands = [\"search-plus.open.recent\", \"search-plus.open.*\"]\n[contributes]\nslots = [\"tab-bar\"]\n"
        ))
        .unwrap();
        assert_eq!(
            manifest.uses,
            ["search-plus.open.recent", "search-plus.open.*"]
        );
        assert_eq!(manifest.contributes.slots, ["tab-bar"]);
        // A built-in plugin contributes the app's commands.
        let builtin = "[plugin]\nname = \"lokyr/quick\"\nkind = \"builtin\"\nlicense = \"MIT\"\n[contributes]\ncommands = [{ name = \"window.toggle-quick\", title = \"Quick\" }]\n";
        assert!(Manifest::parse(builtin).is_ok());
    }
}
