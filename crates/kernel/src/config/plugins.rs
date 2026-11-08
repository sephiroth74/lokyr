//! The `[plugins]` section: each plugin's own, `[plugins.<namespace>]`, by
//! the plugin's namespace, as the file has it, with the place of each of its
//! values, which the plugin's schema checks once the plugins are there, so
//! that its errors tell their lines.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Deserializer};
use serde_json::{Map, Value};
use toml_edit::{Item, TableLike};

use super::{Position, Span};

/// The plugins' sections, by their namespaces.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PluginsConfig {
    sections: BTreeMap<String, PluginSection>,
}

/// A plugin's section: its values, as JSON's, and their places in the file,
/// which two sections of the same values, as of two files, do not tell
/// apart.
#[derive(Clone, Debug)]
pub struct PluginSection {
    values: Map<String, Value>,
    /// The place of each value, by the keys and the indexes that lead to it,
    /// the section's own of none.
    places: BTreeMap<Vec<String>, Position>,
    /// The file.
    path: PathBuf,
    /// The first value that JSON does not hold, as a number that is not
    /// finite, by the keys and the indexes that lead to it, and why, which
    /// refuses the section alone.
    unread: Option<(Vec<String>, String)>,
}

impl PartialEq for PluginSection {
    fn eq(&self, other: &Self) -> bool {
        self.values == other.values && self.unread == other.unread
    }
}

impl<'de> Deserialize<'de> for PluginsConfig {
    /// Nothing of the section, which [`PluginsConfig::read`] reads with its
    /// places, of the file's document.
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        serde::de::IgnoredAny::deserialize(deserializer)?;
        Ok(Self::default())
    }
}

impl PluginsConfig {
    /// The sections, by their namespaces, in their order.
    pub fn sections(&self) -> impl Iterator<Item = (&str, &PluginSection)> {
        self.sections
            .iter()
            .map(|(namespace, section)| (namespace.as_str(), section))
    }

    /// The section of the plugin of `namespace`, if the file has one.
    pub fn get(&self, namespace: &str) -> Option<&PluginSection> {
        self.sections.get(namespace)
    }

    /// Whether `other`'s sections are where these are, in the same file,
    /// each of its values at the same place, which equality, of the values
    /// alone, does not tell.
    pub fn same_places(&self, other: &Self) -> bool {
        self.sections.len() == other.sections.len()
            && self.sections.iter().all(|(namespace, section)| {
                other.sections.get(namespace).is_some_and(|other| {
                    section.places == other.places && section.path == other.path
                })
            })
    }

    /// The `[plugins]` section of `root`, the document of `text`, the file
    /// at `path`: a table of a table of each plugin; the place and the
    /// error of what is not. A value that JSON does not hold, as a number
    /// that is not finite, refuses its section alone, which keeps it.
    pub(super) fn read(
        root: &dyn TableLike,
        text: &str,
        path: &Path,
    ) -> Result<Self, (Span, String)> {
        let Some(item) = root.get("plugins") else {
            return Ok(Self::default());
        };
        let Some(table) = item.as_table_like() else {
            return Err((
                item.span(),
                "`plugins` is a section, not a value".to_owned(),
            ));
        };
        let mut sections = BTreeMap::new();
        for (namespace, item) in table.iter() {
            let key = table.key(namespace).and_then(toml_edit::Key::span);
            let Some(section) = item.as_table_like() else {
                return Err((
                    item.span().or(key),
                    format!(
                        "`plugins.{namespace}` is a plugin's section, of its keys, not a value"
                    ),
                ));
            };
            let mut places = BTreeMap::new();
            let at = |span: Span| span.map(|span| Position::at(text, span.start));
            if let Some(place) = at(item.span().or(key)) {
                places.insert(Vec::new(), place);
            }
            let (mut values, mut unread) = (Map::new(), None);
            let mut path_to = Vec::new();
            for (name, value) in section.iter() {
                path_to.push(name.to_owned());
                if let Some(place) = at(section.key(name).and_then(toml_edit::Key::span)) {
                    places.insert(path_to.clone(), place);
                }
                match item_json(value, &mut path_to, &mut places, text) {
                    Ok(json) => {
                        values.insert(name.to_owned(), json);
                    }
                    // At the value's key, or its index.
                    Err((_, message)) => {
                        unread.get_or_insert((path_to.clone(), message));
                    }
                }
                // A value that did not read leaves its path as deep as it
                // went.
                path_to.clear();
            }
            sections.insert(
                namespace.to_owned(),
                PluginSection {
                    values,
                    places,
                    path: path.to_owned(),
                    unread,
                },
            );
        }
        Ok(Self { sections })
    }

    /// Writes the sections, each `[plugins.<namespace>]`, of their keys and
    /// their values, which read back as these.
    pub(super) fn write(&self, out: &mut String) {
        for (namespace, section) in &self.sections {
            let _ = writeln!(out, "\n[plugins.{}]", toml_edit::Key::new(namespace));
            for (key, value) in &section.values {
                let _ = writeln!(out, "{} = {}", toml_edit::Key::new(key), toml_value(value));
            }
        }
    }
}

