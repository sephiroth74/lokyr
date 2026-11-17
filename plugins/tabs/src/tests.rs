//! The plugin against the SDK's fake host, which records what it does.

use lokyr_plugin_sdk::fake::FakeHost;
use lokyr_plugin_sdk::host::PaneId;
use lokyr_plugin_sdk::message::Request;

use super::*;

/// The plugin's name, as its calls give it.
const NAME: &str = "lokyr/tabs";

/// The window of the tests.
const WINDOW: WindowId = WindowId(1);

/// A host of one window of tabs of `tabs` panes each, from tab 1 and
/// pane 1, whose focused pane is `focused`, and the plugin, activated.
fn host(tabs: &[u64], focused: u64) -> (FakeHost, Tabs) {
    let mut host = FakeHost::new(NAME).with_manifest(crate::MANIFEST);
    let mut pane = 0;
    for (tab, &panes) in (1..).zip(tabs) {
        for _ in 0..panes {
            pane += 1;
            host.pane(PaneId(pane)).tab = TabId(tab);
        }
    }
    host.focus(Some(PaneId(focused)));
    let mut plugin = Tabs::default();
    host.activate(&mut plugin).unwrap();
    (host, plugin)
}

/// Runs `name` of `args`, and returns the requests that it made but its
/// reads.
fn run(host: &mut FakeHost, plugin: &mut Tabs, name: &str, args: Args) -> Vec<Request> {
    host.take_done();
    host.command(plugin, name, args).unwrap();
    host.take_done()
        .into_iter()
        .filter(|request| {
            !matches!(
                request,
                Request::FocusedWindow | Request::Tabs(_) | Request::Windows
            )
        })
        .collect()
}

/// The arguments of `name`, a whole number.
fn number(name: &str, value: i64) -> Args {
    Args::from([(name.to_owned(), Value::Integer(value))])
}

/// The arguments of `name`, a text.
fn text(name: &str, value: &str) -> Args {
    Args::from([(name.to_owned(), Value::String(value.to_owned()))])
}

/// A parameter of a command: its name, its kind, and whether it has a
/// default.
type Parameter<'a> = (&'a str, Kind, bool);

#[test]
fn the_commands_register_with_their_parameters_and_their_titles() {
    let (host, _) = host(&[1], 1);
    let registered: Vec<(&str, Vec<Parameter>)> = host
        .commands()
        .iter()
        .map(|command| {
            assert_eq!(command.category, Category::Tabs);
            let params = command
                .params
                .iter()
                .map(|param| (param.name.as_str(), param.kind, param.default.is_some()));
            (command.name.as_str(), params.collect())
        })
        .collect();
    assert_eq!(
        registered,
        [
            (NEW, vec![]),
            (CLOSE, vec![]),
            (NEXT, vec![]),
            (PREV, vec![]),
            (SWITCH, vec![]),
            (SWITCH_BACK, vec![]),
            (GOTO, vec![("index", Kind::Integer, false)]),
            (MOVE, vec![("delta", Kind::Integer, false)]),
            (RENAME, vec![("title", Kind::String, true)]),
            (SET_COLOR, vec![("color", Kind::String, false)]),
            (DETACH, vec![]),
            (TOGGLE_PIN, vec![]),
            (REOPEN, vec![]),
            (NEW_GROUP, vec![]),
            (ADD_TO_GROUP, vec![("group", Kind::Integer, true)]),
            (REMOVE_FROM_GROUP, vec![]),
            (RENAME_GROUP, vec![("title", Kind::String, true)]),
            (SET_GROUP_COLOR, vec![("color", Kind::String, false)]),
            (FOLD_GROUP, vec![]),
            (UNGROUP, vec![]),
        ]
    );
    // Each title of its own catalogs, English's and Italian's alike.
    for (name, title, _) in commands() {
        let message = format!("command-{} = {title}\n", name.replace('.', "-"));
        assert!(CATALOGS[0].1.contains(&message), "{message}");
        let id = format!("command-{} = ", name.replace('.', "-"));
        assert!(CATALOGS[1].1.contains(&id), "{id}");
    }
    assert!(host.bindings().is_empty());
    // The pane's context menu renames the tab, of the command's title.
    assert_eq!(
        host.menu_items(),
        [MenuItem {
            place: MenuPlace::PaneContext,
            command: RENAME.to_owned(),
            args: Args::new(),
            title: None,
            submenu: None,
            ticked: false,
        }]
    );
}

#[test]
fn a_new_tab_opens_after_the_others_and_shows() {
    let (mut host, mut plugin) = host(&[1], 1);
    let done = run(&mut host, &mut plugin, NEW, Args::new());
    let [
        Request::NewTab {
            window: WINDOW,
            index: None,
            directory: None,
        },
        Request::ActivateTab(tab),
    ] = done[..]
    else {
        panic!("{done:?}");
    };
    assert_ne!(tab, TabId(1));
}

#[test]
fn a_close_asks_first_of_the_tab_or_of_its_pane_while_it_has_splits() {
    let (mut host, mut plugin) = host(&[1, 2], 1);
    assert_eq!(
        run(&mut host, &mut plugin, CLOSE, Args::new()),
        [Request::CloseTab {
            tab: TabId(1),
            ask: true,
            keep: 10
        }]
    );
    let (mut host, mut plugin) = self::host(&[1, 2], 3);
    assert_eq!(
        run(&mut host, &mut plugin, CLOSE, Args::new()),
        [Request::ClosePane {
            pane: PaneId(3),
            ask: true
        }]
    );
}

#[test]
fn the_selection_goes_round_and_by_the_tabs_numbers() {
    let (mut host, mut plugin) = host(&[1, 1, 1], 3);
    assert_eq!(
        run(&mut host, &mut plugin, NEXT, Args::new()),
        [Request::ActivateTab(TabId(1))]
    );
    assert_eq!(
        run(&mut host, &mut plugin, PREV, Args::new()),
        [Request::ActivateTab(TabId(3))]
    );
    // The host's activation shows the third's neighbours.
    let (mut host, mut plugin) = self::host(&[1, 1, 1], 1);
    assert_eq!(
        run(&mut host, &mut plugin, PREV, Args::new()),
        [Request::ActivateTab(TabId(3))]
    );
    assert_eq!(
        run(&mut host, &mut plugin, GOTO, number("index", 2)),
        [Request::ActivateTab(TabId(2))]
    );
    // A number of no tab selects nothing.
    for index in [0, 4, -1] {
        assert_eq!(
            run(&mut host, &mut plugin, GOTO, number("index", index)),
            []
        );
    }
}

