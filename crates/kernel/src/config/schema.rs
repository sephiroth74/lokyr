//! The config's schema: a JSON Schema of its sections and their keys, each
//! of its type, its values, its range, its default and its description, of
//! the plugins' schemas' subset and of Lokyr's annotations of `x-`, each
//! key's platforms, its control where its type does not tell it, the
//! guide's text and the example's value, of which the configuration's
//! guide and the settings' page are made. Its tests hold it to the parser.

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
            let default = node.get("default").map(data_of);
            SettingKey {
                name: key.clone(),
                description: node
                    .get("description")
                    .and_then(Ordered::text)
                    .unwrap_or_default()
                    .to_owned(),
                platforms: texts(node.get("x-platforms")),
                kind: kind_of(node),
                control: node
                    .get("x-control")
                    .and_then(Ordered::text)
                    .map(str::to_owned),
                value: value(key).or_else(|| default.clone()),
                default,
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

/// The first paragraph of `text`, its lines joined.
fn first_paragraph(text: &str) -> String {
    text.lines()
        .take_while(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
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

#[cfg(test)]
mod tests;
