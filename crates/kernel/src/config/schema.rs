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
    let known = !OWN.contains(&section)
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