#[test]
fn a_move_goes_as_far_as_the_window_s_ends() {
    let (mut host, mut plugin) = host(&[1, 1, 1], 2);
    let moved = |index| Request::MoveTab {
        tab: TabId(2),
        window: WINDOW,
        index,
    };
    assert_eq!(
        run(&mut host, &mut plugin, MOVE, number("delta", 1)),
        [moved(2)]
    );
    let (mut host, mut plugin) = self::host(&[1, 1, 1], 2);
    assert_eq!(
        run(&mut host, &mut plugin, MOVE, number("delta", -5)),
        [moved(0)]
    );
    let (mut host, mut plugin) = self::host(&[1, 1, 1], 3);
    assert_eq!(run(&mut host, &mut plugin, MOVE, number("delta", 1)), []);
}

#[test]
fn a_rename_takes_its_title_or_asks_for_it() {
    let (mut host, mut plugin) = host(&[1], 1);
    let titled = |title: Option<&str>| Request::SetTabTitle {
        tab: TabId(1),
        title: title.map(str::to_owned),
    };
    assert_eq!(
        run(&mut host, &mut plugin, RENAME, text("title", " work ")),
        [titled(Some("work"))]
    );
    // Without one, a question of the plugin's messages, of the tab's title
    // as it is.
    let done = run(&mut host, &mut plugin, RENAME, text("title", ""));
    assert_eq!(
        done,
        [Request::AskText {
            window: Some(WINDOW),
            message: "rename-message".to_owned(),
            detail: "rename-detail".to_owned(),
            text: "work".to_owned(),
        }]
    );
    let id = *plugin.renaming.keys().next().unwrap();
    let answer = |host: &mut FakeHost, plugin: &mut Tabs, id, text: Option<&str>| {
        host.take_done();
        host.post(Event::Answered {
            id,
            text: text.map(str::to_owned),
        });
        host.deliver(plugin).unwrap();
        host.take_done()
    };
    assert_eq!(
        answer(&mut host, &mut plugin, id, Some("logs")),
        [titled(Some("logs"))]
    );
    // The title that it asked with, unchanged, leaves the tab as it was.
    run(&mut host, &mut plugin, RENAME, Args::new());
    let id = *plugin.renaming.keys().next().unwrap();
    assert_eq!(answer(&mut host, &mut plugin, id, Some("logs ")), []);
    // An empty title gives the program's back; a cancel, nothing.
    run(&mut host, &mut plugin, RENAME, Args::new());
    let id = *plugin.renaming.keys().next().unwrap();
    assert_eq!(
        answer(&mut host, &mut plugin, id, Some("  ")),
        [titled(None)]
    );
    run(&mut host, &mut plugin, RENAME, Args::new());
    let id = *plugin.renaming.keys().next().unwrap();
    assert_eq!(answer(&mut host, &mut plugin, id, None), []);
    assert!(plugin.renaming.is_empty());
}

#[test]
fn a_color_is_of_its_hex_digits_or_the_program_s_back() {
    let (mut host, mut plugin) = host(&[1], 1);
    let colored = |color| Request::SetTabColor {
        tab: TabId(1),
        color,
    };
    assert_eq!(
        run(&mut host, &mut plugin, SET_COLOR, text("color", "#ff8800")),
        [colored(Some(Color::Rgb(255, 136, 0)))]
    );
    assert_eq!(
        run(&mut host, &mut plugin, SET_COLOR, text("color", "0A0b0C")),
        [colored(Some(Color::Rgb(10, 11, 12)))]
    );
    assert_eq!(
        run(&mut host, &mut plugin, SET_COLOR, text("color", "")),
        [colored(None)]
    );
    for wrong in ["#ff88", "orange", "#gg0000"] {
        let error = host
            .command(&mut plugin, SET_COLOR, text("color", wrong))
            .unwrap_err();
        assert_eq!(error.code, Code::Failed, "{wrong}");
    }
}

#[test]
fn a_tab_moves_out_to_a_window_of_its_own_but_a_window_s_last() {
    let (mut host, mut plugin) = host(&[1, 1], 2);
    assert_eq!(
        run(&mut host, &mut plugin, DETACH, Args::new()),
        [Request::TabToNewWindow {
            tab: TabId(2),
            at: None
        }]
    );
    let (mut host, mut plugin) = self::host(&[1], 1);
    assert_eq!(run(&mut host, &mut plugin, DETACH, Args::new()), []);
}

/// A click on the node of `key` of `window`'s bar.
fn click(key: &str) -> UiEvent {
    UiEvent {
        view: ViewId::TabBar(WINDOW),
        key: key.into(),
        kind: UiEventKind::Click,
    }
}

