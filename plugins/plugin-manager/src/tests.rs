//! The plugin against the SDK's fake host, of the plugins that the tests
//! give; its messages are their ids, as the fake host has no catalogs.

use lokyr_plugin_sdk::fake::{FakeHost, snapshot};
use lokyr_plugin_sdk::host::{CapabilityDecision, Decision, PaneId, PluginInfo};
use lokyr_plugin_sdk::message::Request;
use lokyr_plugin_sdk::view::{Kind, Role};

use super::*;

/// The plugin's name, as its calls give it.
const NAME: &str = "lokyr/plugin-manager";

/// A plugin of the name `name`, built in or not, active.
fn plugin(name: &str, built_in: bool) -> PluginInfo {
    PluginInfo {
        name: name.to_owned(),
        namespace: name.rsplit('/').next().unwrap().to_owned(),
        version: (!built_in).then(|| "1.2.0".to_owned()),
        description: Some(format!("What {name} does")),
        built_in,
        state: PluginState::Active,
        fault: None,
        capabilities: Vec::new(),
    }
}

/// The plugins of the tests: an installed one of a capability granted and
/// one undecided, a built-in one, and the plugin manager itself.
fn plugins() -> Vec<PluginInfo> {
    vec![
        PluginInfo {
            capabilities: vec![
                CapabilityDecision {
                    capability: "clipboard-write".to_owned(),
                    decision: Decision::Granted,
                },
                CapabilityDecision {
                    capability: "title-text".to_owned(),
                    decision: Decision::Undecided,
                },
            ],
            ..plugin("acme/notes", false)
        },
        plugin("lokyr/tabs", true),
        plugin(NAME, true),
    ]
}

/// A host of one pane, of the plugins of [`plugins`], and the plugin,
/// activated, its page shown.
fn host() -> (FakeHost, Manager) {
    let mut host = FakeHost::new(NAME).with_manifest(crate::MANIFEST);
    host.pane(PaneId(1));
    host.set_plugins(plugins());
    let mut manager = Manager::default();
    host.activate(&mut manager).unwrap();
    host.show_page(PAGE);
    host.deliver(&mut manager).unwrap();
    (host, manager)
}

/// The page's tree, rendered anew.
fn rendered(host: &mut FakeHost, manager: &mut Manager) -> Node {
    host.render(manager, &page()).expect("the page")
}

/// The node of `key` in `tree`, if it has one.
fn find<'t>(tree: &'t Node, key: &str) -> Option<&'t Node> {
    if tree.key.as_ref().is_some_and(|own| own.0 == key) {
        return Some(tree);
    }
    tree.children.iter().find_map(|child| find(child, key))
}

/// The texts of `tree`, in its order.
fn texts(tree: &Node) -> Vec<String> {
    let mut found = Vec::new();
    if let Kind::Text(text) = &tree.kind {
        found.push(text.clone());
    }
    for child in &tree.children {
        found.extend(texts(child));
    }
    found
}

/// The label of the button of `key` in `tree`.
fn button(tree: &Node, key: &str) -> Option<String> {
    match &find(tree, key)?.kind {
        Kind::Button(button) => Some(button.label.clone()),
        _ => None,
    }
}

/// The jobs of the packages' service that run, of the calls that `of`
/// takes.
fn jobs_of(host: &FakeHost, of: impl Fn(&Packages) -> bool) -> Vec<(u64, Packages)> {
    host.packages_jobs()
        .iter()
        .filter(|(_, call)| of(call))
        .map(|(id, call)| (*id, call.clone()))
        .collect()
}

/// A line of Sleip's of the event `event` and the fields `fields`.
fn line(event: &str, fields: &[(&str, Data)]) -> Data {
    let mut map: BTreeMap<String, Data> = fields
        .iter()
        .map(|(name, value)| ((*name).to_owned(), value.clone()))
        .collect();
    map.insert("event".to_owned(), Data::Text(event.to_owned()));
    Data::Map(map)
}

/// A text of Sleip's lines.
fn text(value: &str) -> Data {
    Data::Text(value.to_owned())
}

/// The `found` line of the plugin `name` at `version`, which this Lokyr
/// takes or not, of the capabilities `capabilities`.
fn found_line(name: &str, version: &str, taken: bool, capabilities: &[&str]) -> Data {
    line(
        "found",
        &[
            ("name", text(name)),
            ("version", text(version)),
            ("description", text(&format!("What {name} does"))),
            (
                "capabilities",
                Data::List(
                    capabilities
                        .iter()
                        .map(|capability| text(capability))
                        .collect(),
                ),
            ),
            ("yanked", Data::Boolean(false)),
            ("api", text("^0.1")),
            ("taken", Data::Boolean(taken)),
        ],
    )
}

