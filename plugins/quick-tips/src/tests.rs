//! The plugin against the SDK's fake host, of one window, whose day, keys,
//! registry and notice the tests give; its messages are their ids, as the
//! fake host has no catalogs.

use lokyr_plugin_sdk::fake::{self, FakeHost};
use lokyr_plugin_sdk::host::{CommandInfo, PaneId};

use super::*;

/// The plugin's name, as its calls give it.
const NAME: &str = "lokyr/quick-tips";

/// A host of one window of one pane, focused, of `day`, the palette's keys
/// bound, the plugin's state `state`, and the plugin, activated.
fn host(day: u64, state: Data) -> (FakeHost, Tips) {
    let mut host = FakeHost::new(NAME).with_manifest(crate::MANIFEST);
    host.pane(PaneId(1));
    host.focus(Some(PaneId(1)));
    host.set_day(day);
    host.set_state(state);
    host.set_keys(PALETTE, Args::new(), Some("⇧⌘P"));
    let mut plugin = Tips::default();
    host.activate(&mut plugin).unwrap();
    (host, plugin)
}

/// The state of a tip at the start shown on `last`, of the tips `shown`.
fn saved(last: u64, shown: &[&str]) -> Data {
    Data::Map(BTreeMap::from([
        (STATE_LAST.to_owned(), Data::Integer(last as i64)),
        (
            STATE_SHOWN.to_owned(),
            Data::List(
                shown
                    .iter()
                    .map(|id| Data::Text((*id).to_owned()))
                    .collect(),
            ),
        ),
        (STATE_NEVER.to_owned(), Data::Boolean(false)),
    ]))
}

/// Moves the host's time on by `by` and delivers the events owed.
fn wait(host: &mut FakeHost, plugin: &mut Tips, by: Duration) {
    host.advance(by);
    host.deliver(plugin).unwrap();
}

/// The text that the card shows, if it shows: its tip's message's id, as
/// the fake host has no catalogs.
fn shown(host: &mut FakeHost, plugin: &mut Tips) -> Option<String> {
    if !host.panels().contains_key(&PanelId::from(PANEL)) {
        return None;
    }
    plugin.card.as_ref().map(|card| card.text.clone())
}

/// The state's value of `key`.
fn state_of<'a>(host: &'a FakeHost, key: &str) -> Option<&'a Data> {
    let Data::Map(state) = host.state() else {
        return None;
    };
    state.get(key)
}

/// The ids of the tips shown in the state's round.
fn round(host: &FakeHost) -> Vec<String> {
    match state_of(host, STATE_SHOWN) {
        Some(Data::List(ids)) => ids
            .iter()
            .map(|id| match id {
                Data::Text(id) => id.clone(),
                other => panic!("{other:?}"),
            })
            .collect(),
        other => panic!("{other:?}"),
    }
}

/// The ui event `kind` of the card's node `key`.
fn ui(kind: UiEventKind, key: &str) -> UiEvent {
    UiEvent {
        view: view(),
        key: key.into(),
        kind,
    }
}

#[test]
fn the_first_start_shows_the_first_tip_a_moment_later_at_the_window_s_bottom_right() {
    let (mut host, mut plugin) = host(fake::DAY, Data::Nil);
    assert!(host.commands().iter().any(|command| command.name == SHOW));
    wait(&mut host, &mut plugin, DELAY - Duration::from_millis(1));
    assert_eq!(shown(&mut host, &mut plugin), None);
    wait(&mut host, &mut plugin, Duration::from_millis(1));
    assert_eq!(
        shown(&mut host, &mut plugin).as_deref(),
        Some("tip-palette")
    );
    let window = host.pane(PaneId(1)).window;
    let options = &host.panels()[&PanelId::from(PANEL)];
    assert_eq!(
        options.anchor,
        Anchor::WindowCorner {
            window,
            corner: Corner::BottomRight
        }
    );
    assert_eq!(options.role, PanelRole::Overlay);
    assert!(options.close_typing && !options.close_escape && !options.close_outside);
    assert_eq!(
        state_of(&host, STATE_LAST),
        Some(&Data::Integer(fake::DAY as i64))
    );
    assert_eq!(round(&host), ["palette"]);
}