#[test]
fn a_tab_s_middle_click_double_click_and_context_menu_act_on_it() {
    use lokyr_plugin_sdk::ui::{PanelId, Role};
    let (mut host, mut plugin) = host(&[1, 1, 1], 1);
    let on = |host: &mut FakeHost, plugin: &mut Tabs, view: ViewId, key: &str, kind| {
        host.take_done();
        host.ui_event(
            plugin,
            &UiEvent {
                view,
                key: key.into(),
                kind,
            },
        )
        .unwrap();
        host.take_done()
            .into_iter()
            .filter(|request| !matches!(request, Request::Tabs(_) | Request::Message { .. }))
            .collect::<Vec<_>>()
    };
    let bar = ViewId::TabBar(WINDOW);
    // A middle click closes it, asking first.
    assert_eq!(
        on(
            &mut host,
            &mut plugin,
            bar.clone(),
            "tab-3",
            UiEventKind::MiddleClick
        ),
        [Request::CloseTab {
            tab: TabId(3),
            ask: true,
            keep: 10
        }]
    );
    // A double click asks for its title.
    let asked = on(
        &mut host,
        &mut plugin,
        bar.clone(),
        "tab-2",
        UiEventKind::DoubleClick,
    );
    assert!(matches!(asked[..], [Request::AskText { .. }]), "{asked:?}");
    assert_eq!(
        plugin.renaming.values().next().map(|(tab, _)| *tab),
        Some(TabId(2))
    );
    // The context's button opens its menu at the pointer, of its items.
    let opened = on(
        &mut host,
        &mut plugin,
        bar.clone(),
        "tab-2",
        UiEventKind::ContextMenu,
    );
    let Some(Request::OpenPanel { options, .. }) = opened
        .iter()
        .find(|request| matches!(request, Request::OpenPanel { .. }))
    else {
        panic!("{opened:?}");
    };
    assert_eq!(options.role, Role::Menu);
    let menu = ViewId::Panel(PanelId::from("tab-menu"));
    let tree = host.render(&mut plugin, &menu).expect("the menu's items");
    let keys: Vec<String> = tree
        .children
        .iter()
        .filter_map(|item| item.key.as_ref().map(|key| key.0.clone()))
        .collect();
    assert_eq!(
        keys,
        [
            "menu-pin",
            "menu-rename",
            "menu-detach",
            "menu-new-group",
            "menu-close",
            "menu-close-others"
        ]
    );
    // Its submenu of the tab's color, after its pin and its rename: its
    // title, the colors, each of its dot, and none, ticked of a tab of no
    // color.
    let submenu = &tree.children[2];
    let view::Kind::Text(heading) = &submenu.children[0].kind else {
        panic!("{submenu:?}");
    };
    assert_eq!(heading, "Color");
    let items: Vec<(String, String, bool)> = submenu.children[1..]
        .iter()
        .filter_map(|item| match &item.kind {
            view::Kind::Toggle(toggle) => Some((
                item.key.as_ref().unwrap().0.clone(),
                toggle.label.clone(),
                toggle.on,
            )),
            _ => None,
        })
        .collect();
    let item =
        |name: &str, label: &str, on: bool| (format!("menu-color-{name}"), label.to_owned(), on);
    assert_eq!(
        items,
        [
            item("red", "🔴 Red", false),
            item("orange", "🟠 Orange", false),
            item("yellow", "🟡 Yellow", false),
            item("green", "🟢 Green", false),
            item("blue", "🔵 Blue", false),
            item("purple", "🟣 Purple", false),
            item("gray", "⚫ Gray", false),
            item("none", "No Color", true),
        ]
    );
    assert!(matches!(submenu.children[8].kind, view::Kind::Separator));
    // A color, chosen, colors the tab that the menu opened over.
    // The pointer's moves over the platform's menu choose nothing: its
    // items' hovers, the menu open still.
    for kind in [UiEventKind::HoverEnter, UiEventKind::HoverLeave] {
        assert!(on(&mut host, &mut plugin, menu.clone(), "menu-close", kind).is_empty());
    }
    let colored = on(
        &mut host,
        &mut plugin,
        menu.clone(),
        "menu-color-blue",
        UiEventKind::Toggle { on: true },
    );
    assert!(
        colored.contains(&Request::SetTabColor {
            tab: TabId(2),
            color: Some(Color::Rgb(0x00, 0x7A, 0xFF)),
        }),
        "{colored:?}"
    );
    // Opened again, of its color ticked; none takes it away.
    on(
        &mut host,
        &mut plugin,
        bar.clone(),
        "tab-2",
        UiEventKind::ContextMenu,
    );
    let tree = host.render(&mut plugin, &menu).expect("the menu's items");
    let ticked: Vec<&str> = tree.children[2]
        .children
        .iter()
        .filter(|item| matches!(&item.kind, view::Kind::Toggle(toggle) if toggle.on))
        .filter_map(|item| item.key.as_ref().map(|key| key.0.as_str()))
        .collect();
    assert_eq!(ticked, ["menu-color-blue"]);
    let cleared = on(
        &mut host,
        &mut plugin,
        menu.clone(),
        "menu-color-none",
        UiEventKind::Toggle { on: true },
    );
    assert!(
        cleared.contains(&Request::SetTabColor {
            tab: TabId(2),
            color: None,
        }),
        "{cleared:?}"
    );
    on(
        &mut host,
        &mut plugin,
        bar.clone(),
        "tab-2",
        UiEventKind::ContextMenu,
    );
    // Its items act on the tab that it opened over.
    let chosen = on(
        &mut host,
        &mut plugin,
        menu.clone(),
        "menu-close-others",
        UiEventKind::Click,
    );
    let closed: Vec<&Request> = chosen
        .iter()
        .filter(|request| matches!(request, Request::CloseTab { .. }))
        .collect();
    assert_eq!(
        closed,
        [&Request::CloseTab {
            tab: TabId(1),
            ask: true,
            keep: 10
        }]
    );
    // Of no menu open, an item does nothing.
    assert!(
        on(
            &mut host,
            &mut plugin,
            menu,
            "menu-close",
            UiEventKind::Click
        )
        .is_empty()
    );
}

#[test]
fn the_bar_shows_a_window_s_tabs_and_its_clicks_select_close_and_open() {
    let (mut host, mut plugin) = host(&[1, 1], 1);
    host.set_keys("tabs.goto", number("index", 1), Some("⌘1"));
    let view = ViewId::TabBar(WINDOW);
    let node = host.render(&mut plugin, &view).expect("a bar of two tabs");
    let mut texts = Vec::new();
    let mut walk = vec![&node];
    while let Some(node) = walk.pop() {
        if let lokyr_plugin_sdk::view::Kind::Text(text) = &node.kind {
            texts.push(text.clone());
        }
        walk.extend(node.children.iter().rev());
    }
    assert_eq!(texts, ["Tab 1", "⌘1", "Tab 2"]);
    let clicked = |host: &mut FakeHost, plugin: &mut Tabs, key: &str| {
        host.take_done();
        host.ui_event(plugin, &click(key)).unwrap();
        host.take_done()
    };
    assert_eq!(
        clicked(&mut host, &mut plugin, "tab-2"),
        [Request::ActivateTab(TabId(2))]
    );
    assert_eq!(
        clicked(&mut host, &mut plugin, "close-1"),
        [Request::CloseTab {
            tab: TabId(1),
            ask: true,
            keep: 10
        }]
    );
    let opened = clicked(&mut host, &mut plugin, bar::NEW_KEY);
    assert!(
        matches!(
            opened[..],
            [
                Request::NewTab {
                    window: WINDOW,
                    index: None,
                    directory: None,
                },
                Request::ActivateTab(_)
            ]
        ),
        "{opened:?}"
    );
    // A window of one tab shows its title in its titlebar, by default, as
    // its render puts it there, and its bar again of a second tab.
    let (mut single, mut plugin) = self::host(&[1], 1);
    let title = single.render(&mut plugin, &view).expect("a title");
    assert_eq!(title.key, Some(bar::TITLE_KEY.into()));
    assert_eq!(single.bar_place(WINDOW), BarPlace::Title);
    let places = |host: &mut FakeHost| -> Vec<Request> {
        host.take_done()
            .into_iter()
            .filter(|request| matches!(request, Request::SetWindowTabBarPlace { .. }))
            .collect()
    };
    // Of the window's title, as the app gives the system's, which dims as
    // the window loses the focus, and comes back as it takes it again.
    single.title_window(WINDOW, "lokyr — zsh — 80×24");
    let title = single.render(&mut plugin, &view).expect("a title");
    let middle = &title.children[1];
    assert!(
        matches!(&middle.children.last().unwrap().kind, view::Kind::Text(text) if text == "lokyr — zsh — 80×24"),
        "{title:?}"
    );
    assert_eq!(
        middle.children.last().unwrap().style.color,
        Some(view::Token::from("text"))
    );
    single.post(Event::WindowFocus {
        window: WINDOW,
        focused: false,
    });
    single.deliver(&mut plugin).unwrap();
    let title = single.render(&mut plugin, &view).expect("a title");
    assert_eq!(
        title.children[1].children.last().unwrap().style.color,
        Some(view::Token::from("text.secondary"))
    );
    single.post(Event::WindowFocus {
        window: WINDOW,
        focused: true,
    });
    single.deliver(&mut plugin).unwrap();
    let title = single.render(&mut plugin, &view).expect("a title");
    assert_eq!(
        title.children[1].children.last().unwrap().style.color,
        Some(view::Token::from("text"))
    );
    // Once, not at every render.
    places(&mut single);
    let _ = single.render(&mut plugin, &view);
    assert_eq!(places(&mut single), []);
    run(&mut single, &mut plugin, NEW, Args::new());
    let bar = single.render(&mut plugin, &view).expect("a bar");
    assert_ne!(bar.key, Some(bar::TITLE_KEY.into()));
    assert_eq!(single.bar_place(WINDOW), BarPlace::Titlebar);
    assert_eq!(
        places(&mut single),
        [Request::SetWindowTabBarPlace {
            window: WINDOW,
            place: None
        }]
    );
}

