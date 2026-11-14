//! The plugin against the SDK's fake host, of the settings that the tests
//! give; its messages are their ids, as the fake host has no catalogs, a
//! section's title its name.

use lokyr_plugin_sdk::fake::{FakeHost, snapshot};
use lokyr_plugin_sdk::host::{PaneId, SettingKind};
use lokyr_plugin_sdk::message::Request;

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
/// family unset, and `[window]`, of a choice and a boolean at their
/// defaults, then the plugin `lokyr/tabs`'s, of an integer at its default.
fn settings() -> Settings {
    let size = SettingKind::Number {
        minimum: Some(4.0),
        maximum: Some(200.0),
        step: Some(1.0),
        integer: false,
    };
    let text = SettingKind::Text {
        min_length: Some(1),
        max_length: Some(256),
        examples: Vec::new(),
    };
    let width = SettingKind::Number {
        minimum: Some(100.0),
        maximum: Some(400.0),
        step: None,
        integer: true,
    };
    let scrollbar = SettingKind::Choice {
        choices: ["system", "always", "never"]
            .map(|choice| Data::Text(choice.to_owned()))
            .to_vec(),
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
                        size,
                    ),
                ],
            },
            SettingsSection {
                name: "window".to_owned(),
                plugin: None,
                description: String::new(),
                keys: vec![
                    key(
                        "scrollbar",
                        Some(Data::Text("system".to_owned())),
                        Some(Data::Text("system".to_owned())),
                        scrollbar,
                    ),
                    key(
                        "opaque-titlebar",
                        Some(Data::Boolean(false)),
                        Some(Data::Boolean(false)),
                        SettingKind::Boolean,
                    ),
                ],
            },
            SettingsSection {
                name: "tabs".to_owned(),
                plugin: Some("lokyr/tabs".to_owned()),
                description: "The tab bar".to_owned(),
                keys: vec![key(
                    "column-width",
                    Some(Data::Integer(200)),
                    Some(Data::Integer(200)),
                    width,
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
fn find_node<'t>(tree: &'t Node, key: &str) -> Option<&'t Node> {
    if tree.key.as_ref().is_some_and(|own| own.0 == key) {
        return Some(tree);
    }
    tree.children.iter().find_map(|child| find_node(child, key))
}

/// The text of the node of `key` in `tree`.
fn text_of(tree: &Node, key: &str) -> Option<String> {
    match &find_node(tree, key)?.kind {
        Kind::Text(text) => Some(text.clone()),
        _ => None,
    }
}

/// The text that the field of `key` in `tree` holds, and its placeholder.
fn held(tree: &Node, key: &str) -> Option<(Option<String>, String)> {
    match &find_node(tree, key)?.kind {
        Kind::Field(field) => Some((field.text.clone(), field.placeholder.clone())),
        _ => None,
    }
}

/// The label of the button or the toggle of `key` in `tree`, and whether
/// a toggle is on.
fn labelled(tree: &Node, key: &str) -> Option<(String, bool)> {
    match &find_node(tree, key)?.kind {
        Kind::Button(button) => Some((button.label.clone(), false)),
        Kind::Toggle(toggle) => Some((toggle.label.clone(), toggle.on)),
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

/// The event `kind` of the node of `key` of the page.
fn on(key: &str, kind: UiEventKind) -> UiEvent {
    UiEvent {
        view: page(),
        key: key.into(),
        kind,
    }
}

/// Return in the field of `key`, of `text`.
fn submit(key: &str, text: &str) -> UiEvent {
    on(
        key,
        UiEventKind::Submit {
            text: text.to_owned(),
            shift: false,
        },
    )
}

/// A key that a call wrote: its section, its plugin, its name and its
/// value.
type Write = (String, Option<String>, String, Option<Data>);

/// The keys that the calls wrote since the last look.
fn written(host: &mut FakeHost) -> Vec<Write> {
    host.take_done()
        .into_iter()
        .filter_map(|request| match request {
            Request::SetSetting {
                section,
                plugin,
                key,
                value,
            } => Some((section, plugin, key, value)),
            _ => None,
        })
        .collect()
}

/// A write of the app's key `key` of `section`, of `value`.
fn app_key(section: &str, key: &str, value: Option<Data>) -> Write {
    (section.to_owned(), None, key.to_owned(), value)
}

#[test]
fn the_sidebar_lists_the_config_s_sections_then_the_plugins_of_a_schema() {
    let (mut host, mut plugin) = host();
    let tree = rendered(&mut host, &mut plugin);
    let list = find_node(&tree, SECTIONS).expect("the sidebar's list");
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
    let sidebar = find_node(&tree, "sidebar").unwrap();
    assert_eq!(sidebar.layout.min_width, sidebar.layout.width);
}

#[test]
fn a_section_shows_each_key_s_control_of_its_value_in_force_marked_at_its_default() {
    let (mut host, mut plugin) = host();
    let tree = rendered(&mut host, &mut plugin);
    // Titled of its name, as the fake host has no catalogs.
    assert_eq!(text_of(&tree, "title").as_deref(), Some("font"));
    assert_eq!(
        text_of(&tree, "description").as_deref(),
        Some("The font of the text")
    );
    // A number set, not its default, in its field between its steps, a
    // button back to its default; a text unset, of an empty field.
    assert_eq!(
        held(&tree, "field:size"),
        Some((Some("15".to_owned()), "setting-unset".to_owned()))
    );
    assert!(labelled(&tree, "less:size").is_some());
    assert!(labelled(&tree, "more:size").is_some());
    assert!(find_node(&tree, "default:size").is_none());
    assert!(labelled(&tree, "reset:size").is_some());
    assert_eq!(
        held(&tree, "field:family"),
        Some((Some(String::new()), "setting-unset".to_owned()))
    );
    assert!(labelled(&tree, "reset:family").is_none());
    assert_eq!(
        text_of(&tree, "description:size").as_deref(),
        Some("What size does")
    );
    // Each key a group of its name, for VoiceOver.
    let row = find_node(&tree, "key:size").unwrap();
    assert_eq!(row.role, Some(Role::Group));
    assert_eq!(row.label.as_deref(), Some("size"));
    // The next section: a choice's menu button of its value, a boolean's
    // toggle, both at their defaults.
    host.ui_event(&mut plugin, &choose(1)).unwrap();
    let tree = rendered(&mut host, &mut plugin);
    assert_eq!(
        labelled(&tree, "choose:scrollbar"),
        Some(("system".to_owned(), false))
    );
    assert_eq!(
        text_of(&tree, "default:scrollbar").as_deref(),
        Some("setting-default")
    );
    assert_eq!(
        labelled(&tree, "set:opaque-titlebar"),
        Some(("setting-off".to_owned(), false))
    );
    // A plugin's section, of its plugin's name.
    host.ui_event(&mut plugin, &choose(2)).unwrap();
    let tree = rendered(&mut host, &mut plugin);
    assert_eq!(text_of(&tree, "plugin").as_deref(), Some("settings-plugin"));
    assert_eq!(
        held(&tree, "field:column-width").map(|(text, _)| text),
        Some(Some("200".to_owned()))
    );
    assert!(find_node(&tree, "less:column-width").is_none());
}

#[test]
fn a_number_and_a_text_are_written_from_their_fields_their_steps_and_back_to_their_defaults() {
    let (mut host, mut plugin) = host();
    let _ = written(&mut host);
    // Return in the number's field writes it, a whole one an integer.
    host.ui_event(&mut plugin, &submit("field:size", "18"))
        .unwrap();
    assert_eq!(
        written(&mut host),
        [app_key("font", "size", Some(Data::Integer(18)))]
    );
    host.deliver(&mut plugin).unwrap();
    let tree = rendered(&mut host, &mut plugin);
    assert_eq!(
        held(&tree, "field:size").map(|(text, _)| text),
        Some(Some("18".to_owned()))
    );
    // A step more, then a fraction.
    host.ui_event(&mut plugin, &on("more:size", UiEventKind::Click))
        .unwrap();
    assert_eq!(
        written(&mut host),
        [app_key("font", "size", Some(Data::Integer(19)))]
    );
    host.ui_event(&mut plugin, &submit("field:size", " 13.5 "))
        .unwrap();
    assert_eq!(
        written(&mut host),
        [app_key("font", "size", Some(Data::Number(13.5)))]
    );
    // Back to its default, as its button takes the key away.
    host.ui_event(&mut plugin, &on("reset:size", UiEventKind::Click))
        .unwrap();
    assert_eq!(written(&mut host), [app_key("font", "size", None)]);
    host.deliver(&mut plugin).unwrap();
    let tree = rendered(&mut host, &mut plugin);
    assert_eq!(
        held(&tree, "field:size").map(|(text, _)| text),
        Some(Some("13".to_owned()))
    );
    assert!(find_node(&tree, "default:size").is_some());
    // A text, and an empty field back to the key's default.
    host.ui_event(&mut plugin, &submit("field:family", "Menlo"))
        .unwrap();
    host.ui_event(&mut plugin, &submit("field:family", ""))
        .unwrap();
    assert_eq!(
        written(&mut host),
        [
            app_key("font", "family", Some(Data::Text("Menlo".to_owned()))),
            app_key("font", "family", None),
        ]
    );
}

#[test]
fn a_value_that_the_key_does_not_take_tells_why_under_it_until_it_is_written() {
    let (mut host, mut plugin) = host();
    let _ = written(&mut host);
    // No number, of the page's own check.
    host.ui_event(&mut plugin, &submit("field:size", "big"))
        .unwrap();
    assert!(written(&mut host).is_empty());
    let tree = rendered(&mut host, &mut plugin);
    assert_eq!(
        text_of(&tree, "refused:size").as_deref(),
        Some("setting-not-a-number")
    );
    // Past its range, of the host's.
    host.ui_event(&mut plugin, &submit("field:size", "500"))
        .unwrap();
    let tree = rendered(&mut host, &mut plugin);
    let why = text_of(&tree, "refused:size").unwrap();
    assert!(why.contains("size"), "{why}");
    // Written, the reason goes.
    host.ui_event(&mut plugin, &submit("field:size", "20"))
        .unwrap();
    let tree = rendered(&mut host, &mut plugin);
    assert!(find_node(&tree, "refused:size").is_none());
    // A plugin's integer, of no fraction.
    host.ui_event(&mut plugin, &choose(2)).unwrap();
    host.ui_event(&mut plugin, &submit("field:column-width", "250.5"))
        .unwrap();
    let tree = rendered(&mut host, &mut plugin);
    assert_eq!(
        text_of(&tree, "refused:column-width").as_deref(),
        Some("setting-not-whole")
    );
    let _ = written(&mut host);
    host.ui_event(&mut plugin, &submit("field:column-width", "250"))
        .unwrap();
    assert_eq!(
        written(&mut host),
        [(
            "tabs".to_owned(),
            Some("lokyr/tabs".to_owned()),
            "column-width".to_owned(),
            Some(Data::Integer(250))
        )]
    );
}

#[test]
fn a_boolean_is_written_of_its_toggle_and_a_choice_of_its_menu() {
    let (mut host, mut plugin) = host();
    host.ui_event(&mut plugin, &choose(1)).unwrap();
    let _ = written(&mut host);
    host.ui_event(
        &mut plugin,
        &on("set:opaque-titlebar", UiEventKind::Toggle { on: true }),
    )
    .unwrap();
    assert_eq!(
        written(&mut host),
        [app_key(
            "window",
            "opaque-titlebar",
            Some(Data::Boolean(true))
        )]
    );
    host.deliver(&mut plugin).unwrap();
    let tree = rendered(&mut host, &mut plugin);
    assert_eq!(
        labelled(&tree, "set:opaque-titlebar"),
        Some(("setting-on".to_owned(), true))
    );
    // The choice's button opens its menu, of its values, the one in force
    // ticked, under it.
    host.ui_event(&mut plugin, &on("choose:scrollbar", UiEventKind::Click))
        .unwrap();
    let panel = PanelId("choices:scrollbar".to_owned());
    let options = host.panels().get(&panel).cloned().expect("the menu");
    assert_eq!(options.role, PanelRole::Menu);
    assert!(matches!(
        &options.anchor,
        Anchor::Node { key, .. } if key.0 == "choose:scrollbar"
    ));
    let menu = host
        .render(&mut plugin, &ViewId::Panel(panel.clone()))
        .expect("the menu's tree");
    let items: Vec<(String, bool)> = menu
        .children
        .iter()
        .map(|item| match &item.kind {
            Kind::Toggle(toggle) => (toggle.label.clone(), toggle.on),
            other => panic!("{other:?}"),
        })
        .collect();
    assert_eq!(
        items,
        [
            ("system".to_owned(), true),
            ("always".to_owned(), false),
            ("never".to_owned(), false)
        ]
    );
    host.ui_event(
        &mut plugin,
        &UiEvent {
            view: ViewId::Panel(panel),
            key: "choice:2".into(),
            kind: UiEventKind::Toggle { on: true },
        },
    )
    .unwrap();
    assert_eq!(
        written(&mut host),
        [app_key(
            "window",
            "scrollbar",
            Some(Data::Text("never".to_owned()))
        )]
    );
}

#[test]
fn the_page_follows_the_settings_as_they_change_and_shows_at_its_first_section() {
    let (mut host, mut plugin) = host();
    host.ui_event(&mut plugin, &choose(1)).unwrap();
    let _ = host.take_renders();
    // New settings, as the config applies anew, of a change of the file:
    // the page renders again, its fields holding the new values.
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
    assert_eq!(
        held(&tree, "field:size").map(|(text, _)| text),
        Some(Some("18".to_owned()))
    );
}

#[test]
fn the_page_renders_while_it_shows_alone() {
    let (mut host, mut plugin) = host();
    host.hide_page();
    host.deliver(&mut plugin).unwrap();
    let _ = host.take_renders();
    // The settings change while the settings window is closed: no render
    // asked, no failure.
    let mut changed = settings();
    changed.sections[0].keys[1].value = Some(Data::Integer(20));
    host.set_settings(changed);
    host.deliver(&mut plugin).unwrap();
    assert!(host.take_renders().is_empty());
}
