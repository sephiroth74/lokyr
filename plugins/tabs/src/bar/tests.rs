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
        activity: false,
        bell: false,
        progress: None,
        running: false,
        alternate: false,
        secure: false,
        agent: None,
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
    let standard = Settings::of(&config(&[("style", text("standard"))]));
    assert_eq!(standard.place(None), BarPlace::BelowTitlebar);
    let bottom = Settings::of(&config(&[("position", text("bottom"))]));
    assert_eq!(bottom.place(None), BarPlace::Bottom);
    let left = Settings::of(&config(&[
        ("position", text("left")),
        ("column-width", Data::Integer(240)),
        ("show-index", Data::Boolean(false)),
        ("hide-when-single", Data::Boolean(false)),
        ("close-button", text("never")),
    ]));
    // On the left through the titlebar, as a sidebar, but of the standard
    // look.
    assert_eq!(left.place(None), BarPlace::Sidebar { width: 240.0 });
    // The user's drag's width, within the limits.
    assert_eq!(left.place(Some(320.0)), BarPlace::Sidebar { width: 320.0 });
    assert_eq!(left.place(Some(5000.0)), BarPlace::Sidebar { width: 600.0 });
    let standard = Settings {
        look: Look::Standard,
        ..left
    };
    assert_eq!(standard.place(None), BarPlace::Left { width: 240.0 });
    assert!(!left.show_index && !left.hide_when_single);
    assert_eq!(left.close_button, CloseButton::Never);
    let right = Settings::of(&config(&[("position", text("right"))]));
    assert_eq!(right.place(None), BarPlace::Right { width: 200.0 });
}

