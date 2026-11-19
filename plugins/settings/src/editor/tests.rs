use lokyr_plugin_sdk::fake::{FakeHost, theme_tokens};
use lokyr_plugin_sdk::host::{ThemeTokens, TokenOrigin};
use lokyr_plugin_sdk::message::{Event, Request};
use lokyr_plugin_sdk::theme::ThemeColor;
use lokyr_plugin_sdk::view::Kind;

use super::*;
use crate::SettingsPage;

/// A host of the settings' plugin, its themes page showing its Editor.
fn shown() -> (FakeHost, SettingsPage) {
    let mut host = FakeHost::new("lokyr/settings").with_manifest(crate::MANIFEST);
    // A window, which the settings' window opens beside.
    host.pane(lokyr_plugin_sdk::host::PaneId(1));
    let mut plugin = SettingsPage::default();
    host.activate(&mut plugin).unwrap();
    host.show_page(crate::THEMES);
    host.deliver(&mut plugin).unwrap();
    host.render(&mut plugin, &themes_page()).unwrap();
    host.ui_event(&mut plugin, &click("themes-side:editor"))
        .unwrap();
    (host, plugin)
}

/// The page's tree, rendered anew.
fn page(host: &mut FakeHost, plugin: &mut SettingsPage) -> Node {
    host.render(plugin, &themes_page()).expect("the page")
}

/// The node of `key` in `tree`.
fn find<'t>(tree: &'t Node, key: &str) -> Option<&'t Node> {
    if tree.key.as_ref().is_some_and(|own| own.0 == key) {
        return Some(tree);
    }
    tree.children.iter().find_map(|child| find(child, key))
}

/// The texts of `tree`, in their order.
fn texts(tree: &Node) -> Vec<String> {
    let mut out = Vec::new();
    if let Kind::Text(text) = &tree.kind {
        out.push(text.clone());
    }
    for child in &tree.children {
        out.extend(texts(child));
    }
    out
}

/// The names of the tokens' rows of `tree`.
fn rows(tree: &Node) -> Vec<String> {
    let mut out = Vec::new();
    let mut walk = vec![tree];
    while let Some(node) = walk.pop() {
        if let Some(name) = node
            .key
            .as_ref()
            .and_then(|key| key.0.strip_prefix("editor-token:"))
        {
            out.push(name.to_owned());
        }
        walk.extend(node.children.iter().rev());
    }
    out
}

/// A click on the node of `key` of the themes page.
fn click(key: &str) -> UiEvent {
    UiEvent {
        view: themes_page(),
        key: key.into(),
        kind: UiEventKind::Click,
    }
}

#[test]
fn the_page_lists_every_token_by_groups_of_its_color_value_and_origin() {
    let (mut host, mut plugin) = shown();
    let tree = page(&mut host, &mut plugin);
    // The terminal's colors first, its keys' then its palette's, then
    // the tokens, in their order, each group under its heading.
    assert_eq!(
        rows(&tree),
        [
            "background",
            "cursor",
            "bright-blue",
            "accent",
            "focus.ring",
            "command-header.pill"
        ]
    );
    for group in [
        "[terminal]",
        "[terminal] palette",
        "accent",
        "focus",
        "command-header",
    ] {
        assert!(
            find(&tree, &format!("editor-group:{group}")).is_some(),
            "{group}"
        );
    }
    // Each one's value under its name.
    let all = texts(&tree);
    for text in ["#1e1e20", "#e6e6e6", "#86bcff", "#4c9bff", "#2a2a2d"] {
        assert!(all.iter().any(|shown| shown == text), "{text}: {all:?}");
    }
    // The swatch of each, of its color, of where it comes from as its
    // tooltip: its theme, its file's line or its rule.
    for (name, origin) in [
        ("background", "Lokyr Dark"),
        ("bright-blue", "Lokyr Dark.toml:8"),
        ("focus.ring", "editor-rule"),
        ("command-header.pill", "Lokyr Dark.toml:3"),
    ] {
        let swatch = find(&tree, &format!("editor-swatch:{name}")).unwrap();
        assert_eq!(swatch.tooltip.as_deref(), Some(origin), "{name}");
    }
    let swatch = find(&tree, "editor-swatch:command-header.pill").unwrap();
    assert_eq!(
        swatch.children[0].style.background,
        Some(Token::from("#2a2a2d"))
    );
    // The theme in force's name on its button.
    assert!(find(&tree, THEME).is_some());
}

#[test]
fn the_filter_keeps_the_tokens_of_its_words_and_empties() {
    let (mut host, mut plugin) = shown();
    let typed = |text: &str| UiEvent {
        view: themes_page(),
        key: FILTER.into(),
        kind: UiEventKind::InputChanged {
            text: text.to_owned(),
            revision: 1,
        },
    };
    host.ui_event(&mut plugin, &typed("FOCUS")).unwrap();
    assert_eq!(rows(&page(&mut host, &mut plugin)), ["focus.ring"]);
    // Of a value, or of an origin.
    host.ui_event(&mut plugin, &typed("2a2a2d")).unwrap();
    assert_eq!(rows(&page(&mut host, &mut plugin)), ["command-header.pill"]);
    host.ui_event(&mut plugin, &typed("rule")).unwrap();
    assert_eq!(
        rows(&page(&mut host, &mut plugin)),
        ["cursor", "focus.ring"]
    );
    // Emptied, as its clear button or Escape tells it.
    host.ui_event(&mut plugin, &typed("")).unwrap();
    assert_eq!(rows(&page(&mut host, &mut plugin)).len(), 6);
}

#[test]
fn a_token_of_a_file_opens_at_its_line_and_another_copies_its_line() {
    let (mut host, mut plugin) = shown();
    let _ = host.take_done();
    host.ui_event(&mut plugin, &click("editor-act:command-header.pill"))
        .unwrap();
    assert!(host.take_done().iter().any(|request| matches!(
        request,
        Request::OpenAt { target, line: 3, .. } if target == "/themes/Lokyr Dark.toml"
    )));
    host.ui_event(&mut plugin, &click("editor-act:focus.ring"))
        .unwrap();
    assert_eq!(host.clipboard(), Some("\"focus.ring\" = \"accent / 60%\""));
    // A key of `[terminal]` of a theme of no file, its bare key.
    host.ui_event(&mut plugin, &click("editor-act:background"))
        .unwrap();
    assert_eq!(host.clipboard(), Some("background = \"#1e1e20\""));
    // A palette's color of a file, at its line.
    host.ui_event(&mut plugin, &click("editor-act:bright-blue"))
        .unwrap();
    assert!(
        host.take_done()
            .iter()
            .any(|request| matches!(request, Request::OpenAt { line: 8, .. }))
    );
    // Within a second of the last notice: copied, of no notice more.
    host.ui_event(&mut plugin, &click("editor-act:accent"))
        .unwrap();
    assert_eq!(host.clipboard(), Some("\"accent\" = \"#4c9bff\""));
    assert_eq!(host.notices().len(), 1);
}

