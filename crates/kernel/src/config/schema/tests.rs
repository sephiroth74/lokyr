//! The schema against the parser: the same sections and keys, the same
//! defaults, each value that it allows taken and one past them refused,
//! and the example that it makes read whole.

use std::collections::BTreeSet;
use std::path::Path;

use serde_json::{Map, Value};
use toml_edit::{DocumentMut, Item};

use super::super::{Config, sections};
use super::SCHEMA;

/// The schema's keys of its own, which no section of the parser's is.
const OWN: [&str; 3] = ["config_version", "keys", "plugins"];

/// The plugins' schemas, of their sections in the guide's example.
const PLUGINS: [(&str, &str); 6] = [
    (
        "tabs",
        include_str!("../../../../../plugins/tabs/tabs.schema.json"),
    ),
    (
        "splits",
        include_str!("../../../../../plugins/splits/splits.schema.json"),
    ),
    (
        "quick-terminal",
        include_str!("../../../../../plugins/quick-terminal/quick-terminal.schema.json"),
    ),
    (
        "search",
        include_str!("../../../../../plugins/search/search.schema.json"),
    ),
    (
        "command-header",
        include_str!("../../../../../plugins/command-header/command-header.schema.json"),
    ),
    (
        "quick-tips",
        include_str!("../../../../../plugins/quick-tips/quick-tips.schema.json"),
    ),
];

fn schema() -> Value {
    serde_json::from_str(SCHEMA).expect("the schema is JSON")
}

fn properties(node: &Value) -> &Map<String, Value> {
    node["properties"]
        .as_object()
        .expect("an object of properties")
}

/// The schema's sections of the parser's, each of its keys.
fn parser_sections(schema: &Value) -> Vec<(String, Map<String, Value>)> {
    properties(schema)
        .iter()
        .filter(|(name, _)| !OWN.contains(&name.as_str()))
        .map(|(name, section)| (name.clone(), properties(section).clone()))
        .collect()
}

fn parses(text: &str) -> bool {
    Config::parse(text, Path::new("config.toml")).is_ok()
}

/// `value` as TOML writes it.
fn toml_of(value: &Value) -> String {
    match value {
        Value::String(text) => serde_json::to_string(text).unwrap(),
        other => other.to_string(),
    }
}

/// `number` as TOML writes it, whole of no fraction.
fn number(number: f64) -> String {
    if number.fract() == 0.0 {
        format!("{}", number as i64)
    } else {
        format!("{number}")
    }
}

#[test]
fn the_schema_reads_as_a_schema_of_the_plugins_subset() {
    if let Err(error) = lokyr_pkg_model::schema::Schema::parse(SCHEMA) {
        panic!("{error}");
    }
}

#[test]
fn the_schema_has_the_parser_s_sections_and_keys_and_back() {
    let schema = schema();
    let mut named = BTreeSet::new();
    for (name, keys) in parser_sections(&schema) {
        let (_, parsed) = sections()
            .into_iter()
            .find(|(section, _)| *section == name)
            .unwrap_or_else(|| panic!("[{name}] is no section of the parser's"));
        let schema_keys: BTreeSet<&str> = keys.keys().map(String::as_str).collect();
        let parser_keys: BTreeSet<&str> = parsed.iter().copied().collect();
        assert_eq!(schema_keys, parser_keys, "[{name}]");
        named.insert(name);
    }
    let parser: BTreeSet<String> = sections()
        .iter()
        .map(|(name, _)| (*name).to_owned())
        .collect();
    assert_eq!(named, parser);
}

