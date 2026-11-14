//! The plugin against the SDK's fake host, of the settings that the tests
//! give; its messages are their ids, as the fake host has no catalogs, a
//! section's title its name.

use lokyr_plugin_sdk::fake::{FakeHost, snapshot};
use lokyr_plugin_sdk::host::{PaneId, SettingKind};

use super::*;

/// The plugin's name, as its calls give it.
const NAME: &str = "lokyr/settings";

/// A key of `name`, of its value, its default and its kind.
fn key(name: &str, value: Option<Data>, default: Option<Data>, kind: SettingKind) -> SettingKey {
    SettingKey {
        name: name.to_owned(),
        description: format!("What {name} does"),
        platforms: vec!["macos".to_owned()],
        kind,
        control: None,
        default,
        value,
    }
}

/// The settings of the tests: the app's `[font]`, of a size set and a
/// family unset, and `[window]`, then the plugin `lokyr/tabs`'s.
fn settings() -> Settings {
    let number = SettingKind::Number {
        minimum: Some(4.0),
        maximum: Some(200.0),
        step: Some(1.0),
        integer: false,
    };
    let text = SettingKind::Text {
        max_length: Some(256),
        examples: Vec::new(),
    };
    Settings {
        sections: vec![
            SettingsSection {
                name: "font".to_owned(),
                plugin: None,
                description: "The font of the text".to_owned(),
                keys: vec![
                    key("family", None, None, text),
                    key(
                        "size",
                        Some(Data::Integer(15)),
                        Some(Data::Integer(13)),
                        number,
                    ),
                ],
            },
            SettingsSection {
                name: "window".to_owned(),
                plugin: None,
                description: String::new(),
                keys: vec![key(
                    "scrollbar",
                    Some(Data::Text("system".to_owned())),
                    Some(Data::Text("system".to_owned())),
                    SettingKind::Choice {
                        choices: vec![Data::Text("system".to_owned())],
                    },
                )],
            },
            SettingsSection {
                name: "tabs".to_owned(),
                plugin: Some("lokyr/tabs".to_owned()),
                description: "The tab bar".to_owned(),
                keys: vec![key(
                    "column-width",
                    Some(Data::Number(200.0)),
                    Some(Data::Number(200.0)),
                    SettingKind::Other,
                )],
            },
        ],
    }
}

/// A host of one pane, of the settings of [`settings`], and the plugin,
/// activated, its page shown.
fn host() -> (FakeHost, SettingsPage) {
    let mut host = FakeHost::new(NAME).with_manifest(crate::MANIFEST);
    host.pane(PaneId(1));
    host.set_settings(settings());
    let mut plugin = SettingsPage::default();
    host.activate(&mut plugin).unwrap();
    host.show_page(PAGE);
    host.deliver(&mut plugin).unwrap();
    (host, plugin)
}

/// The page's tree, rendered anew.
fn rendered(host: &mut FakeHost, plugin: &mut SettingsPage) -> Node {
    host.render(plugin, &page()).expect("the page")
}

/// The node of `key` in `tree`, if it has one.
fn find<'t>(tree: &'t Node, key: &str) -> Option<&'t Node> {
    if tree.key.as_ref().is_some_and(|own| own.0 == key) {
        return Some(tree);
    }
    tree.children.iter().find_map(|child| find(child, key))
}

/// The text of the node of `key` in `tree`.
fn text_of(tree: &Node, key: &str) -> Option<String> {
    match &find(tree, key)?.kind {
        Kind::Text(text) => Some(text.clone()),
        _ => None,
    }
}

/// The sidebar's choice of the section `item`.
fn choose(item: usize) -> UiEvent {
    UiEvent {
        view: page(),
        key: SECTIONS.into(),
        kind: UiEventKind::Select { item },
    }
}

#[test]
fn the_sidebar_lists_the_config_s_sections_then_the_plugins_of_a_schema() {
    let (mut host, mut plugin) = host();
    let tree = rendered(&mut host, &mut plugin);
    let list = find(&tree, SECTIONS).expect("the sidebar's list");
    let Kind::List(list_kind) = &list.kind else {
        panic!("a list");
    };
    let items: Vec<Vec<String>> = list_kind
        .items
        .iter()
        .map(|item| item.texts.clone())
        .collect();
    assert_eq!(
        items,
        [
            ["settings", "font"],
            ["settings", "window"],
            ["extensions", "tabs"],
        ]
        .map(|texts| texts.map(str::to_owned).to_vec()),
        "{}",
        snapshot(&tree)
    );
    assert!(list.style.hides_ring);
    let sidebar = find(&tree, "sidebar").unwrap();
    assert_eq!(sidebar.layout.min_width, sidebar.layout.width);
}

#[test]
fn a_section_shows_its_keys_of_their_values_in_force_marked_at_their_defaults() {
    let (mut host, mut plugin) = host();
    let tree = rendered(&mut host, &mut plugin);
    // Titled of its name, as the fake host has no catalogs.
    assert_eq!(text_of(&tree, "title").as_deref(), Some("font"));
    assert_eq!(
        text_of(&tree, "description").as_deref(),
        Some("The font of the text")
    );
    // A value set, not its default; a key unset of no default.
    assert_eq!(text_of(&tree, "value:size").as_deref(), Some("15"));
    assert!(find(&tree, "default:size").is_none());
    assert_eq!(
        text_of(&tree, "value:family").as_deref(),
        Some("setting-unset")
    );
    assert_eq!(
        text_of(&tree, "description:size").as_deref(),
        Some("What size does")
    );
    // Each key a group of its name, for VoiceOver.
    let row = find(&tree, "key:size").unwrap();
    assert_eq!(row.role, Some(Role::Group));
    assert_eq!(row.label.as_deref(), Some("size"));
    // The next section, of its default value marked.
    host.ui_event(&mut plugin, &choose(1)).unwrap();
    let tree = rendered(&mut host, &mut plugin);
    assert_eq!(
        text_of(&tree, "value:scrollbar").as_deref(),
        Some("\"system\"")
    );
    assert_eq!(
        text_of(&tree, "default:scrollbar").as_deref(),
        Some("setting-default")
    );
    // A plugin's section, of its plugin's name.
    host.ui_event(&mut plugin, &choose(2)).unwrap();
    let tree = rendered(&mut host, &mut plugin);
    assert_eq!(text_of(&tree, "plugin").as_deref(), Some("settings-plugin"));
    assert_eq!(
        text_of(&tree, "value:column-width").as_deref(),
        Some("200.0")
    );
}

#[test]
fn the_page_follows_the_settings_as_they_change_and_shows_at_its_first_section() {
    let (mut host, mut plugin) = host();
    host.ui_event(&mut plugin, &choose(1)).unwrap();
    let _ = host.take_renders();
    // New settings, as the config applies anew: the page renders again.
    let mut changed = settings();
    changed.sections[0].keys[1].value = Some(Data::Integer(18));
    host.set_settings(changed);
    host.deliver(&mut plugin).unwrap();
    assert!(host.take_renders().contains(&page()));
    // Shown again, at its first section.
    host.hide_page();
    host.show_page(PAGE);
    host.deliver(&mut plugin).unwrap();
    let tree = rendered(&mut host, &mut plugin);
    assert_eq!(text_of(&tree, "value:size").as_deref(), Some("18"));
}
