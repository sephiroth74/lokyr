//! The plugin against the SDK's fake host, which records what it does.

use lokyr_plugin_sdk::fake::FakeHost;
use lokyr_plugin_sdk::message::Request;
use lokyr_plugin_sdk::view::{Key, Kind};

use super::*;

/// The plugin's name, as its calls give it.
const NAME: &str = "lokyr/search";

/// The pane of the tests.
const PANE: PaneId = PaneId(1);

/// A host of one pane, of `rows`' texts from the row numbered 0, focused,
/// of `config`, and the plugin, activated.
fn host(rows: &[&str], config: Data) -> (FakeHost, Search) {
    let mut host = FakeHost::new(NAME);
    let pane = host.pane(PANE);
    for (row, text) in (0..).zip(rows) {
        pane.rows.insert(row, ((*text).to_owned(), false));
    }
    host.focus(Some(PANE));
    host.set_config(config);
    let mut plugin = Search::default();
    host.activate(&mut plugin).unwrap();
    (host, plugin)
}

/// The config of `pairs`.
fn config(pairs: &[(&str, Data)]) -> Data {
    Data::Map(
        pairs
            .iter()
            .map(|(name, value)| ((*name).to_owned(), value.clone()))
            .collect(),
    )
}

/// What `event` of the bar's node `key` asks of the plugin, its events
/// delivered after it.
fn on_bar(host: &mut FakeHost, plugin: &mut Search, key: &str, kind: UiEventKind) -> Vec<Request> {
    host.take_done();
    host.ui_event(
        plugin,
        &UiEvent {
            view: bar(),
            key: Key::from(key),
            kind,
        },
    )
    .unwrap();
    host.deliver(plugin).unwrap();
    host.take_done()
}

/// Types `text` in the bar's field.
fn typed(host: &mut FakeHost, plugin: &mut Search, text: &str) -> Vec<Request> {
    let kind = UiEventKind::InputChanged {
        text: text.to_owned(),
        revision: 1,
    };
    on_bar(host, plugin, FIELD, kind)
}

/// Return, or Shift-Return, in the bar's field.
fn returned(host: &mut FakeHost, plugin: &mut Search, shift: bool) -> Vec<Request> {
    let text = plugin.find.as_ref().unwrap().text.clone();
    on_bar(host, plugin, FIELD, UiEventKind::Submit { text, shift })
}

/// Runs `name`, its events delivered after it, and returns its requests.
fn run(host: &mut FakeHost, plugin: &mut Search, name: &str) -> Vec<Request> {
    host.take_done();
    host.command(plugin, name, Args::new()).unwrap();
    host.deliver(plugin).unwrap();
    host.take_done()
}

/// The cells that `requests` select, the first's and the last's.
fn selected(requests: &[Request]) -> Vec<Cells> {
    requests
        .iter()
        .filter_map(|request| match request {
            Request::SelectCells { first, last, .. } => Some((*first, *last)),
            _ => None,
        })
        .collect()
}

/// A match's first and last cells.
type Cells = ((u16, i64), (u16, i64));

/// The current matches that `requests` show, the first's and the last's
/// cells, with whether every match shows too.
fn shown(requests: &[Request]) -> Vec<(Cells, bool)> {
    requests
        .iter()
        .filter_map(|request| match request {
            Request::ShowMatches {
                current: Some(found),
                all,
                ..
            } => Some(((found.first, found.last), *all)),
            _ => None,
        })
        .collect()
}

/// The current matches that `requests` show, the first's and the last's
/// cells.
fn current(requests: &[Request]) -> Vec<Cells> {
    shown(requests)
        .into_iter()
        .map(|(cells, _)| cells)
        .collect()
}

/// The text of the bar's count, as it renders.
fn count(host: &mut FakeHost, plugin: &mut Search) -> String {
    let tree = host.render(plugin, &bar()).expect("the bar");
    let mut left = vec![&tree];
    while let Some(node) = left.pop() {
        if node.key.as_ref().is_some_and(|key| key.0 == COUNT)
            && let Kind::Text(text) = &node.kind
        {
            return text.clone();
        }
        left.extend(&node.children);
    }
    panic!("no count")
}

/// The query of the bar's search, if it searches.
fn query(plugin: &Search) -> Option<SearchQuery> {
    let find = plugin.find.as_ref()?;
    find.search?;
    Some(find.query(plugin.settings))
}

