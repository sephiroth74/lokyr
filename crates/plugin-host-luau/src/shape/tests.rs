use std::collections::BTreeMap;

use lokyr_plugin_sdk::command::{Args, Value as Arg};
use lokyr_plugin_sdk::host::{Layout, PaneId, Scroll, WindowId};
use lokyr_plugin_sdk::message::Data;
use lokyr_plugin_sdk::ui::{Anchor, Corner, PanelOptions, Role, ViewId};
use lokyr_plugin_sdk::view::{Length, Style};
use serde::Deserialize;
use serde::de::DeserializeOwned;

use super::*;

/// The value of the Luau expression `source`, in `lua`.
fn eval(lua: &Lua, source: &str) -> Value {
    lua.load(format!("return {source}")).eval().unwrap()
}

/// The value of `T` of the Luau expression `source`.
fn take<T: DeserializeOwned>(source: &str) -> Result<T, ShapeError> {
    let lua = Lua::new();
    from_lua(eval(&lua, source))
}

/// Whether `value` of the host's comes to Luau as `source` says, checked
/// by Luau's own comparison of the two, and back the same.
fn gives<T: Serialize + DeserializeOwned + PartialEq + fmt::Debug>(value: &T, source: &str) {
    let lua = Lua::new();
    let given = to_lua(&lua, value).unwrap();
    let expected = eval(&lua, source);
    assert!(
        crate::same::value(&given, &expected, 0),
        "{value:?} gave {given:?}, not {source}"
    );
    assert_eq!(&from_lua::<T>(given).unwrap(), value);
}

