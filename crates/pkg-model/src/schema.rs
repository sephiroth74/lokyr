//! The JSON Schema of a plugin's config, of the keywords that the host
//! checks itself: a section of the config file is an object of the plugin's
//! keys, which the schema's `properties` describe, each with its `default`.
//!
//! The keywords are those of flat settings, of
//! [JSON Schema 2020-12](https://json-schema.org/draft/2020-12/json-schema-validation):
//! `type`, `enum`, `const`, `minimum`, `maximum`, `exclusiveMinimum`,
//! `exclusiveMaximum`, `minLength`, `maxLength`, `properties`, `required`,
//! `additionalProperties`, `items`, `minItems` and `maxItems`, with the
//! annotations `$schema`, `$id`, `$comment`, `title`, `description`,
//! `default`, `examples`, `deprecated`, `readOnly` and `writeOnly`, and
//! the schemas `true` and `false`; and the annotations of Lokyr's own,
//! every keyword of `x-`, as the guide's text and the settings' page's
//! control of a key. A schema of any other keyword does not read, by its
//! name.

mod ordered;

use std::collections::BTreeMap;

use serde_json::{Map, Value};

pub use ordered::Ordered;

/// The annotations, which a value is not checked against.
const ANNOTATIONS: [&str; 10] = [
    "$schema",
    "$id",
    "$comment",
    "title",
    "description",
    "default",
    "examples",
    "deprecated",
    "readOnly",
    "writeOnly",
];

/// The kinds of JSON values, of the keyword `type`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Type {
    Null,
    Boolean,
    Integer,
    Number,
    String,
    Array,
    Object,
}

impl Type {
    fn of(name: &str) -> Option<Self> {
        Some(match name {
            "null" => Self::Null,
            "boolean" => Self::Boolean,
            "integer" => Self::Integer,
            "number" => Self::Number,
            "string" => Self::String,
            "array" => Self::Array,
            "object" => Self::Object,
            _ => return None,
        })
    }

    fn name(self) -> &'static str {
        match self {
            Self::Null => "null",
            Self::Boolean => "a boolean",
            Self::Integer => "an integer",
            Self::Number => "a number",
            Self::String => "a string",
            Self::Array => "an array",
            Self::Object => "a table",
        }
    }

    /// Whether `value` is of the kind: an integer is a number whose
    /// fraction is zero, as JSON Schema has it.
    fn holds(self, value: &Value) -> bool {
        match self {
            Self::Null => value.is_null(),
            Self::Boolean => value.is_boolean(),
            Self::Integer => {
                value.is_i64()
                    || value.is_u64()
                    || value.as_f64().is_some_and(|number| number.fract() == 0.0)
            }
            Self::Number => value.is_number(),
            Self::String => value.is_string(),
            Self::Array => value.is_array(),
            Self::Object => value.is_object(),
        }
    }
}

/// What an object takes besides its `properties`.
#[derive(Clone, Debug, PartialEq)]
enum Additional {
    /// Any value, as the schema says nothing of them: at the section's
    /// top, its other keys are ignored, as another version's.
    Unsaid,
    /// Any value.
    Any,
    /// Nothing.
    None,
    /// Values of a schema.
    Of(Box<Node>),
}

/// A schema of a value.
#[derive(Clone, Debug, PartialEq)]
struct Node {
    /// Whether it takes no value, as the schema `false`.
    never: bool,
    types: Option<Vec<Type>>,
    choices: Option<Vec<Value>>,
    constant: Option<Value>,
    default: Option<Value>,
    minimum: Option<f64>,
    maximum: Option<f64>,
    above: Option<f64>,
    below: Option<f64>,
    shortest: Option<usize>,
    longest: Option<usize>,
    properties: BTreeMap<String, Node>,
    required: Vec<String>,
    additional: Additional,
    items: Option<Box<Node>>,
    fewest: Option<usize>,
    most: Option<usize>,
}

/// The schema of a plugin's config.
#[derive(Clone, Debug, PartialEq)]
pub struct Schema {
    root: Node,
    /// Its text, as it read, whose keys' order [`Ordered`] keeps.
    source: String,
}

/// What is wrong with a value of a section: where, by the keys and the
/// indexes that lead to it from the section, and what.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Problem {
    /// The keys and the indexes, as text, from the section to the value.
    pub path: Vec<String>,
    /// What is wrong.
    pub message: String,
}

/// A section checked against its schema.
#[derive(Clone, Debug, PartialEq)]
pub struct Checked {
    /// Its values, with the defaults of the keys left out; the first
    /// problem of a value that the schema refuses.
    pub values: Result<Map<String, Value>, Problem>,
    /// The keys of the section that the schema does not have, which the
    /// values leave out, as those of another version of the plugin.
    pub ignored: Vec<String>,
}