#[test]
fn a_change_of_the_theme_lights_the_tokens_that_it_changed_for_a_while() {
    let (mut host, mut plugin) = shown();
    let mut changed: Vec<ThemeTokens> = theme_tokens();
    changed[0].tokens[0].color = ThemeColor::hex(0xc678dd);
    host.set_theme_tokens(changed);
    host.deliver(&mut plugin).unwrap();
    let tree = page(&mut host, &mut plugin);
    let lit = |tree: &Node, name: &str| {
        find(tree, &format!("editor-token:{name}"))
            .unwrap()
            .style
            .background
            .is_some()
    };
    assert!(lit(&tree, "accent") && !lit(&tree, "focus.ring"));
    let row = find(&tree, "editor-token:accent").unwrap();
    assert!(texts(row).iter().any(|text| text == "#c678dd"));
    // Its light goes as its timer fires.
    host.advance(LIT);
    host.deliver(&mut plugin).unwrap();
    assert!(!lit(&page(&mut host, &mut plugin), "accent"));
}

/// The themes of each kind, of `current` in force for the dark
/// appearance: Dracula held, Lokyr's built in, Night an `.itermcolors` file
/// of the user's folder and Paper one of Lokyr's format.
fn kinds(current: &str) -> lokyr_plugin_sdk::host::Themes {
    use lokyr_plugin_sdk::host::{ThemeFormat, ThemeInfo, ThemeOrigin, Themes};
    let theme = |name: &str, variant, origin, format| ThemeInfo {
        name: name.to_owned(),
        variant,
        origin,
        format,
    };
    let lokyr = ThemeFormat::Lokyr;
    Themes {
        appearance: ThemeVariant::Dark,
        current: current.to_owned(),
        themes: vec![
            theme("Dracula", ThemeVariant::Dark, ThemeOrigin::Held, lokyr),
            theme(
                "Lokyr Dark",
                ThemeVariant::Dark,
                ThemeOrigin::BuiltIn,
                lokyr,
            ),
            theme(
                "Lokyr Light",
                ThemeVariant::Light,
                ThemeOrigin::BuiltIn,
                lokyr,
            ),
            theme(
                "Night",
                ThemeVariant::Dark,
                ThemeOrigin::Folder,
                ThemeFormat::Itermcolors,
            ),
            theme("Paper", ThemeVariant::Light, ThemeOrigin::Folder, lokyr),
        ],
        loading: false,
    }
}

/// Chooses the theme `name` of the Editor's menu.
fn choose(host: &mut FakeHost, plugin: &mut SettingsPage, name: &str) {
    page(host, plugin);
    host.ui_event(plugin, &click(THEME)).unwrap();
    host.render(plugin, &ViewId::Panel(PanelId::from(THEMES_MENU)))
        .unwrap();
    let item = UiEvent {
        view: ViewId::Panel(PanelId::from(THEMES_MENU)),
        key: format!("{THEME_ITEM}{name}").as_str().into(),
        kind: UiEventKind::Toggle { on: true },
    };
    host.ui_event(plugin, &item).unwrap();
}

/// The tooltip of the lock beside the menu, if it shows.
fn lock(tree: &Node) -> Option<String> {
    find(tree, LOCK).map(|node| node.tooltip.clone().unwrap_or_default())
}

/// Answers the question `id` of `text`, as the user does.
fn answer(host: &mut FakeHost, plugin: &mut SettingsPage, id: u64, text: Option<&str>) {
    let _ = host.take_done();
    host.post(Event::Answered {
        id,
        text: text.map(str::to_owned),
    });
    host.deliver(plugin).unwrap();
}

#[test]
fn a_read_only_theme_shows_a_lock_that_duplicates_it_and_shows_its_copy() {
    let (mut host, mut plugin) = shown();
    host.set_themes(kinds("Lokyr Dark"));
    host.deliver(&mut plugin).unwrap();
    // The theme in force, built in: read-only.
    let tree = page(&mut host, &mut plugin);
    assert_eq!(lock(&tree).as_deref(), Some("editor-read-only"));
    // Of the folder, of Lokyr's format: no lock.
    choose(&mut host, &mut plugin, "Paper");
    assert_eq!(lock(&page(&mut host, &mut plugin)), None);
    // Held: the lock asks a copy's name.
    choose(&mut host, &mut plugin, "Dracula");
    assert_eq!(
        lock(&page(&mut host, &mut plugin)).as_deref(),
        Some("editor-read-only")
    );
    let _ = host.take_done();
    host.ui_event(&mut plugin, &click(LOCK)).unwrap();
    assert!(
        host.take_done()
            .contains(&Request::CopyTheme("Dracula".to_owned()))
    );
    // One question at a time.
    host.ui_event(&mut plugin, &click(LOCK)).unwrap();
    assert!(host.take_done().is_empty());
    // Cancelled, the theme stays; made, its copy shows.
    let id = plugin.editor.copying.unwrap();
    answer(&mut host, &mut plugin, id, None);
    assert!(host.take_done().is_empty());
    host.ui_event(&mut plugin, &click(LOCK)).unwrap();
    let id = plugin.editor.copying.unwrap();
    answer(&mut host, &mut plugin, id, Some("Dracula Copy"));
    assert!(!previews(&host.take_done()));
    assert_eq!(plugin.editor.shown_theme(), Some("Dracula Copy"));
    // A copy that the config names for the appearance in force shows as
    // the theme in force.
    choose(&mut host, &mut plugin, "Dracula");
    host.ui_event(&mut plugin, &click(LOCK)).unwrap();
    let id = plugin.editor.copying.unwrap();
    let mut named = kinds("Mine");
    named.themes.push(lokyr_plugin_sdk::host::ThemeInfo {
        name: "Mine".to_owned(),
        variant: ThemeVariant::Dark,
        origin: lokyr_plugin_sdk::host::ThemeOrigin::Folder,
        format: lokyr_plugin_sdk::host::ThemeFormat::Lokyr,
    });
    host.set_themes(named);
    // The tokens of the theme in force, the copy's, as the config names it.
    let mut tokens = theme_tokens();
    tokens[0].theme = "Mine".to_owned();
    host.set_theme_tokens(tokens);
    answer(&mut host, &mut plugin, id, Some("Mine"));
    assert!(!previews(&host.take_done()));
    assert_eq!(plugin.editor.chosen, None);
    assert_eq!(plugin.editor.shown_theme(), Some("Mine"));
    assert_eq!(lock(&page(&mut host, &mut plugin)), None);
}