impl PluginSection {
    /// Its values, by their keys.
    pub fn values(&self) -> &Map<String, Value> {
        &self.values
    }

    /// The first value of the section that JSON does not hold, by the keys
    /// and the indexes that lead to its place, and why, which refuses the
    /// section.
    pub fn unread(&self) -> Option<(&[String], &str)> {
        self.unread
            .as_ref()
            .map(|(path, message)| (path.as_slice(), message.as_str()))
    }

    /// The file.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The place of the value at `path`, by the keys and the indexes that
    /// lead to it, or of the nearest before it that has one, or of the
    /// section.
    pub fn place(&self, path: &[String]) -> Option<Position> {
        (0..=path.len())
            .rev()
            .find_map(|length| self.places.get(&path[..length]).copied())
    }

    /// The file and the place of the value at `path`, as an error starts:
    /// `path:line:column`.
    pub fn at(&self, path: &[String]) -> String {
        match self.place(path) {
            Some(Position { line, column }) => {
                format!("{}:{line}:{column}", self.path.display())
            }
            None => self.path.display().to_string(),
        }
    }
}

/// The JSON value of `item`, at `path`, whose values' places go to `places`.
fn item_json(
    item: &Item,
    path: &mut Vec<String>,
    places: &mut BTreeMap<Vec<String>, Position>,
    text: &str,
) -> Result<Value, (Span, String)> {
    match item {
        Item::None => Ok(Value::Null),
        Item::Value(value) => value_json(value, path, places, text),
        Item::Table(table) => table_json(table, path, places, text),
        Item::ArrayOfTables(tables) => {
            let mut items = Vec::new();
            for (index, table) in tables.iter().enumerate() {
                path.push(index.to_string());
                if let Some(span) = table.span() {
                    places.insert(path.clone(), Position::at(text, span.start));
                }
                items.push(table_json(table, path, places, text)?);
                path.pop();
            }
            Ok(Value::Array(items))
        }
    }
}

/// The JSON object of `table`'s keys, at `path`.
fn table_json(
    table: &dyn TableLike,
    path: &mut Vec<String>,
    places: &mut BTreeMap<Vec<String>, Position>,
    text: &str,
) -> Result<Value, (Span, String)> {
    let mut object = Map::new();
    for (name, item) in table.iter() {
        path.push(name.to_owned());
        if let Some(span) = table.key(name).and_then(toml_edit::Key::span) {
            places.insert(path.clone(), Position::at(text, span.start));
        }
        object.insert(name.to_owned(), item_json(item, path, places, text)?);
        path.pop();
    }
    Ok(Value::Object(object))
}

/// The JSON value of `value`, at `path`: a date or a time as its text.
fn value_json(
    value: &toml_edit::Value,
    path: &mut Vec<String>,
    places: &mut BTreeMap<Vec<String>, Position>,
    text: &str,
) -> Result<Value, (Span, String)> {
    Ok(match value {
        toml_edit::Value::String(text) => Value::String(text.value().clone()),
        toml_edit::Value::Integer(integer) => Value::from(*integer.value()),
        toml_edit::Value::Float(float) => serde_json::Number::from_f64(*float.value())
            .map(Value::Number)
            .ok_or_else(|| {
                (
                    value.span(),
                    format!("`{}` is not a finite number", path.join(".")),
                )
            })?,
        toml_edit::Value::Boolean(boolean) => Value::Bool(*boolean.value()),
        toml_edit::Value::Datetime(datetime) => Value::String(datetime.value().to_string()),
        toml_edit::Value::Array(array) => {
            let mut items = Vec::new();
            for (index, item) in array.iter().enumerate() {
                path.push(index.to_string());
                if let Some(span) = item.span() {
                    places.insert(path.clone(), Position::at(text, span.start));
                }
                items.push(value_json(item, path, places, text)?);
                path.pop();
            }
            Value::Array(items)
        }
        toml_edit::Value::InlineTable(table) => table_json(table, path, places, text)?,
    })
}

/// The TOML of `value`, a value of a plugin's section, which JSON's null is
/// none of.
fn toml_value(value: &Value) -> toml_edit::Value {
    match value {
        Value::Null => toml_edit::Value::from(""),
        Value::Bool(boolean) => toml_edit::Value::from(*boolean),
        Value::Number(number) => match number.as_i64() {
            Some(integer) => toml_edit::Value::from(integer),
            None => toml_edit::Value::from(number.as_f64().unwrap_or_default()),
        },
        Value::String(text) => toml_edit::Value::from(text.as_str()),
        Value::Array(items) => toml_edit::Value::Array(items.iter().map(toml_value).collect()),
        Value::Object(map) => {
            let mut table = toml_edit::InlineTable::new();
            for (key, value) in map {
                table.insert(key, toml_value(value));
            }
            toml_edit::Value::InlineTable(table)
        }
    }
}