impl Schema {
    /// The schema of `text`, a JSON Schema of an object, whose defaults
    /// are of their own schemas, each key once in its object; an error of
    /// what does not read, or of a keyword that the host does not check, by
    /// its place.
    pub fn parse(text: &str) -> Result<Self, String> {
        Self::parse_at(text).map_err(|(message, at)| match at {
            Some((line, column)) => format!("{message} at line {line} column {column}"),
            None => message,
        })
    }

    /// The schema of `text`, as [`Schema::parse`] reads it; an error of its
    /// line and column, from 1, where the text tells them.
    pub fn parse_at(text: &str) -> Result<Self, (String, Option<(usize, usize)>)> {
        // A key twice in one object, which a map would take one of.
        if let Err(error) = serde_json::from_str::<Ordered>(text) {
            let at = (error.line() > 0).then(|| (error.line(), error.column()));
            let told = error.to_string();
            let message = match at {
                Some((line, column)) => told
                    .strip_suffix(&format!(" at line {line} column {column}"))
                    .unwrap_or(&told)
                    .to_owned(),
                None => told,
            };
            let message = if error.is_data() {
                message
            } else {
                format!("it is not JSON: {message}")
            };
            return Err((message, at));
        }
        let value: Value = serde_json::from_str(text)
            .map_err(|error| (format!("it is not JSON: {error}"), None))?;
        Self::of_value(&value, text).map_err(|message| (message, None))
    }

    /// The schema of `value`, read of `text`.
    fn of_value(value: &Value, text: &str) -> Result<Self, String> {
        let root = node(value, "")?;
        if root.types.as_deref() != Some(&[Type::Object]) {
            return Err("the schema's `type` is `\"object\"`, of the section's keys".to_owned());
        }
        Ok(Self {
            root,
            source: text.to_owned(),
        })
    }

    /// Its text, as it read, whose keys' order a page of settings follows.
    pub fn source(&self) -> &str {
        &self.source
    }

    /// The names of the section's keys, as `properties` has them.
    pub fn keys(&self) -> Vec<&str> {
        self.root.properties.keys().map(String::as_str).collect()
    }

    /// Checks `section`, the keys of a plugin's section: the keys of the
    /// schema, with the defaults of those left out, and the others
    /// ignored.
    pub fn check(&self, section: &Map<String, Value>) -> Checked {
        let (mut known, mut ignored) = (Map::new(), Vec::new());
        for (key, value) in section {
            // The others are ignored, but where the schema says what they
            // take, of its `additionalProperties`, as `false` refuses them.
            let said = !matches!(self.root.additional, Additional::Unsaid);
            if self.root.properties.contains_key(key) || said {
                known.insert(key.clone(), value.clone());
            } else {
                ignored.push(key.clone());
            }
        }
        let values =
            check(&self.root, &Value::Object(known), &mut Vec::new()).map(|value| match value {
                Value::Object(values) => values,
                _ => Map::new(),
            });
        Checked { values, ignored }
    }

    /// The section's defaults: the defaults of its keys, those that it
    /// needs, of no default, left out.
    pub fn defaults(&self) -> Map<String, Value> {
        self.root
            .properties
            .iter()
            .filter_map(|(key, property)| Some((key.clone(), property.default.clone()?)))
            .collect()
    }
}