#[test]
fn a_tip_shows_once_a_week_at_most() {
    for (days, shows) in [(0, false), (6, false), (7, true), (30, true)] {
        let (mut host, mut plugin) = host(fake::DAY + days, saved(fake::DAY, &["palette"]));
        wait(&mut host, &mut plugin, DELAY);
        assert_eq!(shown(&mut host, &mut plugin).is_some(), shows, "{days}");
    }
    // A clock set back shows none.
    let (mut host, mut plugin) = host(fake::DAY - 1, saved(fake::DAY, &[]));
    wait(&mut host, &mut plugin, DELAY);
    assert_eq!(shown(&mut host, &mut plugin), None);
}

#[test]
fn a_start_of_no_tip_due_waits_for_nothing() {
    // A tip shown this week.
    let (host, _plugin) = host(fake::DAY + 3, saved(fake::DAY, &["palette"]));
    assert!(host.timers().is_empty());
    // A host that keeps no state, where a tip would show at every start.
    let mut host = FakeHost::new(NAME).with_manifest(crate::MANIFEST);
    host.pane(PaneId(1));
    host.focus(Some(PaneId(1)));
    host.set_keeps_state(false);
    let mut plugin = Tips::default();
    host.activate(&mut plugin).unwrap();
    assert!(host.timers().is_empty());
    // Its command shows one all the same.
    host.command(&mut plugin, SHOW, Args::new()).unwrap();
    assert!(shown(&mut host, &mut plugin).is_some());
}

#[test]
fn no_tip_shows_over_a_notice_nor_once_stopped() {
    let mut host = FakeHost::new(NAME).with_manifest(crate::MANIFEST);
    host.pane(PaneId(1));
    host.focus(Some(PaneId(1)));
    host.set_notice_shows(true);
    let mut plugin = Tips::default();
    host.activate(&mut plugin).unwrap();
    wait(&mut host, &mut plugin, DELAY);
    assert_eq!(shown(&mut host, &mut plugin), None);
    // Nothing kept: the next start tries again.
    assert_eq!(host.state(), &Data::Nil);

    let mut stopped = saved(fake::DAY - 30, &[]);
    if let Data::Map(state) = &mut stopped {
        state.insert(STATE_NEVER.to_owned(), Data::Boolean(true));
    }
    let (mut host, mut plugin) = self::host(fake::DAY, stopped);
    wait(&mut host, &mut plugin, DELAY);
    assert_eq!(shown(&mut host, &mut plugin), None);
}

#[test]
fn each_tip_shows_once_before_any_comes_again_those_of_unbound_keys_left_out() {
    let (mut host, mut plugin) = host(fake::DAY, Data::Nil);
    host.set_keys("search.open", Args::new(), Some("⌘F"));
    let mut seen = Vec::new();
    for _ in 0..CURATED.len() + 1 {
        host.command(&mut plugin, SHOW, Args::new()).unwrap();
        seen.push(shown(&mut host, &mut plugin).unwrap());
        host.ui_event(&mut plugin, &ui(UiEventKind::Click, "dismiss"))
            .unwrap();
        host.deliver(&mut plugin).unwrap();
        assert_eq!(shown(&mut host, &mut plugin), None);
    }
    // The palette's and the find bar's of their keys, then the plain ones;
    // the commands of no keys are left out.
    let plain: Vec<&str> = CURATED
        .iter()
        .filter(|tip| tip.command.is_none())
        .map(|tip| tip.message)
        .collect();
    assert_eq!(seen[0], "tip-palette");
    assert_eq!(seen[1], "tip-search");
    assert_eq!(seen[2..2 + plain.len()], plain[..]);
    // All shown, the round starts again.
    assert_eq!(seen[2 + plain.len()], "tip-palette");
    assert!(seen.len() > 2 + plain.len());
    // The command shows a tip whatever the week, and keeps no day.
    assert_eq!(state_of(&host, STATE_LAST), None);
}

