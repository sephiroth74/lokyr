use serde_json::{Map, Value, json};

use super::*;

/// The schema of a tab bar's plugin, of every keyword that the host checks.
const TABS: &str = r#"{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "title": "lokyr/tabs",
  "type": "object",
  "properties": {
    "position": { "enum": ["top", "bottom", "left", "right"], "default": "top" },
    "style": { "type": "string", "enum": ["minimal", "regular"], "default": "minimal" },
    "max-title": { "type": "integer", "minimum": 5, "maximum": 200, "default": 30 },
    "fade": { "type": "number", "exclusiveMinimum": 0, "exclusiveMaximum": 1 },
    "numbers": { "type": "boolean", "default": true, "description": "⌘1 to ⌘9 on the tabs" },
    "label": { "type": "string", "minLength": 1, "maxLength": 8 },
    "hidden": { "type": "array", "items": { "type": "string" }, "minItems": 1, "maxItems": 3 },
    "version": { "const": 1 },
    "colors": {
      "type": "object",
      "properties": { "active": { "type": "string", "default": "accent" } },
      "additionalProperties": false,
      "required": ["active"]
    },
    "extra": { "type": "object", "additionalProperties": { "type": "integer" } }
  }
}"#;

fn section(value: &Value) -> Map<String, Value> {
    value.as_object().unwrap().clone()
}

fn problem(schema: &Schema, value: &Value) -> Problem {
    schema
        .check(&section(value))
        .values
        .expect_err("the schema refuses it")
}

#[test]
fn a_section_takes_the_defaults_of_the_keys_that_it_leaves_out() {
    let schema = Schema::parse(TABS).unwrap();
    assert_eq!(
        schema.keys(),
        [
            "colors",
            "extra",
            "fade",
            "hidden",
            "label",
            "max-title",
            "numbers",
            "position",
            "style",
            "version"
        ]
    );
    assert_eq!(
        Value::Object(schema.defaults()),
        json!({ "position": "top", "style": "minimal", "max-title": 30, "numbers": true })
    );
    let checked = schema.check(&section(&json!({
        "position": "left",
        "max-title": 12.0,
        "fade": 0.5,
        "label": "αβγ",
        "hidden": ["a"],
        "version": 1,
        "colors": { "active": "red" },
        "extra": { "a": 1 },
    })));
    assert_eq!(
        Value::Object(checked.values.unwrap()),
        json!({
            "position": "left", "style": "minimal", "max-title": 12, "numbers": true,
            "fade": 0.5, "label": "αβγ", "hidden": ["a"], "version": 1,
            "colors": { "active": "red" }, "extra": { "a": 1 },
        })
    );
    assert!(checked.ignored.is_empty());
}

#[test]
fn a_key_that_the_schema_lacks_is_ignored_at_the_section_s_top() {
    let schema = Schema::parse(TABS).unwrap();
    let checked = schema.check(&section(
        &json!({ "newer": 1, "position": "bottom", "older": "x" }),
    ));
    assert_eq!(checked.ignored, ["newer", "older"]);
    let values = checked.values.unwrap();
    assert_eq!(values["position"], "bottom");
    assert!(!values.contains_key("newer"));
}

#[test]
fn each_keyword_refuses_its_wrong_values_at_their_place() {
    let schema = Schema::parse(TABS).unwrap();
    let cases = [
        (
            json!({ "position": "middle" }),
            vec!["position"],
            r#"`position` is one of "top", "bottom", "left", "right", not "middle""#,
        ),
        (
            json!({ "style": 3 }),
            vec!["style"],
            "`style` is a string, not 3",
        ),
        (
            json!({ "max-title": 2.5 }),
            vec!["max-title"],
            "`max-title` is an integer, not 2.5",
        ),
        (
            json!({ "max-title": 4 }),
            vec!["max-title"],
            "`max-title` is at least 5, not 4",
        ),
        (
            json!({ "max-title": 201 }),
            vec!["max-title"],
            "`max-title` is at most 200, not 201",
        ),
        (
            json!({ "fade": 0 }),
            vec!["fade"],
            "`fade` is above 0, not 0",
        ),
        (
            json!({ "fade": 1 }),
            vec!["fade"],
            "`fade` is below 1, not 1",
        ),
        (
            json!({ "label": "" }),
            vec!["label"],
            "`label` is from 1 to 8 characters, not 0",
        ),
        (
            json!({ "label": "ninechars" }),
            vec!["label"],
            "`label` is from 1 to 8 characters, not 9",
        ),
        (
            json!({ "hidden": [] }),
            vec!["hidden"],
            "`hidden` has from 1 to 3 values, not 0",
        ),
        (
            json!({ "hidden": ["a", 2] }),
            vec!["hidden", "1"],
            "`1` is a string, not 2",
        ),
        (
            json!({ "version": 2 }),
            vec!["version"],
            "`version` is 1, not 2",
        ),
        (
            json!({ "colors": {} }),
            vec!["colors"],
            "`colors` needs `active`",
        ),
        (
            json!({ "colors": { "active": "x", "idle": "y" } }),
            vec!["colors", "idle"],
            "`idle` is not a key of `colors`, which has `active`",
        ),
        (
            json!({ "extra": { "a": "1" } }),
            vec!["extra", "a"],
            r#"`a` is an integer, not "1""#,
        ),
        (
            json!({ "numbers": "yes" }),
            vec!["numbers"],
            r#"`numbers` is a boolean, not "yes""#,
        ),
    ];
    for (value, path, message) in cases {
        let problem = problem(&schema, &value);
        assert_eq!(
            (
                problem.path.iter().map(String::as_str).collect::<Vec<_>>(),
                problem.message.as_str()
            ),
            (path, message),
            "{value}"
        );
    }
}

