use lokyr_plugin_sdk::fake::{FakeHost, theme_tokens};
use lokyr_plugin_sdk::host::ThemeTokens;
use lokyr_plugin_sdk::message::Request;
use lokyr_plugin_sdk::theme::ThemeColor;
use lokyr_plugin_sdk::view::Kind;

use super::*;
use crate::SettingsPage;

/// A host of the settings' plugin, its Theme Inspector's page shown.
fn shown() -> (FakeHost, SettingsPage) {
    let mut host = FakeHost::new("lokyr/settings").with_manifest(crate::MANIFEST);
    // A window, which the settings' window opens beside.
    host.pane(lokyr_plugin_sdk::host::PaneId(1));
    let mut plugin = SettingsPage::default();
    host.activate(&mut plugin).unwrap();
    host.show_page(INSPECTOR);
    host.deliver(&mut plugin).unwrap();
    (host, plugin)
}

/// The page's tree, rendered anew.
fn page(host: &mut FakeHost, plugin: &mut SettingsPage) -> Node {
    host.render(plugin, &inspector_page()).expect("the page")
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
            .and_then(|key| key.0.strip_prefix("inspector-token:"))
        {
            out.push(name.to_owned());
        }
        walk.extend(node.children.iter().rev());
    }
    out
}

fn click(key: &str) -> UiEvent {
    UiEvent {
        view: inspector_page(),
        key: key.into(),
        kind: UiEventKind::Click,
    }
}

#[test]
fn the_page_lists_every_token_by_groups_of_its_color_value_and_origin() {
    let (mut host, mut plugin) = shown();
    let tree = page(&mut host, &mut plugin);
    // The fake's tokens, in their order, each group under its heading.
    assert_eq!(rows(&tree), ["accent", "focus.ring", "command-header.pill"]);
    for group in ["accent", "focus", "command-header"] {
        assert!(
            find(&tree, &format!("inspector-group:{group}")).is_some(),
            "{group}"
        );
    }
    let all = texts(&tree);
    for text in [
        "#4c9bff",
        "Lokyr Dark",
        "inspector-rule",
        "Lokyr Dark.toml:3",
        "#2a2a2d",
    ] {
        assert!(all.iter().any(|shown| shown == text), "{text}: {all:?}");
    }
    // The swatch of each, of its color.
    let row = find(&tree, "inspector-token:command-header.pill").unwrap();
    let swatch = &row.children[0].children[0];
    assert_eq!(swatch.style.background, Some(Token::from("#2a2a2d")));
    // The theme in force's name on its button.
    assert!(find(&tree, THEME).is_some());
}

#[test]
fn the_filter_keeps_the_tokens_of_its_words_and_empties() {
    let (mut host, mut plugin) = shown();
    let typed = |text: &str| UiEvent {
        view: inspector_page(),
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
    assert_eq!(rows(&page(&mut host, &mut plugin)), ["focus.ring"]);
    host.ui_event(&mut plugin, &click(FILTER_CLEAR)).unwrap();
    assert_eq!(rows(&page(&mut host, &mut plugin)).len(), 3);
}

#[test]
fn a_token_of_a_file_opens_at_its_line_and_another_copies_its_line() {
    let (mut host, mut plugin) = shown();
    let _ = host.take_done();
    host.ui_event(&mut plugin, &click("inspector-act:command-header.pill"))
        .unwrap();
    assert!(host.take_done().iter().any(|request| matches!(
        request,
        Request::OpenAt { target, line: 3, .. } if target == "/themes/Lokyr Dark.toml"
    )));
    host.ui_event(&mut plugin, &click("inspector-act:focus.ring"))
        .unwrap();
    assert_eq!(host.clipboard(), Some("\"focus.ring\" = \"accent / 60%\""));
    // Within a second of the last notice: copied, of no notice more.
    host.ui_event(&mut plugin, &click("inspector-act:accent"))
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
        find(tree, &format!("inspector-token:{name}"))
            .unwrap()
            .style
            .background
            .is_some()
    };
    assert!(lit(&tree, "accent") && !lit(&tree, "focus.ring"));
    assert!(texts(&tree).iter().any(|text| text == "#c678dd"));
    // Its light goes as its timer fires.
    host.advance(LIT);
    host.deliver(&mut plugin).unwrap();
    assert!(!lit(&page(&mut host, &mut plugin), "accent"));
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
    assert!(find(&menu, IN_FORCE).is_some());
    let _ = host.take_done();
    let item = UiEvent {
        view: ViewId::Panel(PanelId::from(THEMES_MENU)),
        key: format!("{THEME_ITEM}0").as_str().into(),
        kind: UiEventKind::Click,
    };
    host.ui_event(&mut plugin, &item).unwrap();
    let done = host.take_done();
    assert!(
        done.iter().any(|request| matches!(
            request,
            Request::PreviewTheme(Some(name)) if name == "Lokyr Light"
        )),
        "{done:?}"
    );
    // The page hidden, the preview ends.
    host.hide_page();
    host.deliver(&mut plugin).unwrap();
    assert!(host.take_done().contains(&Request::PreviewTheme(None)));
}