#[test]
fn an_itermcolors_theme_asks_for_its_conversion_as_it_shows_and_of_its_lock() {
    let (mut host, mut plugin) = shown();
    host.set_themes(kinds("Lokyr Dark"));
    host.deliver(&mut plugin).unwrap();
    let _ = host.take_done();
    // Shown of the menu: its conversion asked at once, once.
    choose(&mut host, &mut plugin, "Night");
    let convert = Request::ConvertTheme("Night".to_owned());
    assert!(host.take_done().contains(&convert));
    assert_eq!(
        lock(&page(&mut host, &mut plugin)).as_deref(),
        Some("editor-itermcolors")
    );
    // Cancelled: not asked again as it shows on, but of its lock.
    let id = plugin.editor.converting.unwrap();
    answer(&mut host, &mut plugin, id, None);
    choose(&mut host, &mut plugin, "Night");
    assert!(!host.take_done().contains(&convert));
    host.ui_event(&mut plugin, &click(LOCK)).unwrap();
    assert!(host.take_done().contains(&convert));
    // Converted: its tokens read anew, of none before, as the host's
    // fixture had none; the lock gone as the list tells its new format.
    assert_eq!(plugin.editor.tokens, None);
    let mut tokens = theme_tokens();
    let mut night = tokens[0].clone();
    night.theme = "Night".to_owned();
    tokens.push(night);
    host.set_theme_tokens(tokens);
    let id = plugin.editor.converting.unwrap();
    answer(&mut host, &mut plugin, id, Some("Night"));
    assert_eq!(
        plugin
            .editor
            .tokens
            .as_ref()
            .map(|tokens| tokens.theme.as_str()),
        Some("Night")
    );
    let mut converted = kinds("Lokyr Dark");
    converted.themes[3].format = lokyr_plugin_sdk::host::ThemeFormat::Lokyr;
    host.set_themes(converted);
    host.deliver(&mut plugin).unwrap();
    assert_eq!(lock(&page(&mut host, &mut plugin)), None);
    // Another one shown, then it again: asked again.
    choose(&mut host, &mut plugin, "Paper");
    host.set_themes(kinds("Lokyr Dark"));
    host.deliver(&mut plugin).unwrap();
    let _ = host.take_done();
    choose(&mut host, &mut plugin, "Night");
    assert!(host.take_done().contains(&convert));
}

#[test]
fn the_editor_entered_of_an_itermcolors_theme_in_force_asks_for_its_conversion() {
    let mut host = FakeHost::new("lokyr/settings").with_manifest(crate::MANIFEST);
    host.pane(lokyr_plugin_sdk::host::PaneId(1));
    host.set_themes(kinds("Night"));
    // The tokens of the theme in force, Night's.
    let mut tokens = theme_tokens();
    tokens[0].theme = "Night".to_owned();
    host.set_theme_tokens(tokens);
    let mut plugin = SettingsPage::default();
    host.activate(&mut plugin).unwrap();
    host.show_page(crate::THEMES);
    host.deliver(&mut plugin).unwrap();
    host.render(&mut plugin, &themes_page()).unwrap();
    let _ = host.take_done();
    host.ui_event(&mut plugin, &click("themes-side:editor"))
        .unwrap();
    assert!(
        host.take_done()
            .contains(&Request::ConvertTheme("Night".to_owned()))
    );
}

#[test]
fn the_theme_s_button_is_a_menu_of_the_user_s_themes_then_of_each_appearance_s() {
    let (mut host, mut plugin) = shown();
    host.set_themes(kinds("Lokyr Dark"));
    host.deliver(&mut plugin).unwrap();
    // The button: the theme shown, the one in force, of a menu's chevron.
    let tree = page(&mut host, &mut plugin);
    match find(&tree, THEME).map(|node| &node.kind) {
        Some(Kind::Button(button)) => {
            assert_eq!(button.label, "Lokyr Dark");
            assert_eq!(button.icon, "chevron-down");
        }
        other => panic!("{other:?}"),
    }
    host.ui_event(&mut plugin, &click(THEME)).unwrap();
    let menu = host
        .render(&mut plugin, &ViewId::Panel(PanelId::from(THEMES_MENU)))
        .unwrap();
    // Each item a toggle of its theme's name, the one shown ticked.
    let item = |node: &Node| match &node.kind {
        Kind::Toggle(toggle) => format!("{}{}", toggle.label, if toggle.on { " ✓" } else { "" }),
        Kind::Separator => "—".to_owned(),
        Kind::Container => {
            let mut words = texts(node);
            words.truncate(1);
            for child in &node.children[1..] {
                if let Kind::Toggle(toggle) = &child.kind {
                    words.push(format!(
                        "{}{}",
                        toggle.label,
                        if toggle.on { " ✓" } else { "" }
                    ));
                }
            }
            format!("[{}]", words.join(", "))
        }
        other => format!("{other:?}"),
    };
    let items: Vec<String> = menu.children.iter().map(item).collect();
    assert_eq!(
        items,
        [
            "Night",
            "Paper",
            "—",
            "[themes-light, Lokyr Light]",
            "[themes-dark, Dracula, Lokyr Dark ✓]",
        ]
    );
}

