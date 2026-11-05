//! The bar's config and its tree.

use std::collections::BTreeMap;

use lokyr_plugin_sdk::host::{PaneId, TabId, WindowId};
use lokyr_plugin_sdk::view::Kind;

use super::*;

/// A tab of `id`, `title` and `active`, of its window's first pane.
fn tab(id: u64, title: &str, active: bool) -> TabInfo {
    TabInfo {
        id: TabId(id),
        window: WindowId(1),
        title: title.to_owned(),
        color: None,
        active,
        panes: vec![PaneId(id)],
        focused: PaneId(id),
        zoomed: None,
    }
}

/// The config of `pairs`.
fn config(pairs: &[(&str, Data)]) -> Data {
    Data::Map(
        pairs
            .iter()
            .map(|(name, value)| ((*name).to_owned(), value.clone()))
            .collect::<BTreeMap<_, _>>(),
    )
}

/// The texts of `node`'s subtree, in their order.
fn texts(node: &Node) -> Vec<String> {
    let mut found = Vec::new();
    if let Kind::Text(text) = &node.kind {
        found.push(text.clone());
    }
    for child in &node.children {
        found.extend(texts(child));
    }
    found
}

/// The node of `key` in `node`'s subtree.
fn keyed<'a>(node: &'a Node, key: &str) -> Option<&'a Node> {
    if node.key.as_ref().is_some_and(|own| own.0 == key) {
        return Some(node);
    }
    node.children.iter().find_map(|child| keyed(child, key))
}

#[test]
fn the_config_gives_the_bar_its_place_and_its_look() {
    let text = |value: &str| Data::Text(value.to_owned());
    assert_eq!(Settings::of(&Data::Nil), Settings::default());
    assert_eq!(Settings::default().place(None), BarPlace::Titlebar);
    let regular = Settings::of(&config(&[("style", text("regular"))]));
    assert_eq!(regular.place(None), BarPlace::BelowTitlebar);
    let bottom = Settings::of(&config(&[("position", text("bottom"))]));
    assert_eq!(bottom.place(None), BarPlace::Bottom);
    let left = Settings::of(&config(&[
        ("position", text("left")),
        ("column-width", Data::Integer(240)),
        ("show-index", Data::Boolean(false)),
        ("hide-when-single", Data::Boolean(false)),
        ("close-button", text("never")),
    ]));
    assert_eq!(left.place(None), BarPlace::Left { width: 240.0 });
    // The user's drag's width, within the limits.
    assert_eq!(left.place(Some(320.0)), BarPlace::Left { width: 320.0 });
    assert_eq!(left.place(Some(5000.0)), BarPlace::Left { width: 600.0 });
    assert!(!left.show_index && !left.hide_when_single);
    assert_eq!(left.close_button, CloseButton::Never);
    let right = Settings::of(&config(&[("position", text("right"))]));
    assert_eq!(right.place(None), BarPlace::Right { width: 200.0 });
}

#[test]
fn a_bar_shows_its_tabs_their_keys_and_their_close_buttons() {
    let settings = Settings::default();
    // One tab, hidden by default, and shown as the config says.
    assert!(bar(&[tab(1, "zsh", true)], &[], &settings).is_none());
    let shown = Settings {
        hide_when_single: false,
        ..settings
    };
    assert!(bar(&[tab(1, "zsh", true)], &[], &shown).is_some());
    assert!(bar(&[], &[], &shown).is_none());
    let tabs = [tab(1, "src — vim", true), tab(2, "", false)];
    let keys = [Some("⌘1".to_owned()), None];
    let node = bar(&tabs, &keys, &settings).unwrap();
    // Each tab's title, the second of its place, the first's keys alone.
    assert_eq!(texts(&node), ["src — vim", "⌘1", "Tab 2"]);
    assert_eq!(node.layout.direction, Direction::Row);
    // The active tab of its tokens and of no hover; the other's hover; each
    // of its press, faded.
    let first = keyed(&node, "tab-1").unwrap();
    assert_eq!(first.style.background, Some(Token::from("tab.active.bg")));
    assert!(first.style.hover.is_none() && first.style.pressed.is_some());
    let second = keyed(&node, "tab-2").unwrap();
    assert_eq!(
        second.style.background,
        Some(Token::from("tab.inactive.bg"))
    );
    assert!(second.style.hover.is_some());
    assert!(second.style.transition.is_some());
    // The close button, shown under the tab's hover.
    let close = keyed(&node, "close-2").unwrap();
    assert_eq!(close.style.opacity, Some(0.0));
    assert_eq!(
        close
            .style
            .parent_hover
            .as_ref()
            .and_then(|hover| hover.opacity),
        Some(1.0)
    );
    assert!(keyed(&node, NEW_KEY).is_some());
    // Always, never.
    let always = bar(
        &tabs,
        &keys,
        &Settings {
            close_button: CloseButton::Always,
            ..settings
        },
    )
    .unwrap();
    let close = keyed(&always, "close-1").unwrap();
    assert!(close.style.opacity.is_none() && close.style.parent_hover.is_none());
    let never = bar(
        &tabs,
        &keys,
        &Settings {
            close_button: CloseButton::Never,
            show_index: false,
            ..settings
        },
    )
    .unwrap();
    assert!(keyed(&never, "close-1").is_none());
    assert_eq!(texts(&never), ["src — vim", "Tab 2"]);
}

#[test]
fn a_column_lays_its_tabs_down_and_a_minimal_bar_takes_the_terminal_s_colors() {
    let column = Settings {
        position: Position::Left,
        ..Settings::default()
    };
    let tabs = [tab(1, "a", true), tab(2, "b", false)];
    let node = bar(&tabs, &[], &column).unwrap();
    assert_eq!(node.layout.direction, Direction::Column);
    assert_eq!(keyed(&node, "tab-1").unwrap().layout.grow, 0.0);
    let minimal = Settings {
        look: Look::Minimal,
        ..Settings::default()
    };
    let node = bar(&tabs, &[], &minimal).unwrap();
    assert_eq!(node.style.background, Some(Token::from("terminal.bg")));
    assert_eq!(
        keyed(&node, "tab-1").unwrap().style.color,
        Some(Token::from("terminal.fg"))
    );
}

#[test]
fn a_tab_of_a_color_shows_it() {
    let mut colored = tab(1, "a", true);
    colored.color = Some(Color::Rgb(200, 30, 4));
    let node = bar(&[colored, tab(2, "b", false)], &[], &Settings::default()).unwrap();
    let first = keyed(&node, "tab-1").unwrap();
    assert_eq!(
        first.children[0].style.background,
        Some(Token::from("#c81e04"))
    );
}
