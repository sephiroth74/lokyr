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
//! `default`, `examples` and `deprecated`. A schema of any other keyword does
//! not read, by its name.

use std::collections::BTreeMap;

use serde_json::{Map, Value};

/// The annotations, which a value is not checked against.
const ANNOTATIONS: [&str; 8] = [
    "$schema",
    "$id",
    "$comment",
    "title",
    "description",
    "default",
    "examples",
    "deprecated",
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
    /// are of their own schemas; an error of what does not read, or of a
    /// keyword that the host does not check, by its place.
    pub fn parse(text: &str) -> Result<Self, String> {
        let value: Value =
            serde_json::from_str(text).map_err(|error| format!("it is not JSON: {error}"))?;
        let root = node(&value, "")?;
        if root.types.as_deref() != Some(&[Type::Object]) {
            return Err("the schema's `type` is `\"object\"`, of the section's keys".to_owned());
        }
        Ok(Self { root })
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
            if self.root.properties.contains_key(key) {
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

    /// The section's defaults: the values of a section that holds no key.
    pub fn defaults(&self) -> Map<String, Value> {
        self.check(&Map::new()).values.unwrap_or_default()
    }
}

/// The schema of `value`, at the place `at` of the schema, as `/properties/x`.
fn node(value: &Value, at: &str) -> Result<Node, String> {
    let place = if at.is_empty() { "/" } else { at };
    let Some(object) = value.as_object() else {
        return Err(format!("the schema at `{place}` is not an object"));
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
    let count = |key: &str| -> Result<Option<usize>, String> {
        object
            .get(key)
            .map(|value| {
                value
                    .as_u64()
                    .and_then(|count| usize::try_from(count).ok())
                    .ok_or_else(|| format!("`{key}` at `{place}` is not a count"))
            })
            .transpose()
    };
    let mut node = Node {
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
        additional: Additional::Any,
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
            key if ANNOTATIONS.contains(&key) => {}
            key => {
                return Err(format!(
                    "the keyword `{key}` at `{place}` is not one that the host checks"
                ));
            }
        }
    }
    if let Some(default) = &node.default
        && let Err(problem) = check(&node, default, &mut Vec::new())
    {
        return Err(format!(
            "the default at `{place}` is not of its schema: {}",
            problem.message.replacen("the section", "it", 1)
        ));
    }
    Ok(node)
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
        && !choices.contains(value)
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
        && constant != value
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
                    (None, Additional::Any) => Ok(item.clone()),
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
        _ => Ok(value.clone()),
    }
}

#[cfg(test)]
mod tests;