#[test]
fn the_schema_s_defaults_are_the_parser_s() {
    let schema = schema();
    let written: DocumentMut = Config::default()
        .written(false)
        .parse()
        .expect("the defaults read");
    let same = |json: &Value, toml: &toml_edit::Value| match (json, toml) {
        (Value::Bool(a), toml_edit::Value::Boolean(b)) => a == b.value(),
        (Value::Number(a), toml_edit::Value::Integer(b)) => a.as_f64() == Some(*b.value() as f64),
        (Value::Number(a), toml_edit::Value::Float(b)) => {
            (a.as_f64().unwrap() - b.value()).abs() < 1e-6
        }
        (Value::String(a), toml_edit::Value::String(b)) => a == b.value(),
        (Value::Array(a), toml_edit::Value::Array(b)) => {
            a.len() == b.len()
                && a.iter()
                    .zip(b.iter())
                    .all(|(a, b)| a.as_str().is_some_and(|a| b.as_str() == Some(a)))
        }
        _ => false,
    };
    for (name, keys) in parser_sections(&schema) {
        for (key, node) in &keys {
            let parsed = written
                .get(&name)
                .and_then(|section| section.get(key))
                .and_then(Item::as_value);
            match (node.get("default"), parsed) {
                (None, None) => {}
                (Some(default), Some(parsed)) => {
                    assert!(
                        same(default, parsed),
                        "[{name}] {key}: {default} against {parsed}"
                    );
                }
                other => panic!("[{name}] {key}: {other:?}"),
            }
        }
    }
    let version = &properties(&schema)["config_version"];
    assert!(same(
        &version["default"],
        written["config_version"].as_value().unwrap()
    ));
}

#[test]
fn each_value_that_the_schema_allows_reads_and_one_past_them_does_not() {
    let schema = schema();
    let mut problems = Vec::new();
    let mut check = |holds: bool, problem: String| {
        if !holds {
            problems.push(problem);
        }
    };
    for (name, keys) in parser_sections(&schema) {
        for (key, node) in &keys {
            let set = |value: &str| format!("[{name}]\n{key} = {value}\n");
            if let Some(choices) = node.get("enum").and_then(Value::as_array) {
                for choice in choices {
                    let value = toml_of(choice);
                    check(
                        parses(&set(&value)),
                        format!("[{name}] {key} = {value} refused"),
                    );
                }
                let wrong = if choices[0].is_string() {
                    "\"not-a-value\""
                } else {
                    "999999"
                };
                check(
                    !parses(&set(wrong)),
                    format!("[{name}] {key} = {wrong} taken"),
                );
            }
            let (Some(least), Some(most)) = (
                node.get("minimum").and_then(Value::as_f64),
                node.get("maximum").and_then(Value::as_f64),
            ) else {
                continue;
            };
            for within in [least, most] {
                check(
                    parses(&set(&number(within))),
                    format!("[{name}] {key} = {within} refused"),
                );
            }
            for past in [least - 1.0, most + 1.0] {
                check(
                    !parses(&set(&number(past))),
                    format!("[{name}] {key} = {past} taken"),
                );
            }
            // A number takes a fraction, an integer none.
            let fraction = least + 0.5;
            if fraction <= most {
                let whole = node["type"] == "integer";
                check(
                    parses(&set(&number(fraction))) != whole,
                    format!("[{name}] {key} = {fraction}, of the type {}", node["type"]),
                );
            }
        }
    }
    assert!(problems.is_empty(), "{problems:#?}");
    assert!(parses("config_version = 1\n"));
    assert!(!parses("config_version = 2\n"));
}

#[test]
fn every_key_has_its_platforms_its_guide_s_text_and_an_example() {
    let schema = schema();
    // A section's own text, if it has one, before its keys'.
    for (name, section) in properties(&schema) {
        if name != "config_version" {
            assert!(section["x-guide"].is_string(), "[{name}]");
        }
    }
    for (name, keys) in parser_sections(&schema) {
        for (key, node) in &keys {
            let platforms = node["x-platforms"].as_array();
            assert!(
                platforms.is_some_and(|names| !names.is_empty()),
                "[{name}] {key}"
            );
            // Of its own text, or of its group's first key's.
            let first = node["x-group"]
                .as_array()
                .and_then(|group| group.first())
                .and_then(Value::as_str)
                .unwrap_or(key);
            assert!(keys[first]["x-guide"].is_string(), "[{name}] {key}");
            assert!(node["x-example"].is_string(), "[{name}] {key}");
        }
    }
}