#[test]
fn a_bar_shows_its_tabs_their_keys_and_their_close_buttons() {
    let settings = Settings::default();
    // One tab, of its title alone by default, its marks beside it, between
    // the room of the window's buttons on either side; and its bar as the
    // config says.
    let running = TabInfo {
        running: true,
        ..tab(1, "zsh", true)
    };
    let title = bar(&[running], &[], None, &settings).expect("a title");
    assert_eq!(
        title.key.as_ref().map(|key| key.0.as_str()),
        Some(TITLE_KEY)
    );
    assert_eq!(texts(&title), ["zsh"]);
    assert_eq!(marked(&title), ["zsh", "Running", "Dots Indeterminate 0"]);
    let sides: Vec<Length> = [0, 2].map(|at| title.children[at].layout.width).to_vec();
    assert_eq!(sides, [Length::Buttons, Length::Buttons]);
    assert!(bar(&[], &[], None, &settings).is_none());
    let shown = Settings {
        hide_when_single: false,
        ..settings
    };
    assert!(bar(&[tab(1, "zsh", true)], &[], None, &shown).is_some());
    assert!(bar(&[], &[], None, &shown).is_none());
    let tabs = [tab(1, "src — vim", true), tab(2, "", false)];
    let keys = [Some("⌘1".to_owned()), None];
    let node = bar(&tabs, &keys, None, &settings).unwrap();
    // Each tab's title, the second of its place, the first's keys alone.
    assert_eq!(texts(&node), ["src — vim", "⌘1", "Tab 2"]);
    assert_eq!(node.layout.direction, Direction::Row);
    // The active tab of its tokens and of no hover; the other's hover; each
    // of its press, faded.
    let first = keyed(&node, "tab-1").unwrap();
    assert_eq!(first.style.background, Some(Token::from("tab.active.bg")));
    assert!(first.style.hover.is_none() && first.style.pressed.is_some());
    assert!(first.style.border.is_some() && first.style.shadow.is_some());
    // The tabs of equal widths, whatever their titles.
    assert_eq!(first.layout.width, Length::Points(0.0));
    assert_eq!(first.layout.grow, 1.0);
    let second = keyed(&node, "tab-2").unwrap();
    assert_eq!(second.style.background, None);
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
        None,
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
        None,
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
fn a_column_lays_its_tabs_down_and_a_seamless_bar_shows_the_window_s_background() {
    let column = Settings {
        position: Position::Left,
        ..Settings::default()
    };
    let tabs = [tab(1, "a", true), tab(2, "b", false)];
    let node = bar(&tabs, &[], Some("⌥⌘1"), &column).unwrap();
    assert_eq!(node.style.background, Some(Token::from("tab.bar.bg")));
    // The rows down the column, and a line along its edge by the panes.
    let [list, edge] = &node.children[..] else {
        panic!("{node:#?}");
    };
    assert_eq!(list.layout.direction, Direction::Column);
    assert!(matches!(edge.kind, Kind::Separator));
    assert_eq!(keyed(&node, "tab-1").unwrap().layout.grow, 0.0);
    // The button of a new tab, its sign in its middle.
    assert_eq!(
        keyed(&node, NEW_KEY).unwrap().layout.justify,
        Justify::Center
    );
    // The rows of rounded corners, a stadium's in the unified look, raised
    // of its shadow too.
    let radius = |look: Look| {
        let settings = Settings { look, ..column };
        let node = bar(&tabs, &[], None, &settings).unwrap();
        let active = keyed(&node, "tab-1").unwrap();
        (active.style.radius, active.style.shadow.is_some())
    };
    assert_eq!(radius(Look::Standard), (6.0, false));
    assert_eq!(radius(Look::Unified), (ROW / 2.0, true));
    // As a sidebar, the room of the window's buttons first, of the
    // titlebar's height, the window's keys past the buttons, of no key, as
    // the window's drag takes it.
    let body = &list.children;
    let room = &body[0];
    assert_eq!(room.layout.height, Length::Titlebar);
    assert_eq!(room.children[0].layout.width, Length::Buttons);
    assert_eq!(texts(room), ["⌥⌘1"]);
    assert!(room.key.is_none() && room.children.iter().all(|child| child.key.is_none()));
    assert_eq!(body[1].layout.grow, 1.0);
    // Of no keys of the window's, the room alone; under the veil in the
    // seamless look.
    let node = bar(&tabs, &[], None, &column).unwrap();
    assert!(texts(&node.children[0].children[0]).is_empty());
    let seamless_side = Settings {
        look: Look::Seamless,
        ..column
    };
    let node = bar(&tabs, &[], None, &seamless_side).unwrap();
    assert_eq!(
        node.children[0].children[0].style.background,
        Some(Token::from("tab.seamless.veil"))
    );
    // Below the titlebar, the window's keys the titlebar's.
    let standard = Settings {
        look: Look::Standard,
        ..column
    };
    let node = bar(&tabs, &[], Some("⌥⌘1"), &standard).unwrap();
    assert!(!texts(&node).contains(&"⌥⌘1".to_owned()));
    assert_eq!(
        node.children[0].children[0]
            .key
            .as_ref()
            .map(|key| key.0.as_str()),
        Some("tab-1")
    );
    let right = Settings {
        position: Position::Right,
        ..column
    };
    let node = bar(&tabs, &[], None, &right).unwrap();
    assert!(matches!(node.children[0].kind, Kind::Separator));
    let seamless = Settings {
        look: Look::Seamless,
        ..Settings::default()
    };
    let node = bar(&tabs, &[], None, &seamless).unwrap();
    // The window's background through the bar and its active tab, and the
    // others under the veil.
    let veil = Some(Token::from("tab.seamless.veil"));
    assert_eq!(node.style.background, None);
    let active = keyed(&node, "tab-1").unwrap();
    assert_eq!(active.style.background, None);
    assert_eq!(active.style.color, Some(Token::from("terminal.fg")));
    assert_eq!(keyed(&node, "tab-2").unwrap().style.background, veil);
    // The active cell open to the terminal, the other over a line.
    let line = |cell: &Node| cell.children.last().unwrap().style.background.clone();
    assert_eq!(line(active), None);
    assert_eq!(
        line(keyed(&node, "tab-2").unwrap()),
        Some(Token::from("tab.seamless.line"))
    );
    // A line between two cells.
    assert!(
        node.children
            .iter()
            .any(|child| matches!(child.kind, Kind::Separator))
    );
    // At the bottom, the line above.
    let bottom = Settings {
        position: Position::Bottom,
        ..seamless
    };
    let node = bar(&tabs, &[], None, &bottom).unwrap();
    let other = keyed(&node, "tab-2").unwrap();
    assert_eq!(
        other.children[0].style.background,
        Some(Token::from("tab.seamless.line"))
    );
    // Down a seamless column, the active row of the window's background,
    // the others and the rest of the column under the veil.
    let column = Settings {
        position: Position::Left,
        ..seamless
    };
    let node = bar(&tabs, &[], None, &column).unwrap();
    assert_eq!(node.style.background, None);
    assert_eq!(keyed(&node, "tab-1").unwrap().style.background, None);
    assert_eq!(keyed(&node, "tab-2").unwrap().style.background, veil);
    // A sidebar's list, below the room of the window's buttons.
    let list = node.children[0].children.last().unwrap();
    let rest = list.children.last().unwrap();
    assert_eq!(
        (rest.style.background.clone(), rest.layout.grow),
        (veil, 1.0)
    );
}

#[test]
fn a_bar_in_the_titlebar_shows_the_window_s_keys_first() {
    let tabs = [tab(1, "a", true), tab(2, "b", false)];
    for look in [Look::Unified, Look::Seamless] {
        let settings = Settings {
            look,
            ..Settings::default()
        };
        let node = bar(&tabs, &[], Some("⌥⌘1"), &settings).unwrap();
        assert_eq!(texts(&node)[0], "⌥⌘1", "{look:?}");
        // Of the titlebar's background, as the window's buttons are.
        if look == Look::Seamless {
            let keys = &node.children[0];
            assert_eq!(keys.style.background, None);
            assert!(
                keys.children
                    .iter()
                    .all(|child| child.style.background.is_none())
            );
        }
        // None while the config hides them, which the host gives none.
        let node = bar(&tabs, &[], None, &settings).unwrap();
        assert_eq!(texts(&node), ["a", "b"], "{look:?}");
    }
    // Below the titlebar, the titlebar's.
    let standard = Settings {
        look: Look::Standard,
        ..Settings::default()
    };
    let node = bar(&tabs, &[], Some("⌥⌘1"), &standard).unwrap();
    assert_eq!(texts(&node), ["a", "b"]);
}

#[test]
fn a_tab_of_a_color_shows_it() {
    let mut colored = tab(1, "a", true);
    colored.color = Some(Color::Rgb(200, 30, 4));
    let tabs = [colored, tab(2, "b", false)];
    for look in [Look::Unified, Look::Seamless] {
        for position in [Position::Top, Position::Left] {
            let settings = Settings {
                look,
                position,
                ..Settings::default()
            };
            let node = bar(&tabs, &[], None, &settings).unwrap();
            let first = keyed(&node, "tab-1").unwrap();
            assert!(
                colored_dot(first, "#c81e04"),
                "{look:?} {position:?}: {first:#?}"
            );
        }
    }
}

/// Whether `node`'s subtree holds a node of the background `token`.
fn colored_dot(node: &Node, token: &str) -> bool {
    node.style.background == Some(Token::from(token))
        || node.children.iter().any(|child| colored_dot(child, token))
}

/// What `node`'s subtree marks, in its order: the labels of its nodes of
/// one, and its progresses, of their shapes, states and values.
fn marked(node: &Node) -> Vec<String> {
    let mut found = Vec::new();
    if let Some(label) = &node.label {
        found.push(label.clone());
    }
    if let Kind::Progress(progress) = &node.kind {
        found.push(format!(
            "{:?} {:?} {}",
            progress.shape, progress.state, progress.value
        ));
    }
    for child in &node.children {
        found.extend(marked(child));
    }
    found
}

#[test]
fn a_tab_marks_what_came_while_it_did_not_show_what_runs_and_its_program_s_progress() {
    use lokyr_plugin_sdk::host::{AgentState, TabProgress};
    let quiet = tab(1, "quiet", true);
    let busy = TabInfo {
        activity: true,
        bell: true,
        running: true,
        progress: Some(TabProgress {
            state: ProgressState::Normal,
            value: Some(0.4),
        }),
        ..tab(2, "busy", false)
    };
    let secure = TabInfo {
        secure: true,
        running: true,
        agent: Some(AgentState::Waiting),
        progress: Some(TabProgress {
            state: ProgressState::Error,
            value: None,
        }),
        ..tab(3, "secure", false)
    };
    let spinning = TabInfo {
        agent: Some(AgentState::Working),
        progress: Some(TabProgress {
            state: ProgressState::Indeterminate,
            value: None,
        }),
        ..tab(4, "spinning", false)
    };
    // A job that tells no progress, of output that came while the tab did
    // not show; a full-screen program, as vim.
    let job = TabInfo {
        activity: true,
        running: true,
        ..tab(5, "gradle", false)
    };
    let full_screen = TabInfo {
        running: true,
        alternate: true,
        ..tab(6, "vim", false)
    };
    let tabs = [quiet, busy, secure, spinning, job, full_screen];
    for style in ["standard", "unified", "seamless"] {
        for position in ["top", "left"] {
            let text = |value: &str| Data::Text(value.to_owned());
            let settings = Settings::of(&config(&[
                ("style", text(style)),
                ("position", text(position)),
                ("close-button", text("never")),
            ]));
            let bar = bar(&tabs, &[], None, &settings).expect("a bar");
            let of = |id: u64| marked(keyed(&bar, &tab_key(id)).expect("the tab"));
            let place = format!("{style} at the {position}");
            assert!(of(1).is_empty(), "{place}: {:?}", of(1));
            // The bell, then the output's dot, which a running program's
            // ring leaves to it; the outline of its value.
            assert_eq!(
                of(2),
                ["Bell", "New Output", "Outline Normal 0.4"],
                "{place}"
            );
            // The lock, the agent, the ring of what runs; a failure of no
            // value, full.
            assert_eq!(
                of(3),
                [
                    "Secure Keyboard Entry",
                    "Agent Waiting",
                    "Running",
                    "Outline Error 1"
                ],
                "{place}"
            );
            assert_eq!(
                of(4),
                ["Agent Working", "Outline Indeterminate 0"],
                "{place}"
            );
            // The spinner over the output's dot; the ring of a full-screen
            // one.
            assert_eq!(of(5), ["Running", "Dots Indeterminate 0"], "{place}");
            assert_eq!(of(6), ["Running"], "{place}");
        }
    }
}
