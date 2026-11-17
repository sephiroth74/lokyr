//! The config's schema: a JSON Schema of its sections and their keys, each
//! of its type, its values, its range, its default and its description, of
//! the plugins' schemas' subset and of Lokyr's annotations of `x-`, each
//! key's platforms, its control where its type does not tell it, the
//! guide's text and the example's value, of which the configuration's
//! guide and the settings' page are made. Its tests hold it to the parser.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use toml_edit::TableLike;

pub use lokyr_pkg_model::schema::Ordered;
use lokyr_plugin_sdk::host::{SettingKey, SettingKind, SettingsSection};
use lokyr_plugin_sdk::message::Data;

/// The schema, as the kernel holds it, JSON.
pub const SCHEMA: &str = include_str!("../../config.schema.json");

/// A text key's bound: its section, its name, the most characters of its
/// text, or of each of its list's texts.
struct Longest {
    section: String,
    key: String,
    most: usize,
}

/// The text keys that the schema bounds by `maxLength`, of a text or of a
/// list's texts, read once.
fn bounds() -> &'static [Longest] {
    static BOUNDS: OnceLock<Vec<Longest>> = OnceLock::new();
    BOUNDS.get_or_init(|| {
        let schema: serde_json::Value = serde_json::from_str(SCHEMA).expect("the schema is JSON");
        let mut bounds = Vec::new();
        let sections = schema["properties"].as_object().into_iter().flatten();
        for (section, node) in sections {
            for (key, node) in node["properties"].as_object().into_iter().flatten() {
                let most = node["maxLength"]
                    .as_u64()
                    .or_else(|| node["items"]["maxLength"].as_u64());
                if let Some(most) = most.and_then(|most| usize::try_from(most).ok()) {
                    bounds.push(Longest {
                        section: section.clone(),
                        key: key.clone(),
                        most,
                    });
                }
            }
        }
        bounds
    })
}

/// Whether the texts of the config's `root` are within the schema's
/// bounds, each of its characters, a list's each of its own; the first
/// one past them, of its place and why, otherwise.
pub(super) fn check_lengths(
    root: &dyn TableLike,
) -> Result<(), (Option<std::ops::Range<usize>>, String)> {
    for bound in bounds() {
        let Some(value) = root
            .get(&bound.section)
            .and_then(toml_edit::Item::as_table_like)
            .and_then(|section| section.get(&bound.key))
            .and_then(toml_edit::Item::as_value)
        else {
            continue;
        };
        let texts: Vec<&toml_edit::Value> = match value {
            toml_edit::Value::Array(items) => items.iter().collect(),
            value => vec![value],
        };
        for text in texts {
            if text
                .as_str()
                .is_some_and(|text| text.chars().count() > bound.most)
            {
                return Err((
                    text.span(),
                    format!(
                        "`{}` in `[{}]` is a text of {} characters at most",
                        bound.key, bound.section, bound.most
                    ),
                ));
            }
        }
    }
    Ok(())
}

/// The schema's keys of its own, which no section of the settings shows:
/// the file's version, the bindings, and the plugins' sections, which their
/// own schemas describe.
const OWN: [&str; 3] = ["config_version", "keys", "plugins"];

/// The schema, read once in its order.
fn ordered() -> &'static Ordered {
    static ORDERED: OnceLock<Ordered> = OnceLock::new();
    ORDERED.get_or_init(|| serde_json::from_str(SCHEMA).expect("the schema is JSON"))
}

/// The app's sections of the settings, in the schema's order, each key of
/// its value in `config`, as a file of it would write it.
pub fn sections(config: &super::Config) -> Vec<SettingsSection> {
    let written: toml_edit::DocumentMut = config
        .written(false)
        .parse()
        .expect("the config writes as TOML");
    ordered()
        .properties()
        .unwrap_or_default()
        .iter()
        .filter(|(name, _)| !OWN.contains(&name.as_str()))
        .map(|(name, node)| {
            section(name, None, node, |key| {
                written
                    .get(name)
                    .and_then(|section| section.get(key))
                    .and_then(toml_edit::Item::as_value)
                    .map(data_of_toml)
            })
        })
        .collect()
}

