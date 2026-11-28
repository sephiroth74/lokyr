//! Find in All Tabs against the SDK's fake host.

use lokyr_plugin_sdk::command::Capability;
use lokyr_plugin_sdk::host::{MOST_SEARCHES, TabId, WindowId};

use super::*;
use crate::all::{FIELD as ALL_FIELD, FIND_ALL, LIST as ALL_LIST, MOVE, PANEL, panel};

/// The panes of the tests: each of its window, its tab, its tab's title
/// and its rows; the first, of the window that has the focus, the
/// others', the second window's number 2.
fn all_host(config: Data) -> (FakeHost, Search) {
    // The app's command that brings a window forward, which Return runs.
    let goto = Command {
        name: "window.goto".to_owned(),
        title: "Go to Window".to_owned(),
        category: Category::Windows,
        capabilities: vec![Capability::WorkspaceWrite],
        params: vec![Param {
            name: "index".to_owned(),
            kind: ParamKind::Integer,
            default: None,
        }],
        when: None,
    };
    let mut host = FakeHost::new(NAME)
        .with_manifest(crate::MANIFEST)
        .with_commands(vec![goto]);
    /// A pane: its id, its window's and its tab's, its tab's title and its
    /// rows.
    type Held<'a> = (PaneId, u64, u64, &'a str, &'a [(i64, &'a str)]);
    let panes: [Held<'_>; 3] = [
        (PANE, 1, 1, "build", &[(0, "error one"), (1, "fine")]),
        (
            PaneId(20),
            1,
            2,
            "logs",
            &[(5, "nothing"), (6, "an Error here")],
        ),
        (
            PaneId(30),
            3,
            4,
            "server",
            &[(2, "error two"), (3, "errors three")],
        ),
    ];
    for (id, window, tab, title, rows) in panes {
        let pane = host.pane(id);
        pane.window = WindowId(window);
        pane.tab = TabId(tab);
        pane.title = Some(title.to_owned());
        for &(row, text) in rows {
            pane.rows.insert(row, (text.to_owned(), false));
        }
    }
    host.number_window(WindowId(3), Some(2), None);
    host.focus(Some(PANE));
    host.set_config(config);
    let mut plugin = Search::default();
    host.activate(&mut plugin).unwrap();
    (host, plugin)
}

/// What `kind` of the node `key` of Find in All Tabs asks of the plugin,
/// its events delivered after it.
fn on_all(host: &mut FakeHost, plugin: &mut Search, key: &str, kind: UiEventKind) -> Vec<Request> {
    host.take_done();
    host.ui_event(
        plugin,
        &UiEvent {
            view: panel(),
            key: Key::from(key),
            kind,
        },
    )
    .unwrap();
    host.deliver(plugin).unwrap();
    host.take_done()
}

/// Types `text` in its field.
fn typed_all(host: &mut FakeHost, plugin: &mut Search, text: &str) -> Vec<Request> {
    let kind = UiEventKind::InputChanged {
        text: text.to_owned(),
        revision: 1,
    };
    on_all(host, plugin, ALL_FIELD, kind)
}

/// Opens it, its view drawn.
fn opened(host: &mut FakeHost, plugin: &mut Search) {
    run(host, plugin, FIND_ALL);
    host.render(plugin, &panel()).expect("Find in All Tabs");
}

/// The rows that its list shows, each of its line and its tab's title, in
/// their order.
fn rows(host: &mut FakeHost, plugin: &mut Search) -> Vec<(String, String)> {
    let tree = host.render(plugin, &panel()).expect("Find in All Tabs");
    let mut left = vec![&tree];
    while let Some(node) = left.pop() {
        if let Kind::List(list) = &node.kind {
            return list
                .items
                .iter()
                .map(|item| (item.texts[0].clone(), item.texts[1].clone()))
                .collect();
        }
        left.extend(&node.children);
    }
    Vec::new()
}

/// The lines of its rows, in their order.
fn lines(host: &mut FakeHost, plugin: &mut Search) -> Vec<String> {
    rows(host, plugin)
        .into_iter()
        .map(|(line, _)| line)
        .collect()
}

/// The text of its node `key`, if it shows one.
fn text_of(host: &mut FakeHost, plugin: &mut Search, key: &str) -> Option<String> {
    let tree = host.render(plugin, &panel()).expect("Find in All Tabs");
    let mut left = vec![&tree];
    while let Some(node) = left.pop() {
        if node.key.as_ref().is_some_and(|own| own.0 == key)
            && let Kind::Text(text) = &node.kind
        {
            return Some(text.clone());
        }
        left.extend(&node.children);
    }
    None
}