#[test]
fn the_menu_previews_a_theme_and_the_page_hidden_ends_it() {
    let (mut host, mut plugin) = shown();
    page(&mut host, &mut plugin);
    host.ui_event(&mut plugin, &click(THEME)).unwrap();
    assert!(host.panels().contains_key(&PanelId::from(THEMES_MENU)));
    let menu = host
        .render(&mut plugin, &ViewId::Panel(PanelId::from(THEMES_MENU)))
        .unwrap();
    let _ = host.take_done();
    let item_of = |name: &str| UiEvent {
        view: ViewId::Panel(PanelId::from(THEMES_MENU)),
        key: format!("{THEME_ITEM}{name}").as_str().into(),
        kind: UiEventKind::Toggle { on: true },
    };
    // The theme in force chosen: shown as it is.
    assert!(find(&menu, &format!("{THEME_ITEM}Lokyr Dark")).is_some());
    host.ui_event(&mut plugin, &item_of("Lokyr Dark")).unwrap();
    assert!(!previews(&host.take_done()));
    assert_eq!(plugin.editor.chosen, None);
    // Another: the Editor's previews of it, of a part of its name, the app
    // previewing none.
    let item = item_of("Lokyr Light");
    host.ui_event(&mut plugin, &item).unwrap();
    assert!(!previews(&host.take_done()));
    assert_eq!(plugin.editor.shown_theme(), Some("Lokyr Light"));
    let shown = page(&mut host, &mut plugin);
    assert_eq!(part_theme(&shown).as_deref(), Some("Lokyr Light"));
    // An appearance's themes shown, the theme chosen is forgotten; so it
    // is of the page hidden.
    host.ui_event(&mut plugin, &click("themes-side:dark"))
        .unwrap();
    assert!(!previews(&host.take_done()));
    assert_eq!(plugin.editor.chosen, None);
    host.ui_event(&mut plugin, &click("themes-side:editor"))
        .unwrap();
    host.render(&mut plugin, &themes_page()).unwrap();
    host.ui_event(&mut plugin, &click(THEME)).unwrap();
    host.ui_event(&mut plugin, &item).unwrap();
    assert_eq!(plugin.editor.shown_theme(), Some("Lokyr Light"));
    host.hide_page();
    host.deliver(&mut plugin).unwrap();
    assert!(!previews(&host.take_done()));
    assert_eq!(plugin.editor.chosen, None);
}

/// Whether `done` holds a theme previewed in the app, which the Editor
/// asks never.
fn previews(done: &[Request]) -> bool {
    done.iter()
        .any(|request| matches!(request, Request::PreviewTheme(_)))
}

/// The name of the theme of the part that `tree` holds, if it holds one.
fn part_theme(tree: &Node) -> Option<String> {
    let mut left = vec![tree];
    while let Some(node) = left.pop() {
        if let Kind::Themed(name) = &node.kind {
            return Some(name.clone());
        }
        left.extend(&node.children);
    }
    None
}

#[test]
fn the_previews_parts_show_and_a_pick_marks_a_part_s_tokens_or_a_token_s_parts() {
    let (mut host, mut plugin) = shown();
    let tree = page(&mut host, &mut plugin);
    // The window's preview and the panel's, each a part of the theme
    // shown, which hold every part between them, the terminal's specimen
    // in the window's.
    let theme = plugin.editor.shown_theme().unwrap().to_owned();
    for preview in [parts::WINDOW, parts::PANEL] {
        let part = find(&tree, preview).expect(preview);
        assert_eq!(part.kind, Kind::Themed(theme.clone()), "{preview}");
    }
    let within = |preview: &str, id: &str| {
        find(&tree, preview)
            .and_then(|part| find(part, &format!("{}{id}", parts::PART)))
            .is_some()
    };
    for (id, ..) in parts::PARTS {
        let window = ["tabs", "terminal", "scrollbar", "panes"].contains(&id);
        assert_eq!(within(parts::WINDOW, id), window, "{id}");
        assert_eq!(within(parts::PANEL, id), !window, "{id}");
    }
    let terminal = find(&tree, "editor-part:terminal").unwrap();
    assert!(matches!(
        find(terminal, "editor-terminal").map(|node| &node.kind),
        Some(Kind::TerminalSpecimen)
    ));
    let shaded = |tree: &Node, name: &str| {
        find(tree, &format!("editor-token:{name}")).and_then(|row| row.style.background.clone())
    };
    let bordered = |tree: &Node, id: &str| {
        find(tree, &format!("{}{id}", parts::PART))
            .and_then(|card| card.style.border.as_ref())
            .map(|border| border.color.0.clone())
    };
    // A part picked: its tokens' rows marked, it of the accent's border.
    let fake = theme_tokens();
    let mut fake = fake[0].clone();
    fake.tokens.push(lokyr_plugin_sdk::host::ThemeToken {
        name: "tooltip.bg".to_owned(),
        color: ThemeColor::hex(0x323235),
        origin: TokenOrigin::Rule {
            text: "mix(background, foreground, 10%)".to_owned(),
        },
        value: None,
    });
    host.set_theme_tokens(vec![fake]);
    host.deliver(&mut plugin).unwrap();
    // The new token's light gone first, which a mark yields to.
    host.advance(LIT);
    host.deliver(&mut plugin).unwrap();
    host.ui_event(&mut plugin, &click("editor-part:tooltip"))
        .unwrap();
    let tree = page(&mut host, &mut plugin);
    assert_eq!(
        shaded(&tree, "tooltip.bg"),
        Some(Token::from("list.selection.inactive.bg"))
    );
    assert_eq!(shaded(&tree, "accent"), None);
    assert_eq!(bordered(&tree, "tooltip").as_deref(), Some("accent"));
    assert_eq!(bordered(&tree, "panel").as_deref(), Some("transparent"));
    // A token picked: the parts that take it of the accent's border.
    host.ui_event(&mut plugin, &click("editor-token:accent"))
        .unwrap();
    let tree = page(&mut host, &mut plugin);
    assert_eq!(bordered(&tree, "buttons").as_deref(), Some("accent"));
    assert_eq!(bordered(&tree, "tooltip").as_deref(), Some("transparent"));
    assert_eq!(
        shaded(&tree, "accent"),
        Some(Token::from("list.selection.inactive.bg"))
    );
    // Picked again, none.
    host.ui_event(&mut plugin, &click("editor-token:accent"))
        .unwrap();
    let tree = page(&mut host, &mut plugin);
    assert_eq!(bordered(&tree, "buttons").as_deref(), Some("transparent"));
    assert_eq!(shaded(&tree, "accent"), None);
}

