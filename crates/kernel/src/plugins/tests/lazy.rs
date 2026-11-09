//! Plugins activated lazily, at their manifests' triggers: the app's start,
//! a command's run, an event, a slot's view; their commands shown before
//! they activate; and the triggers that the app does not take yet.

use std::cell::RefCell;
use std::rc::Rc;

use lokyr_plugin_sdk::view::Node;

use super::*;

/// The manifest of the plugin `acme/<name>`, of the activation `on` and of
/// what follows its `[plugin]` section, `more`.
fn manifest(name: &str, on: &[&str], more: &str) -> Manifest {
    let on: Vec<String> = on.iter().map(|on| format!("\"{on}\"")).collect();
    Manifest::parse(&format!(
        "[plugin]\nname = \"acme/{name}\"\nversion = \"1.0.0\"\nkind = \"luau\"\napi = \"^0.1\"\nlicense = \"MIT\"\n[activation]\non = [{}]\n{more}",
        on.join(", ")
    ))
    .unwrap()
}

/// What a test's plugin did, in its order: its activation, each command,
/// each batch's events' names and each render.
type Did = Rc<RefCell<Vec<String>>>;

/// A plugin that registers `command` and subscribes to `events` as it
/// activates, and tells what it did in `did`; its views a text.
struct Lazy {
    command: &'static str,
    events: &'static [&'static str],
    did: Did,
}

impl Plugin for Lazy {
    fn activate(&mut self, cx: &mut Cx<'_>) -> Result<(), Error> {
        self.did.borrow_mut().push("activate".to_owned());
        if !self.command.is_empty() {
            cx.register_command(command(self.command))?;
        }
        cx.subscribe(self.events)
    }

    fn on_command(&mut self, _: &mut Cx<'_>, name: &str, _: Args) -> Result<(), Error> {
        self.did.borrow_mut().push(name.to_owned());
        Ok(())
    }

    fn on_event(&mut self, _: &mut Cx<'_>, events: &[Event]) -> Result<(), Error> {
        let names: Vec<&str> = events.iter().map(Event::name).collect();
        self.did.borrow_mut().push(names.join(" "));
        Ok(())
    }

    fn render(&mut self, _: &mut Cx<'_>, _: &ViewId) -> Option<Node> {
        self.did.borrow_mut().push("render".to_owned());
        Some(Node::text("lazy"))
    }
}