/// The host of [`host`], its registry read, of `acme/notes` 1.3.0, newer
/// than the one installed, `acme/sync` 0.1.0, not installed, and
/// `acme/later` 2.0.0, which this Lokyr does not take, and of a note.
fn read() -> (FakeHost, Manager) {
    let (mut host, mut manager) = host();
    let jobs = jobs_of(&host, |call| matches!(call, Packages::Search { .. }));
    let [(job, Packages::Search { query })] = &jobs[..] else {
        panic!("{jobs:?}");
    };
    assert_eq!(query, "", "every plugin of the registry");
    let lines = vec![
        found_line("acme/notes", "1.3.0", true, &["clipboard-write"]),
        found_line("acme/sync", "0.1.0", true, &[]),
        found_line("acme/later", "2.0.0", false, &["process-read"]),
        line("note", &[("message", text("the index is a day old"))]),
        line("done", &[("found", Data::Integer(3))]),
    ];
    host.answer_packages(*job, lines, true);
    host.deliver(&mut manager).unwrap();
    (host, manager)
}

/// The event of the sidebar's choice of the part `item`.
fn choose(item: usize) -> UiEvent {
    UiEvent {
        view: page(),
        key: SECTIONS.into(),
        kind: UiEventKind::Select { item },
    }
}

/// A click on the node of `key` of the page.
fn click(key: &str) -> UiEvent {
    UiEvent {
        view: page(),
        key: key.into(),
        kind: UiEventKind::Click,
    }
}

#[test]
fn the_page_lists_the_installed_plugins_then_the_built_in_ones_each_of_its_buttons() {
    let (mut host, mut manager) = host();
    let tree = rendered(&mut host, &mut manager);
    // Installed first, of its version, what it does, its state and its
    // capabilities' decisions; then the built-in ones.
    let installed = find(&tree, "installed-plugins").expect("the installed group");
    let shown = texts(installed);
    assert_eq!(
        shown,
        [
            "acme/notes",
            "1.2.0",
            "What acme/notes does",
            "plugin-state-active",
            "plugin-capabilities",
        ],
        "{}",
        snapshot(&tree)
    );
    let built_in = texts(find(&tree, "built-in-plugins").expect("the built-in group"));
    assert!(built_in.contains(&"lokyr/tabs".to_owned()));
    assert!(built_in.contains(&NAME.to_owned()));
    // An installed one turns off and is removed; a built-in one turns off
    // alone; the plugin manager neither.
    assert_eq!(
        button(&tree, "toggle:acme/notes").as_deref(),
        Some("plugin-disable")
    );
    assert_eq!(
        button(&tree, "remove:acme/notes").as_deref(),
        Some("plugin-remove")
    );
    assert!(button(&tree, "toggle:lokyr/tabs").is_some());
    assert!(button(&tree, "remove:lokyr/tabs").is_none());
    assert!(button(&tree, &format!("toggle:{NAME}")).is_none());
    // The sidebar's list shows no ring of the keyboard's focus, of a
    // sidebar's own width.
    let sections = find(&tree, SECTIONS).expect("the sidebar's list");
    assert!(sections.style.hides_ring);
    let sidebar = find(&tree, "sidebar").unwrap();
    assert_eq!(sidebar.layout.min_width, sidebar.layout.width);
    // Each plugin a group of its name, for VoiceOver.
    let row = find(&tree, "plugin:acme/notes").unwrap();
    assert_eq!(row.role, Some(Role::Group));
    assert_eq!(row.label.as_deref(), Some("acme/notes"));
    // Of no installed plugin, a text that tells how to install one.
    host.set_plugins(vec![plugin("lokyr/tabs", true)]);
    let tree = rendered(&mut host, &mut manager);
    assert!(find(&tree, "installed-plugins").is_none());
    assert!(find(&tree, "none-installed").is_some());
}

