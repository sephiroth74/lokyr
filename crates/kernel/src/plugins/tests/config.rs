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

/// The plugins that `Toggled` made, and those that it activated.
static TOGGLED_MADE: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// A plugin of a status bar, which registers `toggled.go`, binds it and
/// subscribes as it activates; its command decorates the focused pane's
/// first cells and opens a panel `p` at the pointer.
struct Toggled;

impl Plugin for Toggled {
    fn activate(&mut self, cx: &mut Cx<'_>) -> Result<(), Error> {
        cx.register_command(command("toggled.go"))?;
        cx.register_command(command("toggled.boom"))?;
        cx.bind(binding("cmd+t", "toggled.go", None))?;
        cx.subscribe(&["tab-created"])
    }

    fn on_command(&mut self, cx: &mut Cx<'_>, name: &str, _: Args) -> Result<(), Error> {
        assert_ne!(name, "toggled.boom", "a fault");
        let pane = cx.focused_pane().expect("a focused pane");
        cx.decorate(
            pane,
            (0, 0),
            (3, 0),
            host::DecorationLook {
                underline: Some(host::Underline::Single),
                underline_color: None,
                background: None,
                pointing: false,
            },
        )?;
        let window = cx.focused_window().expect("a focused window");
        cx.open_panel(
            "p",
            lokyr_plugin_sdk::ui::PanelOptions {
                anchor: lokyr_plugin_sdk::ui::Anchor::Pointer(window),
                role: lokyr_plugin_sdk::ui::Role::Menu,
                close_outside: true,
                close_escape: true,
            },
        )
    }

    fn render(&mut self, _: &mut Cx<'_>, _: &ViewId) -> Option<lokyr_plugin_sdk::view::Node> {
        Some(lokyr_plugin_sdk::view::Node::text("toggled"))
    }
}

const TOGGLED: &str = concat!(
    "[plugin]\nname = \"acme/toggled\"\nkind = \"builtin\"\nlicense = \"MIT\"\n",
    "[contributes]\nslots = [\"status-bar\"]\n",
);

fn make_toggled() -> Box<dyn Plugin + Send> {
    TOGGLED_MADE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    Box::new(Toggled)
}

#[test]
fn a_plugin_that_its_section_turns_off_stops_and_one_turned_on_again_starts_anew() {
    use std::sync::atomic::Ordering;

    let made = || TOGGLED_MADE.load(Ordering::Relaxed);
    let mut app = App::default();
    let window = window(&mut app, "echo hello; sleep 30");
    let pane = app.workspace.active_pane(window).unwrap();
    text_once(&app, pane, "hello");
    let mut plugins = Plugins::new();
    plugins
        .add_registered(Registration {
            manifest: TOGGLED,
            make: make_toggled,
        })
        .unwrap();
    assert_eq!(made(), 1);
    // Off from the start: never activated, it keeps the plugin made.
    let off = config("[plugins.toggled]\nenabled = false\n");
    assert!(plugins.configure(&off.plugins).is_empty());
    assert!(plugins.activate(&mut app.context()).is_empty());
    assert_eq!(plugins.status("acme/toggled"), Some(Status::Off));
    assert!(app.registry.get("toggled.go").is_none());
    assert!(plugins.providers(Slot::StatusBar).is_empty());
    let on = config("[plugins.toggled]\nenabled = true\n");
    assert!(plugins.configure(&on.plugins).is_empty());
    assert!(plugins.activate(&mut app.context()).is_empty());
    assert_eq!(plugins.status("acme/toggled"), Some(Status::Active));
    assert_eq!(made(), 1);
    assert!(app.registry.get("toggled.go").is_some());
    assert!(plugins.take_bindings_changed());
    // Active, with a decoration, a panel and a view.
    plugins
        .run("toggled.go", Args::new(), &mut app.context())
        .unwrap();
    let view = ViewId::StatusBar(host::WindowId(window.get()));
    plugins
        .render("acme/toggled", &view, &mut app.context())
        .unwrap();
    assert!(plugins.tree("acme/toggled", &view).is_some());
    let _ = plugins.take_requests();
    let decorations = |app: &mut App| {
        let found = app.workspace.pane_mut(pane).unwrap();
        found.update();
        found.snapshot().decorations().len()
    };
    assert_eq!(decorations(&mut app), 1);
    // Turned off, it stops: its command, its keys, its subscription, its
    // view, its panel and its decoration go.
    assert!(plugins.configure(&off.plugins).is_empty());
    assert!(plugins.activate(&mut app.context()).is_empty());
    assert_eq!(plugins.status("acme/toggled"), Some(Status::Off));
    assert!(app.registry.get("toggled.go").is_none());
    assert!(plugins.take_bindings_changed());
    assert_eq!(plugins.bindings().count(), 0);
    assert!(plugins.take_written());
    assert_eq!(
        plugins.take_requests(),
        [Request::ClosePanel {
            plugin: "acme/toggled".to_owned(),
            id: PanelId::from("p"),
        }]
    );
    assert_eq!(plugins.tree("acme/toggled", &view), None);
    assert!(plugins.providers(Slot::StatusBar).is_empty());
    assert_eq!(decorations(&mut app), 0);
    assert!(!plugins.subscribes("tab-created"));
    assert!(plugins.take_disabled().is_empty());
    // Turned on again, a new one of it starts.
    assert!(plugins.configure(&config("").plugins).is_empty());
    assert!(plugins.activate(&mut app.context()).is_empty());
    assert_eq!(plugins.status("acme/toggled"), Some(Status::Active));
    assert_eq!(made(), 2);
    assert!(app.registry.get("toggled.go").is_some());
    assert!(plugins.subscribes("tab-created"));
    assert_eq!(plugins.providers(Slot::StatusBar), ["acme/toggled"]);
}