/// The schema of `value`, at the place `at` of the schema, as `/properties/x`.
fn node(value: &Value, at: &str) -> Result<Node, String> {
    let place = if at.is_empty() { "/" } else { at };
    // The schema `true` takes any value, and `false` none.
    let empty = Map::new();
    let object = match value {
        Value::Bool(holds) => {
            return node(&Value::Object(empty), at).map(|node| Node {
                never: !holds,
                ..node
            });
        }
        Value::Object(object) => object,
        _ => {
            return Err(format!(
                "the schema at `{place}` is not an object nor a boolean"
            ));
        }
    };
    let number = |key: &str| -> Result<Option<f64>, String> {
        object
            .get(key)
            .map(|value| {
                value
                    .as_f64()
                    .ok_or_else(|| format!("`{key}` at `{place}` is not a number"))
            })
            .transpose()
    };
    // A count is a number of no fraction, as `2` or `2.0`, as JSON Schema
    // takes an integer.
    let count = |key: &str| -> Result<Option<usize>, String> {
        object
            .get(key)
            .map(|value| {
                value
                    .as_u64()
                    .or_else(|| {
                        value
                            .as_f64()
                            .filter(|count| count.fract() == 0.0 && *count >= 0.0 && *count < 9e15)
                            .map(|count| count as u64)
                    })
                    .and_then(|count| usize::try_from(count).ok())
                    .ok_or_else(|| format!("`{key}` at `{place}` is not a count"))
            })
            .transpose()
    };
    let mut node = Node {
        never: false,
        types: None,
        choices: None,
        constant: object.get("const").cloned(),
        default: object.get("default").cloned(),
        minimum: number("minimum")?,
        maximum: number("maximum")?,
        above: number("exclusiveMinimum")?,
        below: number("exclusiveMaximum")?,
        shortest: count("minLength")?,
        longest: count("maxLength")?,
        properties: BTreeMap::new(),
        required: Vec::new(),
        additional: Additional::Unsaid,
        items: None,
        fewest: count("minItems")?,
        most: count("maxItems")?,
    };
    for (key, value) in object {
        match key.as_str() {
            "type" => {
                let names: Vec<&Value> = match value {
                    Value::Array(names) => names.iter().collect(),
                    name => vec![name],
                };
                let types = names
                    .into_iter()
                    .map(|name| name.as_str().and_then(Type::of))
                    .collect::<Option<Vec<Type>>>()
                    .filter(|types| !types.is_empty())
                    .ok_or_else(|| format!("`type` at `{place}` is not of JSON's types"))?;
                node.types = Some(types);
            }
            "enum" => {
                let choices = value
                    .as_array()
                    .filter(|choices| !choices.is_empty())
                    .ok_or_else(|| format!("`enum` at `{place}` is not a list of values"))?;
                node.choices = Some(choices.clone());
            }
            "properties" => {
                let properties = value
                    .as_object()
                    .ok_or_else(|| format!("`properties` at `{place}` is not an object"))?;
                for (name, property) in properties {
                    let property = self::node(property, &format!("{at}/properties/{name}"))?;
                    node.properties.insert(name.clone(), property);
                }
            }
            "required" => {
                node.required = value
                    .as_array()
                    .and_then(|names| {
                        names
                            .iter()
                            .map(|name| name.as_str().map(str::to_owned))
                            .collect()
                    })
                    .ok_or_else(|| format!("`required` at `{place}` is not a list of names"))?;
            }
            "additionalProperties" => {
                node.additional = match value {
                    Value::Bool(true) => Additional::Any,
                    Value::Bool(false) => Additional::None,
                    schema => Additional::Of(Box::new(self::node(
                        schema,
                        &format!("{at}/additionalProperties"),
                    )?)),
                };
            }
            "items" => {
                node.items = Some(Box::new(self::node(value, &format!("{at}/items"))?));
            }
            "const" | "minimum" | "maximum" | "exclusiveMinimum" | "exclusiveMaximum"
            | "minLength" | "maxLength" | "minItems" | "maxItems" => {}
            key if ANNOTATIONS.contains(&key) || key.starts_with("x-") => {}
            key => {
                return Err(format!(
                    "the keyword `{key}` at `{place}` is not one that the host checks"
                ));
            }
        }
    }
    // A default is its schema's value, as a whole number of a fraction of
    // an integer's schema is the integer.
    if let Some(default) = &node.default {
        match check(&node, default, &mut Vec::new()) {
            Ok(checked) => node.default = Some(checked),
            Err(problem) => {
                return Err(format!(
                    "the default at `{place}` is not of its schema: {}",
                    problem.message.replacen("the section", "it", 1)
                ));
            }
        }
    }
    Ok(node)
}

/// Whether `a` and `b` are the same value, their numbers by their values,
/// as `1` and `1.0`, as JSON Schema compares them.
fn same(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(a), Value::Number(b)) => match (a.as_i64(), b.as_i64()) {
            (Some(a), Some(b)) => a == b,
            _ => a.as_f64() == b.as_f64(),
        },
        (Value::Array(a), Value::Array(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(a, b)| same(a, b))
        }
        (Value::Object(a), Value::Object(b)) => {
            a.len() == b.len()
                && a.iter()
                    .all(|(key, value)| b.get(key).is_some_and(|other| same(value, other)))
        }
        _ => a == b,
    }
}

/// `value` written as JSON, for a message.
fn shown(value: &Value) -> String {
    let text = value.to_string();
    if text.chars().count() > 40 {
        format!("{}…", text.chars().take(40).collect::<String>())
    } else {
        text
    }
}

