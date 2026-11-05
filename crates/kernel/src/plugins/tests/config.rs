//! The plugins' sections of the config, checked against their schemas,
//! and their states, kept between runs.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use lokyr_plugin_sdk::message::Data;

use super::*;
use crate::config::Config;

/// The schema of the tests' plugin of keys, `greeter`.
const SCHEMA: &str = r#"{
  "type": "object",
  "properties": {
    "greeting": { "type": "string", "default": "hello" },
    "count": { "type": "integer", "minimum": 1, "default": 3 }
  }
}"#;

/// The manifest of `greeter`, of its schema.
const MANIFEST: &str = concat!(
    "[plugin]\nname = \"acme/greeter\"\nkind = \"builtin\"\nlicense = \"MIT\"\n",
    "[contributes]\nconfig-schema = \"schema.json\"\n",
);

/// A plugin of keys, which reads its config as it activates and as it
/// changes, into `read`; its command `greeter.keep` keeps its argument
/// `history` as its state, and reads the state before and after, into
/// `kept`.
struct Greeter {
    read: Rc<RefCell<Vec<Data>>>,
    kept: Rc<RefCell<Vec<Data>>>,
    schema: Option<&'static str>,
}

impl Plugin for Greeter {
    fn activate(&mut self, cx: &mut Cx<'_>) -> Result<(), Error> {
        self.read.borrow_mut().push(cx.config()?);
        cx.register_command(Command {
            params: vec![lokyr_plugin_sdk::command::Param {
                name: "history".to_owned(),
                kind: lokyr_plugin_sdk::command::Kind::String,
                default: Some(lokyr_plugin_sdk::command::Value::String(String::new())),
            }],
            ..command("greeter.keep")
        })
    }

    fn on_command(&mut self, cx: &mut Cx<'_>, _: &str, args: Args) -> Result<(), Error> {
        let Some(lokyr_plugin_sdk::command::Value::String(history)) = args.get("history") else {
            return Ok(());
        };
        let before = cx.state()?;
        let entry = if history == "NaN" {
            Data::Number(f64::NAN)
        } else {
            Data::Text(history.clone())
        };
        cx.set_state(Data::Map(BTreeMap::from([(
            "history".to_owned(),
            Data::List(vec![entry]),
        )])))?;
        let after = cx.state()?;
        self.kept.borrow_mut().extend([before, after]);
        Ok(())
    }

    fn on_event(&mut self, cx: &mut Cx<'_>, events: &[Event]) -> Result<(), Error> {
        for event in events {
            if *event == Event::ConfigChanged {
                self.read.borrow_mut().push(cx.config()?);
            }
        }
        Ok(())
    }

    fn config_schema(&self) -> Option<&'static str> {
        self.schema
    }
}

/// `greeter`'s config of `greeting` and `count`.
fn greeting(greeting: &str, count: i64) -> Data {
    Data::Map(BTreeMap::from([
        ("count".to_owned(), Data::Integer(count)),
        ("greeting".to_owned(), Data::Text(greeting.to_owned())),
    ]))
}

/// The config of `text`, as the file `config.toml` holds it.
fn config(text: &str) -> Config {
    Config::parse(text, Path::new("config.toml")).unwrap_or_else(|error| panic!("{error}"))
}

/// The plugins, with `greeter`, whose reads of its config `read` takes.
fn greeter(read: &Rc<RefCell<Vec<Data>>>) -> Plugins {
    let mut plugins = Plugins::new();
    plugins
        .add_builtin(
            MANIFEST,
            Box::new(Greeter {
                read: Rc::clone(read),
                kept: Rc::default(),
                schema: Some(SCHEMA),
            }),
        )
        .unwrap();
    plugins
}

#[test]
fn a_plugin_reads_its_keys_with_their_defaults_and_hears_of_a_reload() {
    let mut app = App::default();
    let read = Rc::default();
    let mut plugins = greeter(&read);
    assert!(
        plugins
            .configure(&config("[plugins.greeter]\ncount = 5\n").plugins)
            .is_empty()
    );
    assert!(plugins.activate(&mut app.context()).is_empty());
    assert_eq!(*read.borrow(), [greeting("hello", 5)]);
    assert!(!plugins.has_events());
    // A reload of new values tells it, which it reads.
    let reloaded = config("# mine\n[plugins.greeter]\ngreeting = \"hi\"\ncount = 7\n");
    assert!(plugins.configure(&reloaded.plugins).is_empty());
    assert!(plugins.deliver(&mut app.context()).is_empty());
    assert_eq!(read.borrow().last(), Some(&greeting("hi", 7)));
    // The same values, of other lines, tell it nothing.
    let moved = config("[plugins.greeter]\ncount = 7\n\n\ngreeting = \"hi\"\n");
    assert!(plugins.configure(&moved.plugins).is_empty());
    assert!(!plugins.has_events());
    // A section left out gives the defaults.
    assert!(plugins.configure(&config("").plugins).is_empty());
    assert!(plugins.deliver(&mut app.context()).is_empty());
    assert_eq!(read.borrow().last(), Some(&greeting("hello", 3)));
    assert_eq!(read.borrow().len(), 3);
}

