//! The consent to an installed plugin's capabilities: its activation asks
//! the user first, once, what its trigger brought waiting for the answer;
//! the capabilities granted or refused; and those revoked, which stop it.

use std::sync::{Arc, Mutex};

use lokyr_plugin_sdk::view::Node;

use super::*;

/// What a test's plugin did, in its order.
type Did = Arc<Mutex<Vec<String>>>;

/// A plugin of a thread of its own that subscribes to `tab-created` as it
/// activates, tries to open a page, and tells what it did.
struct Asking(Did);

impl Plugin for Asking {
    fn activate(&mut self, cx: &mut Cx<'_>) -> Result<(), Error> {
        let opened = cx.open("https://lokyr.dev/").is_ok();
        self.0
            .lock()
            .unwrap()
            .push(format!("activate opened={opened}"));
        cx.subscribe(&["tab-created"])
    }

    fn on_command(&mut self, _: &mut Cx<'_>, _: &str, _: Args) -> Result<(), Error> {
        Ok(())
    }

    fn on_event(&mut self, _: &mut Cx<'_>, events: &[Event]) -> Result<(), Error> {
        let names: Vec<&str> = events.iter().map(Event::name).collect();
        self.0.lock().unwrap().push(names.join(" "));
        Ok(())
    }

    fn render(&mut self, _: &mut Cx<'_>, _: &ViewId) -> Option<Node> {
        self.0.lock().unwrap().push("render".to_owned());
        Some(Node::text("asked"))
    }
}

/// The manifest of `acme/asker`, of `open`, of the activation `on` and of
/// what follows, `more`.
fn manifest(on: &[&str], more: &str) -> Manifest {
    let on: Vec<String> = on.iter().map(|on| format!("\"{on}\"")).collect();
    Manifest::parse(&format!(
        "[plugin]\nname = \"acme/asker\"\nversion = \"1.0.0\"\nkind = \"luau\"\napi = \"^0.1\"\nlicense = \"MIT\"\n[activation]\non = [{}]\n[capabilities]\nopen = true\n{more}",
        on.join(", ")
    ))
    .unwrap()
}

/// The plugins of `acme/asker` of `on`, of no decision yet, and what it did.
fn asker(on: &[&str], more: &str) -> (Plugins, Did) {
    let did = Did::default();
    let mut plugins = Plugins::new();
    plugins
        .add_installed(
            manifest(on, more),
            {
                let did = Arc::clone(&did);
                Box::new(move || Box::new(Asking(Arc::clone(&did))))
            },
            (&[], &[]),
        )
        .unwrap();
    (plugins, did)
}

/// The questions that the plugins asked of the app since the last take.
fn questions(plugins: &mut Plugins) -> Vec<(String, Vec<Capability>)> {
    plugins
        .take_requests()
        .into_iter()
        .filter_map(|request| match request {
            Request::AskConsent {
                plugin,
                capabilities,
            } => Some((plugin, capabilities)),
            _ => None,
        })
        .collect()
}