#[test]
fn find_in_all_tabs_opens_at_the_window_s_top_its_arrows_of_its_panel_alone() {
    let (mut host, mut plugin) = all_host(Data::Nil);
    let commands: Vec<&Command> = host.commands().iter().collect();
    let find_all = commands.iter().find(|c| c.name == FIND_ALL).unwrap();
    assert_eq!(find_all.category, Category::Find);
    assert!(find_all.params.is_empty());
    let moving = commands.iter().find(|c| c.name == MOVE).unwrap();
    // Its arrows, while its own panel has the keys, not the find bar.
    assert_eq!(moving.when.as_deref(), Some("search.find-all.focused"));
    opened(&mut host, &mut plugin);
    let options = host
        .panels()
        .get(&PanelId::from(PANEL))
        .expect("Find in All Tabs");
    assert_eq!(
        options.anchor,
        Anchor::WindowCorner {
            window: WindowId(1),
            corner: Corner::Top
        }
    );
    assert!(options.close_escape && options.close_outside);
    assert_eq!(options.role, Role::Dialog);
    // Of no text yet, nothing searched; open, its field takes the keys again.
    assert!(rows(&mut host, &mut plugin).is_empty());
    assert!(host.searches().is_empty());
    let again = run(&mut host, &mut plugin, FIND_ALL);
    assert!(
        again
            .iter()
            .any(|request| matches!(request, Request::FocusNode { key, .. } if key.0 == ALL_FIELD)),
        "{again:?}"
    );
}

#[test]
fn find_in_all_tabs_gathers_the_matches_of_every_pane_of_the_session_s_windows() {
    let (mut host, mut plugin) = all_host(Data::Nil);
    opened(&mut host, &mut plugin);
    typed_all(&mut host, &mut plugin, "error");
    // In the windows', the tabs' and the panes' order, then the rows', each
    // of its tab's title at its end; smart case, of a text of no capital.
    let title = |line: &str, title: &str| (line.to_owned(), title.to_owned());
    assert_eq!(
        rows(&mut host, &mut plugin),
        [
            title("error one", "build"),
            title("an Error here", "logs"),
            title("error two", "server"),
            title("errors three", "server"),
        ]
    );
    // The arrows move the selection, not past the ends.
    let delta = |delta: i64| Args::from([("delta".to_owned(), Value::Integer(delta))]);
    host.take_done();
    host.command(&mut plugin, MOVE, delta(2)).unwrap();
    assert!(host.take_done().iter().any(|request| matches!(
        request,
        Request::Select { key, item: 2, .. } if key.0 == ALL_LIST
    )));
    host.command(&mut plugin, MOVE, delta(10)).unwrap();
    assert_eq!(plugin.all.as_ref().unwrap().selected, 3);
    host.command(&mut plugin, MOVE, delta(-10)).unwrap();
    assert_eq!(plugin.all.as_ref().unwrap().selected, 0);
    // A text of no match, and none, list nothing, the searches ended.
    typed_all(&mut host, &mut plugin, "absent");
    assert!(rows(&mut host, &mut plugin).is_empty());
    assert_eq!(
        text_of(&mut host, &mut plugin, "none").as_deref(),
        Some("No results")
    );
    typed_all(&mut host, &mut plugin, "");
    assert!(rows(&mut host, &mut plugin).is_empty());
    assert!(host.searches().is_empty());
    // Escape ends them too.
    typed_all(&mut host, &mut plugin, "error");
    assert!(!host.searches().is_empty());
    host.close_panel(&PanelId::from(PANEL));
    host.deliver(&mut plugin).unwrap();
    assert!(host.searches().is_empty());
    assert!(plugin.all.is_none());
}