#[test]
fn the_example_that_the_schemas_make_reads_whole_and_holds_every_key() {
    let schema = schema();
    let own = properties(&schema);
    let mut text = format!(
        "config_version = {}\n",
        own["config_version"]["x-example"].as_str().unwrap()
    );
    let mut wanted = Vec::new();
    let mut section = |text: &mut String, name: &str, keys: &Map<String, Value>| {
        text.push_str(&format!("\n[{name}]\n"));
        for (key, node) in keys {
            if let Some(example) = node["x-example"].as_str() {
                text.push_str(&format!("{key} = {example}\n"));
                wanted.push((name.to_owned(), key.clone()));
            }
        }
    };
    for (name, keys) in parser_sections(&schema) {
        section(&mut text, &name, &keys);
    }
    text.push_str("\n[keys]\n");
    for line in own["keys"]["x-example-lines"].as_array().unwrap() {
        text.push_str(line.as_str().unwrap());
        text.push('\n');
    }
    for (namespace, plugin) in PLUGINS {
        let plugin: Value = serde_json::from_str(plugin).unwrap();
        section(
            &mut text,
            &format!("plugins.{namespace}"),
            properties(&plugin),
        );
    }
    let config = Config::parse(&text, Path::new("config.toml")).unwrap_or_else(|error| {
        panic!("{error}\n{text}");
    });
    assert_ne!(config, Config::default());
    let read: DocumentMut = text.parse().unwrap();
    for (name, key) in wanted {
        let mut found = Some(read.as_item());
        for part in name.split('.') {
            found = found.and_then(|item| item.get(part));
        }
        assert!(
            found.and_then(|item| item.get(&key)).is_some(),
            "[{name}] {key}"
        );
    }
}

/// Whether `node` is a number's or an integer's of a range, not one of a
/// few values.
fn ranged(node: &Value) -> bool {
    matches!(node["type"].as_str(), Some("number" | "integer")) && node.get("enum").is_none()
}

/// Whether `node` is a text's of any value, not one of a few.
fn free_text(node: &Value) -> bool {
    node["type"] == "string" && node.get("enum").is_none()
}

#[test]
fn every_number_has_its_step_within_its_range_and_every_text_its_bound() {
    let schema = schema();
    let mut nodes: Vec<(String, Value)> = Vec::new();
    for (name, keys) in parser_sections(&schema) {
        for (key, node) in keys {
            nodes.push((format!("[{name}] {key}"), node));
        }
    }
    for (namespace, plugin) in PLUGINS {
        let plugin: Value = serde_json::from_str(plugin).unwrap();
        for (key, node) in properties(&plugin) {
            nodes.push((format!("[plugins.{namespace}] {key}"), node.clone()));
        }
    }
    for (place, node) in nodes {
        if ranged(&node) {
            let step = node["x-step"]
                .as_f64()
                .unwrap_or_else(|| panic!("{place}: no step"));
            let room = node["maximum"].as_f64().unwrap() - node["minimum"].as_f64().unwrap();
            assert!(step > 0.0 && step <= room, "{place}: a step of {step}");
            if node["type"] == "integer" {
                assert_eq!(step.fract(), 0.0, "{place}: a step of {step}");
            }
        }
        let text = if node["type"] == "array" {
            &node["items"]
        } else {
            &node
        };
        if free_text(text) {
            assert!(text["maxLength"].as_u64().is_some(), "{place}: no bound");
        }
    }
}

/// The texts of a form that a key holds to, as an editor's address of
/// `{path}`, which the bounds' test makes as long as it needs.
const SHAPED: [(&str, &str, &str); 4] = [
    ("links", "editor", "x-editor://{path}"),
    (
        "links",
        "search-engine",
        "https://search.example/?q={query}&a=",
    ),
    ("window", "start-directory", "/"),
    ("window", "new-directory", "/"),
];

