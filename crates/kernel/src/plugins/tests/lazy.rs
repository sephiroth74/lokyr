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
            Box::new(Far(told)),
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