#[test]
fn a_plugin_turned_off_and_on_by_its_button_shows_as_the_plugins_change() {
    let (mut host, mut manager) = host();
    let _ = host.take_renders();
    host.ui_event(&mut manager, &click("toggle:acme/notes"))
        .unwrap();
    let done = host.take_done();
    assert!(
        done.contains(&Request::SetPluginEnabled {
            plugin: "acme/notes".to_owned(),
            enabled: false,
        }),
        "{done:?}"
    );
    // The plugins changed: the page renders again, its button turning the
    // plugin on.
    host.deliver(&mut manager).unwrap();
    assert!(host.take_renders().contains(&page()));
    let tree = rendered(&mut host, &mut manager);
    assert_eq!(
        button(&tree, "toggle:acme/notes").as_deref(),
        Some("plugin-enable")
    );
    assert!(texts(&tree).contains(&"plugin-state-disabled".to_owned()));
    host.ui_event(&mut manager, &click("toggle:acme/notes"))
        .unwrap();
    assert!(host.take_done().contains(&Request::SetPluginEnabled {
        plugin: "acme/notes".to_owned(),
        enabled: true,
    }));
}

#[test]
fn an_installed_plugin_is_removed_by_its_button_through_the_packages_service() {
    let (mut host, mut manager) = host();
    host.ui_event(&mut manager, &click("remove:acme/notes"))
        .unwrap();
    let jobs = jobs_of(&host, |call| matches!(call, Packages::Remove { .. }));
    let [(job, Packages::Remove { plugin: removed })] = &jobs[..] else {
        panic!("{jobs:?}");
    };
    assert_eq!(removed, "acme/notes");
    // Its removal shows under way, of no buttons; a second click starts
    // none.
    let tree = rendered(&mut host, &mut manager);
    assert!(texts(&tree).contains(&"plugin-removing".to_owned()));
    assert!(button(&tree, "remove:acme/notes").is_none());
    // A removal that fails tells Sleip's error.
    let error = Data::Map(BTreeMap::from([
        ("event".to_owned(), Data::Text("error".to_owned())),
        (
            "message".to_owned(),
            Data::Text("`acme/sync` needs it".to_owned()),
        ),
    ]));
    host.answer_packages(*job, vec![error], false);
    host.deliver(&mut manager).unwrap();
    let tree = rendered(&mut host, &mut manager);
    assert!(texts(&tree).contains(&"plugin-change-failed".to_owned()));
    assert!(button(&tree, "remove:acme/notes").is_some());
    // Removed again, it goes as the plugins change, its failure gone.
    host.ui_event(&mut manager, &click("remove:acme/notes"))
        .unwrap();
    let job = jobs_of(&host, |call| matches!(call, Packages::Remove { .. }))[0].0;
    host.answer_packages(job, Vec::new(), true);
    host.set_plugins(vec![plugin("lokyr/tabs", true), plugin(NAME, true)]);
    host.deliver(&mut manager).unwrap();
    let tree = rendered(&mut host, &mut manager);
    assert!(find(&tree, "plugin:acme/notes").is_none());
    assert!(!texts(&tree).contains(&"plugin-change-failed".to_owned()));
}

#[test]
fn its_command_opens_the_settings_window_at_its_page() {
    let mut host = FakeHost::new(NAME).with_manifest(crate::MANIFEST);
    let mut manager = Manager::default();
    host.activate(&mut manager).unwrap();
    assert!(host.commands().iter().any(|command| command.name == OPEN));
    host.command(&mut manager, OPEN, Args::new()).unwrap();
    assert_eq!(host.page(), Some(PAGE));
}

#[test]
fn a_plugin_is_active_in_green_whether_it_runs_or_waits_or_disabled_and_a_fault_tells_why() {
    let (mut host, mut manager) = host();
    host.set_plugins(vec![
        PluginInfo {
            state: PluginState::Waiting,
            ..plugin("acme/notes", false)
        },
        PluginInfo {
            state: PluginState::Faulted,
            fault: Some("it panicked".to_owned()),
            ..plugin("acme/sync", false)
        },
        PluginInfo {
            state: PluginState::Off,
            ..plugin("lokyr/tabs", true)
        },
    ]);
    let tree = rendered(&mut host, &mut manager);
    let state = |name: &str| {
        let node = find(&tree, &format!("state:{name}")).unwrap();
        let Kind::Text(text) = &node.kind else {
            panic!("a text");
        };
        (text.clone(), node.style.color.clone().unwrap().0)
    };
    assert_eq!(
        state("acme/notes"),
        ("plugin-state-active".to_owned(), "text.success".to_owned())
    );
    assert_eq!(
        state("acme/sync"),
        (
            "plugin-state-disabled".to_owned(),
            "text.warning".to_owned()
        )
    );
    assert_eq!(state("lokyr/tabs").0, "plugin-state-disabled");
    let sync = texts(find(&tree, "plugin:acme/sync").unwrap());
    assert!(sync.contains(&"plugin-fault".to_owned()), "{sync:?}");
    assert!(!texts(find(&tree, "plugin:lokyr/tabs").unwrap()).contains(&"plugin-fault".to_owned()));
    // Its button enables a disabled plugin.
    assert_eq!(
        button(&tree, "toggle:lokyr/tabs").as_deref(),
        Some("plugin-enable")
    );
}