/// The bar's field takes the keys, its view drawn, as it opens.
fn focused(host: &mut FakeHost, plugin: &mut Search) -> Vec<Request> {
    host.render(plugin, &bar()).expect("the bar");
    on_bar(host, plugin, FIELD, UiEventKind::Focus)
}

/// Rows of three matches of `needle`, on the rows numbered 0, 2 and 3.
const ROWS: [&str; 4] = ["a needle", "nothing", "needle one", "two needle"];

#[test]
fn the_bar_opens_at_the_pane_s_top_right_and_searches_as_its_text_is_typed() {
    let (mut host, mut plugin) = host(&ROWS, Data::Nil);
    let commands: Vec<&str> = host.commands().iter().map(|c| c.name.as_str()).collect();
    assert_eq!(commands, [OPEN, NEXT, PREVIOUS]);
    let opened = run(&mut host, &mut plugin, OPEN);
    let options = host.panels().get(&PanelId::from(BAR)).expect("the bar");
    assert_eq!(
        options.anchor,
        Anchor::PaneCorner {
            pane: PANE,
            corner: Corner::TopRight
        }
    );
    assert_eq!((options.role, options.close_escape), (Role::Dialog, true));
    // No text yet: no search, nothing to fill.
    assert!(current(&opened).is_empty());
    assert_eq!(query(&plugin), None);
    assert!(focused(&mut host, &mut plugin).is_empty());
    assert_eq!(count(&mut host, &mut plugin), "No results");
    // Typed: a search of it, smart of its case; its matches shown, the
    // last of the first part, the nearest to the bottom, the current one,
    // nothing selected.
    let asked = typed(&mut host, &mut plugin, "needle");
    assert_eq!(host.searches().len(), 1);
    assert_eq!(
        query(&plugin),
        Some(SearchQuery {
            text: "needle".to_owned(),
            regex: false,
            case: SearchCase::Smart,
            whole_word: false,
        })
    );
    assert_eq!(shown(&asked), [(((4, 3), (9, 3)), true)]);
    assert!(selected(&asked).is_empty());
    assert_eq!(count(&mut host, &mut plugin), "3 of 3");
    // Of no match.
    typed(&mut host, &mut plugin, "nowhere");
    assert_eq!(count(&mut host, &mut plugin), "No results");
}

#[test]
fn return_and_the_commands_step_up_and_down_through_the_matches_around_their_ends() {
    let (mut host, mut plugin) = host(&ROWS, Data::Nil);
    run(&mut host, &mut plugin, OPEN);
    typed(&mut host, &mut plugin, "needle");
    // Up, the next, older, then past the first to the last.
    let up = |host: &mut FakeHost, plugin: &mut Search| current(&returned(host, plugin, false));
    assert_eq!(up(&mut host, &mut plugin), [((0, 2), (5, 2))]);
    assert_eq!(count(&mut host, &mut plugin), "2 of 3");
    assert_eq!(up(&mut host, &mut plugin), [((2, 0), (7, 0))]);
    assert_eq!(up(&mut host, &mut plugin), [((4, 3), (9, 3))]);
    // Down, the previous, newer, with Shift-Return and the command, past
    // the last to the first.
    assert_eq!(
        current(&returned(&mut host, &mut plugin, true)),
        [((2, 0), (7, 0))]
    );
    assert_eq!(
        current(&run(&mut host, &mut plugin, PREVIOUS)),
        [((0, 2), (5, 2))]
    );
    assert_eq!(
        current(&run(&mut host, &mut plugin, NEXT)),
        [((2, 0), (7, 0))]
    );
    // The buttons.
    assert_eq!(
        current(&on_bar(&mut host, &mut plugin, DOWN, UiEventKind::Click)),
        [((0, 2), (5, 2))]
    );
    assert_eq!(
        current(&on_bar(&mut host, &mut plugin, UP, UiEventKind::Click)),
        [((2, 0), (7, 0))]
    );
}

#[test]
fn without_wrapping_around_a_step_past_an_end_stays_there() {
    let wrap = config(&[("wrap-around", Data::Boolean(false))]);
    let (mut host, mut plugin) = host(&ROWS, wrap);
    run(&mut host, &mut plugin, OPEN);
    typed(&mut host, &mut plugin, "needle");
    // At the last, down stays.
    assert_eq!(
        current(&returned(&mut host, &mut plugin, true)),
        [((4, 3), (9, 3))]
    );
    returned(&mut host, &mut plugin, false);
    returned(&mut host, &mut plugin, false);
    // At the first, up stays.
    assert_eq!(
        current(&returned(&mut host, &mut plugin, false)),
        [((2, 0), (7, 0))]
    );
}