#[test]
fn enabled_is_true_or_false_the_host_s_key_of_every_section() {
    let mut app = App::default();
    let read = Rc::default();
    let mut plugins = greeter(&read);
    plugins
        .add("keyless", scripted(|_| Ok(()), |_, _, _| Ok(())))
        .unwrap();
    // The host's key, which neither the schema nor a keyless plugin counts.
    let text =
        "[plugins.greeter]\nenabled = true\ncount = 4\n\n[plugins.keyless]\nenabled = false\n";
    assert!(plugins.configure(&config(text).plugins).is_empty());
    assert!(plugins.activate(&mut app.context()).is_empty());
    assert_eq!(*read.borrow(), [greeting("hello", 4)]);
    assert_eq!(plugins.status("keyless"), Some(Status::Off));
    // A plugin turned off takes no keys, which tell nothing.
    let off = "[plugins.greeter]\nenabled = false\ncount = 0\nwrong = 1\n";
    assert!(plugins.configure(&config(off).plugins).is_empty());
    assert!(plugins.activate(&mut app.context()).is_empty());
    assert_eq!(plugins.status("acme/greeter"), Some(Status::Off));
    // Another value stays as it is.
    let wrong = "[plugins.greeter]\nenabled = \"yes\"\n";
    assert_eq!(
        plugins.configure(&config(wrong).plugins),
        [ConfigNotice {
            namespace: "greeter".to_owned(),
            refused: true,
            text: "config.toml:2:1: `enabled` is true or false, not \"yes\" in `[plugins.greeter]`; the plugin `acme/greeter` stays as it is".to_owned(),
        }]
    );
    assert!(plugins.activate(&mut app.context()).is_empty());
    assert_eq!(plugins.status("acme/greeter"), Some(Status::Off));
    // A plugin added without its registration starts again at the next
    // start alone.
    assert!(plugins.configure(&config("").plugins).is_empty());
    let errors = plugins.activate(&mut app.context());
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(
        errors[0]
            .message
            .contains("starts again at the app's next start")
    );
    assert_eq!(plugins.status("acme/greeter"), Some(Status::Off));
    assert_eq!(plugins.status("keyless"), Some(Status::Active));
    // A schema of `enabled` adds no plugin.
    let schema = r#"{ "type": "object", "properties": { "enabled": { "type": "boolean" } } }"#;
    let mut plugins = Plugins::new();
    let refused = plugins
        .add_builtin(
            MANIFEST,
            Box::new(Greeter {
                read: Rc::default(),
                kept: Rc::default(),
                schema: Some(schema),
            }),
        )
        .unwrap_err();
    assert!(
        refused
            .message
            .contains("the key `enabled`, which is the host's"),
        "{}",
        refused.message
    );
}

/// A `Toggled`, which no count tells, of the tests that run beside the one
/// that counts them.
fn make_uncounted() -> Box<dyn Plugin + Send> {
    Box::new(Toggled)
}

#[test]
fn a_plugin_that_stopped_of_a_fault_starts_again_as_its_section_turns_it_off_and_on() {
    let mut app = App::default();
    window(&mut app, "sleep 30");
    let mut plugins = Plugins::new();
    plugins
        .add_registered(Registration {
            manifest: TOGGLED,
            make: make_uncounted,
        })
        .unwrap();
    assert!(plugins.activate(&mut app.context()).is_empty());
    let boom = |plugins: &mut Plugins, app: &mut App| {
        assert!(
            plugins
                .run("toggled.boom", Args::new(), &mut app.context())
                .is_err()
        );
        assert_eq!(plugins.status("acme/toggled"), Some(Status::Disabled));
        let told = plugins.take_disabled();
        assert_eq!(told.len(), 1, "{told:?}");
    };
    boom(&mut plugins, &mut app);
    for text in ["[plugins.toggled]\nenabled = false\n", ""] {
        assert!(plugins.configure(&config(text).plugins).is_empty());
        assert!(plugins.activate(&mut app.context()).is_empty());
    }
    assert_eq!(plugins.status("acme/toggled"), Some(Status::Active));
    // The new one's fault is told too.
    boom(&mut plugins, &mut app);
}

#[test]
fn a_plugin_of_a_thread_of_its_own_that_its_section_turns_off_ends_its_thread() {
    let mut app = App::default();
    let mut plugins = Plugins::new();
    plugins
        .add_registered(Registration {
            manifest: concat!(
                "[plugin]\nname = \"acme/toggled\"\nkind = \"builtin\"\nlicense = \"MIT\"\n",
                "thread = \"worker\"\n",
            ),
            make: make_uncounted,
        })
        .unwrap();
    assert!(plugins.activate(&mut app.context()).is_empty());
    let start = std::time::Instant::now();
    while plugins.waiting() {
        plugins.serve(&mut app.context());
        assert!(start.elapsed() < std::time::Duration::from_secs(10));
    }
    assert!(app.registry.get("toggled.go").is_some());
    assert!(matches!(plugins.entries[0].runner, Runner::Worker(_)));
    assert!(
        plugins
            .configure(&config("[plugins.toggled]\nenabled = false\n").plugins)
            .is_empty()
    );
    assert!(plugins.activate(&mut app.context()).is_empty());
    assert!(matches!(plugins.entries[0].runner, Runner::Stopped));
    assert!(app.registry.get("toggled.go").is_none());
}