#[test]
fn the_panel_s_controls_take_the_pointer_and_mark_their_parts() {
    let (mut host, mut plugin) = shown();
    let of = |key: &str, kind: UiEventKind| UiEvent {
        view: themes_page(),
        key: key.into(),
        kind,
    };
    let bordered = |tree: &Node, id: &str| {
        find(tree, &format!("{}{id}", parts::PART))
            .and_then(|part| part.style.border.as_ref())
            .map(|border| border.color.0.clone())
    };
    let on = |tree: &Node, key: &str| match find(tree, key).map(|node| &node.kind) {
        Some(Kind::Toggle(toggle)) => toggle.on,
        other => panic!("{key}: {other:?}"),
    };
    // Its toggles as they start; one turned off stays off, its part marked.
    let tree = page(&mut host, &mut plugin);
    assert!(on(&tree, "editor-sample:toggles:compress"));
    assert!(!on(&tree, "editor-sample:toggles:encrypt"));
    host.ui_event(
        &mut plugin,
        &of(
            "editor-sample:toggles:compress",
            UiEventKind::Toggle { on: false },
        ),
    )
    .unwrap();
    let tree = page(&mut host, &mut plugin);
    assert!(!on(&tree, "editor-sample:toggles:compress"));
    assert_eq!(bordered(&tree, "toggles").as_deref(), Some("accent"));
    // A segment chosen, kept; a button's click and a field's focus, their
    // parts, each in place of the last.
    host.ui_event(
        &mut plugin,
        &of(
            "editor-sample:segments:pages",
            UiEventKind::Select { item: 2 },
        ),
    )
    .unwrap();
    let tree = page(&mut host, &mut plugin);
    assert!(matches!(
        find(&tree, "editor-sample:segments:pages").map(|node| &node.kind),
        Some(Kind::Tabs(tabs)) if tabs.chosen == 2
    ));
    assert_eq!(bordered(&tree, "segments").as_deref(), Some("accent"));
    assert_eq!(bordered(&tree, "toggles").as_deref(), Some("transparent"));
    for (key, kind, part) in [
        (
            "editor-sample:buttons:cancel",
            UiEventKind::Click,
            "buttons",
        ),
        ("editor-sample:field:folder", UiEventKind::Focus, "field"),
        (
            "editor-sample:scrollbar:log",
            UiEventKind::Click,
            "scrollbar",
        ),
    ] {
        host.ui_event(&mut plugin, &of(key, kind)).unwrap();
        let tree = page(&mut host, &mut plugin);
        assert_eq!(bordered(&tree, part).as_deref(), Some("accent"), "{key}");
    }
    // Its click again leaves its part marked, as a control is clicked
    // again and again.
    host.ui_event(
        &mut plugin,
        &of("editor-sample:scrollbar:log", UiEventKind::Click),
    )
    .unwrap();
    let tree = page(&mut host, &mut plugin);
    assert_eq!(bordered(&tree, "scrollbar").as_deref(), Some("accent"));
    // Nothing asked of the app but renders.
    assert!(!previews(&host.take_done()));
}

#[test]
fn every_part_s_tokens_are_tokens_of_lokyr_s() {
    for (id, _, tokens) in parts::PARTS {
        for token in tokens {
            assert!(
                lokyr_plugin_sdk::theme::TOKENS.contains(token),
                "{id}: {token}"
            );
        }
    }
    for (_, title, _) in parts::PARTS {
        assert!(parts::WORDS.contains(&title), "{title}");
    }
    // Every token of Lokyr's rules in a part, which shows it.
    for token in lokyr_plugin_sdk::theme::rules::tokens() {
        assert!(
            parts::PARTS
                .iter()
                .any(|(_, _, tokens)| tokens.contains(&token)),
            "{token}"
        );
    }
    // Every message of the previews in both catalogs.
    for (language, catalog) in [
        ("en", include_str!("../../locales/en/settings.ftl")),
        ("it", include_str!("../../locales/it/settings.ftl")),
    ] {
        for word in parts::WORDS {
            assert!(
                catalog
                    .lines()
                    .any(|line| line.starts_with(&format!("{word} = "))),
                "{language}: {word}"
            );
        }
    }
}

/// The tokens of the fake host's themes and of Paper, of the folder: its
/// accent and background of its own file, its focus ring of its rule, its
/// bright blue of its palette.
fn paper_tokens() -> Vec<ThemeTokens> {
    let mut all = theme_tokens();
    let mut paper = all[0].clone();
    paper.theme = "Paper".to_owned();
    paper.variant = ThemeVariant::Light;
    let own = |line: u32| TokenOrigin::File {
        path: "/themes/Paper.toml".to_owned(),
        line,
        theme: "Paper".to_owned(),
    };
    for token in paper.terminal.iter_mut().chain(paper.tokens.iter_mut()) {
        if matches!(token.name.as_str(), "background" | "bright-blue" | "accent") {
            token.origin = own(4);
            token.value = Some(token.color.to_string());
        }
    }
    all.push(paper);
    all
}

/// The page's tree of Paper chosen in the Editor, of its color `name`'s
/// row chosen.
fn editing(name: &str) -> (FakeHost, SettingsPage) {
    let (mut host, mut plugin) = shown();
    host.set_themes(kinds("Lokyr Dark"));
    host.set_theme_tokens(paper_tokens());
    host.deliver(&mut plugin).unwrap();
    choose(&mut host, &mut plugin, "Paper");
    page(&mut host, &mut plugin);
    host.ui_event(&mut plugin, &click(&format!("{ROW}{name}")))
        .unwrap();
    let _ = host.take_done();
    (host, plugin)
}

/// The field event `kind` of the color `name`'s field.
fn typed(name: &str, kind: UiEventKind) -> UiEvent {
    UiEvent {
        view: themes_page(),
        key: format!("{VALUE}{name}").as_str().into(),
        kind,
    }
}

/// The request that sets the color `name` of Paper to `value`.
fn set_paper(name: &str, value: Option<&str>) -> Request {
    Request::SetThemeValue {
        theme: "Paper".to_owned(),
        name: name.to_owned(),
        value: value.map(str::to_owned),
    }
}