#[test]
fn return_goes_to_the_match_s_pane_and_opens_the_find_bar_there_of_it() {
    let (mut host, mut plugin) = all_host(Data::Nil);
    opened(&mut host, &mut plugin);
    typed_all(&mut host, &mut plugin, "error");
    // The third, of the other window's pane.
    let delta = Args::from([("delta".to_owned(), Value::Integer(2))]);
    host.command(&mut plugin, MOVE, delta).unwrap();
    let requests = on_all(
        &mut host,
        &mut plugin,
        ALL_FIELD,
        UiEventKind::Submit {
            text: "error".to_owned(),
            shift: false,
        },
    );
    assert!(
        requests.contains(&Request::ActivateTab(TabId(4))),
        "{requests:?}"
    );
    assert!(
        requests.contains(&Request::FocusPane(PaneId(30))),
        "{requests:?}"
    );
    // Its window, by its number.
    assert!(
        requests.iter().any(|request| matches!(
            request,
            Request::RunCommand { name, args }
                if name == "window.goto" && args.get("index") == Some(&Value::Integer(2))
        )),
        "{requests:?}"
    );
    assert_eq!(selected(&requests), [((0, 2), (4, 2))]);
    // Its panel closed, its searches ended, and the find bar open at the
    // pane, of its text, the match its current one.
    assert!(!host.panels().contains_key(&PanelId::from(PANEL)));
    assert!(plugin.all.is_none());
    let options = host.panels().get(&PanelId::from(BAR)).expect("the bar");
    assert_eq!(
        options.anchor,
        Anchor::PaneCorner {
            pane: PaneId(30),
            corner: Corner::TopRight
        }
    );
    let find = plugin.find.as_ref().expect("the bar's search");
    assert_eq!((find.pane, find.text.as_str()), (PaneId(30), "error"));
    assert_eq!(host.searches().len(), 1);
    assert_eq!(current(&requests), [((0, 2), (4, 2))]);
    // Its closing, which it asked for, leaves the bar's search.
    host.deliver(&mut plugin).unwrap();
    assert_eq!(host.searches().len(), 1);
    // A list of no match takes Return as nothing.
    run(&mut host, &mut plugin, FIND_ALL);
    typed_all(&mut host, &mut plugin, "absent");
    let none = on_all(
        &mut host,
        &mut plugin,
        ALL_FIELD,
        UiEventKind::Submit {
            text: "absent".to_owned(),
            shift: false,
        },
    );
    assert!(
        !none
            .iter()
            .any(|request| matches!(request, Request::ClosePanel(_)))
    );
    assert!(plugin.all.is_some());
}

/// The query of Find in All Tabs' searches, if it searches.
fn all_query(plugin: &Search) -> Option<SearchQuery> {
    let all = plugin.all.as_ref()?;
    (!all.searches.is_empty()).then(|| crate::query_of(&all.query, plugin.options, plugin.settings))
}

#[test]
fn find_in_all_tabs_searches_of_the_find_bar_s_toggles_and_its_case() {
    let (mut host, mut plugin) = all_host(Data::Nil);
    opened(&mut host, &mut plugin);
    typed_all(&mut host, &mut plugin, "error");
    // Case counted: the capital's line left out, and the toggle kept for
    // the find bar's next search.
    on_all(
        &mut host,
        &mut plugin,
        CASE,
        UiEventKind::Toggle { on: true },
    );
    assert_eq!(
        lines(&mut host, &mut plugin),
        ["error one", "error two", "errors three"]
    );
    assert!(plugin.options.sensitive);
    // Whole words and regexes, each pane searched anew of them.
    let before: Vec<u64> = plugin
        .all
        .as_ref()
        .unwrap()
        .searches
        .keys()
        .copied()
        .collect();
    on_all(
        &mut host,
        &mut plugin,
        WORD,
        UiEventKind::Toggle { on: true },
    );
    on_all(
        &mut host,
        &mut plugin,
        REGEX,
        UiEventKind::Toggle { on: true },
    );
    let wanted = SearchQuery {
        text: "error".to_owned(),
        regex: true,
        case: SearchCase::Sensitive,
        whole_word: true,
    };
    assert_eq!(all_query(&plugin), Some(wanted.clone()));
    let searches = &plugin.all.as_ref().unwrap().searches;
    assert_eq!(searches.len(), 3);
    assert!(searches.keys().all(|id| !before.contains(id)));
    // The toggles show their states.
    let tree = host
        .render(&mut plugin, &panel())
        .expect("Find in All Tabs");
    let mut left = vec![&tree];
    let mut on = Vec::new();
    while let Some(node) = left.pop() {
        if let Kind::Toggle(toggle) = &node.kind
            && toggle.on
        {
            on.push(node.key.as_ref().unwrap().0.clone());
        }
        left.extend(&node.children);
    }
    on.sort();
    assert_eq!(on, [CASE, REGEX, WORD]);
    // Return opens the find bar of the same options, which finds the match.
    let requests = on_all(
        &mut host,
        &mut plugin,
        ALL_FIELD,
        UiEventKind::Submit {
            text: "error".to_owned(),
            shift: false,
        },
    );
    assert_eq!(query(&plugin), Some(wanted));
    assert_eq!(current(&requests), [((0, 0), (4, 0))]);
    // Of the bar's own options, too, as it keeps them.
    let mut again = FakeHost::new(NAME).with_manifest(crate::MANIFEST);
    again.set_state(host.state().clone());
    let mut kept = Search::default();
    again.activate(&mut kept).unwrap();
    assert!(kept.options.sensitive && kept.options.regex && kept.options.whole_word);
}