/// The section of the settings `name`, a plugin's of the name `plugin`,
/// of `node`, a schema of an object: its description, its own or its
/// guide's first paragraph, and each of its keys, of its value in force,
/// `value`'s, or its default.
pub fn section(
    name: &str,
    plugin: Option<&str>,
    node: &Ordered,
    value: impl Fn(&str) -> Option<Data>,
) -> SettingsSection {
    let description = node
        .get("description")
        .and_then(Ordered::text)
        .map(str::to_owned)
        .or_else(|| {
            node.get("x-guide")
                .and_then(Ordered::text)
                .map(first_paragraph)
        })
        .unwrap_or_default();
    let keys = node
        .properties()
        .unwrap_or_default()
        .iter()
        .map(|(key, node)| {
            let kind = kind_of(node);
            // A whole number as an integer, as `200.0` and `200` are the
            // same value, which a plugin of Luau, of one kind of number,
            // takes alike.
            let as_kind = |data: Data| match data {
                Data::Number(number) if number.fract() == 0.0 && number.abs() < 9.0e15 => {
                    Data::Integer(number as i64)
                }
                data => data,
            };
            let default = node.get("default").map(data_of).map(as_kind);
            SettingKey {
                name: key.clone(),
                description: node
                    .get("description")
                    .and_then(Ordered::text)
                    .unwrap_or_default()
                    .to_owned(),
                platforms: texts(node.get("x-platforms")),
                control: node
                    .get("x-control")
                    .and_then(Ordered::text)
                    .map(str::to_owned),
                value: value(key).map(as_kind).or_else(|| default.clone()),
                default,
                kind,
                guide: guide_of(name, plugin.is_some(), key),
                docs: node.get("x-docs").and_then(Ordered::text).and_then(docs_of),
            }
        })
        .collect();
    SettingsSection {
        name: name.to_owned(),
        plugin: plugin.map(str::to_owned),
        description,
        keys,
    }
}

/// The values that a key of `node`'s schema takes.
fn kind_of(node: &Ordered) -> SettingKind {
    let number = |word: &str| match node.get(word) {
        Some(Ordered::Scalar(value)) => value.as_f64(),
        _ => None,
    };
    let count = |node: &Ordered, word: &str| match node.get(word) {
        Some(Ordered::Scalar(value)) => value.as_u64(),
        _ => None,
    };
    if let Some(choices) = node.get("enum") {
        return SettingKind::Choice {
            choices: choices.items().iter().map(data_of).collect(),
        };
    }
    if let Some(only) = node.get("const") {
        return SettingKind::Choice {
            choices: vec![data_of(only)],
        };
    }
    match node.get("type").and_then(Ordered::text) {
        Some("boolean") => SettingKind::Boolean,
        Some(kind @ ("number" | "integer")) => SettingKind::Number {
            minimum: number("minimum"),
            maximum: number("maximum"),
            step: number("x-step"),
            integer: kind == "integer",
        },
        Some("string") => SettingKind::Text {
            min_length: count(node, "minLength"),
            max_length: count(node, "maxLength"),
            examples: texts(node.get("examples")),
        },
        Some("array") => SettingKind::List {
            max_length: node
                .get("items")
                .and_then(|items| count(items, "maxLength")),
        },
        _ => SettingKind::Other,
    }
}

/// The texts of a list of `node`, none of another.
fn texts(node: Option<&Ordered>) -> Vec<String> {
    node.map(Ordered::items)
        .unwrap_or_default()
        .iter()
        .filter_map(Ordered::text)
        .map(str::to_owned)
        .collect()
}

/// The first paragraph of `text` of words, its lines joined: not one of
/// markup, as a guide's `<sub>` line of the platforms.
fn first_paragraph(text: &str) -> String {
    text.split("\n\n")
        .map(str::trim)
        .find(|paragraph| !paragraph.is_empty() && !paragraph.starts_with('<'))
        .map(|paragraph| paragraph.lines().collect::<Vec<_>>().join(" "))
        .unwrap_or_default()
}

/// `value` as plain data.
fn data_of(value: &Ordered) -> Data {
    match value {
        Ordered::Scalar(serde_json::Value::Bool(value)) => Data::Boolean(*value),
        Ordered::Scalar(serde_json::Value::Number(number)) => match number.as_i64() {
            Some(whole) => Data::Integer(whole),
            None => Data::Number(number.as_f64().unwrap_or_default()),
        },
        Ordered::Scalar(serde_json::Value::String(text)) => Data::Text(text.clone()),
        Ordered::Scalar(_) => Data::Nil,
        Ordered::Array(items) => Data::List(items.iter().map(data_of).collect()),
        Ordered::Object(entries) => Data::Map(
            entries
                .iter()
                .map(|(key, value)| (key.clone(), data_of(value)))
                .collect(),
        ),
    }
}

