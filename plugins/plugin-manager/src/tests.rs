//! The plugin against the SDK's fake host, of the plugins that the tests
//! give; its messages are their ids, as the fake host has no catalogs.

use lokyr_plugin_sdk::fake::{FakeHost, snapshot};
use lokyr_plugin_sdk::host::{CapabilityDecision, PaneId};
use lokyr_plugin_sdk::message::Request;
use lokyr_plugin_sdk::view::Kind;

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
        Some("plugin-turn-off")
    );
    assert_eq!(
        button(&tree, "remove:acme/notes").as_deref(),
        Some("plugin-remove")
    );
    assert!(button(&tree, "toggle:lokyr/tabs").is_some());
    assert!(button(&tree, "remove:lokyr/tabs").is_none());
    assert!(button(&tree, &format!("toggle:{NAME}")).is_none());
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
        Some("plugin-turn-on")
    );
    assert!(texts(&tree).contains(&"plugin-state-off".to_owned()));
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
    let jobs: Vec<(u64, Packages)> = host
        .packages_jobs()
        .iter()
        .map(|(id, job)| (*id, job.clone()))
        .collect();
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
    assert!(texts(&tree).contains(&"plugin-remove-failed".to_owned()));
    assert!(button(&tree, "remove:acme/notes").is_some());
    // Removed again, it goes as the plugins change, its failure gone.
    host.ui_event(&mut manager, &click("remove:acme/notes"))
        .unwrap();
    let job = *host.packages_jobs().keys().next().unwrap();
    host.answer_packages(job, Vec::new(), true);
    host.set_plugins(vec![plugin("lokyr/tabs", true), plugin(NAME, true)]);
    host.deliver(&mut manager).unwrap();
    let tree = rendered(&mut host, &mut manager);
    assert!(find(&tree, "plugin:acme/notes").is_none());
    assert!(!texts(&tree).contains(&"plugin-remove-failed".to_owned()));
}

#[test]
fn its_command_opens_the_settings_tab_at_its_page() {
    let mut host = FakeHost::new(NAME).with_manifest(crate::MANIFEST);
    let mut manager = Manager::default();
    host.activate(&mut manager).unwrap();
    assert!(host.commands().iter().any(|command| command.name == OPEN));
    host.command(&mut manager, OPEN, Args::new()).unwrap();
    assert_eq!(host.page(), Some(PAGE));
}