#[test]
fn a_chosen_row_of_a_theme_of_the_folder_holds_its_value_s_field_and_reset() {
    let (mut host, mut plugin) = editing("accent");
    let tree = page(&mut host, &mut plugin);
    // Reset of each color, dimmed while it has its value as the Editor
    // showed the theme.
    let dimmed = |tree: &Node, name: &str| match find(tree, &format!("{RESET}{name}"))
        .map(|node| &node.kind)
    {
        Some(Kind::Button(button)) => button.disabled,
        other => panic!("{name}: {other:?}"),
    };
    // Its value as the file writes it.
    match find(&tree, &format!("{VALUE}accent")).map(|node| &node.kind) {
        Some(Kind::Field(field)) => assert_eq!(field.text.as_deref(), Some("#4c9bff")),
        other => panic!("{other:?}"),
    }
    assert!(dimmed(&tree, "accent"));
    // A token of its rule: an empty field of the rule's text.
    host.ui_event(&mut plugin, &click(&format!("{ROW}focus.ring")))
        .unwrap();
    let tree = page(&mut host, &mut plugin);
    assert!(find(&tree, &format!("{VALUE}accent")).is_none());
    match find(&tree, &format!("{VALUE}focus.ring")).map(|node| &node.kind) {
        Some(Kind::Field(field)) => {
            assert_eq!(field.text.as_deref(), Some(""));
            assert_eq!(field.placeholder, "accent / 60%");
        }
        other => panic!("{other:?}"),
    }
    assert!(dimmed(&tree, "focus.ring"));
    // The background and a palette's color too.
    for name in ["background", "bright-blue"] {
        host.ui_event(&mut plugin, &click(&format!("{ROW}{name}")))
            .unwrap();
        let tree = page(&mut host, &mut plugin);
        assert!(find(&tree, &format!("{VALUE}{name}")).is_some());
        assert!(dimmed(&tree, name), "{name}");
    }
    // A read-only theme's row: no field.
    choose(&mut host, &mut plugin, "Lokyr Dark");
    let tree = page(&mut host, &mut plugin);
    host.ui_event(&mut plugin, &click(&format!("{ROW}accent")))
        .unwrap();
    assert!(find(&tree, &format!("{VALUE}accent")).is_none());
    assert!(find(&page(&mut host, &mut plugin), &format!("{VALUE}accent")).is_none());
}

/// The values that the plugin wrote since the last look, its other
/// requests left out.
fn written(host: &mut FakeHost) -> Vec<Request> {
    host.take_done()
        .into_iter()
        .filter(|request| matches!(request, Request::SetThemeValue { .. }))
        .collect()
}

#[test]
fn a_value_typed_is_written_as_typing_pauses_on_return_and_as_its_field_is_left() {
    let (mut host, mut plugin) = editing("accent");
    page(&mut host, &mut plugin);
    let changed = |text: &str| UiEventKind::InputChanged {
        text: text.to_owned(),
        revision: 1,
    };
    // Paused: a whole value written.
    host.ui_event(&mut plugin, &typed("accent", changed("#00ff00")))
        .unwrap();
    assert!(written(&mut host).is_empty());
    host.advance(PAUSE);
    host.deliver(&mut plugin).unwrap();
    assert_eq!(written(&mut host), [set_paper("accent", Some("#00ff00"))]);
    // Half typed: left for later as it pauses, refused on Return, why
    // told under the field.
    host.ui_event(&mut plugin, &typed("accent", changed("mix(")))
        .unwrap();
    host.advance(PAUSE);
    host.deliver(&mut plugin).unwrap();
    assert!(written(&mut host).is_empty());
    let refused = |host: &mut FakeHost, plugin: &mut SettingsPage| {
        find(&page(host, plugin), "editor-refused:accent").map(texts)
    };
    assert_eq!(refused(&mut host, &mut plugin), None);
    // Left as it is a while more: why told, of no Return; gone as the
    // typing goes on, and of none told of a value that is written.
    host.advance(TELL);
    host.deliver(&mut plugin).unwrap();
    let why = refused(&mut host, &mut plugin).expect("why");
    assert!(why[0].contains("is left out"), "{why:?}");
    host.ui_event(&mut plugin, &typed("accent", changed("mix(text")))
        .unwrap();
    assert_eq!(refused(&mut host, &mut plugin), None);
    host.ui_event(&mut plugin, &typed("accent", changed("#00aa00")))
        .unwrap();
    host.advance(PAUSE);
    host.deliver(&mut plugin).unwrap();
    assert_eq!(written(&mut host), [set_paper("accent", Some("#00aa00"))]);
    host.advance(TELL);
    host.deliver(&mut plugin).unwrap();
    assert!(written(&mut host).is_empty());
    assert_eq!(refused(&mut host, &mut plugin), None);
    host.ui_event(&mut plugin, &typed("accent", changed("mix(")))
        .unwrap();
    host.ui_event(
        &mut plugin,
        &typed(
            "accent",
            UiEventKind::Submit {
                text: "mix(".to_owned(),
                shift: false,
            },
        ),
    )
    .unwrap();
    let tree = page(&mut host, &mut plugin);
    let why = texts(find(&tree, "editor-refused:accent").expect("why"));
    assert!(why[0].contains("is left out"), "{why:?}");
    // The field typed as it was kept.
    match find(&tree, &format!("{VALUE}accent")).map(|node| &node.kind) {
        Some(Kind::Field(field)) => assert_eq!(field.text.as_deref(), Some("mix(")),
        other => panic!("{other:?}"),
    }
    // Left of an expression: written, the reason gone.
    host.ui_event(&mut plugin, &typed("accent", changed("bright-blue / 80%")))
        .unwrap();
    host.ui_event(&mut plugin, &typed("accent", UiEventKind::Blur))
        .unwrap();
    assert_eq!(
        written(&mut host),
        [set_paper("accent", Some("bright-blue / 80%"))]
    );
    assert!(find(&page(&mut host, &mut plugin), "editor-refused:accent").is_none());
    // Its value as it is, or an empty field: nothing written.
    for text in ["#4c9bff", "  "] {
        host.ui_event(
            &mut plugin,
            &typed(
                "accent",
                UiEventKind::Submit {
                    text: text.to_owned(),
                    shift: false,
                },
            ),
        )
        .unwrap();
        assert!(written(&mut host).is_empty(), "{text:?}");
    }
    // Reset of a value as the Editor showed the theme: nothing written.
    host.ui_event(&mut plugin, &click(&format!("{RESET}accent")))
        .unwrap();
    assert!(written(&mut host).is_empty());
    // Written since, as the file tells: Reset of it, written back; of a
    // token that its rule colored then, taken away.
    let mut tokens = paper_tokens();
    for token in &mut tokens[1].tokens {
        match token.name.as_str() {
            "accent" => token.value = Some("bright-blue / 80%".to_owned()),
            "focus.ring" => {
                token.origin = TokenOrigin::File {
                    path: "/themes/Paper.toml".to_owned(),
                    line: 9,
                    theme: "Paper".to_owned(),
                };
                token.value = Some("accent / 30%".to_owned());
            }
            _ => {}
        }
    }
    host.set_theme_tokens(tokens);
    host.post(Event::ThemeChanged);
    host.deliver(&mut plugin).unwrap();
    let tree = page(&mut host, &mut plugin);
    match find(&tree, &format!("{RESET}accent")).map(|node| &node.kind) {
        Some(Kind::Button(button)) => assert!(!button.disabled),
        other => panic!("{other:?}"),
    }
    host.ui_event(&mut plugin, &click(&format!("{RESET}accent")))
        .unwrap();
    assert_eq!(written(&mut host), [set_paper("accent", Some("#4c9bff"))]);
    host.ui_event(&mut plugin, &click(&format!("{ROW}focus.ring")))
        .unwrap();
    page(&mut host, &mut plugin);
    host.ui_event(&mut plugin, &click(&format!("{RESET}focus.ring")))
        .unwrap();
    assert_eq!(written(&mut host), [set_paper("focus.ring", None)]);
}