/// Serves the plugins' threads until no call is under way.
fn served(plugins: &mut Plugins, app: &mut App) {
    let start = Instant::now();
    while plugins.waiting() {
        plugins.serve(&mut app.context());
        assert!(start.elapsed() < Duration::from_secs(10), "a call hangs");
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn an_activation_asks_once_and_runs_of_the_capabilities_granted_or_without_those_refused() {
    for allowed in [true, false] {
        let mut app = App::default();
        let (mut plugins, did) = asker(&[], "");
        assert!(plugins.activate(&mut app.context()).is_empty());
        assert_eq!(
            questions(&mut plugins),
            [("acme/asker".to_owned(), vec![Capability::Open])]
        );
        assert_eq!(plugins.status("acme/asker"), Some(Status::Inactive));
        // Asked once, however many times it is activated.
        assert!(plugins.activate(&mut app.context()).is_empty());
        assert!(questions(&mut plugins).is_empty());
        assert!(
            plugins
                .consent(
                    "acme/asker",
                    &[Capability::Open],
                    allowed,
                    &mut app.context()
                )
                .is_empty()
        );
        served(&mut plugins, &mut app);
        assert_eq!(plugins.status("acme/asker"), Some(Status::Active));
        assert_eq!(*did.lock().unwrap(), [format!("activate opened={allowed}")]);
        // Answered, a second answer changes nothing.
        assert!(
            plugins
                .consent(
                    "acme/asker",
                    &[Capability::Open],
                    !allowed,
                    &mut app.context()
                )
                .is_empty()
        );
    }
}

#[test]
fn the_events_and_the_views_of_a_trigger_come_once_the_user_answers() {
    let mut app = App::default();
    let window = window(&mut app, "sleep 10");
    let (mut plugins, did) = asker(
        &["event:tab-created", "slot:status-bar"],
        "[contributes]\nslots = [\"status-bar\"]\n",
    );
    assert!(plugins.activate(&mut app.context()).is_empty());
    assert!(questions(&mut plugins).is_empty());
    let created = Event::Workspace(WorkspaceEvent::TabCreated {
        tab: host::TabId(7),
        window: host::WindowId(window.get()),
        index: 0,
    });
    plugins.post(created);
    assert!(plugins.deliver(&mut app.context()).is_empty());
    let view = ViewId::StatusBar(host::WindowId(window.get()));
    plugins
        .render("acme/asker", &view, &mut app.context())
        .unwrap();
    // One question, of both triggers, which nothing came to before.
    assert_eq!(questions(&mut plugins).len(), 1);
    served(&mut plugins, &mut app);
    assert!(did.lock().unwrap().is_empty());
    assert!(plugins.take_renders().is_empty());
    assert!(
        plugins
            .consent("acme/asker", &[Capability::Open], true, &mut app.context())
            .is_empty()
    );
    served(&mut plugins, &mut app);
    assert_eq!(plugins.take_renders(), [("acme/asker".to_owned(), view)]);
    assert!(plugins.deliver(&mut app.context()).is_empty());
    served(&mut plugins, &mut app);
    assert_eq!(
        *did.lock().unwrap(),
        ["activate opened=true", "tab-created"]
    );
}

#[test]
fn a_capability_revoked_stops_its_plugin_and_one_decided_is_not_asked() {
    let mut app = App::default();
    let did = Did::default();
    let mut plugins = Plugins::new();
    let granted = [Capability::Open];
    plugins
        .add_installed(
            manifest(&[], ""),
            {
                let did = Arc::clone(&did);
                Box::new(move || Box::new(Asking(Arc::clone(&did))))
            },
            (&granted, &[]),
        )
        .unwrap();
    assert!(plugins.activate(&mut app.context()).is_empty());
    assert!(questions(&mut plugins).is_empty());
    served(&mut plugins, &mut app);
    assert_eq!(*did.lock().unwrap(), ["activate opened=true"]);
    // Granted still, it goes on.
    assert!(!plugins.set_consent("acme/asker", (&granted, &[]), &mut app.context()));
    assert_eq!(plugins.status("acme/asker"), Some(Status::Active));
    // Revoked, it stops, as a fault does.
    assert!(plugins.set_consent("acme/asker", (&[], &granted), &mut app.context()));
    assert_eq!(plugins.status("acme/asker"), Some(Status::Disabled));
    let disabled = plugins.take_disabled();
    assert_eq!(disabled.len(), 1);
    assert!(
        disabled[0]
            .1
            .contains("lost `open`, which the user revoked"),
        "{disabled:?}"
    );
    assert!(!plugins.waiting());
    // Not a fault of its own: it does not start again after a while, but
    // as it is turned off and on again.
    plugins.schedule_restarts(Instant::now());
    assert_eq!(plugins.next_restart(), None);
}

#[test]
fn a_plugin_turned_off_as_it_asks_does_not_activate_at_the_answer() {
    let mut app = App::default();
    let (mut plugins, did) = asker(&[], "");
    assert!(plugins.activate(&mut app.context()).is_empty());
    assert_eq!(questions(&mut plugins).len(), 1);
    let config = |text: &str| {
        crate::config::Config::parse(text, std::path::Path::new("config.toml")).unwrap()
    };
    assert!(
        plugins
            .configure(&config("[plugins.asker]\nenabled = false\n").plugins)
            .is_empty()
    );
    assert!(plugins.activate(&mut app.context()).is_empty());
    assert!(
        plugins
            .consent("acme/asker", &[Capability::Open], true, &mut app.context())
            .is_empty()
    );
    assert_eq!(plugins.status("acme/asker"), Some(Status::Off));
    assert!(did.lock().unwrap().is_empty());
}

#[test]
fn an_answer_grants_the_capabilities_of_its_question_alone_and_the_others_are_asked_again() {
    let mut app = App::default();
    let (mut plugins, did) = asker(&[], "clipboard-write = true\n");
    assert!(plugins.activate(&mut app.context()).is_empty());
    assert_eq!(
        questions(&mut plugins),
        [(
            "acme/asker".to_owned(),
            vec![Capability::ClipboardWrite, Capability::Open]
        )]
    );
    // An answer of a question of `open` alone, as of an older version's.
    assert!(
        plugins
            .consent("acme/asker", &[Capability::Open], true, &mut app.context())
            .is_empty()
    );
    assert_eq!(plugins.status("acme/asker"), Some(Status::Inactive));
    assert_eq!(
        questions(&mut plugins),
        [("acme/asker".to_owned(), vec![Capability::ClipboardWrite])]
    );
    assert!(did.lock().unwrap().is_empty());
    // Refused, it runs of `open` alone.
    assert!(
        plugins
            .consent(
                "acme/asker",
                &[Capability::ClipboardWrite],
                false,
                &mut app.context()
            )
            .is_empty()
    );
    served(&mut plugins, &mut app);
    assert_eq!(plugins.status("acme/asker"), Some(Status::Active));
    assert_eq!(*did.lock().unwrap(), ["activate opened=true"]);
}