#[test]
fn the_registry_is_read_as_the_page_shows_and_a_newer_version_updates_an_installed_plugin() {
    let (mut host, mut manager) = read();
    // Read once while the plugin runs, whatever the page's showings.
    host.show_page(PAGE);
    host.deliver(&mut manager).unwrap();
    assert!(jobs_of(&host, |call| matches!(call, Packages::Search { .. })).is_empty());
    // The installed plugin of a newer version that this Lokyr takes.
    let tree = rendered(&mut host, &mut manager);
    assert_eq!(
        button(&tree, "update:acme/notes").as_deref(),
        Some("plugin-update")
    );
    assert!(button(&tree, "update:lokyr/tabs").is_none());
    host.ui_event(&mut manager, &click("update:acme/notes"))
        .unwrap();
    let jobs = jobs_of(&host, |call| matches!(call, Packages::Add { .. }));
    let [(job, Packages::Add { plugin, constraint })] = &jobs[..] else {
        panic!("{jobs:?}");
    };
    assert_eq!((plugin.as_str(), constraint), ("acme/notes", &None));
    let tree = rendered(&mut host, &mut manager);
    assert!(texts(&tree).contains(&"plugin-updating".to_owned()));
    assert!(button(&tree, &format!("cancel:{job}")).is_some());
    // An update that fails tells Sleip's error.
    let error = line("error", &[("message", text("no network"))]);
    host.answer_packages(*job, vec![error], false);
    host.deliver(&mut manager).unwrap();
    let tree = rendered(&mut host, &mut manager);
    assert!(texts(&tree).contains(&"plugin-change-failed".to_owned()));
}

#[test]
fn the_registry_s_plugins_are_filtered_installed_and_cancelled_from_the_sidebar_s_part() {
    let (mut host, mut manager) = read();
    host.ui_event(&mut manager, &choose(1)).unwrap();
    let tree = rendered(&mut host, &mut manager);
    // Each plugin of what it asks for: installed, of a newer version, or
    // to install; one that this Lokyr does not take, told so.
    assert!(
        find(&tree, "found:acme/notes").is_some(),
        "{}",
        snapshot(&tree)
    );
    assert_eq!(
        button(&tree, "update:acme/notes").as_deref(),
        Some("plugin-update")
    );
    assert_eq!(
        button(&tree, "install:acme/sync").as_deref(),
        Some("plugin-install")
    );
    assert!(
        texts(find(&tree, "found:acme/later").unwrap()).contains(&"version-not-taken".to_owned())
    );
    assert!(texts(&tree).contains(&"the index is a day old".to_owned()));
    // The field filters them as it is typed in, of their names and what
    // they do.
    host.ui_event(
        &mut manager,
        &UiEvent {
            view: page(),
            key: QUERY.into(),
            kind: UiEventKind::InputChanged {
                text: "SYNC".to_owned(),
                revision: 1,
            },
        },
    )
    .unwrap();
    let tree = rendered(&mut host, &mut manager);
    assert!(find(&tree, "found:acme/sync").is_some());
    assert!(find(&tree, "found:acme/notes").is_none());
    // Installed, then cancelled: no failure told.
    host.ui_event(&mut manager, &click("install:acme/sync"))
        .unwrap();
    let job = jobs_of(&host, |call| matches!(call, Packages::Add { .. }))[0].0;
    let tree = rendered(&mut host, &mut manager);
    assert!(texts(&tree).contains(&"plugin-installing".to_owned()));
    host.ui_event(&mut manager, &click(&format!("cancel:{job}")))
        .unwrap();
    host.deliver(&mut manager).unwrap();
    let tree = rendered(&mut host, &mut manager);
    assert!(!texts(&tree).contains(&"plugin-change-failed".to_owned()));
    assert!(button(&tree, "install:acme/sync").is_some());
    // Installed again: it shows installed as the plugins change.
    host.ui_event(&mut manager, &click("install:acme/sync"))
        .unwrap();
    let job = jobs_of(&host, |call| matches!(call, Packages::Add { .. }))[0].0;
    host.answer_packages(job, Vec::new(), true);
    let mut installed = plugins();
    installed.push(PluginInfo {
        version: Some("0.1.0".to_owned()),
        ..plugin("acme/sync", false)
    });
    host.set_plugins(installed);
    host.deliver(&mut manager).unwrap();
    let tree = rendered(&mut host, &mut manager);
    assert!(
        find(&tree, "installed:acme/sync").is_some(),
        "{}",
        snapshot(&tree)
    );
    // The page shown again shows its installed part.
    host.hide_page();
    host.show_page(PAGE);
    host.deliver(&mut manager).unwrap();
    let tree = rendered(&mut host, &mut manager);
    assert!(find(&tree, "installed-plugins").is_some());
}