#[test]
fn an_installed_plugin_s_commands_have_tips_of_their_keys_or_the_palette_s() {
    let (mut host, mut plugin) = host(fake::DAY, Data::Nil);
    let info =
        |name: &str, title: &str, params: Vec<lokyr_plugin_sdk::command::Param>| CommandInfo {
            name: name.to_owned(),
            title: title.to_owned(),
            category: Category::View,
            params,
            plugin: None,
            keys: None,
            when: None,
            enabled: None,
        };
    let delta = lokyr_plugin_sdk::command::Param {
        name: "delta".to_owned(),
        kind: lokyr_plugin_sdk::command::Kind::Integer,
        default: None,
    };
    host.set_registry(vec![
        info("tabs.new", "New Tab", Vec::new()),
        info("awake.stay", "Stay Awake", Vec::new()),
        info("font-picker.choose", "Choose Font…", Vec::new()),
        info("font-picker.move", "Move in the Font Picker", vec![delta]),
    ]);
    host.set_keys("awake.stay", Args::new(), Some("⌃⌥A"));
    let mut plugins = Vec::new();
    for _ in 0..CURATED.len() + 2 {
        host.command(&mut plugin, SHOW, Args::new()).unwrap();
        let ids = round(&host);
        let last = ids.last().unwrap().clone();
        if last.starts_with("command:") {
            plugins.push(last);
        }
        host.ui_event(&mut plugin, &ui(UiEventKind::Click, "dismiss"))
            .unwrap();
        host.deliver(&mut plugin).unwrap();
    }
    assert_eq!(
        plugins,
        ["command:awake.stay", "command:font-picker.choose"]
    );
}

#[test]
fn the_card_goes_after_a_while_held_while_the_pointer_rests_on_it() {
    let (mut host, mut plugin) = host(fake::DAY, Data::Nil);
    wait(&mut host, &mut plugin, DELAY);
    assert!(plugin.card.as_ref().unwrap().entering);
    wait(&mut host, &mut plugin, ENTERS);
    assert!(!plugin.card.as_ref().unwrap().entering);
    host.ui_event(&mut plugin, &ui(UiEventKind::HoverEnter, "card"))
        .unwrap();
    wait(&mut host, &mut plugin, SHOWS * 2);
    assert!(shown(&mut host, &mut plugin).is_some(), "held");
    host.ui_event(&mut plugin, &ui(UiEventKind::HoverLeave, "card"))
        .unwrap();
    wait(&mut host, &mut plugin, SHOWS - Duration::from_millis(1));
    assert!(shown(&mut host, &mut plugin).is_some());
    wait(&mut host, &mut plugin, Duration::from_millis(1));
    assert_eq!(shown(&mut host, &mut plugin), None);
    assert!(plugin.card.is_none());
    assert!(host.timers().is_empty());
}

#[test]
fn a_key_typed_closes_the_card_as_the_host_tells() {
    let (mut host, mut plugin) = host(fake::DAY, Data::Nil);
    wait(&mut host, &mut plugin, DELAY);
    // The host closes it at the first key typed into a pane.
    host.close_panel(&PanelId::from(PANEL));
    host.deliver(&mut plugin).unwrap();
    assert!(plugin.card.is_none());
    assert!(host.timers().is_empty());
    // The command shows another at once.
    host.command(&mut plugin, SHOW, Args::new()).unwrap();
    assert!(shown(&mut host, &mut plugin).is_some());
}