#[test]
fn a_tab_shown_is_the_one_that_its_bar_scrolls_to() {
    use lokyr_plugin_sdk::event::WorkspaceEvent;
    let (mut host, mut plugin) = host(&[1, 1], 1);
    let view = ViewId::TabBar(WINDOW);
    let _ = host.render(&mut plugin, &view);
    host.take_done();
    host.post(Event::Workspace(WorkspaceEvent::TabActivated {
        tab: TabId(2),
        window: WINDOW,
    }));
    host.deliver(&mut plugin).unwrap();
    let revealed: Vec<Request> = host
        .take_done()
        .into_iter()
        .filter(|request| matches!(request, Request::Reveal { .. }))
        .collect();
    assert_eq!(
        revealed,
        [Request::Reveal {
            view,
            key: "tab-2".into()
        }]
    );
}

#[test]
fn a_tab_dropped_at_another_place_of_its_bar_moves_there() {
    let (mut host, mut plugin) = host(&[1, 1, 1], 1);
    let view = ViewId::TabBar(WINDOW);
    for style in ["standard", "unified", "seamless"] {
        for position in ["top", "left"] {
            host.set_config(Data::Map(BTreeMap::from([
                ("style".to_owned(), Data::Text(style.to_owned())),
                ("position".to_owned(), Data::Text(position.to_owned())),
            ])));
            host.deliver(&mut plugin).unwrap();
            let node = host
                .render(&mut plugin, &view)
                .expect("a bar of three tabs");
            // The tabs move among each other, the button of a new tab not.
            let mut reorderable = Vec::new();
            let mut walk = vec![&node];
            while let Some(node) = walk.pop() {
                if node.reorderable {
                    reorderable.extend(node.key.as_ref().map(|key| key.0.clone()));
                    // Each carries its tab out of the bar.
                    let carried = node.carries.map(|tab| format!("tab-{}", tab.0));
                    assert_eq!(carried, node.key.as_ref().map(|key| key.0.clone()));
                }
                walk.extend(node.children.iter().rev());
            }
            assert_eq!(
                reorderable,
                ["tab-1", "tab-2", "tab-3"],
                "{style} at the {position}"
            );
        }
    }
    host.take_done();
    host.ui_event(
        &mut plugin,
        &UiEvent {
            view,
            key: "tab-3".into(),
            kind: UiEventKind::Reorder { index: 0 },
        },
    )
    .unwrap();
    assert_eq!(
        host.take_done(),
        [Request::MoveTab {
            tab: TabId(3),
            window: WINDOW,
            index: 0
        }]
    );
}

#[test]
fn a_tab_dropped_out_of_its_bar_moves_to_another_window_or_to_one_of_its_own() {
    let (mut host, mut plugin) = host(&[1, 1, 1], 1);
    // Another window, of a tab of its own.
    let other = WindowId(7);
    let pane = host.pane(PaneId(9));
    pane.window = other;
    pane.tab = TabId(9);
    let view = ViewId::TabBar(WINDOW);
    host.render(&mut plugin, &view)
        .expect("a bar of three tabs");
    host.take_done();
    let drop = |host: &mut FakeHost, plugin: &mut Tabs, kind| {
        host.ui_event(
            plugin,
            &UiEvent {
                view: view.clone(),
                key: "tab-2".into(),
                kind,
            },
        )
        .unwrap();
        host.take_done()
    };
    assert_eq!(
        drop(
            &mut host,
            &mut plugin,
            UiEventKind::DropOnBar {
                window: other,
                index: 1
            }
        ),
        [
            Request::MoveTab {
                tab: TabId(2),
                window: other,
                index: 1
            },
            Request::ActivateTab(TabId(2))
        ]
    );
    let at = ScreenPoint { x: 300.0, y: 140.0 };
    assert_eq!(
        drop(
            &mut host,
            &mut plugin,
            UiEventKind::DropOutside { x: at.x, y: at.y }
        ),
        [Request::TabToNewWindow {
            tab: TabId(2),
            at: Some(at)
        }]
    );
    // Into a pane of another tab, beside it, at its edge.
    use lokyr_plugin_sdk::host::Edge;
    let into = host
        .tab_layout(TabId(1))
        .and_then(|layout| match layout {
            lokyr_plugin_sdk::host::Layout::Pane(pane) => Some(pane),
            lokyr_plugin_sdk::host::Layout::Split { .. } => None,
        })
        .expect("the first tab's pane");
    assert_eq!(
        drop(
            &mut host,
            &mut plugin,
            UiEventKind::DropOnPane {
                pane: into,
                edge: Edge::Right
            }
        ),
        [Request::TabIntoPane {
            tab: TabId(2),
            pane: into,
            edge: Edge::Right
        }]
    );
    assert!(matches!(
        host.tab_layout(TabId(1)),
        Some(lokyr_plugin_sdk::host::Layout::Split { .. })
    ));
}