#[test]
fn the_guide_for_theme_authors_opens_of_its_link_at_the_version_s_tag() {
    let (mut host, mut plugin) = shown();
    let tree = page(&mut host, &mut plugin);
    assert!(find(&tree, GUIDE).is_some());
    assert!(GUIDE_ADDRESS.contains(&format!("/blob/v{}/", env!("CARGO_PKG_VERSION"))));
    let _ = host.take_done();
    host.ui_event(&mut plugin, &click(GUIDE)).unwrap();
    assert!(
        host.take_done()
            .contains(&Request::Open(GUIDE_ADDRESS.to_owned()))
    );
}

#[test]
fn a_swatch_opens_the_color_panel_whose_colors_show_at_once_and_are_written_at_rest() {
    let (mut host, mut plugin) = editing("accent");
    page(&mut host, &mut plugin);
    let accent = ThemeColor::hex(0x4c9bff);
    // A token's swatch: the panel of its color, of its opacity's slider.
    host.ui_event(&mut plugin, &click(&format!("{SWATCH}accent")))
        .unwrap();
    assert!(host.take_done().contains(&Request::ShowColorPanel {
        color: accent,
        alpha: true,
    }));
    // A color chosen: shown at once, in its field too; written once the
    // panel rests.
    let red = ThemeColor::hex(0xff0000).with_alpha(128);
    host.post(Event::ColorPicked { color: red });
    host.deliver(&mut plugin).unwrap();
    let done = host.take_done();
    assert!(
        done.contains(&Request::PreviewThemeValue {
            theme: "Paper".to_owned(),
            name: "accent".to_owned(),
            value: Some("#ff000080".to_owned()),
        }),
        "{done:?}"
    );
    assert!(
        !done
            .iter()
            .any(|request| matches!(request, Request::SetThemeValue { .. }))
    );
    match find(&page(&mut host, &mut plugin), &format!("{VALUE}accent")).map(|node| &node.kind) {
        Some(Kind::Field(field)) => assert_eq!(field.text.as_deref(), Some("#ff000080")),
        other => panic!("{other:?}"),
    }
    host.advance(COLOR_PAUSE);
    host.deliver(&mut plugin).unwrap();
    assert_eq!(written(&mut host), [set_paper("accent", Some("#ff000080"))]);
    // A terminal's color: no opacity's slider; another row chosen, the
    // panel of its color, the last color written first.
    host.post(Event::ColorPicked {
        color: ThemeColor::hex(0x00ff00),
    });
    host.deliver(&mut plugin).unwrap();
    let _ = host.take_done();
    host.ui_event(&mut plugin, &click(&format!("{ROW}background")))
        .unwrap();
    let done = host.take_done();
    assert!(
        done.contains(&set_paper("accent", Some("#00ff00"))),
        "{done:?}"
    );
    assert!(
        done.iter()
            .any(|request| matches!(request, Request::ShowColorPanel { alpha: false, .. })),
        "{done:?}"
    );
    // Closed by the user: its last color written at once.
    host.post(Event::ColorPicked {
        color: ThemeColor::hex(0x101010),
    });
    host.post(Event::ColorPanelClosed);
    host.deliver(&mut plugin).unwrap();
    assert_eq!(
        written(&mut host),
        [set_paper("background", Some("#101010"))]
    );
    // A color chosen of no panel of the plugin's: nothing.
    host.post(Event::ColorPicked {
        color: ThemeColor::hex(0x202020),
    });
    host.deliver(&mut plugin).unwrap();
    assert!(written(&mut host).is_empty());
    // Of another theme shown, or the Editor hidden, the panel closed.
    host.ui_event(&mut plugin, &click(&format!("{SWATCH}accent")))
        .unwrap();
    choose(&mut host, &mut plugin, "Lokyr Dark");
    assert!(host.take_done().contains(&Request::CloseColorPanel));
    // A read-only theme's swatch: no panel.
    page(&mut host, &mut plugin);
    host.ui_event(&mut plugin, &click(&format!("{SWATCH}accent")))
        .unwrap();
    assert!(
        !host
            .take_done()
            .iter()
            .any(|request| matches!(request, Request::ShowColorPanel { .. }))
    );
    choose(&mut host, &mut plugin, "Paper");
    page(&mut host, &mut plugin);
    host.ui_event(&mut plugin, &click(&format!("{SWATCH}accent")))
        .unwrap();
    let _ = host.take_done();
    host.ui_event(&mut plugin, &click("themes-side:dark"))
        .unwrap();
    assert!(host.take_done().contains(&Request::CloseColorPanel));
}