/// `value`, of TOML, as plain data.
fn data_of_toml(value: &toml_edit::Value) -> Data {
    match value {
        toml_edit::Value::Boolean(value) => Data::Boolean(*value.value()),
        toml_edit::Value::Integer(value) => Data::Integer(*value.value()),
        toml_edit::Value::Float(value) => Data::Number(*value.value()),
        toml_edit::Value::String(value) => Data::Text(value.value().clone()),
        toml_edit::Value::Datetime(value) => Data::Text(value.value().to_string()),
        toml_edit::Value::Array(items) => Data::List(items.iter().map(data_of_toml).collect()),
        toml_edit::Value::InlineTable(table) => Data::Map(
            table
                .iter()
                .map(|(key, value)| (key.to_owned(), data_of_toml(value)))
                .collect(),
        ),
    }
}

/// The config guide, as the schemas make it, whose headings name its keys'
/// places.
const GUIDE: &str = include_str!("../../../../docs/CONFIGURATION.md");

/// The address of the config guide of this version, on its repository at
/// the version's tag.
const GUIDE_ADDRESS: &str = concat!(
    "https://github.com/sephiroth74/lokyr/blob/v",
    env!("CARGO_PKG_VERSION"),
    "/docs/CONFIGURATION.md"
);

/// The documentation's site, whose pages a key's `x-docs` names.
const DOCS_ADDRESS: &str = "https://lokyr.dev/docs";

/// The address of the documentation's page `page`, of `docs/` and of its
/// anchor, as a key's `x-docs` names it, `shell-integration.md#limits`: the
/// site's page of it; none of a page that is not of Markdown.
pub fn docs_of(page: &str) -> Option<String> {
    let (path, anchor) = page.split_once('#').unwrap_or((page, ""));
    let stem = path.strip_suffix(".md")?;
    if stem.is_empty() || stem.contains("..") || stem.starts_with('/') {
        return None;
    }
    let anchor = if anchor.is_empty() {
        String::new()
    } else {
        format!("#{anchor}")
    };
    Some(format!("{DOCS_ADDRESS}/{stem}.html{anchor}"))
}

/// The address of the text of the key `key` of the section `section`, a
/// plugin's of its namespace where `plugin`, in the config guide: of the
/// anchor that GitHub makes of the key's heading under its section's;
/// none of a key that the guide lacks.
pub fn guide_of(section: &str, plugin: bool, key: &str) -> Option<String> {
    static ANCHORS: OnceLock<BTreeMap<(String, String), String>> = OnceLock::new();
    let anchors = ANCHORS.get_or_init(|| anchors(GUIDE));
    let section = if plugin {
        format!("plugins.{section}")
    } else {
        section.to_owned()
    };
    anchors
        .get(&(section, key.to_owned()))
        .map(|anchor| format!("{GUIDE_ADDRESS}#{anchor}"))
}

/// The anchors of the keys of `guide`, by their sections' names and their
/// own: each heading of keys, `### \`size\``, under its section's, `##
/// \`[font]\``, the anchor that GitHub makes of it, of its words in lower
/// case, of no punctuation but `-` and `_`, its spaces `-`, one made twice
/// of `-1`, `-2` after; the headings of code blocks none.
fn anchors(guide: &str) -> BTreeMap<(String, String), String> {
    let mut found = BTreeMap::new();
    let mut made: BTreeMap<String, usize> = BTreeMap::new();
    let mut section = String::new();
    let mut code = false;
    for line in guide.lines() {
        if line.trim_start().starts_with("```") {
            code = !code;
            continue;
        }
        if code {
            continue;
        }
        let level = line.chars().take_while(|c| *c == '#').count();
        let Some(heading) = line
            .get(level..)
            .and_then(|rest| rest.strip_prefix(' '))
            .filter(|_| level > 0)
        else {
            continue;
        };
        let slug: String = heading
            .to_lowercase()
            .chars()
            .filter_map(|c| match c {
                ' ' => Some('-'),
                c if c.is_alphanumeric() || c == '-' || c == '_' => Some(c),
                _ => None,
            })
            .collect();
        let times = made.entry(slug.clone()).or_insert(0);
        let anchor = if *times == 0 {
            slug.clone()
        } else {
            format!("{slug}-{times}")
        };
        *times += 1;
        // The names in backticks: a section's, `[font]`, or its keys'.
        let names: Vec<&str> = heading.split('`').skip(1).step_by(2).collect();
        match level {
            2 => {
                section = names
                    .first()
                    .and_then(|name| name.strip_prefix('['))
                    .and_then(|name| name.strip_suffix(']'))
                    .unwrap_or_default()
                    .to_owned();
            }
            3 if !section.is_empty() => {
                for name in names {
                    found.insert((section.clone(), name.to_owned()), anchor.clone());
                }
            }
            _ => {}
        }
    }
    found
}