#[test]
fn a_struct_s_fields_are_their_rust_names_and_a_variant_a_field_or_a_text() {
    let options = PanelOptions {
        anchor: Anchor::PaneCorner {
            pane: PaneId(3),
            corner: Corner::TopRight,
        },
        role: Role::Dialog,
        close_outside: false,
        close_escape: true,
        takes_keys: false,
    };
    gives(
        &options,
        r#"{ anchor = { pane_corner = { pane = 3, corner = "top-right" } }, role = "dialog",
             close_outside = false, close_escape = true, takes_keys = false }"#,
    );
    // The fields left out take their defaults.
    assert_eq!(
        take::<PanelOptions>(
            r#"{ anchor = { pane_corner = { pane = 3, corner = "top-right" } }, role = "dialog",
                 close_escape = true }"#
        ),
        Ok(options)
    );
    // A struct of fields that serde names in kebab case.
    let style = Style {
        font_size: Some(15.0),
        hides_ring: true,
        ..Style::default()
    };
    let lua = Lua::new();
    let Value::Table(given) = to_lua(&lua, &style).unwrap() else {
        panic!("a style is a table");
    };
    assert!(given.raw_get::<bool>("hides_ring").unwrap());
    assert!(given.raw_get::<Value>("hides-ring").unwrap().is_nil());
    assert_eq!(
        take::<Style>("{ font_size = 15, hides_ring = true }"),
        Ok(style.clone())
    );
    // serde's own names too.
    assert_eq!(
        take::<Style>(r#"{ ["font-size"] = 15, ["hides-ring"] = true }"#),
        Ok(style)
    );
    gives(&ViewId::TabBar(WindowId(2)), "{ tab_bar = 2 }");
    gives(&Scroll::PageUp, r#""page-up""#);
    gives(&Scroll::Up(3), "{ up = 3 }");
    gives(&Layout::Pane(PaneId(4)), "{ pane = 4 }");
    let layout = lokyr_plugin_sdk::view::Layout {
        min_width: Length::Points(40.0),
        gap: 2.0,
        ..lokyr_plugin_sdk::view::Layout::default()
    };
    assert_eq!(
        take::<lokyr_plugin_sdk::view::Layout>("{ min_width = { points = 40 }, gap = 2 }"),
        Ok(layout)
    );
}

#[test]
fn plain_values_are_as_a_luau_table_is() {
    let data = Data::Map(BTreeMap::from([
        ("count".to_owned(), Data::Integer(3)),
        ("ratio".to_owned(), Data::Number(0.5)),
        ("name".to_owned(), Data::Text("Ada".to_owned())),
        ("on".to_owned(), Data::Boolean(true)),
        (
            "items".to_owned(),
            Data::List(vec![Data::Text("a".to_owned()), Data::Integer(2)]),
        ),
        ("my-key".to_owned(), Data::Map(BTreeMap::new())),
    ]));
    // A map's keys as they are, kebab case's too; an empty table a map.
    let lua = Lua::new();
    let given = to_lua(&lua, &data).unwrap();
    let expected = eval(
        &lua,
        r#"{ count = 3, ratio = 0.5, name = "Ada", on = true, items = { "a", 2 }, ["my-key"] = {} }"#,
    );
    assert!(crate::same::value(&given, &expected, 0), "{given:?}");
    assert_eq!(from_lua::<Data>(given), Ok(data));
    assert_eq!(take::<Data>("{}"), Ok(Data::Map(BTreeMap::new())));
    // A list of no items comes as an empty table, and back as a map.
    gives(&Data::Map(BTreeMap::new()), "{}");
    assert_eq!(take::<Data>("nil"), Ok(Data::Nil));
    assert_eq!(take::<Data>("2.0"), Ok(Data::Integer(2)));
    // A command's arguments.
    let args = Args::from([
        ("n".to_owned(), Arg::Integer(3)),
        ("text".to_owned(), Arg::String("x".to_owned())),
        ("on".to_owned(), Arg::Boolean(false)),
        ("ratio".to_owned(), Arg::Number(0.25)),
    ]);
    gives(&args, r#"{ n = 3, text = "x", on = false, ratio = 0.25 }"#);
    assert!(
        take::<Args>("{ n = {} }")
            .unwrap_err()
            .0
            .contains("a text, a number or a boolean")
    );
    assert!(
        take::<Data>("{ f = function() end }")
            .unwrap_err()
            .0
            .contains("data")
    );
}

#[test]
fn a_value_of_another_shape_is_refused_of_what_it_is() {
    let error = |source: &str| take::<PanelOptions>(source).unwrap_err().0;
    let anchor = r#"anchor = { pane_corner = { pane = 3, corner = "top-right" } }"#;
    assert!(
        error(&format!(
            "{{ {anchor}, role = \"dialog\", close_escpe = true }}"
        ))
        .contains("no field `close_escpe`, of anchor, role, close_outside, close_escape")
    );
    assert!(
        error(&format!("{{ {anchor}, role = \"modal\" }}"))
            .contains("`modal` is no variant of Role")
    );
    assert!(error(&format!("{{ {anchor}, role = 3 }}")).contains("a variant of Role"));
    assert!(error(r#"{ role = "dialog" }"#).contains("missing field `anchor`"));
    assert!(
        error(r#"{ anchor = { pane_corner = { pane = 1.5, corner = "top" } }, role = "dialog" }"#)
            .contains("whole number")
    );
    assert!(
        error(r#"{ anchor = { pane_corner = { pane = -1, corner = "top" } }, role = "dialog" }"#)
            .contains("from 0")
    );
    assert!(
        take::<(u16, i64)>("{ 70000, 1 }")
            .unwrap_err()
            .0
            .contains("from 0 to 65535")
    );
    assert!(
        take::<(u16, i64)>("{ 1 }")
            .unwrap_err()
            .0
            .contains("a list of 2")
    );
    assert!(
        take::<Vec<u64>>("{ 1, 2, x = 3 }")
            .unwrap_err()
            .0
            .contains("expected a list")
    );
    assert!(
        take::<Vec<u64>>("{ [1] = 1, [3] = 3 }")
            .unwrap_err()
            .0
            .contains("expected a list")
    );
    assert!(
        take::<String>("true")
            .unwrap_err()
            .0
            .contains("expected a text, not a boolean")
    );
    assert!(
        take::<bool>("1")
            .unwrap_err()
            .0
            .contains("a boolean, not a number")
    );
}

/// A table nested `levels` deep, of lists, as Luau makes it.
fn nested(lua: &Lua, levels: usize) -> Value {
    let source = format!("local t = {{}} for _ = 2, {levels} do t = {{ t }} end return t");
    lua.load(source).eval().unwrap()
}

/// A value of tables of any depth, each a list of the next.
#[derive(Debug, Deserialize)]
struct Deep(#[allow(dead_code)] Vec<Deep>);

#[test]
fn a_value_too_deep_or_that_holds_itself_is_refused_of_no_overflow() {
    let lua = Lua::new();
    assert!(from_lua::<Deep>(nested(&lua, MOST_LEVELS)).is_ok());
    for value in [
        nested(&lua, MOST_LEVELS + 1),
        nested(&lua, 100_000),
        eval(&lua, "(function() local t = {} t[1] = t return t end)()"),
    ] {
        let error = from_lua::<Deep>(value).unwrap_err().0;
        assert!(error.contains("levels of tables at most"), "{error}");
    }
}

#[test]
fn a_plain_value_nests_as_deep_as_the_host_takes_it_and_no_deeper() {
    let lua = Lua::new();
    // As deep as the host takes it, both ways.
    let deep = from_lua::<Data>(nested(&lua, Data::DEEPEST)).unwrap();
    assert!(deep.nests_within_deepest());
    gives(
        &deep,
        &format!(
            "{}{}",
            "{ ".repeat(Data::DEEPEST),
            " }".repeat(Data::DEEPEST)
        ),
    );
    // A level more, inside a value of the SDK's too, and a value that holds
    // itself.
    let deeper = || nested(&lua, Data::DEEPEST + 1);
    let state = lua.create_table().unwrap();
    state.raw_set("value", deeper()).unwrap();
    for error in [
        from_lua::<Data>(deeper()).unwrap_err(),
        from_lua::<BTreeMap<String, Data>>(Value::Table(state)).unwrap_err(),
        from_lua::<Data>(eval(
            &lua,
            "(function() local t = {} t.again = t return t end)()",
        ))
        .unwrap_err(),
    ] {
        assert!(
            error
                .0
                .contains("a plain value nests 64 levels of tables at most"),
            "{error}"
        );
    }
}

#[test]
fn a_map_s_key_of_another_kind_than_a_text_is_refused() {
    for source in [
        "{ [1] = 'a', [3] = 'b' }",
        "{ 'a', x = 1 }",
        "{ [true] = 1 }",
    ] {
        let error = take::<Data>(source).unwrap_err().0;
        assert!(
            error.contains("a map's key: expected a text"),
            "{source}: {error}"
        );
    }
}

#[test]
fn a_conversion_takes_its_values_alone() {
    let lua = Lua::new();
    let wide = lua
        .load("local t = {} for i = 1, 2000 do t[i] = i end return t")
        .eval::<Value>()
        .unwrap();
    let budget = Budget::of(1000);
    let error = Vec::<u64>::deserialize(Taking::new(wide.clone(), &budget)).unwrap_err();
    assert!(error.0.contains("values at most"), "{error}");
    let budget = Budget::default();
    assert_eq!(
        Vec::<u64>::deserialize(Taking::new(wide, &budget))
            .unwrap()
            .len(),
        2000
    );
}

#[test]
fn a_conversion_holds_no_more_values_read_out_of_tables_than_it_takes() {
    // A table that holds itself, many times: each level's items wait for
    // the first's, which a conversion would read anew at each level until
    // its depth's bound, as many times the values that it takes.
    let lua = Lua::new();
    let wide = eval(
        &lua,
        "(function() local t = {} for i = 1, 2^18 do t[i] = t end return t end)()",
    );
    let error = from_lua::<Deep>(wide.clone()).unwrap_err().0;
    assert!(error.contains("values at most"), "{error}");
    let error = from_lua::<Data>(wide).unwrap_err().0;
    assert!(error.contains("values at most"), "{error}");
    // It lets go of them as it converts them: two lists of 400 000 values,
    // each read as the one before converted.
    let lists = eval(
        &lua,
        "(function() local t = {} for i = 1, 400000 do t[i] = i end return { t, t } end)()",
    );
    let converted = from_lua::<Vec<Vec<u64>>>(lists).unwrap();
    assert_eq!(converted[1].len(), 400_000);
}

#[test]
fn a_plugin_s_metamethods_run_no_code_as_its_value_converts() {
    let value = take::<Data>(
        r#"setmetatable({ a = 1 }, {
            __index = function() error("ran") end,
            __len = function() error("ran") end,
            __iter = function() error("ran") end,
        })"#,
    );
    assert_eq!(
        value,
        Ok(Data::Map(BTreeMap::from([(
            "a".to_owned(),
            Data::Integer(1)
        )])))
    );
}

#[test]
fn a_conversion_stops_as_its_call_s_deadline_passes() {
    let lua = Lua::new();
    let list = eval(
        &lua,
        "(function() local t = {} for i = 1, 1000 do t[i] = i end return t end)()",
    );
    let Value::Table(table) = list.clone() else {
        panic!("a table");
    };
    let gone = || Budget::within(crate::watchdog::Deadline::gone());
    let past = "the call's deadline passed";
    // As a list is read, and as a map's entries are.
    assert!(sequence(&table, &gone()).unwrap_err().0.contains(past));
    assert!(entries(&table, &gone()).unwrap_err().0.contains(past));
    assert!(sequence(&table, &Budget::default()).unwrap().is_some());
    // As values convert, a few at a time.
    let budget = gone();
    let stopped = (0..2 * LOOK_EVERY).find_map(|_| budget.count().err());
    assert!(stopped.is_some_and(|error| error.0.contains(past)));
    // A value's conversion.
    let error = Vec::<u64>::deserialize(Taking::new(list, &gone())).unwrap_err();
    assert!(error.0.contains(past), "{error}");
}

#[test]
fn a_capability_is_its_text_as_a_manifest_writes_it() {
    use lokyr_plugin_sdk::command::Capability;

    gives(&Capability::PtyWrite, r#""pty-write""#);
    gives(
        &Capability::Net("example.com".to_owned()),
        r#""net:example.com""#,
    );
    gives(
        &vec![
            Capability::FsRead("~/notes".to_owned()),
            Capability::EnvRead("HOME".to_owned()),
        ],
        r#"{ "fs-read:~/notes", "env-read:HOME" }"#,
    );
    // A host's text after its first `:`, whatever it holds.
    assert_eq!(
        take::<Capability>(r#""net:[::1]""#),
        Ok(Capability::Net("[::1]".to_owned()))
    );
    assert!(
        take::<Capability>(r#""nett:example.com""#)
            .unwrap_err()
            .0
            .contains("`nett` is no variant of Capability")
    );
    assert!(take::<Capability>(r#""pty-write:x""#).is_err());
}

#[test]
fn a_refusal_tells_the_first_characters_of_a_long_text() {
    let long = r#"string.rep("y", 1024 * 1024)"#;
    // A variant of no name of the enum's, a field of no name of the
    // struct's, a character of many: each told short.
    let variant = take::<Corner>(long).unwrap_err().0;
    assert!(
        variant.starts_with(&format!("`{}…` is no variant", "y".repeat(64))),
        "{}",
        &variant[..100]
    );
    let field = take::<PanelOptions>(&format!("{{ [{long}] = 1 }}"))
        .unwrap_err()
        .0;
    assert!(
        field.starts_with(&format!("no field `{}…`", "y".repeat(64))),
        "{}",
        &field[..100]
    );
    let character = take::<char>(long).unwrap_err().0;
    assert!(character.len() < 200, "{}", character.len());
    // A short text, whole.
    assert_eq!(excerpt("short"), "short");
    assert_eq!(excerpt(&"é".repeat(65)), format!("{}…", "é".repeat(64)));
}