/// The plugin of `command` and `events`, and what it did.
fn lazy(command: &'static str, events: &'static [&'static str]) -> (Box<dyn Plugin>, Did) {
    let did = Did::default();
    let plugin = Lazy {
        command,
        events,
        did: Rc::clone(&did),
    };
    (Box::new(plugin), did)
}

#[test]
fn a_plugin_of_no_trigger_or_of_the_start_activates_at_the_start() {
    let mut app = App::default();
    let mut plugins = Plugins::new();
    let (none, did_none) = lazy("", &[]);
    let (started, did_started) = lazy("", &[]);
    plugins
        .add_declared(manifest("none", &[], ""), none)
        .unwrap();
    plugins
        .add_declared(
            manifest("started", &["startup", "event:tab-created"], ""),
            started,
        )
        .unwrap();
    assert!(plugins.activate(&mut app.context()).is_empty());
    assert_eq!(plugins.status("acme/none"), Some(Status::Active));
    assert_eq!(plugins.status("acme/started"), Some(Status::Active));
    assert_eq!(*did_none.borrow(), ["activate"]);
    assert_eq!(*did_started.borrow(), ["activate"]);
}

#[test]
fn a_plugin_of_a_command_s_trigger_activates_as_the_command_first_runs() {
    let mut app = App::default();
    let mut plugins = Plugins::new();
    let (plugin, did) = lazy("late.go", &[]);
    let more = "[contributes]\ncommands = [{ name = \"late.go\", title = \"Go Late\", category = \"tabs\" }]\n";
    plugins
        .add_declared(manifest("late", &["command:late.*"], more), plugin)
        .unwrap();
    assert!(plugins.take_declarations_changed());
    assert!(plugins.activate(&mut app.context()).is_empty());
    assert_eq!(plugins.status("acme/late"), Some(Status::Inactive));
    assert!(did.borrow().is_empty());
    // Its command shows, of its manifest, before it activates.
    let shown = plugins.commands_in("en");
    assert_eq!(shown.len(), 1);
    assert_eq!(shown[0].0.name, "late.go");
    assert_eq!(shown[0].0.title, "Go Late");
    assert_eq!(shown[0].0.category, commands::Category::Tabs);
    assert_eq!(shown[0].1, "acme/late");
    assert!(plugins.owns("late.go"));
    assert_eq!(plugins.owner_of("late.go"), Some("acme/late"));
    assert!(!plugins.owns("late.other") || plugins.owner_of("late.other") == Some("acme/late"));
    // As the app's registry holds it, which its activation's command
    // takes the place of.
    app.registry
        .register_for(shown[0].0.clone(), &shown[0].1)
        .unwrap();
    // A menu's question activates nothing.
    assert_eq!(
        plugins.applies("late.go", &Args::new(), &mut app.context()),
        Ok(true)
    );
    assert!(did.borrow().is_empty());
    // Its first run activates it, then runs the command.
    plugins
        .run("late.go", Args::new(), &mut app.context())
        .unwrap();
    assert_eq!(plugins.status("acme/late"), Some(Status::Active));
    assert_eq!(*did.borrow(), ["activate", "late.go"]);
    assert_eq!(
        app.registry.get("late.go").map(|command| command.category),
        Some(commands::Category::View)
    );
    plugins
        .run("late.go", Args::new(), &mut app.context())
        .unwrap();
    assert_eq!(*did.borrow(), ["activate", "late.go", "late.go"]);
    // A command of its pattern that its activation does not register.
    let error = plugins
        .run("late.never", Args::new(), &mut app.context())
        .unwrap_err();
    assert_eq!(error.code, Code::NotFound);
}

#[test]
fn a_plugin_of_an_event_s_trigger_activates_as_the_event_comes_and_hears_it() {
    let mut app = App::default();
    let mut plugins = Plugins::new();
    let (plugin, did) = lazy("", &["tab-created", "tab-closed"]);
    plugins
        .add_declared(manifest("watcher", &["event:tab-created"], ""), plugin)
        .unwrap();
    assert!(!plugins.take_declarations_changed());
    assert!(plugins.activate(&mut app.context()).is_empty());
    let tab = |created: bool| {
        let (tab, window) = (host::TabId(7), host::WindowId(1));
        Event::Workspace(if created {
            WorkspaceEvent::TabCreated {
                tab,
                window,
                index: 0,
            }
        } else {
            WorkspaceEvent::TabClosed { tab, window }
        })
    };
    // Another event wakes it not.
    plugins.post(tab(false));
    assert!(plugins.deliver(&mut app.context()).is_empty());
    assert_eq!(plugins.status("acme/watcher"), Some(Status::Inactive));
    plugins.post(tab(true));
    assert!(plugins.deliver(&mut app.context()).is_empty());
    assert_eq!(plugins.status("acme/watcher"), Some(Status::Active));
    assert_eq!(*did.borrow(), ["activate", "tab-created"]);
    plugins.post(tab(false));
    assert!(plugins.deliver(&mut app.context()).is_empty());
    assert_eq!(*did.borrow(), ["activate", "tab-created", "tab-closed"]);
}

#[test]
fn a_plugin_of_a_slot_s_trigger_activates_as_its_view_first_shows() {
    let mut app = App::default();
    let window = window(&mut app, "sleep 10");
    let mut plugins = Plugins::new();
    let (plugin, did) = lazy("", &[]);
    plugins
        .add_declared(
            manifest(
                "bar",
                &["slot:status-bar"],
                "[contributes]\nslots = [\"status-bar\"]\n",
            ),
            plugin,
        )
        .unwrap();
    assert!(plugins.activate(&mut app.context()).is_empty());
    assert_eq!(plugins.status("acme/bar"), Some(Status::Inactive));
    // It fills its slot as it waits, and not another.
    assert_eq!(plugins.providers(Slot::StatusBar), ["acme/bar"]);
    assert!(plugins.providers(Slot::TabBar).is_empty());
    let view = ViewId::StatusBar(host::WindowId(window.get()));
    plugins
        .render("acme/bar", &view, &mut app.context())
        .unwrap();
    assert_eq!(plugins.status("acme/bar"), Some(Status::Active));
    assert_eq!(*did.borrow(), ["activate", "render"]);
    assert!(plugins.tree("acme/bar", &view).is_some());
}

#[test]
fn a_plugin_of_a_thread_of_its_own_runs_its_trigger_s_command_after_its_activation() {
    let mut app = App::default();
    let mut plugins = Plugins::new();
    let ran = Arc::new(std::sync::Mutex::new(Vec::new()));
    let told = Arc::clone(&ran);
    let more = "[contributes]\ncommands = [{ name = \"far.go\", title = \"Go Far\" }]\n";
    struct Far(Arc<std::sync::Mutex<Vec<String>>>);
    impl Plugin for Far {
        fn activate(&mut self, cx: &mut Cx<'_>) -> Result<(), Error> {
            self.0.lock().unwrap().push("activate".to_owned());
            cx.register_command(command("far.go"))
        }
        fn on_command(&mut self, _: &mut Cx<'_>, name: &str, _: Args) -> Result<(), Error> {
            self.0.lock().unwrap().push(name.to_owned());
            Ok(())
        }
    }
    plugins
        .add_installed(
            manifest("far", &["command:far.go"], more),
            {
                let told = Arc::clone(&told);
                Box::new(move || Box::new(Far(Arc::clone(&told))))
            },
            (&[], &[]),
        )
        .unwrap();
    assert!(plugins.activate(&mut app.context()).is_empty());
    assert_eq!(plugins.status("acme/far"), Some(Status::Inactive));
    plugins
        .run("far.go", Args::new(), &mut app.context())
        .unwrap();
    let start = Instant::now();
    while plugins.waiting() || ran.lock().unwrap().len() < 2 {
        plugins.serve(&mut app.context());
        assert!(start.elapsed() < Duration::from_secs(10), "a call hangs");
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(*ran.lock().unwrap(), ["activate", "far.go"]);
    assert_eq!(plugins.status("acme/far"), Some(Status::Active));
    assert_eq!(plugins.commands_in("en")[0].0.title, "far.go");
}

#[test]
fn an_activation_that_the_app_does_not_take_yet_refuses_its_plugin() {
    let mut plugins = Plugins::new();
    let (plugin, _) = lazy("", &[]);
    let error = plugins
        .add_declared(manifest("osc", &["osc:1337"], ""), plugin)
        .unwrap_err();
    assert_eq!(error.code, Code::Unavailable);
    assert!(error.message.contains("`osc:1337`"), "{error:?}");
    assert_eq!(plugins.status("acme/osc"), None);
    assert!(matches_pattern("late.*", "late.go"));
    assert!(matches_pattern("*.go", "late.go"));
    assert!(matches_pattern("l*e.g*", "late.go"));
    assert!(!matches_pattern("late.*", "lately.go"));
    assert!(!matches_pattern("late.go", "late.gone"));
    assert!(matches_pattern("*", "anything"));
}

#[test]
fn a_lazy_plugin_turned_off_and_on_again_waits_for_its_trigger_again() {
    let mut app = App::default();
    let mut plugins = Plugins::new();
    let (plugin, did) = lazy("late.go", &[]);
    let more = "[contributes]\ncommands = [{ name = \"late.go\", title = \"Go Late\" }]\n";
    plugins
        .add_declared(manifest("late", &["command:late.go"], more), plugin)
        .unwrap();
    assert!(plugins.activate(&mut app.context()).is_empty());
    let config = |text: &str| {
        crate::config::Config::parse(text, std::path::Path::new("config.toml")).unwrap()
    };
    assert!(
        plugins
            .configure(&config("[plugins.late]\nenabled = false\n").plugins)
            .is_empty()
    );
    assert!(plugins.activate(&mut app.context()).is_empty());
    assert_eq!(plugins.status("acme/late"), Some(Status::Off));
    assert!(!plugins.owns("late.go"));
    assert!(plugins.commands_in("en").is_empty());
    assert!(plugins.configure(&config("").plugins).is_empty());
    assert!(plugins.activate(&mut app.context()).is_empty());
    assert_eq!(plugins.status("acme/late"), Some(Status::Inactive));
    assert!(plugins.owns("late.go"));
    assert!(did.borrow().is_empty());
}

#[test]
fn a_plugin_removed_stops_and_is_forgotten() {
    let mut app = App::default();
    let mut plugins = Plugins::new();
    let (plugin, did) = lazy("gone.go", &["tab-created"]);
    plugins
        .add_declared(manifest("gone", &[], ""), plugin)
        .unwrap();
    assert!(plugins.activate(&mut app.context()).is_empty());
    assert!(app.registry.get("gone.go").is_some());
    // An event that waits for it goes with it.
    plugins.post(Event::Workspace(WorkspaceEvent::TabCreated {
        tab: host::TabId(1),
        window: host::WindowId(1),
        index: 0,
    }));
    assert!(plugins.remove("acme/gone", &mut app.context()));
    assert!(!plugins.remove("acme/gone", &mut app.context()));
    assert_eq!(plugins.status("acme/gone"), None);
    assert!(!plugins.owns("gone.go"));
    assert!(app.registry.get("gone.go").is_none());
    assert!(plugins.take_declarations_changed());
    assert!(plugins.deliver(&mut app.context()).is_empty());
    assert_eq!(*did.borrow(), ["activate"]);
    // Its namespace is free again, and a fault of the new one is told, as
    // the old one's was.
    for _ in 0..2 {
        plugins
            .add_declared(
                manifest("gone", &[], ""),
                scripted(
                    |_| Err(Error::new(Code::Failed, "it fails")),
                    |_, _, _| Ok(()),
                ),
            )
            .unwrap();
        assert_eq!(plugins.activate(&mut app.context()).len(), 1);
        assert_eq!(plugins.take_disabled().len(), 1);
        assert!(plugins.remove("acme/gone", &mut app.context()));
    }
}

/// A plugin that registers `argued.go`, of a required `path` and a
/// `count` of 3 by default, and keeps the arguments of each of its calls.
struct Argued(Arc<std::sync::Mutex<Vec<Args>>>);

impl Plugin for Argued {
    fn activate(&mut self, cx: &mut Cx<'_>) -> Result<(), Error> {
        cx.register_command(Command {
            params: vec![
                Param {
                    name: "path".to_owned(),
                    kind: Kind::String,
                    default: None,
                },
                Param {
                    name: "count".to_owned(),
                    kind: Kind::Integer,
                    default: Some(Value::Integer(3)),
                },
            ],
            ..command("argued.go")
        })
    }

    fn on_command(&mut self, _: &mut Cx<'_>, _: &str, args: Args) -> Result<(), Error> {
        self.0.lock().unwrap().push(args);
        Ok(())
    }
}

/// The manifest of `acme/argued`, which waits for `argued.go`, of the
/// capabilities `capabilities`.
fn argued_manifest(capabilities: &str) -> Manifest {
    manifest(
        "argued",
        &["command:argued.go"],
        &format!(
            "[capabilities]\n{capabilities}\n[contributes]\ncommands = [{{ name = \"argued.go\", title = \"Go\" }}]\n"
        ),
    )
}

/// The arguments of a `path`, and of `more`.
fn path_args(more: &[(&str, Value)]) -> Args {
    let mut args = Args::from([("path".to_owned(), Value::String("here".to_owned()))]);
    args.extend(
        more.iter()
            .map(|(name, value)| ((*name).to_owned(), value.clone())),
    );
    args
}

/// Registers the plugins' commands in the app's registry, as the app does:
/// those of the plugins that wait as theirs.
fn registered(plugins: &Plugins, app: &mut App) {
    for (command, plugin, waits) in plugins.commands_in("en") {
        let _ = app.registry.unregister(&command.name);
        if waits {
            app.registry.register_waiting(command, &plugin).unwrap();
        } else {
            app.registry.register_for(command, &plugin).unwrap();
        }
    }
}

#[test]
fn a_command_s_trigger_takes_its_plugin_s_namespace_alone() {
    let mut plugins = Plugins::new();
    for pattern in ["*", "window.*", "app.quit", "*.go", "other.go"] {
        let mut taking = manifest("taker", &["command:taker.go"], "");
        taking.activation = vec![format!("command:{pattern}")];
        let (plugin, _) = lazy("", &[]);
        let error = plugins.add_declared(taking, plugin).unwrap_err();
        assert_eq!(error.code, Code::InvalidConfig, "{pattern}");
        assert!(error.message.contains("another namespace"), "{error:?}");
    }
    assert_eq!(plugins.status("acme/taker"), None);
    // A plugin's own, of its manifest's commands too, takes no other's.
    let (plugin, _) = lazy("taker.go", &[]);
    plugins
        .add_declared(manifest("taker", &["command:taker.*"], ""), plugin)
        .unwrap();
    assert!(plugins.owns("taker.go"));
    assert!(plugins.owns("taker.other"));
    assert!(!plugins.owns("app.quit"));
    assert!(!plugins.owns("window.new"));
}

#[test]
fn a_waiting_plugin_s_command_takes_arguments_that_its_own_command_checks() {
    let mut app = App::default();
    let mut plugins = Plugins::new();
    let calls = Arc::new(std::sync::Mutex::new(Vec::new()));
    plugins
        .add_declared(argued_manifest(""), Box::new(Argued(Arc::clone(&calls))))
        .unwrap();
    assert!(plugins.activate(&mut app.context()).is_empty());
    registered(&plugins, &mut app);
    // The registry takes any arguments of its manifest's command, as a
    // binding or a menu item gives them.
    assert!(app.registry.is_waiting("argued.go"));
    let args = app
        .registry
        .check("argued.go", path_args(&[("count", Value::Integer(5))]))
        .unwrap();
    plugins
        .run("argued.go", path_args(&[]), &mut app.context())
        .unwrap();
    assert_eq!(plugins.status("acme/argued"), Some(Status::Active));
    // Its own command's defaults, once it activated.
    assert_eq!(
        *calls.lock().unwrap(),
        [path_args(&[("count", Value::Integer(3))])]
    );
    plugins.run("argued.go", args, &mut app.context()).unwrap();
    assert_eq!(calls.lock().unwrap().len(), 2);
    // Arguments that its own command refuses, as it activates.
    for (args, code) in [
        (
            path_args(&[("nope", Value::Integer(1))]),
            Code::UnknownArgument,
        ),
        (Args::new(), Code::MissingArgument),
    ] {
        let mut app = App::default();
        let mut fresh = Plugins::new();
        fresh
            .add_declared(argued_manifest(""), Box::new(Argued(Arc::clone(&calls))))
            .unwrap();
        assert!(fresh.activate(&mut app.context()).is_empty());
        let error = fresh
            .run("argued.go", args, &mut app.context())
            .unwrap_err();
        assert_eq!(error.code, code, "{error:?}");
        assert_eq!(fresh.status("acme/argued"), Some(Status::Active));
    }
    assert_eq!(calls.lock().unwrap().len(), 2);
}

#[test]
fn a_waiting_plugin_on_its_thread_runs_the_command_that_woke_it_of_its_own_checks() {
    let mut app = App::default();
    let mut plugins = Plugins::new();
    let calls = Arc::new(std::sync::Mutex::new(Vec::new()));
    let made = Arc::clone(&calls);
    plugins
        .add_installed(
            argued_manifest(""),
            Box::new(move || Box::new(Argued(Arc::clone(&made)))),
            (&[], &[]),
        )
        .unwrap();
    assert!(plugins.activate(&mut app.context()).is_empty());
    plugins
        .run("argued.go", path_args(&[]), &mut app.context())
        .unwrap();
    plugins
        .run(
            "argued.go",
            path_args(&[("nope", Value::Integer(1))]),
            &mut app.context(),
        )
        .unwrap();
    let start = Instant::now();
    let mut failed = Vec::new();
    while plugins.waiting() || calls.lock().unwrap().is_empty() {
        failed.extend(plugins.serve(&mut app.context()).commands);
        assert!(start.elapsed() < Duration::from_secs(10), "a call hangs");
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(
        *calls.lock().unwrap(),
        [path_args(&[("count", Value::Integer(3))])]
    );
    assert_eq!(failed.len(), 1, "{failed:?}");
    assert_eq!(failed[0].code, Code::UnknownArgument);
}

#[test]
fn another_plugin_runs_a_waiting_plugin_s_command_of_its_plugin_s_capabilities() {
    let mut app = App::default();
    let mut plugins = Plugins::new();
    let calls = Arc::new(std::sync::Mutex::new(Vec::new()));
    plugins
        .add_declared(
            argued_manifest("clipboard-read = true"),
            Box::new(Argued(Arc::clone(&calls))),
        )
        .unwrap();
    // Two callers, of `[uses]` of it: one without `clipboard-read`.
    let caller = |name: &str, capabilities: &str| {
        manifest(
            name,
            &[],
            &format!("[capabilities]\n{capabilities}\n[uses]\ncommands = [\"argued.go\"]\n"),
        )
    };
    let ran = Rc::new(RefCell::new(Vec::new()));
    for (name, capabilities) in [("bare", ""), ("reader", "clipboard-read = true")] {
        let told = Rc::clone(&ran);
        let named = name.to_owned();
        let call = format!("{name}.call");
        plugins
            .add_declared(
                caller(name, capabilities),
                scripted(
                    move |cx| cx.register_command(command(&call)),
                    move |cx, _, _| {
                        let asked = cx.run_command("argued.go", path_args(&[]));
                        told.borrow_mut()
                            .push((named.clone(), asked.map_err(|error| error.code)));
                        Ok(())
                    },
                ),
            )
            .unwrap();
    }
    assert!(plugins.activate(&mut app.context()).is_empty());
    registered(&plugins, &mut app);
    plugins
        .run("bare.call", Args::new(), &mut app.context())
        .unwrap();
    plugins
        .run("reader.call", Args::new(), &mut app.context())
        .unwrap();
    assert_eq!(
        *ran.borrow(),
        [
            ("bare".to_owned(), Err(Code::MissingCapability)),
            ("reader".to_owned(), Ok(())),
        ]
    );
}

#[test]
fn a_waiting_plugin_s_event_trigger_tells_the_app_to_post_its_events() {
    let mut app = App::default();
    let mut plugins = Plugins::new();
    let (plugin, did) = lazy("", &[POINTER]);
    plugins
        .add_declared(manifest("pointed", &["event:pointer"], ""), plugin)
        .unwrap();
    assert!(plugins.activate(&mut app.context()).is_empty());
    assert!(plugins.subscribes(POINTER));
    assert!(!plugins.subscribes(VIEWPORT_CHANGED));
    assert_eq!(plugins.status("acme/pointed"), Some(Status::Inactive));
    assert!(did.borrow().is_empty());
}