#[test]
fn the_bar_s_own_texts_are_its_catalogs_in_the_language_shown() {
    let (mut host, mut plugin) = host(&[1, 1], 1);
    host.set_message(NEW_TAB, &[], Some("Nuovo tab"))
        .set_message(CLOSE_TAB, &[], Some("Chiudi tab"));
    for index in [1, 2] {
        let title = format!("Scheda {index}");
        host.set_message(UNTITLED, &[("index", Data::Integer(index))], Some(&title));
    }
    let node = host
        .render(&mut plugin, &ViewId::TabBar(WINDOW))
        .expect("a bar of two tabs");
    let mut found = (Vec::new(), Vec::new(), Vec::new());
    let mut walk = vec![&node];
    while let Some(node) = walk.pop() {
        if let lokyr_plugin_sdk::view::Kind::Text(text) = &node.kind {
            found.0.push(text.clone());
        }
        found.1.extend(node.label.clone());
        found.2.extend(node.tooltip.clone());
        walk.extend(node.children.iter().rev());
    }
    assert_eq!(found.0, ["Scheda 1", "Scheda 2"]);
    assert!(found.1.contains(&"Nuovo tab".to_owned()), "{:?}", found.1);
    assert!(found.1.contains(&"Chiudi tab".to_owned()), "{:?}", found.1);
    assert_eq!(found.2, ["Nuovo tab"]);
}

#[test]
fn the_config_puts_the_bar_at_its_place_and_keeps_a_drag_s_width() {
    use lokyr_plugin_sdk::host::BarPlace;

    let placed = |done: Vec<Request>| {
        done.into_iter().find_map(|request| match request {
            Request::SetTabBarPlace(place) => Some(place),
            _ => None,
        })
    };
    let mut host = FakeHost::new(NAME).with_manifest(crate::MANIFEST);
    host.pane(PaneId(1)).tab = TabId(1);
    host.focus(Some(PaneId(1)));
    let mut plugin = Tabs::default();
    host.activate(&mut plugin).unwrap();
    assert_eq!(placed(host.take_done()), Some(BarPlace::Titlebar));
    // On the left, as the config changes, of its width.
    let config = Data::Map(BTreeMap::from([
        ("position".to_owned(), Data::Text("left".to_owned())),
        ("column-width".to_owned(), Data::Integer(240)),
    ]));
    host.set_config(config.clone());
    host.deliver(&mut plugin).unwrap();
    assert_eq!(
        placed(host.take_done()),
        Some(BarPlace::Sidebar { width: 240.0 })
    );
    // The user's drag's width, which the state keeps for the next start.
    host.post(Event::TabBarResized { width: 300.0 });
    host.deliver(&mut plugin).unwrap();
    let Data::Map(state) = host.state().clone() else {
        panic!("a state of the width");
    };
    assert_eq!(state.get("column-width"), Some(&Data::Number(300.0)));
    let mut next = FakeHost::new(NAME).with_manifest(crate::MANIFEST);
    next.pane(PaneId(1)).tab = TabId(1);
    next.set_config(config);
    next.set_state(Data::Map(state));
    let mut again = Tabs::default();
    next.activate(&mut again).unwrap();
    assert_eq!(
        placed(next.take_done()),
        Some(BarPlace::Sidebar { width: 300.0 })
    );
    // The file's new width takes the drag's place at once, and at the
    // next start too, which the state forgets.
    let wider = |width: i64| {
        Data::Map(BTreeMap::from([
            ("position".to_owned(), Data::Text("left".to_owned())),
            ("column-width".to_owned(), Data::Integer(width)),
        ]))
    };
    next.set_config(wider(150));
    next.deliver(&mut again).unwrap();
    assert_eq!(
        placed(next.take_done()),
        Some(BarPlace::Sidebar { width: 150.0 })
    );
    let mut third = FakeHost::new(NAME).with_manifest(crate::MANIFEST);
    third.pane(PaneId(1)).tab = TabId(1);
    third.set_config(wider(150));
    third.set_state(next.state().clone());
    let mut plugin = Tabs::default();
    third.activate(&mut plugin).unwrap();
    assert_eq!(
        placed(third.take_done()),
        Some(BarPlace::Sidebar { width: 150.0 })
    );
    // A drag, then the file changed while Lokyr is closed: the file's.
    third.post(Event::TabBarResized { width: 320.0 });
    third.deliver(&mut plugin).unwrap();
    let mut fourth = FakeHost::new(NAME).with_manifest(crate::MANIFEST);
    fourth.pane(PaneId(1)).tab = TabId(1);
    fourth.set_config(wider(350));
    fourth.set_state(third.state().clone());
    let mut plugin = Tabs::default();
    fourth.activate(&mut plugin).unwrap();
    assert_eq!(
        placed(fourth.take_done()),
        Some(BarPlace::Sidebar { width: 350.0 })
    );
}

#[test]
fn a_closed_window_s_view_goes_and_a_tab_s_index_takes_any_number() {
    use lokyr_plugin_sdk::event::WorkspaceEvent;
    let (mut host, mut plugin) = self::host(&[1], 1);
    let view = ViewId::TabBar(WINDOW);
    let _ = host.render(&mut plugin, &view);
    assert_eq!(plugin.views.len(), 1);
    host.post(Event::Workspace(WorkspaceEvent::WindowClosed {
        window: WINDOW,
    }));
    host.deliver(&mut plugin).unwrap();
    assert!(plugin.views.is_empty(), "{:?}", plugin.views);
    // The smallest index selects no tab, of no overflow.
    let (mut host, mut plugin) = self::host(&[1, 1], 1);
    assert!(run(&mut host, &mut plugin, GOTO, number("index", i64::MIN)).is_empty());
}

#[test]
fn the_guide_tells_every_key_of_the_schema_in_its_example_and_its_section() {
    let guide = include_str!("../../../docs/CONFIGURATION.md");
    let schema = include_str!("../tabs.schema.json");
    // The schema's keys, of its properties' first level.
    let keys: Vec<&str> = schema
        .lines()
        .filter_map(|line| line.strip_prefix("    \"")?.split_once("\": {"))
        .map(|(key, _)| key)
        .collect();
    assert_eq!(
        keys,
        [
            "position",
            "style",
            "hide-when-single",
            "show-index",
            "close-button",
            "column-width",
            "control-tab",
            "reopen-seconds"
        ]
    );
    let example = guide
        .split_once("\n[plugins.tabs]\n")
        .and_then(|(_, rest)| rest.split_once("```"))
        .map(|(example, _)| example)
        .expect("the complete example's section");
    let section = guide
        .split_once("\n## `[plugins.tabs]`\n")
        .map(|(_, rest)| {
            rest.split_once("\n## ")
                .map_or(rest, |(section, _)| section)
        })
        .expect("the guide's section");
    for key in keys {
        assert!(
            example
                .lines()
                .any(|line| line.starts_with(&format!("{key} = "))),
            "`{key}` in the example"
        );
        assert!(
            section.contains(&format!("\n### `{key}`\n")),
            "`{key}` in the section"
        );
    }
}