#[test]
fn a_text_of_its_bound_reads_and_one_past_it_does_not() {
    let schema = schema();
    let mut problems = Vec::new();
    for (name, keys) in parser_sections(&schema) {
        for (key, node) in &keys {
            let list = node["type"] == "array";
            let Some(most) = (if list { &node["items"] } else { node })["maxLength"].as_u64()
            else {
                continue;
            };
            let base = SHAPED
                .iter()
                .find(|(section, shaped, _)| *section == name && shaped == key)
                .map_or("", |(_, _, base)| base);
            let set = |length: u64| {
                let fill = usize::try_from(length).unwrap() - base.chars().count();
                let text = format!("\"{base}{}\"", "a".repeat(fill));
                let value = if list { format!("[{text}]") } else { text };
                format!("[{name}]\n{key} = {value}\n")
            };
            if let Err(error) = Config::parse(&set(most), Path::new("config.toml")) {
                let error = error.to_string();
                problems.push(format!(
                    "[{name}] {key} of {most} characters: {}",
                    error.chars().take(160).collect::<String>()
                ));
                continue;
            }
            let past = Config::parse(&set(most + 1), Path::new("config.toml"))
                .expect_err("a text past its bound");
            assert!(
                past.to_string().contains(&format!(
                    "`{key}` in `[{name}]` is a text of {most} characters at most"
                )),
                "{past}"
            );
            assert_eq!(past.position.map(|at| at.line), Some(2), "{past}");
        }
    }
    assert!(problems.is_empty(), "{problems:#?}");
}

#[test]
fn the_settings_sections_are_the_schema_s_in_its_order_of_the_values_in_force() {
    let config = Config::parse(
        "[font]\nsize = 15\n\n[window]\nscrollbar = \"never\"\n",
        Path::new("config.toml"),
    )
    .unwrap();
    let sections = super::sections(&config);
    let names: Vec<&str> = sections
        .iter()
        .map(|section| section.name.as_str())
        .collect();
    let parser: Vec<&str> = sections_of_the_parser();
    assert_eq!(names, parser, "the guide's order, as the parser's");
    let font = &sections[0];
    assert!(font.plugin.is_none());
    assert!(!font.description.is_empty());
    let key = |section: &lokyr_plugin_sdk::host::SettingsSection, name: &str| {
        section
            .keys
            .iter()
            .find(|key| key.name == name)
            .unwrap_or_else(|| panic!("{name}"))
            .clone()
    };
    use lokyr_plugin_sdk::host::SettingKind;
    use lokyr_plugin_sdk::message::Data;
    let size = key(font, "size");
    assert_eq!(size.value, Some(Data::Integer(15)));
    assert_eq!(size.default, Some(Data::Integer(13)));
    assert_eq!(size.platforms, ["macos"]);
    assert!(matches!(
        size.kind,
        SettingKind::Number {
            minimum: Some(4.0),
            maximum: Some(200.0),
            step: Some(1.0),
            integer: false
        }
    ));
    // A key of no default and no value has none; its control told.
    let family = key(font, "family");
    assert_eq!((family.value, family.default), (None, None));
    assert_eq!(family.control.as_deref(), Some("font-family"));
    assert!(matches!(
        family.kind,
        SettingKind::Text {
            max_length: Some(256),
            ..
        }
    ));
    let window = sections
        .iter()
        .find(|section| section.name == "window")
        .unwrap();
    let scrollbar = key(window, "scrollbar");
    assert_eq!(scrollbar.value, Some(Data::Text("never".to_owned())));
    assert!(matches!(scrollbar.kind, SettingKind::Choice { .. }));
    // The first keys in the schema's order, as the guide's.
    assert_eq!(font.keys[0].name, "family");
    assert_eq!(font.keys[1].name, "size");
}

/// The parser's sections' names, in their order.
fn sections_of_the_parser() -> Vec<&'static str> {
    sections().iter().map(|(name, _)| *name).collect()
}