#[test]
fn the_lock_of_the_theme_in_force_shows_its_copy_of_its_own_values() {
    use lokyr_plugin_sdk::host::{ThemeFormat, ThemeInfo, ThemeOrigin};
    let (mut host, mut plugin) = shown();
    host.set_themes(kinds("Lokyr Dark"));
    host.deliver(&mut plugin).unwrap();
    // The theme in force, built in, shown of no preview: its lock.
    let tree = page(&mut host, &mut plugin);
    assert_eq!(lock(&tree).as_deref(), Some("editor-read-only"));
    host.ui_event(&mut plugin, &click(LOCK)).unwrap();
    let id = plugin.editor.copying.unwrap();
    // The copy named in `[theme]`, of the same colors: no `theme-changed`.
    let mut named = kinds("Mine");
    named.themes.push(ThemeInfo {
        name: "Mine".to_owned(),
        variant: ThemeVariant::Dark,
        origin: ThemeOrigin::Folder,
        format: ThemeFormat::Lokyr,
    });
    host.set_themes(named);
    let mut tokens = theme_tokens();
    tokens[0].theme = "Mine".to_owned();
    for token in &mut tokens[0].tokens {
        if token.name == "accent" {
            token.origin = TokenOrigin::File {
                path: "/themes/Mine.toml".to_owned(),
                line: 3,
                theme: "Mine".to_owned(),
            };
            token.value = Some("#4c9bff".to_owned());
        }
    }
    host.set_theme_tokens(tokens);
    // The answer alone, of no `theme-changed` before it.
    host.events(
        &mut plugin,
        &[Event::Answered {
            id,
            text: Some("Mine".to_owned()),
        }],
    )
    .unwrap();
    // The copy shown, of no lock, its values as it showed kept for Reset.
    assert_eq!(plugin.editor.shown_theme(), Some("Mine"));
    assert_eq!(lock(&page(&mut host, &mut plugin)), None);
    assert_eq!(plugin.editor.original("accent"), Some(Some("#4c9bff")));
}

#[test]
fn values_typed_are_written_as_the_editor_hides_and_left_behind_as_another_theme_shows() {
    let changed = |text: &str| UiEventKind::InputChanged {
        text: text.to_owned(),
        revision: 1,
    };
    // Typed, then the page hidden within the pause: written once.
    let (mut host, mut plugin) = editing("accent");
    page(&mut host, &mut plugin);
    host.ui_event(&mut plugin, &typed("accent", changed("#00ff00")))
        .unwrap();
    host.hide_page();
    host.deliver(&mut plugin).unwrap();
    assert_eq!(written(&mut host), [set_paper("accent", Some("#00ff00"))]);
    host.advance(PAUSE);
    host.deliver(&mut plugin).unwrap();
    assert!(written(&mut host).is_empty());
    // A color picked, then the page hidden: written, the panel closed.
    let (mut host, mut plugin) = editing("accent");
    page(&mut host, &mut plugin);
    host.ui_event(&mut plugin, &click(&format!("{SWATCH}accent")))
        .unwrap();
    host.post(Event::ColorPicked {
        color: ThemeColor::hex(0x123456),
    });
    host.deliver(&mut plugin).unwrap();
    let _ = host.take_done();
    host.hide_page();
    host.deliver(&mut plugin).unwrap();
    let done = host.take_done();
    assert!(
        done.contains(&set_paper("accent", Some("#123456"))),
        "{done:?}"
    );
    assert!(done.contains(&Request::CloseColorPanel), "{done:?}");
    // A whole value typed, then another theme shown: written to its own
    // theme at once; a refused one forgotten, of neither theme.
    let (mut host, mut plugin) = editing("accent");
    page(&mut host, &mut plugin);
    host.ui_event(&mut plugin, &typed("accent", changed("#abcdef")))
        .unwrap();
    choose(&mut host, &mut plugin, "Dracula");
    assert_eq!(written(&mut host), [set_paper("accent", Some("#abcdef"))]);
    host.advance(PAUSE);
    host.deliver(&mut plugin).unwrap();
    assert!(written(&mut host).is_empty());
    // The row chosen still, of the theme shown again.
    choose(&mut host, &mut plugin, "Paper");
    host.ui_event(
        &mut plugin,
        &typed(
            "accent",
            UiEventKind::Submit {
                text: "mix(".to_owned(),
                shift: false,
            },
        ),
    )
    .unwrap();
    assert!(plugin.editor.refused.contains_key("accent"));
    choose(&mut host, &mut plugin, "Dracula");
    assert!(plugin.editor.typed.is_empty() && plugin.editor.refused.is_empty());
    choose(&mut host, &mut plugin, "Paper");
    let tree = page(&mut host, &mut plugin);
    match find(&tree, &format!("{VALUE}accent")).map(|node| &node.kind) {
        Some(Kind::Field(field)) => assert_eq!(field.text.as_deref(), Some("#4c9bff")),
        other => panic!("{other:?}"),
    }
    assert!(find(&tree, "editor-refused:accent").is_none());
    assert!(written(&mut host).is_empty());
}

#[test]
fn tokens_unknown_to_lokyr_of_its_groups_show_under_a_heading_of_their_own() {
    let (mut host, mut plugin) = shown();
    // Lokyr Dark's tokens, then, of no rule, a plugin's and two of Lokyr's
    // groups that Lokyr does not know, as the read lists them.
    let mut tokens = theme_tokens();
    // A token of the tab bar's group among the rules', as the list has them.
    let mut active = tokens[0].tokens[1].clone();
    active.name = "tabs.active.bg".to_owned();
    tokens[0].tokens.insert(2, active);
    for name in ["panel.bgg", "tabs.activ.bg"] {
        let mut extra = tokens[0].tokens[0].clone();
        extra.name = name.to_owned();
        tokens[0].tokens.push(extra);
    }
    host.set_theme_tokens(tokens);
    host.deliver(&mut plugin).unwrap();
    let tree = page(&mut host, &mut plugin);
    // Every key once, as the host refuses a tree of a key twice.
    let mut keys = Vec::new();
    let mut walk = vec![&tree];
    while let Some(node) = walk.pop() {
        if let Some(key) = &node.key {
            keys.push(key.0.clone());
        }
        walk.extend(node.children.iter());
    }
    let mut unique = keys.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(unique.len(), keys.len(), "{keys:?}");
    // Both under their heading, its own, after the plugin's.
    let shown = texts(&tree);
    let at = |text: &str| shown.iter().position(|each| each == text).unwrap();
    assert!(at("editor-unknown") < at("panel.bgg"));
    assert!(at("editor-unknown") < at("tabs.activ.bg"));
    assert!(at("command-header") < at("editor-unknown"));
    // The tab bar's heading once, of its own token alone.
    assert_eq!(shown.iter().filter(|each| *each == "tabs").count(), 1);
    assert!(at("tabs") < at("tabs.active.bg") && at("tabs.active.bg") < at("command-header"));
}