#[test]
fn control_tab_steps_through_the_order_of_use_while_control_is_held() {
    let (mut host, mut plugin) = host(&[1, 1, 1], 1);
    // Used: the first, the second, then the third.
    run(&mut host, &mut plugin, GOTO, number("index", 2));
    run(&mut host, &mut plugin, GOTO, number("index", 3));
    // Control held: the tab used before, then the one before that, the
    // order held.
    host.hold_tab_order(true);
    let switch = |host: &mut FakeHost, plugin: &mut Tabs, name: &str| {
        run(host, plugin, name, Args::new())
            .into_iter()
            .filter(|request| !matches!(request, Request::RecentTabs(_)))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        switch(&mut host, &mut plugin, SWITCH),
        [Request::ActivateTab(TabId(2))]
    );
    assert_eq!(
        switch(&mut host, &mut plugin, SWITCH),
        [Request::ActivateTab(TabId(1))]
    );
    // Released: the first is the tab used last, then the third.
    host.hold_tab_order(false);
    assert_eq!(
        switch(&mut host, &mut plugin, SWITCH),
        [Request::ActivateTab(TabId(3))]
    );
    // The other way, from the tab used longest ago: the second.
    assert_eq!(
        switch(&mut host, &mut plugin, SWITCH_BACK),
        [Request::ActivateTab(TabId(2))]
    );
    // Of `control-tab = "order"`, the tabs' own order, as before.
    host.set_config(Data::Map(BTreeMap::from([(
        "control-tab".to_owned(),
        Data::Text("order".to_owned()),
    )])));
    host.deliver(&mut plugin).unwrap();
    assert_eq!(
        switch(&mut host, &mut plugin, SWITCH),
        [Request::ActivateTab(TabId(3))]
    );
    assert_eq!(
        switch(&mut host, &mut plugin, SWITCH_BACK),
        [Request::ActivateTab(TabId(2))]
    );
}

#[test]
fn a_pinned_tab_goes_first_narrow_and_its_key_closes_its_pane_alone() {
    let (mut host, mut plugin) = host(&[1, 1], 2);
    assert_eq!(
        run(&mut host, &mut plugin, TOGGLE_PIN, Args::new()),
        [Request::SetTabPinned {
            tab: TabId(2),
            pinned: true
        }]
    );
    // First in the bar, narrow, of its whole title in its tooltip.
    host.deliver(&mut plugin).unwrap();
    let bar = host
        .render(&mut plugin, &ViewId::TabBar(WINDOW))
        .expect("a bar of two tabs");
    let mut found = Vec::new();
    let mut walk = vec![&bar];
    while let Some(node) = walk.pop() {
        if node.carries.is_some() {
            found.push(node.clone());
        }
        walk.extend(node.children.iter().rev());
    }
    assert_eq!(
        found.iter().map(|node| node.carries).collect::<Vec<_>>(),
        [Some(TabId(2)), Some(TabId(1))]
    );
    assert_eq!(
        found[0].layout.width,
        lokyr_plugin_sdk::view::Length::Points(56.0)
    );
    assert!(found[0].tooltip.is_some());
    // Of its pin, which accessibility tells.
    let mut labels = Vec::new();
    let mut walk = vec![&found[0]];
    while let Some(node) = walk.pop() {
        labels.extend(node.label.clone());
        walk.extend(node.children.iter());
    }
    assert!(labels.contains(&"Pinned".to_owned()), "{labels:?}");
    assert_eq!(
        found[1].layout.width,
        lokyr_plugin_sdk::view::Length::Points(0.0)
    );
    // Its key closes no pinned tab of a single pane.
    assert!(run(&mut host, &mut plugin, CLOSE, Args::new()).is_empty());
    // Unpinned, the same command.
    assert_eq!(
        run(&mut host, &mut plugin, TOGGLE_PIN, Args::new()),
        [Request::SetTabPinned {
            tab: TabId(2),
            pinned: false
        }]
    );
    // Of splits, its focused pane.
    let (mut host, mut plugin) = self::host(&[2], 2);
    run(&mut host, &mut plugin, TOGGLE_PIN, Args::new());
    assert_eq!(
        run(&mut host, &mut plugin, CLOSE, Args::new()),
        [Request::ClosePane {
            pane: PaneId(2),
            ask: true
        }]
    );
}

#[test]
fn a_closed_tab_is_kept_for_the_config_s_seconds_and_reopen_gives_it_back() {
    let (mut host, mut plugin) = host(&[1, 1], 2);
    assert_eq!(
        run(&mut host, &mut plugin, REOPEN, Args::new()),
        [Request::ReopenTab(WINDOW)]
    );
    // Of `reopen-seconds = 0`, none kept.
    host.set_config(Data::Map(BTreeMap::from([(
        "reopen-seconds".to_owned(),
        Data::Integer(0),
    )])));
    host.deliver(&mut plugin).unwrap();
    assert_eq!(
        run(&mut host, &mut plugin, CLOSE, Args::new()),
        [Request::CloseTab {
            tab: TabId(2),
            ask: true,
            keep: 0
        }]
    );
}

/// The node of the tab `tab` in `bar`, which carries it.
fn tab_node(bar: &Node, tab: TabId) -> Node {
    let mut walk = vec![bar];
    while let Some(node) = walk.pop() {
        if node.carries == Some(tab) {
            return node.clone();
        }
        walk.extend(node.children.iter());
    }
    panic!("no node of the tab {tab:?}");
}

/// The labels of `node`'s subtree, in their order.
fn labels(node: &Node) -> Vec<String> {
    let mut found = Vec::new();
    let mut walk = vec![node];
    while let Some(node) = walk.pop() {
        found.extend(node.label.clone());
        walk.extend(node.children.iter().rev());
    }
    found
}