#[test]
fn japanese_typed_searches_and_the_toggles_search_again_of_their_options() {
    let (mut host, mut plugin) = host(&["日本語のテキスト", "Needle", "needle"], Data::Nil);
    run(&mut host, &mut plugin, OPEN);
    let asked = typed(&mut host, &mut plugin, "日本");
    assert_eq!(
        query(&plugin).map(|query| query.text),
        Some("日本".to_owned())
    );
    assert_eq!(current(&asked), [((0, 0), (1, 0))]);
    // Case counts of a capital letter, smart; and of the toggle.
    typed(&mut host, &mut plugin, "needle");
    assert_eq!(count(&mut host, &mut plugin), "2 of 2");
    on_bar(
        &mut host,
        &mut plugin,
        CASE,
        UiEventKind::Toggle { on: true },
    );
    assert_eq!(
        query(&plugin).map(|query| query.case),
        Some(SearchCase::Sensitive)
    );
    assert_eq!(count(&mut host, &mut plugin), "1 of 1");
    on_bar(
        &mut host,
        &mut plugin,
        WORD,
        UiEventKind::Toggle { on: true },
    );
    assert!(query(&plugin).is_some_and(|query| query.whole_word));
    on_bar(
        &mut host,
        &mut plugin,
        REGEX,
        UiEventKind::Toggle { on: true },
    );
    assert!(query(&plugin).is_some_and(|query| query.regex && query.whole_word));
    // The searches before ended, the last alone kept.
    assert_eq!(host.searches().len(), 1);
}