/// `data` as a TOML value, of none of [`Data::Nil`], which TOML lacks.
pub fn toml_value_of(data: &Data) -> Option<toml_edit::Value> {
    Some(match data {
        Data::Nil => return None,
        Data::Boolean(value) => toml_edit::Value::from(*value),
        Data::Integer(value) => toml_edit::Value::from(*value),
        Data::Number(value) => toml_edit::Value::from(*value),
        Data::Text(value) => toml_edit::Value::from(value.as_str()),
        Data::List(items) => {
            let items: Option<Vec<toml_edit::Value>> = items.iter().map(toml_value_of).collect();
            toml_edit::Value::Array(items?.into_iter().collect())
        }
        Data::Map(map) => {
            let mut table = toml_edit::InlineTable::new();
            for (key, value) in map {
                table.insert(key, toml_value_of(value)?);
            }
            toml_edit::Value::InlineTable(table)
        }
    })
}

/// Whether `value` may be written to the app's key `key` of `[section]`:
/// a key of its section of the schema, and of a value that the parser
/// takes, the rest of `config` as it is; of none, the key going back to
/// its default, which the parser takes. The section of the bindings,
/// `[keys]`, is none of the settings'.
pub fn check_app_write(
    config: &super::Config,
    section: &str,
    key: &str,
    value: Option<&Data>,
) -> Result<(), (lokyr_plugin_sdk::command::Code, String)> {
    use lokyr_plugin_sdk::command::Code;
    // A binding of any keys, which the parser reads of their grammar.
    let binding = section == "keys";
    let known = binding
        || !OWN.contains(&section)
            && ordered()
                .properties()
                .unwrap_or_default()
                .iter()
                .find(|(name, _)| name == section)
                .and_then(|(_, node)| node.properties())
                .is_some_and(|keys| keys.iter().any(|(name, _)| name == key));
    if !known {
        return Err((
            Code::NotFound,
            format!("the settings have no key `{key}` in `[{section}]`"),
        ));
    }
    let Some(value) = value else {
        return Ok(());
    };
    let refused = |message: String| (Code::InvalidConfig, message);
    let value = toml_value_of(value)
        .ok_or_else(|| refused(format!("`{key}` in `[{section}]` takes no empty value")))?;
    let mut written: toml_edit::DocumentMut = config
        .written(false)
        .parse()
        .expect("the config writes as TOML");
    let table = written
        .entry(section)
        .or_insert_with(toml_edit::table)
        .as_table_like_mut()
        .ok_or_else(|| refused(format!("`{section}` is a section, not a value")))?;
    table.insert(key, toml_edit::Item::Value(value));
    super::Config::parse(&written.to_string(), std::path::Path::new("config.toml"))
        .map(drop)
        .map_err(|error| refused(error.message))
}