#[test]
fn a_pinned_tab_tells_its_whole_title_and_its_agent_and_shows_its_most_urgent_mark() {
    use lokyr_plugin_sdk::fake::TabMarks;
    use lokyr_plugin_sdk::host::AgentState;

    let marks = TabMarks {
        agent: Some(AgentState::Waiting),
        agent_message: Some("Allow the edit?".to_owned()),
        bell: true,
        activity: true,
        running: true,
        secure: true,
        pinned: true,
        ..TabMarks::default()
    };
    for look in ["unified", "seamless"] {
        let (mut host, mut plugin) = host(&[1, 1], 1);
        host.set_config(Data::Map(BTreeMap::from([(
            "style".to_owned(),
            Data::Text(look.to_owned()),
        )])));
        host.pane(PaneId(2)).title = Some("cargo test".to_owned());
        host.set_tab_marks(TabId(2), marks.clone());
        host.deliver(&mut plugin).unwrap();
        let bar = host
            .render(&mut plugin, &ViewId::TabBar(WINDOW))
            .expect("a bar of two tabs");
        let pinned = tab_node(&bar, TabId(2));
        // Its whole title, which its letters cut, and the agent's message,
        // in its tooltip; its title its label to accessibility.
        assert_eq!(
            pinned.tooltip.as_deref(),
            Some("cargo test — Allow the edit?"),
            "{look}"
        );
        assert_eq!(pinned.label.as_deref(), Some("cargo test"), "{look}");
        // Beside its pin, the waiting agent alone, of its marks.
        assert_eq!(
            labels(&pinned),
            ["cargo test", "Pinned", "Agent Waiting: Allow the edit?"],
            "{look}"
        );
        // Of no agent, its bell.
        host.set_tab_marks(
            TabId(2),
            TabMarks {
                agent: None,
                agent_message: None,
                ..marks.clone()
            },
        );
        host.deliver(&mut plugin).unwrap();
        let bar = host
            .render(&mut plugin, &ViewId::TabBar(WINDOW))
            .expect("a bar of two tabs");
        let pinned = tab_node(&bar, TabId(2));
        assert_eq!(pinned.tooltip.as_deref(), Some("cargo test"), "{look}");
        assert_eq!(labels(&pinned), ["cargo test", "Pinned", "Bell"], "{look}");
        // A tab not pinned keeps every mark, and its tooltip the message
        // alone, as it shows its title.
        host.set_tab_marks(
            TabId(2),
            TabMarks {
                pinned: false,
                ..marks.clone()
            },
        );
        host.deliver(&mut plugin).unwrap();
        let bar = host
            .render(&mut plugin, &ViewId::TabBar(WINDOW))
            .expect("a bar of two tabs");
        let shown = tab_node(&bar, TabId(2));
        assert_eq!(shown.tooltip.as_deref(), Some("Allow the edit?"), "{look}");
        let every = labels(&shown);
        for mark in ["Secure Keyboard Entry", "Bell"] {
            assert!(every.contains(&mark.to_owned()), "{look}: {every:?}");
        }
    }
}

#[test]
fn close_other_tabs_spares_the_pinned_tabs() {
    use lokyr_plugin_sdk::fake::TabMarks;

    let (mut host, mut plugin) = host(&[1, 1, 1], 1);
    host.set_tab_marks(
        TabId(2),
        TabMarks {
            pinned: true,
            ..TabMarks::default()
        },
    );
    host.deliver(&mut plugin).unwrap();
    let bar = ViewId::TabBar(WINDOW);
    host.render(&mut plugin, &bar).expect("a bar of three tabs");
    let mut on = |view: ViewId, key: &str, kind| {
        host.take_done();
        host.ui_event(
            &mut plugin,
            &UiEvent {
                view,
                key: key.into(),
                kind,
            },
        )
        .unwrap();
        host.take_done()
    };
    on(bar, "tab-1", UiEventKind::ContextMenu);
    let menu = ViewId::Panel(PanelId::from("tab-menu"));
    let closed: Vec<TabId> = on(menu, "menu-close-others", UiEventKind::Click)
        .iter()
        .filter_map(|request| match request {
            Request::CloseTab { tab, .. } => Some(*tab),
            _ => None,
        })
        .collect();
    assert_eq!(closed, [TabId(3)]);
}

#[test]
fn a_pinned_tab_dropped_into_another_window_s_group_joins_none() {
    let (mut host, mut plugin) = host(&[1, 1, 1], 1);
    // Another window, of two tabs in a group.
    let other = WindowId(7);
    for id in [9, 10] {
        let pane = host.pane(PaneId(id));
        pane.window = other;
        pane.tab = TabId(id);
    }
    let group = host.add_group(&[TabId(9), TabId(10)], "ops", None);
    // The second tab pinned.
    host.focus(Some(PaneId(2)));
    host.command(&mut plugin, "tabs.toggle-pin", Args::new())
        .unwrap();
    let view = ViewId::TabBar(WINDOW);
    host.render(&mut plugin, &view)
        .expect("a bar of three tabs");
    host.take_done();
    // Dropped between the group's tabs: it moves there, of no group, as a
    // pinned tab joins none, rather than failing the drop.
    host.ui_event(
        &mut plugin,
        &UiEvent {
            view,
            key: "tab-2".into(),
            kind: UiEventKind::DropOnBar {
                window: other,
                index: 2,
            },
        },
    )
    .unwrap();
    let done = host.take_done();
    assert!(
        done.iter()
            .any(|request| matches!(request, Request::MoveTab { tab: TabId(2), window, .. } if *window == other)),
        "{done:?}"
    );
    assert!(
        !done
            .iter()
            .any(|request| matches!(request, Request::AddToGroup { .. })),
        "{done:?}"
    );
    let groups = host.groups(other);
    assert_eq!(
        groups.iter().find(|shown| shown.id == group).unwrap().tabs,
        [TabId(9), TabId(10)]
    );
}

#[test]
fn a_tab_dragged_into_a_group_joins_it_out_of_it_leaves_it_and_a_click_folds_it() {
    let (mut host, mut plugin) = host(&[1, 1, 1, 1, 1], 1);
    let group = host.add_group(&[TabId(2), TabId(3)], "work", None);
    host.deliver(&mut plugin).unwrap();
    let bar = ViewId::TabBar(WINDOW);
    host.render(&mut plugin, &bar).expect("a bar of five tabs");
    let mut on = |key: &str, kind| {
        host.take_done();
        host.ui_event(
            &mut plugin,
            &UiEvent {
                view: bar.clone(),
                key: key.into(),
                kind,
            },
        )
        .unwrap();
        host.take_done()
            .into_iter()
            .filter(|request| {
                !matches!(
                    request,
                    Request::Tabs(_) | Request::Groups(_) | Request::Message { .. }
                )
            })
            .collect::<Vec<_>>()
    };
    // The fifth, dropped right after the group's pill: its first.
    assert_eq!(
        on("tab-5", UiEventKind::Reorder { index: 2 }),
        [
            Request::AddToGroup {
                tab: TabId(5),
                group
            },
            Request::MoveTab {
                tab: TabId(5),
                window: WINDOW,
                index: 1
            }
        ]
    );
    // The third, dropped first, before the pill: out of it.
    assert_eq!(
        on("tab-3", UiEventKind::Reorder { index: 0 }),
        [
            Request::RemoveFromGroup(TabId(3)),
            Request::MoveTab {
                tab: TabId(3),
                window: WINDOW,
                index: 0
            }
        ]
    );
    // A click on the pill folds it, and again unfolds it.
    assert_eq!(
        on(&format!("group-{}", group.0), UiEventKind::Click),
        [Request::FoldGroup {
            group,
            folded: true
        }]
    );
    let groups = host.groups(WINDOW);
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].tabs, [TabId(5), TabId(2)]);
    assert!(groups[0].folded);
}

