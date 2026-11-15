//! The plugin against the SDK's fake host, of the settings that the tests
//! give; its messages are their ids, as the fake host has no catalogs, a
//! section's title its name.

use lokyr_plugin_sdk::command::Category;
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

/// A binding of the settings' keys, of a command, its title and its
/// category, a default's or the config's own.
fn binding(
    keys: &str,
    command: Option<&str>,
    (title, category): (Option<&str>, Option<Category>),
    default: bool,
) -> lokyr_plugin_sdk::host::SettingBinding {
    lokyr_plugin_sdk::host::SettingBinding {
        keys: keys.to_owned(),
        command: command.map(str::to_owned),
        title: title.map(str::to_owned),
        category,
        args: lokyr_plugin_sdk::command::Args::new(),
        when: None,
        detailed: false,
        default,
        system: false,
    }
}

/// The places of the bindings of [`settings`].
const CLEAR: usize = 0;
const GOTO: usize = 2;
const DOWN: usize = 3;
const QUIT: usize = 4;
const REOPEN: usize = 5;
const SETTINGS: usize = 6;

/// The bindings of [`settings`]: the defaults of `pane.clear`, of
/// `window.new`, of `window.goto` of an argument and of `copy-mode.down` in
/// a context; then the config's `false` that takes Quit's keys away, and
/// its own binding of a command that the app lacks; and macOS's own
/// default of Settings.
fn bindings() -> Vec<lokyr_plugin_sdk::host::SettingBinding> {
    let mut goto = binding(
        "cmd+1",
        Some("window.goto"),
        (Some("Go to Window"), Some(Category::Windows)),
        true,
    );
    goto.args.insert(
        "index".to_owned(),
        lokyr_plugin_sdk::command::Value::Integer(1),
    );
    goto.detailed = true;
    let mut down = binding(
        "j",
        Some("copy-mode.down"),
        (Some("Down"), Some(Category::Edit)),
        true,
    );
    down.when = Some("copy-mode.open && pane.focused".to_owned());
    down.detailed = true;
    vec![
        binding(
            "cmd+k",
            Some("pane.clear"),
            (Some("Clear"), Some(Category::Edit)),
            true,
        ),
        binding(
            "cmd+n",
            Some("window.new"),
            (Some("New Window"), Some(Category::Windows)),
            true,
        ),
        goto,
        down,
        binding("cmd+q", None, (Some("Quit"), Some(Category::App)), false),
        binding("cmd+shift+t", Some("acme.reopen"), (None, None), false),
        lokyr_plugin_sdk::host::SettingBinding {
            system: true,
            ..binding(
                "cmd+,",
                Some("app.settings"),
                (Some("Settings…"), Some(Category::App)),
                true,
            )
        },
    ]
}