/// The bindings of keys in force, as a read of the settings tells them:
/// the defaults, `defaults`, whose keys no entry of `config`'s `[keys]`
/// names, however it writes them, and that no later default of the same
/// keys hides, as one of no context, or of the same, does, each of its
/// command's title and category that `info` tells, its arguments and its
/// context; then the entries of `[keys]`, in their keys' order, of their
/// commands, or of none for `false`, whose title and category are those of
/// the default of the same keys that ran, the last.
pub fn bindings(
    config: &super::Config,
    defaults: &[crate::keymap::Binding],
    info: impl Fn(&str) -> Option<(String, lokyr_plugin_sdk::command::Category)>,
) -> Vec<lokyr_plugin_sdk::host::SettingBinding> {
    use super::KeyValue;
    use lokyr_plugin_sdk::command::Args;
    use lokyr_plugin_sdk::host::SettingBinding;
    use lokyr_plugin_sdk::keys::{keys_text, parse_keys, same_keys};
    let named: Vec<Vec<crate::keymap::Combo>> = config
        .keys
        .bindings
        .keys()
        .filter_map(|keys| parse_keys(keys).ok())
        .collect();
    // A default that a later one of the same keys hides wherever it holds,
    // which never runs: ⌘W's of the window, under the tab's.
    let hidden = |at: usize, binding: &crate::keymap::Binding| {
        defaults[at + 1..].iter().any(|later| {
            same_keys(&later.keys, &binding.keys)
                && (later.when.is_none() || later.when == binding.when)
        })
    };
    let mut bindings: Vec<SettingBinding> = defaults
        .iter()
        .enumerate()
        .filter(|&(at, binding)| {
            !named.iter().any(|named| same_keys(named, &binding.keys)) && !hidden(at, binding)
        })
        .map(|(_, binding)| {
            let (title, category) = info(&binding.invocation.command).unzip();
            SettingBinding {
                keys: keys_text(&binding.keys),
                command: Some(binding.invocation.command.clone()),
                title,
                category,
                args: binding.invocation.args.clone(),
                when: binding.when.as_ref().map(ToString::to_string),
                detailed: binding.when.is_some() || !binding.invocation.args.is_empty(),
                default: true,
                system: lokyr_plugin_sdk::keys::is_system(
                    &binding.keys,
                    Some(&binding.invocation.command),
                ),
            }
        })
        .collect();
    for (keys, value) in &config.keys.bindings {
        let (command, args, when, detailed) = match value {
            KeyValue::Command(command) => (Some(command.clone()), Args::new(), None, false),
            KeyValue::Unbound(_) => (None, Args::new(), None, false),
            KeyValue::Binding(binding) => {
                let args = crate::keymap::args_of(&binding.args);
                let detailed = binding.when.is_some() || !args.is_empty();
                (
                    Some(binding.command.clone()),
                    args,
                    binding.when.clone(),
                    detailed,
                )
            }
            KeyValue::Bindings(bindings) => (
                bindings.first().map(|binding| binding.command.clone()),
                Args::new(),
                None,
                true,
            ),
        };
        // Of `false`, the default whose keys it takes away.
        let (title, category) = match &command {
            Some(command) => info(command).unzip(),
            None => parse_keys(keys)
                .ok()
                .and_then(|combos| {
                    defaults
                        .iter()
                        .rev()
                        .find(|binding| same_keys(&binding.keys, &combos))
                })
                .and_then(|binding| info(&binding.invocation.command))
                .unzip(),
        };
        bindings.push(SettingBinding {
            keys: keys.clone(),
            command,
            title,
            category,
            args,
            when,
            detailed,
            default: false,
            system: false,
        });
    }
    bindings
}

/// Whether `value` is one that a key of `kind` takes, as a plugin's
/// section's schema tells it: a boolean, a number of its range, an
/// integer of a key of integers, a text of its lengths, one of its
/// choices; a list's and another value's left to the schema's check as
/// the config applies.
pub fn check_kind(kind: &SettingKind, value: &Data) -> Result<(), String> {
    let number = match value {
        Data::Integer(value) => Some(*value as f64),
        Data::Number(value) => Some(*value),
        _ => None,
    };
    match kind {
        SettingKind::Boolean if matches!(value, Data::Boolean(_)) => Ok(()),
        SettingKind::Boolean => Err("it takes `true` or `false`".to_owned()),
        SettingKind::Number {
            minimum,
            maximum,
            integer,
            ..
        } => {
            let Some(number) = number.filter(|number| number.is_finite()) else {
                return Err("it takes a number".to_owned());
            };
            if *integer && number.fract() != 0.0 {
                return Err("it takes a whole number".to_owned());
            }
            if minimum.is_some_and(|least| number < least)
                || maximum.is_some_and(|most| number > most)
            {
                let at =
                    |bound: &Option<f64>| bound.map_or_else(String::new, |bound| bound.to_string());
                return Err(format!(
                    "it takes a number from {} to {}",
                    at(minimum),
                    at(maximum)
                ));
            }
            Ok(())
        }
        SettingKind::Text {
            min_length,
            max_length,
            ..
        } => {
            let Data::Text(text) = value else {
                return Err("it takes a text".to_owned());
            };
            let count = text.chars().count() as u64;
            if min_length.is_some_and(|least| count < least) {
                return Err("it takes a text that is not empty".to_owned());
            }
            if let Some(most) = max_length.filter(|most| count > *most) {
                return Err(format!("it takes a text of {most} characters at most"));
            }
            Ok(())
        }
        SettingKind::Choice { choices } => {
            let same = |choice: &Data| match (choice, value) {
                (Data::Integer(a), Data::Number(b)) | (Data::Number(b), Data::Integer(a)) => {
                    *a as f64 == *b
                }
                (a, b) => a == b,
            };
            if choices.iter().any(same) {
                Ok(())
            } else {
                Err("it takes one of its values".to_owned())
            }
        }
        SettingKind::List { .. } | SettingKind::Other => Ok(()),
    }
}

#[cfg(test)]
mod tests;