#[test]
fn escape_ends_the_search_and_its_text_opens_the_bar_again_among_the_recent_ones() {
    let (mut host, mut plugin) = host(&ROWS, Data::Nil);
    run(&mut host, &mut plugin, OPEN);
    typed(&mut host, &mut plugin, "needle");
    let search = plugin.find.as_ref().unwrap().search.unwrap();
    // Escape closes the panel, which the plugin hears: the search ends,
    // its highlights going, its current match selected.
    host.take_done();
    host.close_panel(&PanelId::from(BAR));
    host.deliver(&mut plugin).unwrap();
    let closed = host.take_done();
    assert!(!host.searches().contains(&search));
    assert_eq!(selected(&closed), [((4, 3), (9, 3))]);
    assert!(current(&closed).is_empty());
    assert!(plugin.find.is_none());
    let Data::Map(state) = host.state() else {
        panic!("a state: {:?}", host.state());
    };
    assert_eq!(
        state.get(STATE_RECENT),
        Some(&Data::List(vec![Data::Text("needle".to_owned())]))
    );
    // Opened again, of its last text, searched again, its field filled as
    // it takes the keys.
    let opened = run(&mut host, &mut plugin, OPEN);
    assert_eq!(current(&opened), [((4, 3), (9, 3))]);
    let filled = focused(&mut host, &mut plugin);
    assert!(filled.iter().any(|request| matches!(
        request,
        Request::SetText { text, .. } if text == "needle"
    )));
    // Its text selected after, as the field takes the focus again.
    assert!(matches!(
        filled.last(),
        Some(Request::FocusNode { key, .. }) if key.0 == FIELD
    ));
    // The recent searches' menu, whose item searches its text again.
    typed(&mut host, &mut plugin, "one");
    returned(&mut host, &mut plugin, false);
    on_bar(&mut host, &mut plugin, PAST, UiEventKind::Click);
    let menu = host.panels().get(&PanelId::from(RECENT)).expect("the menu");
    assert_eq!(menu.role, Role::Menu);
    let items = host
        .render(&mut plugin, &ViewId::Panel(PanelId::from(RECENT)))
        .unwrap();
    let labels: Vec<String> = items
        .children
        .iter()
        .filter_map(|item| match &item.kind {
            Kind::Button(button) => Some(button.label.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(labels, ["one", "needle"]);
    host.take_done();
    host.ui_event(
        &mut plugin,
        &UiEvent {
            view: ViewId::Panel(PanelId::from(RECENT)),
            key: Key::from("recent-1"),
            kind: UiEventKind::Click,
        },
    )
    .unwrap();
    host.deliver(&mut plugin).unwrap();
    let chosen = host.take_done();
    assert!(chosen.iter().any(|request| matches!(
        request,
        Request::SetText { text, .. } if text == "needle"
    )));
    assert_eq!(
        query(&plugin).map(|query| query.text),
        Some("needle".to_owned())
    );
}

#[test]
fn command_f_on_an_open_bar_gives_its_field_the_keys_and_a_step_opens_a_closed_one() {
    let (mut host, mut plugin) = host(&ROWS, Data::Nil);
    // A step of no bar opens it, of no text yet.
    run(&mut host, &mut plugin, NEXT);
    assert!(host.panels().contains_key(&PanelId::from(BAR)));
    typed(&mut host, &mut plugin, "needle");
    let search = plugin.find.as_ref().unwrap().search;
    // ⌘F again: the bar stays as it is, its field taking the keys again,
    // its search kept.
    host.render(&mut plugin, &bar()).expect("the bar");
    let again = run(&mut host, &mut plugin, OPEN);
    assert_eq!(
        again,
        [Request::FocusNode {
            view: bar(),
            key: Key::from(FIELD),
        }]
    );
    assert_eq!(plugin.find.as_ref().unwrap().search, search);
    assert_eq!(count(&mut host, &mut plugin), "3 of 3");
    // At another pane, it moves there, its search ending, its text
    // searched there again.
    host.pane(PaneId(2));
    host.focus(Some(PaneId(2)));
    let moved = run(&mut host, &mut plugin, OPEN);
    assert!(moved.contains(&Request::ClosePanel(PanelId::from(BAR))));
    assert_eq!(plugin.find.as_ref().unwrap().pane, PaneId(2));
    assert!(!host.searches().contains(&search.unwrap()));
    assert!(host.panels().contains_key(&PanelId::from(BAR)));
}

#[test]
fn the_bar_opens_of_the_options_of_the_last_search_as_the_state_keeps_them() {
    let (mut host, mut plugin) = host(&ROWS, Data::Nil);
    run(&mut host, &mut plugin, OPEN);
    typed(&mut host, &mut plugin, "needle");
    on_bar(
        &mut host,
        &mut plugin,
        REGEX,
        UiEventKind::Toggle { on: true },
    );
    on_bar(
        &mut host,
        &mut plugin,
        WORD,
        UiEventKind::Toggle { on: true },
    );
    host.close_panel(&PanelId::from(BAR));
    host.deliver(&mut plugin).unwrap();
    // Opened again, of the same options, as its toggles show.
    run(&mut host, &mut plugin, OPEN);
    let options = Options {
        sensitive: false,
        regex: true,
        whole_word: true,
    };
    assert_eq!(plugin.find.as_ref().unwrap().options, options);
    let tree = host.render(&mut plugin, &bar()).expect("the bar");
    let mut on = Vec::new();
    let mut left = vec![&tree];
    while let Some(node) = left.pop() {
        if let Kind::Toggle(toggle) = &node.kind
            && toggle.on
        {
            on.push(node.key.as_ref().unwrap().0.clone());
        }
        left.extend(&node.children);
    }
    on.sort();
    assert_eq!(on, [REGEX, WORD]);
    // A plugin made anew, as at the app's next start, takes them from its
    // state.
    let state = host.state().clone();
    let mut again = FakeHost::new(NAME);
    again.pane(PANE);
    again.focus(Some(PANE));
    again.set_state(state);
    let mut made = Search::default();
    again.activate(&mut made).unwrap();
    again.command(&mut made, OPEN, Args::new()).unwrap();
    assert_eq!(made.find.as_ref().unwrap().options, options);
    assert_eq!(made.find.as_ref().unwrap().text, "needle");
}

#[test]
fn the_count_keeps_its_width_of_short_numbers_past_9999() {
    assert_eq!(short(9999), "9999");
    assert_eq!(short(12_345), "12K");
    assert_eq!(short(100_000), "100K");
    assert_eq!(short(3_400_000), "3M");
}

#[test]
fn without_highlight_all_the_current_match_alone_shows_until_the_config_says_otherwise() {
    let off = config(&[("highlight-all", Data::Boolean(false))]);
    let (mut host, mut plugin) = host(&ROWS, off);
    run(&mut host, &mut plugin, OPEN);
    let asked = typed(&mut host, &mut plugin, "needle");
    assert_eq!(shown(&asked), [(((4, 3), (9, 3)), false)]);
    let stepped = returned(&mut host, &mut plugin, false);
    assert_eq!(shown(&stepped), [(((0, 2), (5, 2)), false)]);
    // The config changed: every match shows, the current one kept.
    host.take_done();
    host.set_config(config(&[("highlight-all", Data::Boolean(true))]));
    host.deliver(&mut plugin).unwrap();
    assert_eq!(shown(&host.take_done()), [(((0, 2), (5, 2)), true)]);
}

#[test]
fn a_recent_search_s_item_ends_in_an_ellipsis_past_its_most_characters_of_whole_clusters() {
    assert_eq!(label("short", 40), "short");
    let long = "a".repeat(50);
    let shown = label(&long, 40);
    assert_eq!(shown.chars().count(), 40);
    assert!(shown.ends_with('\u{2026}'));
    // A mark at the cut goes with its letter, before the ellipsis.
    let marked = format!("{}e\u{301}yyy", "x".repeat(38));
    assert_eq!(label(&marked, 40), format!("{}\u{2026}", "x".repeat(38)));
    // A joined emoji at the cut goes whole.
    let joined = format!("{}\u{1F469}\u{200D}\u{1F4BB}zz", "x".repeat(37));
    assert_eq!(label(&joined, 40), format!("{}\u{2026}", "x".repeat(37)));
    // The space before the cut goes too.
    let spaced = format!("{} {}", "x".repeat(38), "y".repeat(5));
    assert_eq!(label(&spaced, 40), format!("{}\u{2026}", "x".repeat(38)));
}

#[test]
fn the_recent_menu_shows_a_long_search_short_and_searches_it_whole() {
    let (mut host, mut plugin) = host(&["needle"], Data::Nil);
    run(&mut host, &mut plugin, OPEN);
    let long = format!("needle {}", "z".repeat(60));
    typed(&mut host, &mut plugin, &long);
    returned(&mut host, &mut plugin, false);
    host.render(&mut plugin, &bar()).expect("the bar");
    on_bar(&mut host, &mut plugin, PAST, UiEventKind::Click);
    let recent = ViewId::Panel(PanelId::from(RECENT));
    let menu = host.render(&mut plugin, &recent).expect("the recent menu");
    let Kind::Button(button) = &menu.children[0].kind else {
        panic!("a button: {:?}", menu.children[0]);
    };
    assert_eq!(button.label, label(&long, RECENT_LABEL));
    assert_eq!(button.label.chars().count(), RECENT_LABEL);
    let item = UiEvent {
        view: recent,
        key: Key::from(format!("{PAST_ITEM}0").as_str()),
        kind: UiEventKind::Click,
    };
    host.ui_event(&mut plugin, &item).expect("the item's click");
    host.deliver(&mut plugin).unwrap();
    assert_eq!(query(&plugin).map(|query| query.text), Some(long));
}

#[test]
fn the_count_of_before_stays_until_a_new_search_tells_something_sure() {
    let (mut host, mut plugin) = host(&ROWS, Data::Nil);
    run(&mut host, &mut plugin, OPEN);
    // No text: no result; then the first search, which leaves it until
    // it tells.
    assert_eq!(count(&mut host, &mut plugin), "No results");
    host.ui_event(
        &mut plugin,
        &UiEvent {
            view: bar(),
            key: Key::from(FIELD),
            kind: UiEventKind::InputChanged {
                text: "needle".to_owned(),
                revision: 1,
            },
        },
    )
    .unwrap();
    assert_eq!(count(&mut host, &mut plugin), "No results");
    host.deliver(&mut plugin).unwrap();
    assert_eq!(count(&mut host, &mut plugin), "3 of 3");
    // A key typed, and a toggle: the count of before stays until the new
    // search tells, rather than flicker.
    for kind in [
        UiEventKind::InputChanged {
            text: "needles".to_owned(),
            revision: 2,
        },
        UiEventKind::Toggle { on: true },
    ] {
        let key = match kind {
            UiEventKind::Toggle { .. } => CASE,
            _ => FIELD,
        };
        let before = count(&mut host, &mut plugin);
        host.ui_event(
            &mut plugin,
            &UiEvent {
                view: bar(),
                key: Key::from(key),
                kind,
            },
        )
        .unwrap();
        assert_eq!(count(&mut host, &mut plugin), before);
        host.deliver(&mut plugin).unwrap();
        assert_eq!(count(&mut host, &mut plugin), "No results");
    }
}