#[test]
fn a_wrong_value_refuses_its_section_alone_at_its_line() {
    let mut app = App::default();
    let read = Rc::default();
    let mut plugins = greeter(&read);
    assert!(
        plugins
            .configure(&config("[plugins.greeter]\ncount = 5\n").plugins)
            .is_empty()
    );
    assert!(plugins.activate(&mut app.context()).is_empty());
    let wrong = config("[font]\nsize = 14\n\n[plugins.greeter]\ngreeting = \"hi\"\ncount = 0\n");
    assert_eq!(wrong.font.size.points(), 14.0);
    let notices = plugins.configure(&wrong.plugins);
    assert_eq!(
        notices,
        [ConfigNotice {
            namespace: "greeter".to_owned(),
            refused: true,
            text: "config.toml:6:1: `count` is at least 1, not 0 in `[plugins.greeter]`; the plugin `acme/greeter` keeps its config in force".to_owned(),
        }]
    );
    // Its config in force stays, of neither key.
    assert!(!plugins.has_events());
    assert_eq!(*read.borrow(), [greeting("hello", 5)]);
    // A value deep in a table tells its own line.
    let deep = r#"{ "type": "object", "properties": { "colors": { "type": "array", "items": { "type": "string" } } } }"#;
    let mut plugins = Plugins::new();
    plugins
        .add_builtin(
            MANIFEST,
            Box::new(Greeter {
                read: Rc::default(),
                kept: Rc::default(),
                schema: Some(deep),
            }),
        )
        .unwrap();
    let notices =
        plugins.configure(&config("[plugins.greeter]\ncolors = [\n  \"red\",\n  2,\n]\n").plugins);
    assert_eq!(notices.len(), 1);
    assert!(
        notices[0]
            .text
            .starts_with("config.toml:4:3: `1` is a string, not 2"),
        "{}",
        notices[0].text
    );
}

#[test]
fn what_another_version_would_know_is_ignored_with_a_notice() {
    let mut app = App::default();
    let read = Rc::default();
    let mut plugins = greeter(&read);
    plugins
        .add("keyless", scripted(|_| Ok(()), |_, _, _| Ok(())))
        .unwrap();
    let text = "[plugins.greeter]\ncont = 2\nnewer = true\n\n[plugins.absent]\nx = 1\n\n[plugins.keyless]\ny = 2\n";
    let notices = plugins.configure(&config(text).plugins);
    let texts: Vec<(&str, bool, &str)> = notices
        .iter()
        .map(|notice| {
            (
                notice.namespace.as_str(),
                notice.refused,
                notice.text.as_str(),
            )
        })
        .collect();
    assert_eq!(
        texts,
        [
            (
                "greeter",
                false,
                "config.toml:2:1: unknown key `cont` in `[plugins.greeter]`, and 1 more, which the plugin `acme/greeter` ignores; did you mean `count`?"
            ),
            (
                "keyless",
                false,
                "config.toml:8:1: the plugin `keyless` has no keys, and `[plugins.keyless]` is ignored"
            ),
            (
                "absent",
                false,
                "config.toml:5:1: no plugin of the namespace `absent` is here, and `[plugins.absent]` is ignored"
            ),
        ]
    );
    assert!(plugins.activate(&mut app.context()).is_empty());
    assert_eq!(*read.borrow(), [greeting("hello", 3)]);
}

