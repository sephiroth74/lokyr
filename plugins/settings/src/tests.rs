//! The plugin against the SDK's fake host, of the settings that the tests
//! give; its messages are their ids, as the fake host has no catalogs, a
//! section's title its name.

use lokyr_plugin_sdk::fake::{FakeHost, snapshot};
use lokyr_plugin_sdk::host::{PaneId, SettingKind};
use lokyr_plugin_sdk::message::Request;

use super::*;

/// The plugin's name, as its calls give it.
const NAME: &str = "lokyr/settings";

/// The sidebar's place of the theme's section, of the keys' bindings and of
/// the plugin's section, after the app's three sections.
const THEME: usize = 2;
const BINDINGS: usize = 3;
const TABS: usize = 4;

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
        guide: Some(format!("https://lokyr.dev/guide#{name}")),
    }
}

/// A key of a text of `name`, unset or of `value`, of the control
/// `control`.
fn controlled(name: &str, value: Option<&str>, control: &str) -> SettingKey {
    let text = SettingKind::Text {
        min_length: Some(1),
        max_length: Some(256),
        examples: Vec::new(),
    };
    SettingKey {
        control: Some(control.to_owned()),
        ..key(
            name,
            value.map(|value| Data::Text(value.to_owned())),
            None,
            text,
        )
    }
}

/// A binding of the settings' keys, of a command and its title.
fn binding(
    keys: &str,
    command: Option<&str>,
    title: Option<&str>,
) -> lokyr_plugin_sdk::host::SettingBinding {
    lokyr_plugin_sdk::host::SettingBinding {
        keys: keys.to_owned(),
        command: command.map(str::to_owned),
        title: title.map(str::to_owned),
        detailed: false,
    }
}

/// The settings of the tests: the app's `[font]`, of a size set and a
/// family unset, a style and a family of the text past ASCII of their
/// menus, `[window]`, of a choice and a boolean at their defaults, and
/// `[theme]`, of its two themes' menus, then the bindings of a command and
/// of keys taken away, and the plugin `lokyr/tabs`'s section, of an integer
/// at its default and a theme's color.
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
                    controlled("style", None, "font-style"),
                    controlled("non-ascii-family", None, "font-family"),
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
                    key(
                        "programs",
                        Some(programs(&["claude", "codex"])),
                        Some(programs(&["claude", "codex"])),
                        SettingKind::List {
                            max_length: Some(256),
                        },
                    ),
                ],
            },
            SettingsSection {
                name: "theme".to_owned(),
                plugin: None,
                description: "The themes".to_owned(),
                keys: vec![
                    controlled("light", Some("Lokyr Light"), "theme"),
                    controlled("dark", Some("Lokyr Dark"), "theme"),
                ],
            },
            SettingsSection {
                name: "tabs".to_owned(),
                plugin: Some("lokyr/tabs".to_owned()),
                description: "The tab bar".to_owned(),
                keys: vec![
                    key(
                        "column-width",
                        Some(Data::Integer(200)),
                        Some(Data::Integer(200)),
                        width,
                    ),
                    controlled("background", Some("panel.bg"), "token"),
                ],
            },
        ],
        keys: vec![
            binding("cmd+k", Some("pane.clear"), Some("Clear")),
            binding("cmd+q", None, None),
        ],
        error: None,
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
            ["text-size", "font"],
            ["window", "window"],
            ["symbol-color", "theme"],
            ["keyboard", "section-keys"],
            ["browser", "tabs"],
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
    // A number set, not its default, in its field of steps, of no
    // buttons, a link back to its default; a text unset, of an empty field,
    // its link dimmed.
    assert_eq!(
        held(&tree, "field:size"),
        Some((Some("15".to_owned()), "setting-unset".to_owned()))
    );
    assert!(matches!(
        &find_node(&tree, "field:size").unwrap().kind,
        Kind::Field(field) if field.stepped
    ));
    // Of the interface's small text, whatever the terminal's font's size.
    let style = &find_node(&tree, "field:size").unwrap().style;
    assert_eq!(
        (style.font_family, style.text_size),
        (None, Some(TextSize::Small))
    );
    assert!(find_node(&tree, "less:size").is_none() && find_node(&tree, "more:size").is_none());
    assert!(find_node(&tree, "default:size").is_none());
    assert_eq!(reset_of(&tree, "size"), Some(false));
    assert_eq!(
        held(&tree, "field:family"),
        Some((Some(String::new()), "setting-unset".to_owned()))
    );
    assert_eq!(reset_of(&tree, "family"), Some(true));
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
    // At its default, of no text that says so, its link dimmed.
    assert!(find_node(&tree, "default:scrollbar").is_none());
    assert_eq!(reset_of(&tree, "scrollbar"), Some(true));
    assert_eq!(
        labelled(&tree, "set:opaque-titlebar"),
        Some(("setting-off".to_owned(), false))
    );
    // A plugin's section, of its plugin's name.
    host.ui_event(&mut plugin, &choose(TABS)).unwrap();
    let tree = rendered(&mut host, &mut plugin);
    assert_eq!(text_of(&tree, "plugin").as_deref(), Some("settings-plugin"));
    assert_eq!(
        held(&tree, "field:column-width").map(|(text, _)| text),
        Some(Some("200".to_owned()))
    );
    assert!(find_node(&tree, "less:column-width").is_none());
}