/// The commands of the bindings of [`bindings`] that the app has.
fn bound_commands() -> Vec<lokyr_plugin_sdk::command::Command> {
    [
        ("pane.clear", "Clear", Category::Edit),
        ("window.new", "New Window", Category::Windows),
        ("window.goto", "Go to Window", Category::Windows),
        ("copy-mode.down", "Down", Category::Edit),
        ("app.quit", "Quit", Category::App),
    ]
    .map(
        |(name, title, category)| lokyr_plugin_sdk::command::Command {
            name: name.to_owned(),
            title: title.to_owned(),
            category,
            capabilities: Vec::new(),
            params: Vec::new(),
            when: None,
        },
    )
    .to_vec()
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
                    key(
                        "minimum-contrast",
                        Some(Data::Number(1.0)),
                        Some(Data::Number(1.0)),
                        SettingKind::Number {
                            minimum: Some(1.0),
                            maximum: Some(21.0),
                            step: Some(0.5),
                            integer: false,
                        },
                    ),
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
        keys: bindings(),
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
fn the_themes_leave_the_settings_page_for_their_own_and_a_plugin_s_color_is_a_theme_s() {
    let (mut host, mut plugin) = host();
    let _ = written(&mut host);
    host.ui_event(&mut plugin, &choose(THEME)).unwrap();
    // The theme's section of the contrast alone, `[theme]`'s themes on
    // the themes' page.
    let tree = rendered(&mut host, &mut plugin);
    assert!(
        find_node(&tree, "choose:light").is_none(),
        "{}",
        snapshot(&tree)
    );
    assert!(find_node(&tree, "choose:dark").is_none());
    assert!(find_node(&tree, "field:minimum-contrast").is_some());
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

/// A host of the settings of [`settings`], of the commands of
/// [`bound_commands`], and the plugin, activated, its page shown at its
/// bindings.
fn keyed_host() -> (FakeHost, SettingsPage) {
    let mut host = FakeHost::new(NAME).with_manifest(crate::MANIFEST);
    host.set_registry(
        bound_commands()
            .into_iter()
            .map(|command| lokyr_plugin_sdk::host::CommandInfo {
                name: command.name,
                title: command.title,
                category: command.category,
                params: command.params,
                plugin: None,
                keys: None,
                when: None,
                enabled: None,
            })
            .collect(),
    );
    host.pane(PaneId(1));
    host.set_settings(settings());
    let mut plugin = SettingsPage::default();
    host.activate(&mut plugin).unwrap();
    host.show_page(PAGE);
    host.deliver(&mut plugin).unwrap();
    rendered(&mut host, &mut plugin);
    host.ui_event(&mut plugin, &choose(BINDINGS)).unwrap();
    (host, plugin)
}

/// The place of the binding of `keys` among the settings' bindings, as
/// the writes left them.
fn place(host: &FakeHost, keys: &str) -> usize {
    host.settings()
        .keys
        .iter()
        .position(|binding| binding.keys == keys)
        .unwrap_or_else(|| panic!("no binding of {keys}"))
}

/// The text of the keys of the binding at `at`: a button's label, or the
/// text of keys that do not change.
fn keys_of(tree: &Node, at: usize) -> Option<String> {
    let node = find_node(tree, &format!("keys:{at}"))
        .or_else(|| find_node(tree, &format!("keys-text:{at}")))?;
    match &node.kind {
        Kind::Button(button) => Some(button.label.clone()),
        Kind::Text(text) => Some(text.clone()),
        _ => None,
    }
}

/// The headings of the Keys part's categories, in their order.
fn categories_of(tree: &Node) -> Vec<String> {
    let mut found = Vec::new();
    let mut left = vec![tree];
    while let Some(node) = left.pop() {
        if let Some(key) = &node.key
            && key.0.starts_with("category-")
        {
            found.push(key.0.clone());
        }
        left.extend(node.children.iter().rev());
    }
    found
}

#[test]
fn the_bindings_in_force_are_listed_by_category_of_their_keys_titles_and_contexts() {
    let (mut host, mut plugin) = keyed_host();
    let tree = rendered(&mut host, &mut plugin);
    assert_eq!(text_of(&tree, "title").as_deref(), Some("section-keys"));
    // By the commands' categories, the bindings of none last.
    assert_eq!(
        categories_of(&tree),
        [
            "category-app",
            "category-windows",
            "category-edit",
            "category-other"
        ],
        "{}",
        snapshot(&tree)
    );
    // Each of its keys as macOS writes them, its title, its command, its
    // arguments, its context, and whether it is the config's.
    assert_eq!(keys_of(&tree, CLEAR).as_deref(), Some("⌘K"));
    assert_eq!(text_of(&tree, "command:0").as_deref(), Some("Clear"));
    assert_eq!(text_of(&tree, "details:0").as_deref(), Some("pane.clear"));
    assert_eq!(
        text_of(&tree, &format!("details:{GOTO}")).as_deref(),
        Some("window.goto (index = 1)")
    );
    assert_eq!(
        text_of(&tree, &format!("details:{DOWN}")).as_deref(),
        Some("copy-mode.down · binding-when")
    );
    // Keys taken away, of the default's title, which Restore gives back.
    assert_eq!(
        text_of(&tree, &format!("command:{QUIT}")).as_deref(),
        Some("binding-removed")
    );
    assert_eq!(
        text_of(&tree, &format!("details:{QUIT}")).as_deref(),
        Some("binding-yours")
    );
    assert!(labelled(&tree, &format!("restore:{QUIT}")).is_some());
    assert!(labelled(&tree, &format!("unbind:{QUIT}")).is_none());
    // The config's own, of a command that the app lacks, of its name.
    assert_eq!(
        text_of(&tree, &format!("command:{REOPEN}")).as_deref(),
        Some("acme.reopen")
    );
    // macOS's own: its keys a text, of no change nor removal.
    assert!(matches!(
        find_node(&tree, &format!("keys-text:{SETTINGS}")).map(|node| &node.kind),
        Some(Kind::Text(text)) if text == "⌘,"
    ));
    assert!(labelled(&tree, &format!("unbind:{SETTINGS}")).is_none());
    assert_eq!(
        text_of(&tree, &format!("details:{SETTINGS}")).as_deref(),
        Some("app.settings · binding-system")
    );
    // A search of a context's name, or of a category's title.
    host.ui_event(&mut plugin, &search("copy-mode.open"))
        .unwrap();
    let tree = rendered(&mut host, &mut plugin);
    assert!(find_node(&tree, &format!("binding:{DOWN}")).is_some());
    assert!(find_node(&tree, &format!("binding:{CLEAR}")).is_none());
    host.ui_event(&mut plugin, &search("category-windows"))
        .unwrap();
    let tree = rendered(&mut host, &mut plugin);
    assert_eq!(categories_of(&tree), ["category-windows"]);
}

/// The recorder of `key`'s keys, recorded as `keys`.
fn recorded(key: &str, keys: Option<&str>) -> UiEvent {
    on(
        key,
        UiEventKind::Recorded {
            keys: keys.map(str::to_owned),
        },
    )
}

#[test]
fn a_default_s_keys_recorded_write_the_new_keys_and_false_for_the_old() {
    let (mut host, mut plugin) = keyed_host();
    rendered(&mut host, &mut plugin);
    let _ = written(&mut host);
    // A click on the keys shows their recorder, of their keys as macOS
    // writes them, focused.
    host.ui_event(&mut plugin, &on("keys:0", UiEventKind::Click))
        .unwrap();
    let tree = rendered(&mut host, &mut plugin);
    let field = find_node(&tree, "keys-field:0").expect("the recorder");
    assert!(matches!(&field.kind, Kind::Field(field) if field.records));
    assert_eq!(
        held(&tree, "keys-field:0").map(|(text, _)| text),
        Some(Some("⌘K".to_owned()))
    );
    assert!(host.take_done().iter().any(|request| matches!(
        request,
        Request::FocusNode { key, .. } if key.0 == "keys-field:0"
    )));
    // Keys recorded: the command at its new keys, and `false` at the old.
    host.ui_event(&mut plugin, &recorded("keys-field:0", Some("cmd+shift+k")))
        .unwrap();
    assert_eq!(
        written(&mut host),
        [
            app_key(
                "keys",
                "cmd+shift+k",
                Some(Data::Text("pane.clear".to_owned()))
            ),
            app_key("keys", "cmd+k", Some(Data::Boolean(false))),
        ]
    );
    host.deliver(&mut plugin).unwrap();
    let tree = rendered(&mut host, &mut plugin);
    assert!(
        find_node(&tree, "keys-field:0").is_none(),
        "the recorder goes"
    );
    // A default of arguments, as a table of them.
    let goto = place(&host, "cmd+1");
    host.ui_event(
        &mut plugin,
        &on(&format!("keys:{goto}"), UiEventKind::Click),
    )
    .unwrap();
    rendered(&mut host, &mut plugin);
    host.ui_event(
        &mut plugin,
        &recorded(&format!("keys-field:{goto}"), Some("ctrl+1")),
    )
    .unwrap();
    let mut table = BTreeMap::new();
    table.insert("command".to_owned(), Data::Text("window.goto".to_owned()));
    table.insert(
        "args".to_owned(),
        Data::Map(BTreeMap::from([("index".to_owned(), Data::Integer(1))])),
    );
    let done = written(&mut host);
    assert_eq!(done[0], app_key("keys", "ctrl+1", Some(Data::Map(table))));
    assert_eq!(
        done[1],
        app_key("keys", "cmd+1", Some(Data::Boolean(false)))
    );
}

#[test]
fn a_recorder_of_the_same_keys_or_macos_s_writes_nothing_and_its_leaving_puts_it_away() {
    let (mut host, mut plugin) = keyed_host();
    rendered(&mut host, &mut plugin);
    let _ = written(&mut host);
    host.ui_event(&mut plugin, &on("keys:0", UiEventKind::Click))
        .unwrap();
    rendered(&mut host, &mut plugin);
    // macOS's own keys, told under the row, the recorder staying.
    host.ui_event(&mut plugin, &recorded("keys-field:0", Some("cmd+q")))
        .unwrap();
    assert!(written(&mut host).is_empty());
    let tree = rendered(&mut host, &mut plugin);
    let why = text_of(&tree, "refused:binding:0").expect("why");
    assert!(why.contains("macOS's own"), "{why}");
    assert!(find_node(&tree, "keys-field:0").is_some());
    // The same keys: nothing, the recorder gone.
    host.ui_event(&mut plugin, &recorded("keys-field:0", Some("cmd+k")))
        .unwrap();
    assert!(written(&mut host).is_empty());
    let tree = rendered(&mut host, &mut plugin);
    assert!(find_node(&tree, "keys-field:0").is_none());
    assert!(text_of(&tree, "refused:binding:0").is_none());
    // Left of no keys recorded: put away, nothing written.
    host.ui_event(&mut plugin, &on("keys:1", UiEventKind::Click))
        .unwrap();
    rendered(&mut host, &mut plugin);
    host.ui_event(&mut plugin, &on("keys-field:1", UiEventKind::Blur))
        .unwrap();
    assert!(written(&mut host).is_empty());
    let tree = rendered(&mut host, &mut plugin);
    assert!(find_node(&tree, "keys-field:1").is_none());
}

#[test]
fn a_default_removed_writes_false_the_config_s_own_goes_and_one_taken_away_comes_back() {
    let (mut host, mut plugin) = keyed_host();
    rendered(&mut host, &mut plugin);
    let _ = written(&mut host);
    host.ui_event(&mut plugin, &on("unbind:1", UiEventKind::Click))
        .unwrap();
    assert_eq!(
        written(&mut host),
        [app_key("keys", "cmd+n", Some(Data::Boolean(false)))]
    );
    host.deliver(&mut plugin).unwrap();
    rendered(&mut host, &mut plugin);
    // The config's own, whatever its place now.
    let reopen = place(&host, "cmd+shift+t");
    host.ui_event(
        &mut plugin,
        &on(&format!("unbind:{reopen}"), UiEventKind::Click),
    )
    .unwrap();
    assert_eq!(written(&mut host), [app_key("keys", "cmd+shift+t", None)]);
    host.deliver(&mut plugin).unwrap();
    rendered(&mut host, &mut plugin);
    // Keys taken away, given back.
    let quit = place(&host, "cmd+q");
    host.ui_event(
        &mut plugin,
        &on(&format!("restore:{quit}"), UiEventKind::Click),
    )
    .unwrap();
    assert_eq!(written(&mut host), [app_key("keys", "cmd+q", None)]);
}

#[test]
fn keys_that_do_not_change_take_no_click_and_a_default_shared_stays_as_told() {
    let (mut host, mut plugin) = keyed_host();
    let mut given = settings();
    // A default of copy mode's `j` for the palette too, in its context.
    let mut palette = binding(
        "j",
        Some("palette.next"),
        (Some("Next"), Some(Category::Edit)),
        true,
    );
    palette.when = Some("palette.open".to_owned());
    given.keys.push(palette);
    host.set_settings(given);
    host.deliver(&mut plugin).unwrap();
    rendered(&mut host, &mut plugin);
    let _ = written(&mut host);
    // macOS's own: its keys' click, by their text's key or a button's,
    // records nothing.
    for key in [format!("keys-text:{SETTINGS}"), format!("keys:{SETTINGS}")] {
        host.ui_event(&mut plugin, &on(&key, UiEventKind::Click))
            .unwrap();
        let tree = rendered(&mut host, &mut plugin);
        assert!(find_node(&tree, &format!("keys-field:{SETTINGS}")).is_none());
    }
    // A default of keys that another default shares: neither removed nor
    // moved, the other named.
    host.ui_event(
        &mut plugin,
        &on(&format!("unbind:{DOWN}"), UiEventKind::Click),
    )
    .unwrap();
    assert!(written(&mut host).is_empty());
    let tree = rendered(&mut host, &mut plugin);
    let why = text_of(&tree, &format!("refused:binding:{DOWN}")).expect("why");
    assert!(why.contains("binding-shared"), "{why}");
    host.ui_event(
        &mut plugin,
        &on(&format!("keys:{DOWN}"), UiEventKind::Click),
    )
    .unwrap();
    host.ui_event(
        &mut plugin,
        &recorded(&format!("keys-field:{DOWN}"), Some("k")),
    )
    .unwrap();
    assert!(written(&mut host).is_empty());
}

#[test]
fn keys_that_another_binding_takes_are_told_and_a_second_press_takes_their_place() {
    let (mut host, mut plugin) = keyed_host();
    let mut given = settings();
    // The config's own, of its own spelling of ⇧⌘Y, and a global one.
    given.keys.push(binding(
        "shift+cmd+y",
        Some("window.new"),
        (Some("New Window"), Some(Category::Windows)),
        false,
    ));
    given.keys.push(binding(
        "global:ctrl+`",
        Some("window.new"),
        (Some("New Window"), Some(Category::Windows)),
        false,
    ));
    host.set_settings(given);
    host.deliver(&mut plugin).unwrap();
    rendered(&mut host, &mut plugin);
    let _ = written(&mut host);
    // ⌘K's Clear to ⇧⌘Y, which the config's own takes: told, nothing
    // written, the recorder staying.
    host.ui_event(
        &mut plugin,
        &on(&format!("keys:{CLEAR}"), UiEventKind::Click),
    )
    .unwrap();
    host.ui_event(
        &mut plugin,
        &recorded(&format!("keys-field:{CLEAR}"), Some("cmd+shift+y")),
    )
    .unwrap();
    assert!(written(&mut host).is_empty());
    let tree = rendered(&mut host, &mut plugin);
    let told = text_of(&tree, &format!("taken:binding:{CLEAR}")).expect("told");
    assert!(told.contains("binding-taken-again"), "{told}");
    assert!(find_node(&tree, &format!("keys-field:{CLEAR}")).is_some());
    // Pressed again: written at the config's own spelling, in its place.
    host.ui_event(
        &mut plugin,
        &recorded(&format!("keys-field:{CLEAR}"), Some("cmd+shift+y")),
    )
    .unwrap();
    assert_eq!(
        written(&mut host),
        [
            app_key(
                "keys",
                "shift+cmd+y",
                Some(Data::Text("pane.clear".to_owned()))
            ),
            app_key("keys", "cmd+k", Some(Data::Boolean(false))),
        ]
    );
    host.deliver(&mut plugin).unwrap();
    rendered(&mut host, &mut plugin);
    // A global key stays one.
    let global = place(&host, "global:ctrl+`");
    host.ui_event(
        &mut plugin,
        &on(&format!("keys:{global}"), UiEventKind::Click),
    )
    .unwrap();
    host.ui_event(
        &mut plugin,
        &recorded(&format!("keys-field:{global}"), Some("ctrl+shift+space")),
    )
    .unwrap();
    assert_eq!(
        written(&mut host)[0],
        app_key(
            "keys",
            "global:ctrl+shift+space",
            Some(Data::Text("window.new".to_owned()))
        )
    );
}

/// The panel of the new binding, and its menu of the commands.
fn new_binding() -> ViewId {
    ViewId::Panel(PanelId::from(NEW_BINDING))
}

/// The event `kind` of the node of `key` of the new binding's panel, or
/// of its menu of the commands.
fn in_panel(panel: &str, key: &str, kind: UiEventKind) -> UiEvent {
    UiEvent {
        view: ViewId::Panel(PanelId::from(panel)),
        key: key.into(),
        kind,
    }
}

#[test]
fn add_opens_a_panel_of_a_recorder_and_a_menu_of_the_commands_which_writes_the_binding() {
    let (mut host, mut plugin) = keyed_host();
    rendered(&mut host, &mut plugin);
    let _ = written(&mut host);
    // Add, above the bindings and below them, opens the panel, a dialog
    // in the middle of the window.
    let tree = rendered(&mut host, &mut plugin);
    assert!(labelled(&tree, ADD_BINDING).is_some() && labelled(&tree, ADD_BINDING_BELOW).is_some());
    host.ui_event(&mut plugin, &on(ADD_BINDING_BELOW, UiEventKind::Click))
        .unwrap();
    let options = host
        .panels()
        .get(&PanelId::from(NEW_BINDING))
        .expect("the panel");
    assert_eq!(
        options.anchor,
        Anchor::ViewWindow {
            view: page(),
            corner: lokyr_plugin_sdk::ui::Corner::Center,
        }
    );
    assert_eq!(options.role, PanelRole::Dialog);
    let panel = host.render(&mut plugin, &new_binding()).expect("the panel");
    let recorder = find_node(&panel, RECORDER).expect("its recorder");
    assert!(matches!(&recorder.kind, Kind::Field(field) if field.records));
    // Add waits for the keys and a command.
    assert!(matches!(
        &find_node(&panel, CONFIRM).unwrap().kind,
        Kind::Button(button) if button.disabled
    ));
    // Keys that a binding takes: told, of its command's title.
    host.ui_event(
        &mut plugin,
        &in_panel(
            NEW_BINDING,
            RECORDER,
            UiEventKind::Recorded {
                keys: Some("cmd+k".to_owned()),
            },
        ),
    )
    .unwrap();
    let panel = host.render(&mut plugin, &new_binding()).unwrap();
    assert_eq!(
        held(&panel, RECORDER).map(|(text, _)| text),
        Some(Some("⌘K".to_owned()))
    );
    assert_eq!(text_of(&panel, "taken").as_deref(), Some("binding-taken"));
    // macOS's own: none to take, Add off.
    host.ui_event(
        &mut plugin,
        &in_panel(
            NEW_BINDING,
            RECORDER,
            UiEventKind::Recorded {
                keys: Some("cmd+q".to_owned()),
            },
        ),
    )
    .unwrap();
    host.ui_event(
        &mut plugin,
        &in_panel(COMMANDS, "command-item:window.new", UiEventKind::Click),
    )
    .unwrap();
    let panel = host.render(&mut plugin, &new_binding()).unwrap();
    assert_eq!(
        text_of(&panel, "taken").as_deref(),
        Some("binding-taken-system")
    );
    assert!(matches!(
        &find_node(&panel, CONFIRM).unwrap().kind,
        Kind::Button(button) if button.disabled
    ));
    // The commands' menu, of the categories' submenus, opened below its
    // button.
    host.ui_event(
        &mut plugin,
        &in_panel(NEW_BINDING, CHOOSE_COMMAND, UiEventKind::Click),
    )
    .unwrap();
    assert!(host.panels().contains_key(&PanelId::from(COMMANDS)));
    let menu = host
        .render(&mut plugin, &ViewId::Panel(PanelId::from(COMMANDS)))
        .unwrap();
    let windows = menu
        .children
        .iter()
        .find(|group| {
            matches!(&group.children.first().map(|node| &node.kind),
                Some(Kind::Text(title)) if title == "category-windows")
        })
        .unwrap_or_else(|| panic!("the windows' submenu: {}", snapshot(&menu)));
    assert!(find_node(windows, "command-item:window.new").is_some());
    // Free keys and a command: Add writes the binding and closes.
    host.ui_event(
        &mut plugin,
        &in_panel(
            NEW_BINDING,
            RECORDER,
            UiEventKind::Recorded {
                keys: Some("cmd+shift+y".to_owned()),
            },
        ),
    )
    .unwrap();
    let panel = host.render(&mut plugin, &new_binding()).unwrap();
    assert!(text_of(&panel, "taken").is_none());
    assert_eq!(
        labelled(&panel, CHOOSE_COMMAND)
            .map(|(label, _)| label)
            .as_deref(),
        Some("New Window")
    );
    host.ui_event(
        &mut plugin,
        &in_panel(NEW_BINDING, CONFIRM, UiEventKind::Click),
    )
    .unwrap();
    assert_eq!(
        written(&mut host),
        [app_key(
            "keys",
            "cmd+shift+y",
            Some(Data::Text("window.new".to_owned()))
        )]
    );
    assert!(!host.panels().contains_key(&PanelId::from(NEW_BINDING)));
}

#[test]
fn cancel_closes_the_new_binding_s_panel_and_a_refusal_is_told_in_it() {
    let (mut host, mut plugin) = keyed_host();
    rendered(&mut host, &mut plugin);
    host.ui_event(&mut plugin, &on(ADD_BINDING, UiEventKind::Click))
        .unwrap();
    host.render(&mut plugin, &new_binding());
    // A command that the app lacks, refused, told in the panel.
    host.ui_event(
        &mut plugin,
        &in_panel(
            NEW_BINDING,
            RECORDER,
            UiEventKind::Recorded {
                keys: Some("cmd+shift+y".to_owned()),
            },
        ),
    )
    .unwrap();
    host.ui_event(
        &mut plugin,
        &in_panel(COMMANDS, "command-item:none.such", UiEventKind::Click),
    )
    .unwrap();
    host.ui_event(
        &mut plugin,
        &in_panel(NEW_BINDING, CONFIRM, UiEventKind::Click),
    )
    .unwrap();
    let panel = host.render(&mut plugin, &new_binding()).unwrap();
    let why = text_of(&panel, "refused:new").expect("why");
    assert!(why.contains("none.such"), "{why}");
    // Cancel closes it.
    host.ui_event(
        &mut plugin,
        &in_panel(NEW_BINDING, CANCEL, UiEventKind::Click),
    )
    .unwrap();
    assert!(host.panels().is_empty());
    host.deliver(&mut plugin).unwrap();
    assert!(host.render(&mut plugin, &new_binding()).is_none());
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
fn a_step_writes_the_decimals_of_the_step_and_a_text_typed_shows_until_it_is_written() {
    let mut host = FakeHost::new(NAME).with_manifest(crate::MANIFEST);
    host.pane(PaneId(1));
    let mut given = settings();
    given.sections[0].keys[1].value = Some(Data::Number(0.95));
    given.sections[0].keys[1].kind = SettingKind::Number {
        minimum: None,
        maximum: None,
        step: Some(0.05),
        integer: false,
    };
    host.set_settings(given);
    let mut plugin = SettingsPage::default();
    host.activate(&mut plugin).unwrap();
    host.show_page(PAGE);
    host.deliver(&mut plugin).unwrap();
    let _ = written(&mut host);
    // 0.95 less 0.05 is 0.9, not 0.8999999999999999.
    host.ui_event(
        &mut plugin,
        &on("field:size", UiEventKind::Step { up: false }),
    )
    .unwrap();
    assert_eq!(
        written(&mut host),
        [app_key("font", "size", Some(Data::Number(0.9)))]
    );
    // A text typed and not written yet is what the field holds, whatever
    // renders meanwhile, so that the host keeps its caret and what follows.
    host.ui_event(
        &mut plugin,
        &on(
            "field:size",
            UiEventKind::InputChanged {
                text: "0.97".to_owned(),
                revision: 1,
            },
        ),
    )
    .unwrap();
    let tree = rendered(&mut host, &mut plugin);
    assert_eq!(
        held(&tree, "field:size").map(|(text, _)| text),
        Some(Some("0.97".to_owned()))
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
    // Left half typed: as it reads, its reason told, its text kept in
    // its field to be mended.
    host.ui_event(&mut plugin, &on("field:size", typed("2")))
        .unwrap();
    host.ui_event(&mut plugin, &on("field:size", UiEventKind::Blur))
        .unwrap();
    let tree = rendered(&mut host, &mut plugin);
    assert!(text_of(&tree, "refused:size").is_some());
    assert_eq!(
        held(&tree, "field:size").map(|(text, _)| text),
        Some(Some("2".to_owned()))
    );
    // Reset takes it away with the key's value.
    host.ui_event(&mut plugin, &on("reset:size", UiEventKind::Click))
        .unwrap();
    host.deliver(&mut plugin).unwrap();
    let tree = rendered(&mut host, &mut plugin);
    assert_eq!(
        held(&tree, "field:size").map(|(text, _)| text),
        Some(Some("13".to_owned()))
    );
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

/// The texts of the sidebar's items in `tree`, each of its icon and title.
fn sidebar_of(tree: &Node) -> Vec<String> {
    let Kind::List(list) = &find_node(tree, SECTIONS).expect("the sidebar's list").kind else {
        panic!("a list");
    };
    list.items
        .iter()
        .map(|item| item.texts[1].clone())
        .collect()
}

/// The search's text `text`, as it is typed.
fn search(text: &str) -> UiEvent {
    on(
        SEARCH,
        UiEventKind::InputChanged {
            text: text.to_owned(),
            revision: 1,
        },
    )
}

/// The items that the calls selected since the last look in the lists of
/// the page.
fn selected(host: &mut FakeHost) -> Vec<usize> {
    host.take_done()
        .into_iter()
        .filter_map(|request| match request {
            Request::Select { key, item, .. } if key.0 == SECTIONS => Some(item),
            _ => None,
        })
        .collect()
}

#[test]
fn a_search_shows_the_sections_and_the_keys_that_hold_its_words() {
    let (mut host, mut plugin) = host();
    let tree = rendered(&mut host, &mut plugin);
    assert!(find_node(&tree, SEARCH).is_some(), "{}", snapshot(&tree));
    assert!(find_node(&tree, SEARCH_CLEAR).is_none());
    // The field's box, of the ring of its text's focus.
    let field = find_node(&tree, "search-row").unwrap();
    assert!(field.style.holds_ring && field.style.border.is_some());

    // A key's name, of its case aside: the font's section alone, of that
    // key alone, still chosen.
    host.ui_event(&mut plugin, &search("SIZE")).unwrap();
    assert!(selected(&mut host).contains(&0));
    let tree = rendered(&mut host, &mut plugin);
    assert_eq!(sidebar_of(&tree), ["font"], "{}", snapshot(&tree));
    assert!(reset_of(&tree, "size").is_some(), "{}", snapshot(&tree));
    assert!(reset_of(&tree, "family").is_none());
    // The button that empties it, inside the field's box, at its end.
    let field = find_node(&tree, "search-row").unwrap();
    let last = field.children.last().and_then(|node| node.key.as_ref());
    assert_eq!(last.map(|key| key.0.as_str()), Some(SEARCH_CLEAR));

    // Words in any order, of keys' names and what they do, across
    // sections: the first of them chosen, as the font's holds none.
    host.ui_event(&mut plugin, &search("does ro")).unwrap();
    assert_eq!(selected(&mut host), [0]);
    let tree = rendered(&mut host, &mut plugin);
    assert_eq!(sidebar_of(&tree), ["window", "tabs"], "{}", snapshot(&tree));
    assert!(
        reset_of(&tree, "scrollbar").is_some(),
        "{}",
        snapshot(&tree)
    );
    assert!(reset_of(&tree, "programs").is_some());
    assert!(reset_of(&tree, "opaque-titlebar").is_none());
}

#[test]
fn a_search_chooses_its_first_section_where_the_chosen_one_holds_none() {
    let (mut host, mut plugin) = host();
    rendered(&mut host, &mut plugin);
    host.ui_event(&mut plugin, &search("scrollbar")).unwrap();
    // The window's section, chosen, the sidebar's first item.
    assert_eq!(selected(&mut host), [0]);
    let tree = rendered(&mut host, &mut plugin);
    assert_eq!(sidebar_of(&tree), ["window"], "{}", snapshot(&tree));
    assert!(
        reset_of(&tree, "scrollbar").is_some(),
        "{}",
        snapshot(&tree)
    );
    assert!(reset_of(&tree, "opaque-titlebar").is_none());

    // A section's title shows all of its keys; the sidebar's items are of
    // the sections shown.
    host.ui_event(&mut plugin, &search("tabs")).unwrap();
    host.ui_event(&mut plugin, &choose(0)).unwrap();
    let tree = rendered(&mut host, &mut plugin);
    assert_eq!(sidebar_of(&tree), ["tabs"], "{}", snapshot(&tree));
    assert!(
        reset_of(&tree, "column-width").is_some(),
        "{}",
        snapshot(&tree)
    );
    assert!(reset_of(&tree, "background").is_some());
}

#[test]
fn the_page_shown_again_shows_no_search_nor_refusal_as_its_fields_are_empty() {
    let (mut host, mut plugin) = host();
    let whole = sidebar_of(&rendered(&mut host, &mut plugin));
    host.ui_event(&mut plugin, &search("nothing at all"))
        .unwrap();
    host.ui_event(&mut plugin, &submit("field:size", "2"))
        .unwrap();
    let tree = rendered(&mut host, &mut plugin);
    assert_ne!(sidebar_of(&tree), whole);
    // The window closed and opened again: its search field is empty, and
    // so is the page's search, of no refusal nor text typed.
    host.hide_page();
    host.deliver(&mut plugin).unwrap();
    host.show_page(PAGE);
    host.deliver(&mut plugin).unwrap();
    let tree = rendered(&mut host, &mut plugin);
    assert_eq!(sidebar_of(&tree), whole);
    assert!(find_node(&tree, "no-match").is_none());
    assert!(find_node(&tree, "refused:size").is_none());
    assert_eq!(
        held(&tree, "field:size").map(|(text, _)| text),
        Some(Some("15".to_owned()))
    );
}

/// A text typed into the Keys part's filter.
fn filter(text: &str) -> UiEvent {
    on(
        "keys-filter",
        UiEventKind::InputChanged {
            text: text.to_owned(),
            revision: 1,
        },
    )
}

#[test]
fn the_keys_part_s_filter_shows_the_bindings_of_its_words_and_keeps_them_as_the_part_shows_again() {
    let (mut host, mut plugin) = keyed_host();
    let tree = rendered(&mut host, &mut plugin);
    // Beside Add, of no button that empties it while it is empty.
    assert!(find_node(&tree, "keys-filter").is_some());
    assert!(find_node(&tree, "add-binding").is_some());
    assert!(find_node(&tree, "keys-filter-clear").is_none());
    // A command's title, keys as macOS writes them and as the config does,
    // and a context.
    for (words, shown) in [
        ("clear", CLEAR),
        ("⌘K", CLEAR),
        ("cmd+k", CLEAR),
        ("copy-mode.open", DOWN),
    ] {
        host.ui_event(&mut plugin, &filter(words)).unwrap();
        let tree = rendered(&mut host, &mut plugin);
        assert!(keys_of(&tree, shown).is_some(), "{words}");
        let others: Vec<usize> = [CLEAR, DOWN, QUIT, REOPEN]
            .into_iter()
            .filter(|&at| at != shown && keys_of(&tree, at).is_some())
            .collect();
        assert!(others.is_empty(), "{words}: {others:?}");
        assert!(find_node(&tree, "keys-filter-clear").is_some());
    }
    // Words of no binding: said so.
    host.ui_event(&mut plugin, &filter("nothing at all"))
        .unwrap();
    let tree = rendered(&mut host, &mut plugin);
    assert!(text_of(&tree, "keys-no-match").is_some());
    // Another part and back: the filter stays.
    host.ui_event(&mut plugin, &filter("clear")).unwrap();
    host.ui_event(&mut plugin, &choose(0)).unwrap();
    host.ui_event(&mut plugin, &choose(BINDINGS)).unwrap();
    let tree = rendered(&mut host, &mut plugin);
    assert!(keys_of(&tree, CLEAR).is_some());
    assert!(keys_of(&tree, QUIT).is_none());
    // Its button empties it, every binding back.
    host.ui_event(&mut plugin, &on("keys-filter-clear", UiEventKind::Click))
        .unwrap();
    let tree = rendered(&mut host, &mut plugin);
    assert!(keys_of(&tree, QUIT).is_some() && keys_of(&tree, CLEAR).is_some());
    // The page shown anew, of an empty field, filters nothing.
    host.ui_event(&mut plugin, &filter("clear")).unwrap();
    host.hide_page();
    host.deliver(&mut plugin).unwrap();
    host.show_page(PAGE);
    host.deliver(&mut plugin).unwrap();
    host.ui_event(&mut plugin, &choose(BINDINGS)).unwrap();
    let tree = rendered(&mut host, &mut plugin);
    assert!(keys_of(&tree, QUIT).is_some());
}

#[test]
fn a_search_finds_the_bindings_of_their_keys_and_commands() {
    let (mut host, mut plugin) = host();
    rendered(&mut host, &mut plugin);
    host.ui_event(&mut plugin, &search("clear")).unwrap();
    let tree = rendered(&mut host, &mut plugin);
    assert_eq!(sidebar_of(&tree), ["section-keys"], "{}", snapshot(&tree));
    assert_eq!(keys_of(&tree, CLEAR).as_deref(), Some("⌘K"));
    assert_eq!(keys_of(&tree, QUIT), None);

    // The keys as macOS writes them, and as the config does.
    host.ui_event(&mut plugin, &search("⌘Q")).unwrap();
    let tree = rendered(&mut host, &mut plugin);
    assert_eq!(keys_of(&tree, QUIT).as_deref(), Some("⌘Q"));
    assert_eq!(keys_of(&tree, CLEAR), None);
    host.ui_event(&mut plugin, &search("cmd+k")).unwrap();
    let tree = rendered(&mut host, &mut plugin);
    assert_eq!(keys_of(&tree, CLEAR).as_deref(), Some("⌘K"));
}

#[test]
fn a_search_of_no_match_says_so_and_its_button_empties_it() {
    let (mut host, mut plugin) = host();
    rendered(&mut host, &mut plugin);
    host.ui_event(&mut plugin, &choose(TABS)).unwrap();
    host.ui_event(&mut plugin, &search("nowhere")).unwrap();
    let tree = rendered(&mut host, &mut plugin);
    assert!(sidebar_of(&tree).is_empty());
    assert_eq!(
        text_of(&tree, "no-match").as_deref(),
        Some("settings-no-match"),
        "{}",
        snapshot(&tree)
    );

    // The button empties the field, every section back, the one chosen
    // before still chosen.
    host.take_done();
    host.ui_event(&mut plugin, &on(SEARCH_CLEAR, UiEventKind::Click))
        .unwrap();
    assert!(host.take_done().iter().any(|request| matches!(
        request,
        Request::SetText { key, text, .. } if key.0 == SEARCH && text.is_empty()
    )));
    let tree = rendered(&mut host, &mut plugin);
    assert_eq!(
        sidebar_of(&tree),
        ["font", "window", "theme", "section-keys", "tabs"]
    );
    assert!(
        reset_of(&tree, "column-width").is_some(),
        "{}",
        snapshot(&tree)
    );
    assert!(find_node(&tree, SEARCH_CLEAR).is_none());
}

/// Highlights the item `at` of the menu of `name`, as the arrows or the
/// pointer go over it.
fn highlight(host: &mut FakeHost, plugin: &mut SettingsPage, name: &str, at: usize) {
    host.ui_event(
        plugin,
        &UiEvent {
            view: ViewId::Panel(PanelId(format!("choices:{name}"))),
            key: format!("choice:{at}").as_str().into(),
            kind: UiEventKind::HoverEnter,
        },
    )
    .unwrap();
}

/// The font's previews that the calls asked since the last look.
fn previews(host: &mut FakeHost) -> Vec<Request> {
    host.take_done()
        .into_iter()
        .filter(|request| {
            matches!(
                request,
                Request::PreviewFont(_) | Request::PreviewFontStyle { .. }
            )
        })
        .collect()
}

#[test]
fn a_font_s_family_and_a_face_s_style_show_as_their_items_are_highlighted() {
    let (mut host, mut plugin) = host();
    // The family of the monospaced ones' menu.
    let mut shown = settings();
    shown.sections[0].keys[0] = controlled("family", None, "monospaced-family");
    host.set_settings(shown);
    host.deliver(&mut plugin).unwrap();
    let _ = host.take_done();
    let families = menu_of(&mut host, &mut plugin, "family");
    assert_eq!(labels(&families), ["Menlo", "Monaco", "SF Mono"]);
    let _ = host.take_done();
    // Items gone through, none shown, the last once it rests.
    highlight(&mut host, &mut plugin, "family", 0);
    host.advance(std::time::Duration::from_millis(100));
    host.deliver(&mut plugin).unwrap();
    highlight(&mut host, &mut plugin, "family", 1);
    host.advance(std::time::Duration::from_millis(149));
    host.deliver(&mut plugin).unwrap();
    assert!(previews(&mut host).is_empty(), "none while the items pass");
    host.advance(std::time::Duration::from_millis(1));
    host.deliver(&mut plugin).unwrap();
    assert_eq!(
        previews(&mut host),
        [Request::PreviewFont(Some("Monaco".to_owned()))]
    );
    // Closed of no choice: the font in force back, nothing written.
    host.close_panel(&PanelId::from("choices:family"));
    host.deliver(&mut plugin).unwrap();
    assert_eq!(previews(&mut host), [Request::PreviewFont(None)]);
    // A face's style, of the regular face; a menu of no preview, none.
    menu_of(&mut host, &mut plugin, "style");
    let _ = host.take_done();
    highlight(&mut host, &mut plugin, "style", 2);
    host.advance(std::time::Duration::from_millis(150));
    host.deliver(&mut plugin).unwrap();
    assert_eq!(
        previews(&mut host),
        [Request::PreviewFontStyle {
            face: FontFace::Regular,
            style: "Bold".to_owned(),
        }]
    );
    // An item chosen before it rests shows no preview after the choice.
    highlight(&mut host, &mut plugin, "style", 1);
    pick(&mut host, &mut plugin, "style", 2);
    host.advance(std::time::Duration::from_millis(150));
    host.deliver(&mut plugin).unwrap();
    assert!(
        previews(&mut host).is_empty(),
        "no preview after the choice"
    );
    host.close_panel(&PanelId::from("choices:style"));
    host.deliver(&mut plugin).unwrap();
    let done = host.take_done();
    assert!(done.contains(&Request::PreviewFont(None)), "{done:?}");
    menu_of(&mut host, &mut plugin, "non-ascii-family");
    let _ = host.take_done();
    highlight(&mut host, &mut plugin, "non-ascii-family", 1);
    host.advance(std::time::Duration::from_millis(150));
    host.deliver(&mut plugin).unwrap();
    assert!(previews(&mut host).is_empty());
}

/// The themes of the tests' themes' page: two light ones and four dark
/// ones, one of them installed, the appearance in force dark.
fn many_themes() -> lokyr_plugin_sdk::host::Themes {
    use lokyr_plugin_sdk::host::{ThemeInfo, ThemeOrigin};
    let theme = |name: &str, variant| ThemeInfo {
        name: name.to_owned(),
        variant,
        origin: ThemeOrigin::BuiltIn,
        package: None,
    };
    lokyr_plugin_sdk::host::Themes {
        appearance: ThemeVariant::Dark,
        current: "Lokyr Dark".to_owned(),
        themes: vec![
            theme("Lokyr Light", ThemeVariant::Light),
            theme("Paper", ThemeVariant::Light),
            theme("Lokyr Dark", ThemeVariant::Dark),
            theme("Dracula", ThemeVariant::Dark),
            theme("Nord", ThemeVariant::Dark),
            ThemeInfo {
                origin: ThemeOrigin::Installed,
                package: Some("acme/tides".to_owned()),
                ..theme("Night", ThemeVariant::Dark)
            },
        ],
        loading: false,
    }
}

#[test]
fn the_themes_page_shows_the_themes_of_an_appearance_of_its_sidebar_the_one_in_force_first() {
    let (mut host, mut plugin) = host();
    host.set_themes(many_themes());
    host.show_page(THEMES);
    host.deliver(&mut plugin).unwrap();
    let tree = host.render(&mut plugin, &themes_page()).expect("the page");
    // The sidebar of the appearances, the dark one first, as in force.
    let Kind::List(list) = &find_node(&tree, VARIANTS).expect("the sidebar").kind else {
        panic!("a list");
    };
    let items: Vec<Vec<String>> = list.items.iter().map(|item| item.texts.clone()).collect();
    assert_eq!(
        items,
        [["circle-filled", "themes-dark"], ["circle", "themes-light"]]
            .map(|texts| texts.map(str::to_owned).to_vec())
    );
    // The dark themes alone, in rows of three, the last of the room of the
    // cards that it lacks.
    assert_eq!(text_of(&tree, "title").as_deref(), Some("themes-dark"));
    let dark = find_node(&tree, "themes-row:dark:0").unwrap();
    assert_eq!(dark.children.len(), CARDS_ACROSS);
    let last = find_node(&tree, "themes-row:dark:1").unwrap();
    assert_eq!(last.children.len(), CARDS_ACROSS);
    assert!(find_node(&tree, "theme:light:Paper").is_none());
    // Each card of a preview of its theme and of its name; the one that
    // `[theme]` names of the accent's border.
    let card = find_node(&tree, "theme:dark:Dracula").expect("Dracula's card");
    assert!(matches!(&card.children[0].kind, Kind::ThemePreview(name) if name == "Dracula"));
    // An installed one's of its package's name in its tooltip.
    assert_eq!(card.tooltip, None);
    let installed = find_node(&tree, "theme:dark:Night").expect("Night's card");
    assert_eq!(installed.tooltip.as_deref(), Some("acme/tides"));
    let border = |tree: &Node, key: &str| {
        find_node(tree, key)
            .and_then(|node| node.style.border.as_ref())
            .map(|border| border.color.0.clone())
    };
    assert_eq!(
        border(&tree, "theme:dark:Lokyr Dark").as_deref(),
        Some("accent")
    );
    assert_eq!(
        border(&tree, "theme:dark:Dracula").as_deref(),
        Some("separator")
    );
    // The light ones, of the sidebar's second item.
    host.ui_event(
        &mut plugin,
        &UiEvent {
            view: themes_page(),
            key: VARIANTS.into(),
            kind: UiEventKind::Select { item: 1 },
        },
    )
    .unwrap();
    let tree = host.render(&mut plugin, &themes_page()).unwrap();
    assert_eq!(text_of(&tree, "title").as_deref(), Some("themes-light"));
    assert!(find_node(&tree, "theme:dark:Dracula").is_none());
    assert_eq!(
        border(&tree, "theme:light:Lokyr Light").as_deref(),
        Some("accent")
    );
}

#[test]
fn the_themes_page_shows_a_progress_while_their_list_is_read() {
    let (mut host, mut plugin) = host();
    let mut reading = many_themes();
    reading.loading = true;
    host.set_themes(reading);
    host.show_page(THEMES);
    host.deliver(&mut plugin).unwrap();
    let tree = host.render(&mut plugin, &themes_page()).unwrap();
    assert!(
        find_node(&tree, "themes-loading").is_some(),
        "{}",
        snapshot(&tree)
    );
    assert!(find_node(&tree, "themes-row:dark:0").is_none());
    // The list read: the plugin hears it and shows the cards.
    let _ = host.take_done();
    host.set_themes(many_themes());
    host.deliver(&mut plugin).unwrap();
    assert!(host.take_done().iter().any(|request| matches!(
        request,
        Request::RequestRender(view) if *view == themes_page()
    )));
    let tree = host.render(&mut plugin, &themes_page()).unwrap();
    assert!(find_node(&tree, "themes-loading").is_none());
    assert!(find_node(&tree, "theme:dark:Dracula").is_some());
}

#[test]
fn a_theme_s_card_names_it_in_the_config_for_its_appearance() {
    let (mut host, mut plugin) = host();
    host.set_themes(many_themes());
    host.show_page(THEMES);
    host.deliver(&mut plugin).unwrap();
    host.render(&mut plugin, &themes_page());
    let _ = host.take_done();
    let click = |key: &str| UiEvent {
        view: themes_page(),
        key: key.into(),
        kind: UiEventKind::Click,
    };
    host.ui_event(&mut plugin, &click("theme:dark:Nord"))
        .unwrap();
    host.ui_event(&mut plugin, &click("theme:light:Paper"))
        .unwrap();
    let chosen: Vec<Request> = host
        .take_done()
        .into_iter()
        .filter(|request| matches!(request, Request::SetTheme { .. }))
        .collect();
    assert_eq!(
        chosen,
        [
            Request::SetTheme {
                variant: ThemeVariant::Dark,
                name: "Nord".to_owned(),
            },
            Request::SetTheme {
                variant: ThemeVariant::Light,
                name: "Paper".to_owned(),
            },
        ]
    );
}