#[test]
fn a_regex_that_does_not_compile_tells_so_in_place_of_no_results() {
    let texts = AllTexts {
        none: "No results".to_owned(),
        invalid: "Invalid regex".to_owned(),
        ..AllTexts::default()
    };
    let told = |invalid: bool| {
        let tree = find_all(
            &AllView {
                invalid,
                ..AllView::default()
            },
            &texts,
        );
        let mut left = vec![&tree];
        while let Some(node) = left.pop() {
            if node.key.as_ref().is_some_and(|key| key.0 == "none")
                && let Kind::Text(text) = &node.kind
            {
                return text.clone();
            }
            left.extend(&node.children);
        }
        panic!("no line of none")
    };
    assert_eq!(told(false), "No results");
    assert_eq!(told(true), "Invalid regex");
}

#[test]
fn find_in_all_tabs_searches_insensitive_of_the_config_s_case() {
    let insensitive = config(&[("case", Data::Text("insensitive".to_owned()))]);
    let (mut host, mut plugin) = all_host(insensitive);
    opened(&mut host, &mut plugin);
    typed_all(&mut host, &mut plugin, "ERROR");
    assert_eq!(rows(&mut host, &mut plugin).len(), 4);
}

#[test]
fn find_in_all_tabs_searches_more_panes_than_a_call_starts_searches() {
    let (mut host, mut plugin) = host(&[], Data::Nil);
    let more = MOST_SEARCHES + 4;
    for at in 0..more {
        let id = if at == 0 {
            PANE
        } else {
            PaneId(100 + at as u64)
        };
        let pane = host.pane(id);
        pane.tab = TabId(1);
        pane.rows.insert(0, (format!("needle {at}"), false));
    }
    opened(&mut host, &mut plugin);
    typed_all(&mut host, &mut plugin, "needle");
    // The first ones' parts bring the next ones' searches, at each call.
    for _ in 0..4 {
        host.deliver(&mut plugin).unwrap();
    }
    assert_eq!(rows(&mut host, &mut plugin).len(), more);
}

#[test]
fn find_in_all_tabs_and_the_find_bar_close_each_other_as_one_opens() {
    let (mut host, mut plugin) = all_host(Data::Nil);
    run(&mut host, &mut plugin, OPEN);
    focused(&mut host, &mut plugin);
    typed(&mut host, &mut plugin, "fine");
    let bar_search = plugin.find.as_ref().unwrap().search.unwrap();
    // Its search of the bar's pane would replace the bar's: the bar closes
    // as Escape closes it, its match selected and its text kept.
    run(&mut host, &mut plugin, FIND_ALL);
    assert!(!host.panels().contains_key(&PanelId::from(BAR)));
    assert!(plugin.find.is_none());
    assert!(!host.searches().contains(&bar_search));
    assert_eq!(plugin.recent.first().map(String::as_str), Some("fine"));
    host.render(&mut plugin, &panel())
        .expect("Find in All Tabs");
    typed_all(&mut host, &mut plugin, "error");
    assert_eq!(rows(&mut host, &mut plugin).len(), 4);
    // The bar's opening closes it, its searches ended.
    let reopened = run(&mut host, &mut plugin, OPEN);
    assert!(reopened.contains(&Request::ClosePanel(PanelId::from(PANEL))));
    assert!(plugin.all.is_none());
    assert_eq!(host.searches().len(), 1);
    assert!(host.panels().contains_key(&PanelId::from(BAR)));
    // Its closing, which it asked for, leaves the bar.
    host.deliver(&mut plugin).unwrap();
    assert!(plugin.find.is_some());
    assert_eq!(host.searches().len(), 1);
}