/// Whether the link back to the default of the key `name` in `tree` is
/// dimmed, if it is a link there.
fn reset_of(tree: &Node, name: &str) -> Option<bool> {
    match &find_node(tree, &format!("reset:{name}"))?.kind {
        Kind::Button(button) if button.link => Some(button.disabled),
        _ => None,
    }
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
    // A step more, of the arrow up, then a fraction.
    host.ui_event(
        &mut plugin,
        &on("field:size", UiEventKind::Step { up: true }),
    )
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
    assert_eq!(reset_of(&tree, "size"), Some(true));
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
    host.ui_event(&mut plugin, &choose(TABS)).unwrap();
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

/// The items of the menu of the key `name` of the part shown, each its
/// label and whether it is ticked, as its button opens it.
fn menu_of(host: &mut FakeHost, plugin: &mut SettingsPage, name: &str) -> Vec<(String, bool)> {
    // The page as it shows, of the menu's button.
    rendered(host, plugin);
    host.ui_event(plugin, &on(&format!("choose:{name}"), UiEventKind::Click))
        .unwrap();
    let panel = PanelId(format!("choices:{name}"));
    assert!(host.panels().contains_key(&panel), "{name}'s menu");
    let menu = host
        .render(plugin, &ViewId::Panel(panel))
        .expect("the menu's tree");
    menu.children
        .iter()
        .map(|item| match &item.kind {
            Kind::Toggle(toggle) => (toggle.label.clone(), toggle.on),
            other => panic!("{other:?}"),
        })
        .collect()
}

/// The choice of the item `at` of the menu of the key `name`.
fn pick(host: &mut FakeHost, plugin: &mut SettingsPage, name: &str, at: usize) {
    host.ui_event(
        plugin,
        &UiEvent {
            view: ViewId::Panel(PanelId(format!("choices:{name}"))),
            key: format!("choice:{at}").as_str().into(),
            kind: UiEventKind::Toggle { on: true },
        },
    )
    .unwrap();
}

/// The labels of `items`.
fn labels(items: &[(String, bool)]) -> Vec<&str> {
    items.iter().map(|(label, _)| label.as_str()).collect()
}

#[test]
fn a_family_and_a_style_are_picked_from_the_system_s_fonts() {
    let (mut host, mut plugin) = host();
    let _ = written(&mut host);
    let tree = rendered(&mut host, &mut plugin);
    assert_eq!(
        labelled(&tree, "choose:non-ascii-family"),
        Some(("setting-unset".to_owned(), false))
    );
    // Every family, monospaced or not, of the text past ASCII.
    let families = menu_of(&mut host, &mut plugin, "non-ascii-family");
    assert_eq!(
        labels(&families),
        ["Helvetica", "Menlo", "Monaco", "SF Mono", "SF Pro"]
    );
    pick(&mut host, &mut plugin, "non-ascii-family", 4);
    assert_eq!(
        written(&mut host),
        [app_key(
            "font",
            "non-ascii-family",
            Some(Data::Text("SF Pro".to_owned()))
        )]
    );
    // The styles of the family in force, as the section names none.
    let styles = menu_of(&mut host, &mut plugin, "style");
    assert_eq!(
        labels(&styles),
        ["Regular", "Italic", "Bold", "Bold Italic"]
    );
    pick(&mut host, &mut plugin, "style", 2);
    assert_eq!(
        written(&mut host),
        [app_key(
            "font",
            "style",
            Some(Data::Text("Bold".to_owned()))
        )]
    );
}

#[test]
fn a_theme_is_picked_of_its_appearance_and_a_color_of_the_theme_s() {
    let (mut host, mut plugin) = host();
    let _ = written(&mut host);
    host.ui_event(&mut plugin, &choose(THEME)).unwrap();
    // The light themes alone, the one in force ticked.
    assert_eq!(
        menu_of(&mut host, &mut plugin, "light"),
        [("Lokyr Light".to_owned(), true)]
    );
    assert_eq!(
        menu_of(&mut host, &mut plugin, "dark"),
        [("Lokyr Dark".to_owned(), true)]
    );
    pick(&mut host, &mut plugin, "dark", 0);
    assert_eq!(
        written(&mut host),
        [app_key(
            "theme",
            "dark",
            Some(Data::Text("Lokyr Dark".to_owned()))
        )]
    );
    // A plugin's color, of a swatch of it, from the theme's tokens.
    host.ui_event(&mut plugin, &choose(TABS)).unwrap();
    let tree = rendered(&mut host, &mut plugin);
    let swatch = find_node(&tree, "swatch:background").expect("the swatch");
    assert_eq!(
        swatch
            .style
            .background
            .as_ref()
            .map(|token| token.0.as_str()),
        Some("panel.bg")
    );
    let tokens = menu_of(&mut host, &mut plugin, "background");
    assert_eq!(tokens.len(), lokyr_plugin_sdk::theme::TOKENS.len());
    assert!(tokens.contains(&("panel.bg".to_owned(), true)));
    let at = tokens
        .iter()
        .position(|(token, _)| token == "accent")
        .unwrap();
    pick(&mut host, &mut plugin, "background", at);
    assert_eq!(
        written(&mut host),
        [(
            "tabs".to_owned(),
            Some("lokyr/tabs".to_owned()),
            "background".to_owned(),
            Some(Data::Text("accent".to_owned()))
        )]
    );
}

#[test]
fn the_bindings_are_listed_of_their_commands_titles_taken_away_and_added() {
    let mut host = FakeHost::new(NAME)
        .with_manifest(crate::MANIFEST)
        .with_commands(vec![lokyr_plugin_sdk::command::Command {
            name: "window.new".to_owned(),
            title: "New Window".to_owned(),
            category: lokyr_plugin_sdk::command::Category::App,
            capabilities: Vec::new(),
            params: Vec::new(),
            when: None,
        }]);
    host.pane(PaneId(1));
    host.set_settings(settings());
    let mut plugin = SettingsPage::default();
    host.activate(&mut plugin).unwrap();
    host.show_page(PAGE);
    host.deliver(&mut plugin).unwrap();
    host.ui_event(&mut plugin, &choose(BINDINGS)).unwrap();
    let tree = rendered(&mut host, &mut plugin);
    assert_eq!(text_of(&tree, "title").as_deref(), Some("section-keys"));
    // Each of its keys, its command's title, or that it takes nothing.
    assert_eq!(text_of(&tree, "keys:cmd+k").as_deref(), Some("⌘K"));
    assert_eq!(text_of(&tree, "command:cmd+k").as_deref(), Some("Clear"));
    assert_eq!(
        text_of(&tree, "command:cmd+q").as_deref(),
        Some("binding-nothing")
    );
    let _ = written(&mut host);
    // Taken away.
    host.ui_event(&mut plugin, &on("unbind:cmd+q", UiEventKind::Click))
        .unwrap();
    assert_eq!(written(&mut host), [app_key("keys", "cmd+q", None)]);
    // A new one, its keys then its command, written as Return is pressed,
    // its fields emptied.
    let typed = |text: &str| UiEventKind::InputChanged {
        text: text.to_owned(),
        revision: 3,
    };
    host.ui_event(&mut plugin, &on(NEW_KEYS, typed("cmd+shift+n")))
        .unwrap();
    host.ui_event(&mut plugin, &submit(NEW_COMMAND, "window.new"))
        .unwrap();
    assert_eq!(
        written(&mut host),
        [app_key(
            "keys",
            "cmd+shift+n",
            Some(Data::Text("window.new".to_owned()))
        )]
    );
    host.deliver(&mut plugin).unwrap();
    let tree = rendered(&mut host, &mut plugin);
    assert_eq!(
        text_of(&tree, "command:cmd+shift+n").as_deref(),
        Some("New Window")
    );
    assert!(find_node(&tree, "binding:cmd+q").is_none());
    // A command that is none is refused, told under the fields.
    host.ui_event(&mut plugin, &on(NEW_KEYS, typed("cmd+j")))
        .unwrap();
    host.ui_event(&mut plugin, &submit(NEW_COMMAND, "none.such"))
        .unwrap();
    let tree = rendered(&mut host, &mut plugin);
    let why = text_of(&tree, "refused:keys").unwrap();
    assert!(why.contains("none.such"), "{why}");
}

#[test]
fn a_file_that_does_not_read_shows_its_error_and_the_page_writes_nothing() {
    let (mut host, mut plugin) = host();
    let mut broken = settings();
    broken.error = Some(lokyr_plugin_sdk::host::ConfigProblem {
        message: "a key `sise` that `[font]` has not".to_owned(),
        line: Some(3),
        column: Some(1),
    });
    host.set_settings(broken);
    host.deliver(&mut plugin).unwrap();
    let tree = rendered(&mut host, &mut plugin);
    assert_eq!(
        text_of(&tree, "file-error").as_deref(),
        Some("settings-file-error-at")
    );
    let _ = written(&mut host);
    host.ui_event(&mut plugin, &submit("field:size", "18"))
        .unwrap();
    assert!(written(&mut host).is_empty());
    let tree = rendered(&mut host, &mut plugin);
    let why = text_of(&tree, "refused:size").unwrap();
    assert!(why.contains("does not read"), "{why}");
    // The file reads again: its error goes.
    host.set_settings(settings());
    host.deliver(&mut plugin).unwrap();
    let tree = rendered(&mut host, &mut plugin);
    assert!(find_node(&tree, "file-error").is_none());
}

#[test]
fn a_field_is_written_as_typing_pauses_and_as_it_is_left_once_of_a_value() {
    let (mut host, mut plugin) = host();
    let _ = written(&mut host);
    let typed = |text: &str| UiEventKind::InputChanged {
        text: text.to_owned(),
        revision: 1,
    };
    // Typing: nothing until it pauses.
    host.ui_event(&mut plugin, &on("field:size", typed("1")))
        .unwrap();
    host.advance(std::time::Duration::from_millis(300));
    host.ui_event(&mut plugin, &on("field:size", typed("17")))
        .unwrap();
    host.advance(std::time::Duration::from_millis(400));
    host.deliver(&mut plugin).unwrap();
    assert!(written(&mut host).is_empty());
    host.advance(std::time::Duration::from_millis(300));
    host.deliver(&mut plugin).unwrap();
    assert_eq!(
        written(&mut host),
        [app_key("font", "size", Some(Data::Integer(17)))]
    );
    // Left: written at once, the pause's timer gone.
    host.ui_event(&mut plugin, &on("field:size", typed("19")))
        .unwrap();
    host.ui_event(&mut plugin, &on("field:size", UiEventKind::Blur))
        .unwrap();
    assert_eq!(
        written(&mut host),
        [app_key("font", "size", Some(Data::Integer(19)))]
    );
    host.advance(std::time::Duration::from_secs(1));
    host.deliver(&mut plugin).unwrap();
    assert!(written(&mut host).is_empty());
    // The value in force typed again writes nothing; nor does a field left
    // of nothing typed.
    host.ui_event(&mut plugin, &on("field:size", typed("19")))
        .unwrap();
    host.ui_event(&mut plugin, &submit("field:size", "19"))
        .unwrap();
    host.ui_event(&mut plugin, &on("field:family", UiEventKind::Blur))
        .unwrap();
    assert!(written(&mut host).is_empty());
    // A step down, within the range.
    host.ui_event(
        &mut plugin,
        &on("field:size", UiEventKind::Step { up: false }),
    )
    .unwrap();
    assert_eq!(
        written(&mut host),
        [app_key("font", "size", Some(Data::Integer(18)))]
    );
}

#[test]
fn a_pause_writes_a_whole_value_alone_an_empty_or_half_typed_field_waiting() {
    let (mut host, mut plugin) = host();
    let _ = written(&mut host);
    let typed = |text: &str| UiEventKind::InputChanged {
        text: text.to_owned(),
        revision: 1,
    };
    let pause = |host: &mut FakeHost, plugin: &mut SettingsPage| {
        host.advance(std::time::Duration::from_secs(1));
        host.deliver(plugin).unwrap();
    };
    // Emptied, as a new value is to be typed: not back to the default.
    host.ui_event(&mut plugin, &on("field:size", typed("")))
        .unwrap();
    pause(&mut host, &mut plugin);
    assert!(written(&mut host).is_empty());
    // Half typed, not as the value writes it: nothing, nor its reason.
    host.ui_event(&mut plugin, &on("field:size", typed("13.")))
        .unwrap();
    pause(&mut host, &mut plugin);
    assert!(written(&mut host).is_empty());
    host.ui_event(&mut plugin, &on("field:size", typed("2")))
        .unwrap();
    pause(&mut host, &mut plugin);
    assert!(written(&mut host).is_empty());
    let tree = rendered(&mut host, &mut plugin);
    assert!(find_node(&tree, "refused:size").is_none());
    // Whole: written.
    host.ui_event(&mut plugin, &on("field:size", typed("13.5")))
        .unwrap();
    pause(&mut host, &mut plugin);
    assert_eq!(
        written(&mut host),
        [app_key("font", "size", Some(Data::Number(13.5)))]
    );
    // Left half typed: as it reads, its reason told.
    host.ui_event(&mut plugin, &on("field:size", typed("2")))
        .unwrap();
    host.ui_event(&mut plugin, &on("field:size", UiEventKind::Blur))
        .unwrap();
    let tree = rendered(&mut host, &mut plugin);
    assert!(text_of(&tree, "refused:size").is_some());
}

#[test]
fn a_key_names_its_platforms_where_they_differ_and_links_its_text_in_the_guide() {
    let mut host = FakeHost::new(NAME).with_manifest(crate::MANIFEST);
    host.pane(PaneId(1));
    let mut given = settings();
    // A key of Linux alone, and one of no text in the guide.
    given.sections[0].keys[0].platforms = vec!["linux".to_owned()];
    given.sections[0].keys[1].guide = None;
    host.set_settings(given);
    let mut plugin = SettingsPage::default();
    host.activate(&mut plugin).unwrap();
    host.show_page(PAGE);
    host.deliver(&mut plugin).unwrap();
    let tree = rendered(&mut host, &mut plugin);
    let row = |key: &str| texts_of(find_node(&tree, &format!("key:{key}")).unwrap());
    assert!(row("family").contains(&"setting-platforms".to_owned()));
    assert!(!row("style").contains(&"setting-platforms".to_owned()));
    // More info, a link, of a key that the guide has.
    assert!(matches!(
        &find_node(&tree, "guide:style").unwrap().kind,
        Kind::Button(button) if button.link
    ));
    assert!(find_node(&tree, "guide:size").is_none());
    host.ui_event(&mut plugin, &on("guide:style", UiEventKind::Click))
        .unwrap();
    assert!(
        host.take_done()
            .contains(&Request::Open("https://lokyr.dev/guide#style".to_owned()))
    );
}

/// The texts of `tree`, in its order.
fn texts_of(tree: &Node) -> Vec<String> {
    let mut found = Vec::new();
    if let Kind::Text(text) = &tree.kind {
        found.push(text.clone());
    }
    for child in &tree.children {
        found.extend(texts_of(child));
    }
    found
}

#[test]
fn each_section_has_an_icon_of_its_own_of_the_host_s() {
    let section = |name: &str, plugin: bool| SettingsSection {
        name: name.to_owned(),
        plugin: plugin.then(|| format!("lokyr/{name}")),
        description: String::new(),
        keys: Vec::new(),
    };
    let app = [
        "font",
        "text",
        "cursor",
        "window",
        "interface",
        "slots",
        "terminal",
        "scrollback",
        "keyboard",
        "clipboard",
        "links",
        "bell",
        "notifications",
        "agents",
        "theme",
        "app",
        "updates",
        "debug",
    ];
    let plugins = [
        "tabs",
        "splits",
        "quick-terminal",
        "search",
        "command-header",
        "quick-tips",
    ];
    let mut icons: Vec<&str> = app
        .iter()
        .map(|name| icon_of(&section(name, false)))
        .chain(plugins.iter().map(|name| icon_of(&section(name, true))))
        .collect();
    // The keys' part's, too.
    icons.push("keyboard");
    assert!(
        icons
            .iter()
            .all(|icon| lokyr_plugin_sdk::view::is_icon(icon)),
        "{icons:?}"
    );
    let mut unique = icons.clone();
    unique.sort_unstable();
    unique.dedup();
    assert_eq!(unique.len(), icons.len(), "{icons:?}");
    // Another's plugin, the plugins' icon.
    assert_eq!(icon_of(&section("acme", true)), "extensions");
}

/// A list of the texts `texts`, as a list's value.
fn programs(texts: &[&str]) -> Data {
    Data::List(
        texts
            .iter()
            .map(|text| Data::Text((*text).to_owned()))
            .collect(),
    )
}

#[test]
fn a_list_s_values_are_taken_away_and_added_and_its_default_given_back() {
    let (mut host, mut plugin) = host();
    host.ui_event(&mut plugin, &choose(1)).unwrap();
    let tree = rendered(&mut host, &mut plugin);
    // Each value, of a link that takes it away, and a field that adds one;
    // at its default, its Reset dimmed.
    assert_eq!(text_of(&tree, "item:programs:0").as_deref(), Some("claude"));
    assert_eq!(text_of(&tree, "item:programs:1").as_deref(), Some("codex"));
    assert!(matches!(
        &find_node(&tree, "remove:programs:1").unwrap().kind,
        Kind::Button(button) if button.link
    ));
    assert!(held(&tree, "add:programs").is_some());
    assert_eq!(reset_of(&tree, "programs"), Some(true));
    let _ = written(&mut host);
    host.ui_event(&mut plugin, &on("remove:programs:1", UiEventKind::Click))
        .unwrap();
    assert_eq!(
        written(&mut host),
        [app_key("window", "programs", Some(programs(&["claude"])))]
    );
    host.deliver(&mut plugin).unwrap();
    // Added, of its field, which empties; one there already, nothing.
    host.ui_event(&mut plugin, &submit("add:programs", " aider "))
        .unwrap();
    let done = host.take_done();
    assert!(done.contains(&Request::SetSetting {
        section: "window".to_owned(),
        plugin: None,
        key: "programs".to_owned(),
        value: Some(programs(&["claude", "aider"])),
    }));
    assert!(done.iter().any(|request| matches!(
        request,
        Request::SetText { key, text, .. } if key.0 == "add:programs" && text.is_empty()
    )));
    host.deliver(&mut plugin).unwrap();
    host.ui_event(&mut plugin, &submit("add:programs", "claude"))
        .unwrap();
    assert!(written(&mut host).is_empty());
    // Off its default, Reset gives it back.
    let tree = rendered(&mut host, &mut plugin);
    assert_eq!(reset_of(&tree, "programs"), Some(false));
    host.ui_event(&mut plugin, &on("reset:programs", UiEventKind::Click))
        .unwrap();
    assert_eq!(written(&mut host), [app_key("window", "programs", None)]);
}