/// Checks `value`, at `path`, against `node`: the value, with the defaults
/// of the keys of its objects left out, or the first problem.
fn check(node: &Node, value: &Value, path: &mut Vec<String>) -> Result<Value, Problem> {
    let fail = |path: &[String], message: String| Problem {
        path: path.to_vec(),
        message,
    };
    let what = match path.last() {
        Some(key) => format!("`{key}`"),
        None => "the section".to_owned(),
    };
    if node.never {
        return Err(fail(path, format!("{what} takes no value")));
    }
    if let Some(types) = &node.types
        && !types.iter().any(|kind| kind.holds(value))
    {
        let names: Vec<&str> = types.iter().map(|kind| kind.name()).collect();
        return Err(fail(
            path,
            format!("{} is {}, not {}", what, names.join(" or "), shown(value)),
        ));
    }
    if let Some(choices) = &node.choices
        && !choices.iter().any(|choice| same(choice, value))
    {
        let names: Vec<String> = choices.iter().map(shown).collect();
        return Err(fail(
            path,
            format!(
                "{} is one of {}, not {}",
                what,
                names.join(", "),
                shown(value)
            ),
        ));
    }
    if let Some(constant) = &node.constant
        && !same(constant, value)
    {
        return Err(fail(
            path,
            format!("{} is {}, not {}", what, shown(constant), shown(value)),
        ));
    }
    if let Some(number) = value.as_f64() {
        let bounds = [
            (
                node.minimum,
                "at least",
                number >= node.minimum.unwrap_or(f64::MIN),
            ),
            (
                node.maximum,
                "at most",
                number <= node.maximum.unwrap_or(f64::MAX),
            ),
            (
                node.above,
                "above",
                node.above.is_none_or(|above| number > above),
            ),
            (
                node.below,
                "below",
                node.below.is_none_or(|below| number < below),
            ),
        ];
        for (bound, word, holds) in bounds {
            if let Some(bound) = bound
                && !holds
            {
                return Err(fail(
                    path,
                    format!("{} is {word} {bound}, not {}", what, shown(value)),
                ));
            }
        }
    }
    if let Some(text) = value.as_str() {
        let length = text.chars().count();
        if node.shortest.is_some_and(|shortest| length < shortest)
            || node.longest.is_some_and(|longest| length > longest)
        {
            let (shortest, longest) = (node.shortest.unwrap_or(0), node.longest);
            let range = match longest {
                Some(longest) => format!("from {shortest} to {longest} characters"),
                None => format!("of {shortest} characters at least"),
            };
            return Err(fail(path, format!("{} is {range}, not {length}", what)));
        }
    }
    match value {
        Value::Array(items) => {
            let length = items.len();
            if node.fewest.is_some_and(|fewest| length < fewest)
                || node.most.is_some_and(|most| length > most)
            {
                let (fewest, most) = (node.fewest.unwrap_or(0), node.most);
                let range = match most {
                    Some(most) => format!("from {fewest} to {most} values"),
                    None => format!("of {fewest} values at least"),
                };
                return Err(fail(path, format!("{} has {range}, not {length}", what)));
            }
            let mut checked = Vec::with_capacity(length);
            for (index, item) in items.iter().enumerate() {
                path.push(index.to_string());
                let item = match &node.items {
                    Some(items) => check(items, item, path),
                    None => Ok(item.clone()),
                };
                path.pop();
                checked.push(item?);
            }
            Ok(Value::Array(checked))
        }
        Value::Object(object) => {
            let owner = what.clone();
            let mut checked = Map::new();
            for (key, item) in object {
                path.push(key.clone());
                let item = match (node.properties.get(key), &node.additional) {
                    (Some(property), _) => check(property, item, path),
                    (None, Additional::Any | Additional::Unsaid) => Ok(item.clone()),
                    (None, Additional::Of(schema)) => check(schema, item, path),
                    (None, Additional::None) => {
                        let known: Vec<String> = node
                            .properties
                            .keys()
                            .map(|key| format!("`{key}`"))
                            .collect();
                        let known = if known.is_empty() {
                            "none".to_owned()
                        } else {
                            known.join(", ")
                        };
                        Err(fail(
                            path,
                            format!("`{key}` is not a key of {owner}, which has {known}"),
                        ))
                    }
                };
                path.pop();
                checked.insert(key.clone(), item?);
            }
            for name in &node.required {
                if !checked.contains_key(name) {
                    return Err(fail(path, format!("{} needs `{name}`", what)));
                }
            }
            for (key, property) in &node.properties {
                if !checked.contains_key(key)
                    && let Some(default) = &property.default
                {
                    checked.insert(key.clone(), default.clone());
                }
            }
            Ok(Value::Object(checked))
        }
        // A whole number of a fraction, as `5.0`, of a schema of integers
        // alone is the integer, as the plugin takes it.
        Value::Number(number)
            if !number.is_i64()
                && !number.is_u64()
                && node.types.as_deref().is_some_and(|types| {
                    types.contains(&Type::Integer) && !types.contains(&Type::Number)
                }) =>
        {
            match number.as_f64() {
                Some(whole) if whole.fract() == 0.0 && whole.abs() < 9.0e18 => {
                    Ok(Value::from(whole as i64))
                }
                _ => Ok(value.clone()),
            }
        }
        _ => Ok(value.clone()),
    }
}

#[cfg(test)]
mod tests;