#[test]
fn the_menus_and_the_commands_of_groups_make_change_and_take_them_apart() {
    let (mut host, mut plugin) = host(&[1, 1, 1], 1);
    host.deliver(&mut plugin).unwrap();
    let bar = ViewId::TabBar(WINDOW);
    let tab_menu = ViewId::Panel(PanelId::from("tab-menu"));
    let group_menu = ViewId::Panel(PanelId::from("group-menu"));
    host.render(&mut plugin, &bar).expect("a bar of three tabs");
    let on = |host: &mut FakeHost, plugin: &mut Tabs, view: &ViewId, key: &str, kind| {
        host.take_done();
        host.ui_event(
            plugin,
            &UiEvent {
                view: view.clone(),
                key: key.into(),
                kind,
            },
        )
        .unwrap();
        host.take_done()
            .into_iter()
            .filter(|request| {
                matches!(
                    request,
                    Request::NewGroup { .. }
                        | Request::AddToGroup { .. }
                        | Request::RemoveFromGroup(_)
                        | Request::RenameGroup { .. }
                        | Request::SetGroupColor { .. }
                        | Request::FoldGroup { .. }
                        | Request::Ungroup(_)
                        | Request::AskText { .. }
                )
            })
            .collect::<Vec<_>>()
    };
    // The first tab's menu makes a group of it, of the first system color,
    // and asks for its name, which it takes.
    on(
        &mut host,
        &mut plugin,
        &bar,
        "tab-1",
        UiEventKind::ContextMenu,
    );
    let done = on(
        &mut host,
        &mut plugin,
        &tab_menu,
        "menu-new-group",
        UiEventKind::Click,
    );
    let red = Color::Rgb(0xFF, 0x3B, 0x30);
    assert!(
        matches!(&done[..], [Request::NewGroup { tabs, name, color: Some(color) }, Request::AskText { .. }]
            if tabs == &[TabId(1)] && name.is_empty() && *color == red),
        "{done:?}"
    );
    let groups = host.groups(WINDOW);
    let first = groups[0].id;
    let asked = *plugin.naming.keys().next().unwrap();
    host.post(Event::Answered {
        id: asked,
        text: Some("work".to_owned()),
    });
    host.deliver(&mut plugin).unwrap();
    assert_eq!(host.groups(WINDOW)[0].name, "work");
    // The second tab's menu offers the group, which it joins; the palette's
    // command takes it out again, and adds it to the nearest group.
    host.render(&mut plugin, &bar).expect("a bar");
    on(
        &mut host,
        &mut plugin,
        &bar,
        "tab-2",
        UiEventKind::ContextMenu,
    );
    let menu = host.render(&mut plugin, &tab_menu).expect("the menu");
    let add = format!("menu-add-group-{}", first.0);
    let heading = menu
        .children
        .iter()
        .find_map(|item| match item.children.first() {
            Some(Node {
                kind: view::Kind::Text(text),
                ..
            }) => Some(text.clone()),
            _ => None,
        });
    assert!(
        menu.children.iter().any(|item| item
            .children
            .iter()
            .any(|child| child.key.as_ref().is_some_and(|key| key.0 == add))),
        "{menu:?}"
    );
    assert!(heading.is_some());
    assert_eq!(
        on(&mut host, &mut plugin, &tab_menu, &add, UiEventKind::Click),
        [Request::AddToGroup {
            tab: TabId(2),
            group: first
        }]
    );
    host.focus(Some(PaneId(2)));
    host.keyboard();
    assert_eq!(
        run(&mut host, &mut plugin, REMOVE_FROM_GROUP, Args::new()),
        [Request::RemoveFromGroup(TabId(2))]
    );
    assert_eq!(
        run(&mut host, &mut plugin, ADD_TO_GROUP, Args::new()),
        [Request::AddToGroup {
            tab: TabId(2),
            group: first
        }]
    );
    // Its pill's menu renames it, colors it, folds it and takes it apart.
    let pill = format!("group-{}", first.0);
    let group_item = |host: &mut FakeHost, plugin: &mut Tabs, key: &str| {
        on(host, plugin, &bar, &pill, UiEventKind::ContextMenu);
        on(host, plugin, &group_menu, key, UiEventKind::Click)
    };
    assert!(matches!(
        &group_item(&mut host, &mut plugin, "group-menu-rename")[..],
        [Request::AskText { .. }]
    ));
    assert_eq!(
        group_item(&mut host, &mut plugin, "group-menu-color-blue"),
        [Request::SetGroupColor {
            group: first,
            color: Some(Color::Rgb(0x00, 0x7A, 0xFF))
        }]
    );
    assert_eq!(
        group_item(&mut host, &mut plugin, "group-menu-fold"),
        [Request::FoldGroup {
            group: first,
            folded: true
        }]
    );
    assert_eq!(
        group_item(&mut host, &mut plugin, "group-menu-ungroup"),
        [Request::Ungroup(first)]
    );
    assert!(host.groups(WINDOW).is_empty());
    plugin.naming.clear();
    // The palette's commands of the focused tab's group.
    let group = host.add_group(&[TabId(2)], "ops", None);
    host.focus(Some(PaneId(2)));
    host.keyboard();
    assert_eq!(
        run(&mut host, &mut plugin, RENAME_GROUP, text("title", "build")),
        [Request::RenameGroup {
            group,
            name: "build".to_owned()
        }]
    );
    assert_eq!(
        run(
            &mut host,
            &mut plugin,
            SET_GROUP_COLOR,
            text("color", "#ff8800")
        ),
        [Request::SetGroupColor {
            group,
            color: Some(Color::Rgb(0xff, 0x88, 0x00))
        }]
    );
    assert_eq!(
        run(&mut host, &mut plugin, FOLD_GROUP, Args::new()),
        [Request::FoldGroup {
            group,
            folded: true
        }]
    );
    host.focus(Some(PaneId(2)));
    host.keyboard();
    assert_eq!(
        run(&mut host, &mut plugin, UNGROUP, Args::new()),
        [Request::Ungroup(group)]
    );
}