#[test]
fn a_schema_of_a_keyword_that_the_host_does_not_check_does_not_read() {
    let refused = [
        (
            r#"{"type": "object", "properties": {"a": {"type": "string", "pattern": "^x"}}}"#,
            "the keyword `pattern` at `/properties/a` is not one that the host checks",
        ),
        (
            r##"{"type": "object", "$ref": "#/defs/a"}"##,
            "the keyword `$ref` at `/` is not one that the host checks",
        ),
        (
            r#"{"type": "array"}"#,
            "the schema's `type` is `\"object\"`, of the section's keys",
        ),
        (
            r#"{"properties": {}}"#,
            "the schema's `type` is `\"object\"`, of the section's keys",
        ),
        (
            r#"{"type": "object", "properties": {"a": {"type": "integer", "default": "x"}}}"#,
            r#"the default at `/properties/a` is not of its schema: it is an integer, not "x""#,
        ),
        (
            r#"{"type": "object", "properties": {"a": {"type": "text"}}}"#,
            "`type` at `/properties/a` is not of JSON's types",
        ),
        (
            r#"{"type": "object", "properties": {"a": {"minimum": "1"}}}"#,
            "`minimum` at `/properties/a` is not a number",
        ),
        (
            r#"{"type": "object", "properties": {"a": {"maxLength": -1}}}"#,
            "`maxLength` at `/properties/a` is not a count",
        ),
        (
            r#"{"type": "object", "properties": {"a": {"enum": []}}}"#,
            "`enum` at `/properties/a` is not a list of values",
        ),
        (
            r#"{"type": "object", "properties": {"a": 3}}"#,
            "the schema at `/properties/a` is not an object",
        ),
    ];
    for (text, message) in refused {
        assert_eq!(Schema::parse(text).unwrap_err(), message, "{text}");
    }
    assert!(
        Schema::parse("{")
            .unwrap_err()
            .starts_with("it is not JSON")
    );
}

#[test]
fn numbers_are_the_same_of_the_same_value_and_a_whole_one_of_an_integer_s_schema_is_the_integer() {
    let schema = Schema::parse(
        r#"{
            "type": "object",
            "properties": {
                "level": { "enum": [1, 2, [3]] },
                "fixed": { "const": { "n": 4 } },
                "count": { "type": "integer", "default": 7.0 },
                "share": { "type": "number" },
                "either": { "type": ["integer", "number"] }
            }
        }"#,
    )
    .unwrap();
    let values = schema
        .check(&section(&json!({
            "level": 1.0,
            "fixed": { "n": 4.0 },
            "share": 5.0,
            "either": 6.0,
        })))
        .values
        .unwrap();
    assert_eq!(values["level"], json!(1.0));
    // An integer's of a fraction, its default too, is the integer; a
    // number's, or one of either, stays as it is.
    assert!(values["count"].is_i64(), "{:?}", values["count"]);
    assert_eq!(values["count"], json!(7));
    assert!(values["share"].is_f64());
    assert!(values["either"].is_f64());
    let values = schema
        .check(&section(&json!({ "count": 5.0, "level": [3.0] })))
        .values
        .unwrap();
    assert_eq!(values["count"], json!(5));
    assert_eq!(
        problem(&schema, &json!({ "level": 1.5 })).message,
        "`level` is one of 1, 2, [3], not 1.5"
    );
}