#[test]
fn a_plugin_s_details_tell_its_versions_and_what_each_asks_for() {
    let (mut host, mut manager) = read();
    host.ui_event(&mut manager, &choose(1)).unwrap();
    host.ui_event(&mut manager, &click("details:acme/sync"))
        .unwrap();
    let jobs = jobs_of(&host, |call| matches!(call, Packages::Info { .. }));
    let [(job, Packages::Info { plugin })] = &jobs[..] else {
        panic!("{jobs:?}");
    };
    assert_eq!(plugin, "acme/sync");
    let tree = rendered(&mut host, &mut manager);
    assert!(texts(&tree).contains(&"plugin-reading".to_owned()));
    let version = |number: &str, yanked: bool, capabilities: &[&str]| {
        Data::Map(BTreeMap::from([
            ("version".to_owned(), text(number)),
            ("yanked".to_owned(), Data::Boolean(yanked)),
            (
                "capabilities".to_owned(),
                Data::List(
                    capabilities
                        .iter()
                        .map(|capability| text(capability))
                        .collect(),
                ),
            ),
            ("api".to_owned(), text("^0.1")),
            ("taken".to_owned(), Data::Boolean(true)),
        ]))
    };
    let info = line(
        "info",
        &[
            ("name", text("acme/sync")),
            ("description", text("Syncs the notes")),
            ("uses", Data::List(vec![text("tabs.*")])),
            (
                "versions",
                Data::List(vec![
                    version("0.2.0", true, &["net:lokyr.dev"]),
                    version("0.1.0", false, &["clipboard-write"]),
                ]),
            ),
        ],
    );
    host.answer_packages(*job, vec![info], true);
    host.deliver(&mut manager).unwrap();
    let tree = rendered(&mut host, &mut manager);
    assert!(
        find(&tree, "version:0.2.0").is_some(),
        "{}",
        snapshot(&tree)
    );
    assert!(texts(find(&tree, "version:0.2.0").unwrap()).contains(&"version-yanked".to_owned()));
    assert!(texts(&tree).contains(&"Syncs the notes".to_owned()));
    assert!(texts(&tree).contains(&"plugin-uses".to_owned()));
    // Its install, of the newest version that is not yanked.
    assert_eq!(
        button(&tree, "install:acme/sync").as_deref(),
        Some("plugin-install")
    );
    // Back to the registry's plugins.
    host.ui_event(&mut manager, &click(BACK)).unwrap();
    let tree = rendered(&mut host, &mut manager);
    assert!(find(&tree, "found-plugins").is_some());
}

#[test]
fn a_registry_that_cannot_be_read_tells_why_and_is_read_again() {
    let (mut host, mut manager) = host();
    let job = jobs_of(&host, |call| matches!(call, Packages::Search { .. }))[0].0;
    let tree = rendered(&mut host, &mut manager);
    // The installed part shows meanwhile, of no update.
    assert!(find(&tree, "installed-plugins").is_some());
    host.ui_event(&mut manager, &choose(1)).unwrap();
    let tree = rendered(&mut host, &mut manager);
    assert!(find(&tree, "reading").is_some());
    let error = line(
        "error",
        &[("message", text("pkg.lokyr.dev did not answer"))],
    );
    host.answer_packages(job, vec![error], false);
    host.deliver(&mut manager).unwrap();
    let tree = rendered(&mut host, &mut manager);
    assert!(
        find(&tree, "registry-failed").is_some(),
        "{}",
        snapshot(&tree)
    );
    assert!(find(&tree, "found-plugins").is_none());
    host.ui_event(&mut manager, &click(REFRESH)).unwrap();
    assert_eq!(
        jobs_of(&host, |call| matches!(call, Packages::Search { .. })).len(),
        1
    );
    let tree = rendered(&mut host, &mut manager);
    assert!(find(&tree, "registry-failed").is_none());
}