#[test]
fn the_checkbox_stops_the_tips_for_good() {
    let (mut host, mut plugin) = host(fake::DAY, Data::Nil);
    wait(&mut host, &mut plugin, DELAY);
    let tree = host.render(&mut plugin, &view()).unwrap();
    assert!(find(&tree, "never").is_some_and(|node| !toggled(node)));
    host.ui_event(
        &mut plugin,
        &ui(UiEventKind::Toggle { on: true }, "never-label"),
    )
    .unwrap();
    assert_eq!(state_of(&host, STATE_NEVER), Some(&Data::Boolean(true)));
    let tree = host.render(&mut plugin, &view()).unwrap();
    assert!(find(&tree, "never").is_some_and(toggled));
    assert!(find(&tree, "never-label").is_some_and(toggled));
    host.ui_event(&mut plugin, &ui(UiEventKind::Toggle { on: false }, "never"))
        .unwrap();
    assert_eq!(state_of(&host, STATE_NEVER), Some(&Data::Boolean(false)));
    host.ui_event(&mut plugin, &ui(UiEventKind::Toggle { on: true }, "never"))
        .unwrap();
    // The next start a month later shows none.
    let state = host.state().clone();
    let (mut host, mut plugin) = self::host(fake::DAY + 30, state);
    wait(&mut host, &mut plugin, DELAY);
    assert_eq!(shown(&mut host, &mut plugin), None);
}

#[test]
fn the_card_holds_the_lightbulb_its_title_text_and_dismiss() {
    let (mut host, mut plugin) = host(fake::DAY, Data::Nil);
    wait(&mut host, &mut plugin, DELAY);
    let tree = host.render(&mut plugin, &view()).unwrap();
    let mut icons = Vec::new();
    let mut texts = Vec::new();
    walk(&tree, &mut |node| match &node.kind {
        Kind::Icon(name) => icons.push(name.clone()),
        Kind::Text(text) => texts.push(text.clone()),
        _ => {}
    });
    assert_eq!(icons, ["lightbulb"]);
    assert_eq!(texts, ["card-title", "tip-palette"]);
    assert!(find(&tree, "dismiss").is_some());
    assert!(find(&tree, "card").is_some_and(|card| {
        card.label
            .as_deref()
            .is_some_and(|label| label.contains("tip-palette"))
    }));
}

#[test]
fn the_first_party_namespaces_are_those_of_the_app_s_commands() {
    let commands = include_str!("../../../crates/plugin-host-luau/luau/commands.json");
    let mut found = 0;
    for line in commands.lines() {
        // A command's name, not a parameter's, which has no dot.
        let Some((namespace, _)) = line
            .trim()
            .strip_prefix("\"name\": \"")
            .and_then(|name| name.split_once('.'))
        else {
            continue;
        };
        assert!(FIRST_PARTY.contains(&namespace), "{line}");
        found += 1;
    }
    assert!(found > 50, "{found}");
}

#[test]
fn every_tip_has_its_message_in_every_catalog() {
    let messages = CURATED.iter().map(|tip| tip.message).chain([
        "tip-plugin-keys",
        "tip-plugin-palette",
        "card-title",
        "card-dismiss",
        "card-never",
        "command-quick-tips-show",
    ]);
    for message in messages {
        for (language, catalog) in &CATALOGS {
            assert!(
                catalog
                    .lines()
                    .any(|line| line.starts_with(&format!("{message} = "))),
                "{message} in {language}"
            );
        }
    }
    let mut ids: Vec<&str> = CURATED.iter().map(|tip| tip.id).collect();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), CURATED.len());
}

/// Calls `visit` on `node` and every node under it.
fn walk(node: &Node, visit: &mut dyn FnMut(&Node)) {
    visit(node);
    for child in &node.children {
        walk(child, visit);
    }
}

/// The node of `key` in `tree`.
fn find<'a>(tree: &'a Node, key: &str) -> Option<&'a Node> {
    if tree.key.as_ref().is_some_and(|found| found.0 == key) {
        return Some(tree);
    }
    tree.children.iter().find_map(|child| find(child, key))
}

/// Whether `node` is a toggle that is on.
fn toggled(node: &Node) -> bool {
    matches!(&node.kind, Kind::Toggle(toggle) if toggle.on)
}