#[test]
fn a_schema_that_its_manifest_does_not_name_or_that_does_not_read_adds_no_plugin() {
    let plugin = |schema| {
        Box::new(Greeter {
            read: Rc::default(),
            kept: Rc::default(),
            schema,
        })
    };
    let unnamed = MANIFEST.replace("[contributes]\nconfig-schema = \"schema.json\"\n", "");
    let cases = [
        (
            unnamed.as_str(),
            Some(SCHEMA),
            "gives a config schema that its manifest does not name",
        ),
        (
            MANIFEST,
            None,
            "names the config schema `schema.json`, which the plugin does not give",
        ),
        (
            MANIFEST,
            Some(r#"{ "type": "object", "properties": { "a": { "pattern": "x" } } }"#),
            "does not read: the keyword `pattern` at `/properties/a` is not one that the host checks",
        ),
    ];
    for (manifest, schema, message) in cases {
        let mut plugins = Plugins::new();
        let error = plugins.add_builtin(manifest, plugin(schema)).unwrap_err();
        assert_eq!(error.code, Code::InvalidConfig);
        assert!(error.message.contains(message), "{}", error.message);
        assert_eq!(plugins.status("acme/greeter"), None);
    }
}

/// A directory of the test's own, empty.
fn directory(test: &str) -> PathBuf {
    let directory = std::env::temp_dir().join(format!("lokyr-{test}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    directory
}

/// The plugins of `greeter` whose states are kept in `state`, activated,
/// whose state's reads, before and after its writes, `kept` takes.
fn keeping(app: &mut App, state: Option<PathBuf>, kept: &Rc<RefCell<Vec<Data>>>) -> Plugins {
    let mut plugins = Plugins::new();
    plugins.set_state_directory(state);
    plugins
        .add_builtin(
            MANIFEST,
            Box::new(Greeter {
                read: Rc::default(),
                kept: Rc::clone(kept),
                schema: Some(SCHEMA),
            }),
        )
        .unwrap();
    assert!(plugins.activate(&mut app.context()).is_empty());
    plugins
}

/// Runs `greeter.keep` of `history`.
fn keep(plugins: &mut Plugins, app: &mut App, history: &str) -> Result<(), Error> {
    plugins.run(
        "greeter.keep",
        Args::from([(
            "history".to_owned(),
            lokyr_plugin_sdk::command::Value::String(history.to_owned()),
        )]),
        &mut app.context(),
    )
}

#[test]
fn a_plugin_keeps_its_state_for_the_next_run() {
    let state = directory("plugin-state");
    let history = |text: &str| {
        Data::Map(BTreeMap::from([(
            "history".to_owned(),
            Data::List(vec![Data::Text(text.to_owned())]),
        )]))
    };
    let mut app = App::default();
    let kept = Rc::default();
    let mut plugins = keeping(&mut app, Some(state.clone()), &kept);
    keep(&mut plugins, &mut app, "ls").unwrap();
    // The read after the write, in the same call, reads the state before.
    assert_eq!(*kept.borrow(), [Data::Nil, Data::Nil]);
    let file = state.join("plugins/greeter.json");
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        r#"{"history":["ls"]}"#
    );
    keep(&mut plugins, &mut app, "pwd").unwrap();
    assert_eq!(kept.borrow()[2], history("ls"));
    // The next run reads it.
    let mut app = App::default();
    let kept = Rc::default();
    let mut plugins = keeping(&mut app, Some(state.clone()), &kept);
    keep(&mut plugins, &mut app, "cd").unwrap();
    assert_eq!(kept.borrow()[0], history("pwd"));
    assert!(plugins.take_faults().is_empty());
    // A file that does not read starts anew, as the log tells.
    std::fs::write(&file, "{").unwrap();
    let mut app = App::default();
    let kept = Rc::default();
    let mut plugins = keeping(&mut app, Some(state.clone()), &kept);
    keep(&mut plugins, &mut app, "x").unwrap();
    assert_eq!(kept.borrow()[0], Data::Nil);
    let [fault] = &plugins.take_faults()[..] else {
        panic!("a fault of the file");
    };
    assert!(fault.message.contains("starts anew"), "{}", fault.message);
    // Too big a state, and one of a number that is not finite, are refused.
    let big = "x".repeat(MOST_STATE);
    let refused = keep(&mut plugins, &mut app, &big).unwrap_err();
    assert_eq!(refused.code, Code::Failed);
    assert!(
        refused.message.contains("bytes at most"),
        "{}",
        refused.message
    );
    let refused = keep(&mut plugins, &mut app, "NaN").unwrap_err();
    assert!(
        refused.message.contains("finite numbers"),
        "{}",
        refused.message
    );
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        r#"{"history":["x"]}"#
    );
    let _ = std::fs::remove_dir_all(&state);
    // Without a state directory, it keeps nothing.
    let mut app = App::default();
    let kept = Rc::default();
    let mut plugins = keeping(&mut app, None, &kept);
    let refused = keep(&mut plugins, &mut app, "x").unwrap_err();
    assert_eq!(refused.code, Code::Unavailable);
}
