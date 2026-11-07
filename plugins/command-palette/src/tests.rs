//! The plugin against the SDK's fake host, which records what it does.

use lokyr_plugin_sdk::command::{Capability, Code};
use lokyr_plugin_sdk::fake::FakeHost;
use lokyr_plugin_sdk::host::PaneId;
use lokyr_plugin_sdk::message::Request;
use lokyr_plugin_sdk::view::Key;

use super::*;

/// The plugin's name, as its calls give it.
const NAME: &str = "lokyr/command-palette";

/// The pane of the tests, whose window has the focus.
const PANE: PaneId = PaneId(1);

/// A command of the registry of the tests, of `params`.
fn info(name: &str, title: &str, keys: Option<&str>, params: Vec<Param>) -> CommandInfo {
    CommandInfo {
        name: name.to_owned(),
        title: title.to_owned(),
        category: Category::View,
        params,
        plugin: None,
        keys: keys.map(str::to_owned),
        when: None,
        enabled: Some(true),
    }
}

/// A host of a focused window, of a registry of the app's commands, and
/// the plugin, activated.
fn host() -> (FakeHost, Palette) {
    host_of(FakeHost::new(NAME))
}

/// The host `host` of a focused window, of a registry of the app's
/// commands, and the plugin, activated.
fn host_of(mut host: FakeHost) -> (FakeHost, Palette) {
    host.focus(Some(PANE));
    let index = Param {
        name: "index".to_owned(),
        kind: Kind::Integer,
        default: None,
    };
    host.set_registry(vec![
        info("window.new", "New Window", Some("⌘N"), Vec::new()),
        info("pane.copy", "Copy", Some("⌘C"), Vec::new()),
        info("pane.paste", "Paste", Some("⌘V"), Vec::new()),
        info("tabs.new", "New Tab", Some("⌘T"), Vec::new()),
        // Of an argument that it needs, which the palette leaves out.
        info("tabs.goto", "Go to Tab", None, vec![index]),
        // Dimmed by the menus, left out too.
        CommandInfo {
            enabled: Some(false),
            ..info("pane.clear-selection", "Clear Selection", None, Vec::new())
        },
    ]);
    let mut plugin = Palette::default();
    host.activate(&mut plugin).unwrap();
    (host, plugin)
}

/// Runs `name`, of `args`, its events delivered after it, and returns its
/// requests.
fn run(host: &mut FakeHost, plugin: &mut Palette, name: &str, args: Args) -> Vec<Request> {
    host.take_done();
    host.command(plugin, name, args).unwrap();
    host.deliver(plugin).unwrap();
    // The palette's view, which the host draws as it opens.
    if plugin.open.is_some() {
        host.render(plugin, &panel());
    }
    host.take_done()
}

/// What `kind` of the palette's node `key` asks of the plugin.
fn on_panel(
    host: &mut FakeHost,
    plugin: &mut Palette,
    key: &str,
    kind: UiEventKind,
) -> Vec<Request> {
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

/// Types `text` in the palette's field.
fn typed(host: &mut FakeHost, plugin: &mut Palette, text: &str) -> Vec<Request> {
    let kind = UiEventKind::InputChanged {
        text: text.to_owned(),
        revision: 1,
    };
    on_panel(host, plugin, FIELD, kind)
}

/// The titles that the palette's list shows, in their order.
fn shown(host: &mut FakeHost, plugin: &mut Palette) -> Vec<String> {
    let tree = host.render(plugin, &panel()).expect("the palette");
    let mut left = vec![&tree];
    while let Some(node) = left.pop() {
        if let lokyr_plugin_sdk::view::Kind::List(list) = &node.kind {
            return list
                .items
                .iter()
                .map(|item| item.texts[0].clone())
                .collect();
        }
        left.extend(&node.children);
    }
    Vec::new()
}

/// The commands that `requests` run.
fn ran(requests: &[Request]) -> Vec<String> {
    requests
        .iter()
        .filter_map(|request| match request {
            Request::RunCommand { name, .. } => Some(name.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn the_palette_opens_at_the_window_s_top_of_the_commands_that_run_without_arguments() {
    let (mut host, mut plugin) = host();
    let commands: Vec<&str> = host.commands().iter().map(|c| c.name.as_str()).collect();
    assert_eq!(commands, [OPEN, THEME, MOVE]);
    run(&mut host, &mut plugin, OPEN, Args::new());
    let options = host
        .panels()
        .get(&PanelId::from(PANEL))
        .expect("the palette");
    assert!(
        matches!(
            options.anchor,
            Anchor::WindowCorner {
                corner: Corner::Top,
                ..
            }
        ),
        "{:?}",
        options.anchor
    );
    assert!(options.close_escape && options.close_outside);
    // In the registry's order, but those of an argument, the dimmed ones
    // and its own opening; its theme picker last, its own.
    assert_eq!(
        shown(&mut host, &mut plugin),
        ["New Window", "Copy", "Paste", "New Tab", "Choose Theme…"]
    );
}

#[test]
fn typing_ranks_the_commands_and_return_runs_the_first_as_the_user_s() {
    let (mut host, mut plugin) = host();
    run(&mut host, &mut plugin, OPEN, Args::new());
    let asked = typed(&mut host, &mut plugin, "nt");
    assert_eq!(shown(&mut host, &mut plugin)[0], "New Tab");
    assert!(
        asked
            .iter()
            .any(|request| matches!(request, Request::Select { item: 0, .. }))
    );
    typed(&mut host, &mut plugin, "zzz");
    assert!(shown(&mut host, &mut plugin).is_empty());
    typed(&mut host, &mut plugin, "co");
    let submitted = on_panel(
        &mut host,
        &mut plugin,
        FIELD,
        UiEventKind::Submit {
            text: "co".to_owned(),
            shift: false,
        },
    );
    assert_eq!(ran(&submitted), ["pane.copy"]);
    assert!(host.panels().get(&PanelId::from(PANEL)).is_none());
    assert!(plugin.open.is_none());
}

#[test]
fn the_arrows_move_the_selection_and_a_double_click_runs_its_command() {
    let (mut host, mut plugin) = host();
    run(&mut host, &mut plugin, OPEN, Args::new());
    let delta = |delta: i64| Args::from([("delta".to_owned(), Value::Integer(delta))]);
    let moved = run(&mut host, &mut plugin, MOVE, delta(1));
    assert!(
        moved
            .iter()
            .any(|request| matches!(request, Request::Select { item: 1, .. }))
    );
    // Not past the ends.
    run(&mut host, &mut plugin, MOVE, delta(10));
    assert_eq!(plugin.open.as_ref().unwrap().selected, 4);
    run(&mut host, &mut plugin, MOVE, delta(-10));
    assert_eq!(plugin.open.as_ref().unwrap().selected, 0);
    run(&mut host, &mut plugin, MOVE, delta(2));
    let submitted = on_panel(
        &mut host,
        &mut plugin,
        FIELD,
        UiEventKind::Submit {
            text: String::new(),
            shift: false,
        },
    );
    assert_eq!(ran(&submitted), ["pane.paste"]);
    // A click on an item, which the menu's list activates, the selected
    // one's too, runs it; the selection's move alone runs nothing.
    run(&mut host, &mut plugin, OPEN, Args::new());
    let moved = on_panel(
        &mut host,
        &mut plugin,
        LIST,
        UiEventKind::Select { item: 3 },
    );
    assert!(ran(&moved).is_empty());
    let clicked = on_panel(
        &mut host,
        &mut plugin,
        LIST,
        UiEventKind::Activate { item: 3 },
    );
    assert_eq!(ran(&clicked), ["tabs.new"]);
    // The first row, selected as the palette opens, of a click.
    run(&mut host, &mut plugin, OPEN, Args::new());
    let first = on_panel(
        &mut host,
        &mut plugin,
        LIST,
        UiEventKind::Activate { item: 0 },
    );
    // The first of the list, of the recent ones first: New Tab.
    assert_eq!(ran(&first), ["tabs.new"]);
    // The list's own keys, while Tab gave it the keyboard, move alone,
    // and its row activated runs.
    run(&mut host, &mut plugin, OPEN, Args::new());
    on_panel(&mut host, &mut plugin, LIST, UiEventKind::Focus);
    let moved = on_panel(
        &mut host,
        &mut plugin,
        LIST,
        UiEventKind::Select { item: 1 },
    );
    assert!(ran(&moved).is_empty());
    assert_eq!(plugin.open.as_ref().unwrap().selected, 1);
    let activated = on_panel(
        &mut host,
        &mut plugin,
        LIST,
        UiEventKind::Activate { item: 1 },
    );
    // The second of the list, of the recent ones first: New Tab, Paste.
    assert_eq!(ran(&activated), ["pane.paste"]);
}

#[test]
fn each_row_tells_its_title_and_its_keys_in_a_menu_s_list() {
    let (mut host, mut plugin) = host();
    run(&mut host, &mut plugin, OPEN, Args::new());
    let tree = host.render(&mut plugin, &panel()).expect("the palette");
    let mut left = vec![&tree];
    let mut found = None;
    while let Some(node) = left.pop() {
        if let lokyr_plugin_sdk::view::Kind::List(list) = &node.kind {
            found = Some(list.clone());
        }
        left.extend(&node.children);
    }
    let list = found.expect("the list");
    assert!(list.menu);
    assert_eq!(list.items[0].texts, ["New Window", "⌘N"]);
}

#[test]
fn the_recent_commands_come_first_and_the_state_keeps_them() {
    let (mut host, mut plugin) = host();
    // Paste, the third; then Copy, the third too once Paste is first.
    for item in [2, 2] {
        run(&mut host, &mut plugin, OPEN, Args::new());
        on_panel(&mut host, &mut plugin, LIST, UiEventKind::Activate { item });
    }
    run(&mut host, &mut plugin, OPEN, Args::new());
    assert_eq!(
        shown(&mut host, &mut plugin),
        ["Copy", "Paste", "New Window", "New Tab", "Choose Theme…"]
    );
    let Data::Map(state) = host.state() else {
        panic!("a state: {:?}", host.state());
    };
    assert_eq!(
        state.get(STATE_RECENT),
        Some(&Data::List(vec![
            Data::Text("pane.copy".to_owned()),
            Data::Text("pane.paste".to_owned())
        ]))
    );
    // A new plugin of the same state, as the app starts again.
    let mut other = FakeHost::new(NAME);
    other.set_state(host.state().clone());
    let mut again = Palette::default();
    other.activate(&mut again).unwrap();
    assert_eq!(again.recent, ["pane.copy", "pane.paste"]);
    // Escape closes it, running nothing.
    host.take_done();
    host.close_panel(&PanelId::from(PANEL));
    host.deliver(&mut plugin).unwrap();
    assert!(ran(&host.take_done()).is_empty());
    assert!(plugin.open.is_none());
}

#[test]
fn a_palette_runs_its_choice_of_user_commands_alone() {
    let submit = |host: &mut FakeHost, plugin: &mut Palette| {
        host.command(plugin, OPEN, Args::new()).unwrap();
        host.render(plugin, &panel());
        host.ui_event(
            plugin,
            &UiEvent {
                view: panel(),
                key: Key::from(FIELD),
                kind: UiEventKind::Submit {
                    text: String::new(),
                    shift: false,
                },
            },
        )
    };
    // A palette of a third party, of no capability: its run is refused, as
    // the command is not one that its manifest declares.
    let (mut host, mut plugin) = host_of(FakeHost::new(NAME).with_capabilities(Vec::new()));
    let refused = submit(&mut host, &mut plugin);
    assert!(
        refused
            .as_ref()
            .is_err_and(|error| error.code == Code::Undeclared),
        "{refused:?}"
    );
    // Of `user-commands`, it runs it.
    let granted = FakeHost::new(NAME).with_capabilities(vec![Capability::UserCommands]);
    let (mut host, mut plugin) = host_of(granted);
    assert!(submit(&mut host, &mut plugin).is_ok());
    assert_eq!(ran(&host.take_done()), ["window.new"]);
}

/// A host of the light appearance in force, of `Paper` its theme, and of
/// themes of both appearances.
fn themed() -> (FakeHost, Palette) {
    use lokyr_plugin_sdk::host::{ThemeInfo, ThemeOrigin, Themes};
    let (mut host, plugin) = host();
    let theme = |name: &str, variant, origin| ThemeInfo {
        name: name.to_owned(),
        variant,
        origin,
    };
    host.set_themes(Themes {
        appearance: ThemeVariant::Light,
        current: "Paper".to_owned(),
        themes: vec![
            theme("Dracula", ThemeVariant::Dark, ThemeOrigin::Held),
            theme("Lokyr Dark", ThemeVariant::Dark, ThemeOrigin::BuiltIn),
            theme("Lokyr Light", ThemeVariant::Light, ThemeOrigin::BuiltIn),
            theme("Paper", ThemeVariant::Light, ThemeOrigin::Folder),
            theme("Rose Pine Dawn", ThemeVariant::Light, ThemeOrigin::Held),
        ],
    });
    (host, plugin)
}

/// The themes that `requests` preview, in their order.
fn previewed(requests: &[Request]) -> Vec<Option<String>> {
    requests
        .iter()
        .filter_map(|request| match request {
            Request::PreviewTheme(name) => Some(name.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn the_theme_picker_lists_the_appearance_in_force_s_themes_its_own_first() {
    let (mut host, mut plugin) = themed();
    let opened = run(&mut host, &mut plugin, THEME, Args::new());
    assert!(previewed(&opened).is_empty());
    assert_eq!(
        shown(&mut host, &mut plugin),
        ["Paper", "Lokyr Light", "Rose Pine Dawn"]
    );
    let tree = host.render(&mut plugin, &panel()).expect("the picker");
    let mut left = vec![&tree];
    let mut keys = None;
    while let Some(node) = left.pop() {
        if let lokyr_plugin_sdk::view::Kind::List(list) = &node.kind {
            keys = Some(list.items[0].texts[1].clone());
        }
        left.extend(&node.children);
    }
    assert_eq!(keys.as_deref(), Some("Current"));
}

#[test]
fn the_theme_picker_previews_the_selected_theme_and_names_the_chosen_one() {
    let (mut host, mut plugin) = themed();
    run(&mut host, &mut plugin, THEME, Args::new());
    let delta = |delta: i64| Args::from([("delta".to_owned(), Value::Integer(delta))]);
    let moved = run(&mut host, &mut plugin, MOVE, delta(1));
    assert_eq!(previewed(&moved), [Some("Lokyr Light".to_owned())]);
    let typed = typed(&mut host, &mut plugin, "rose");
    assert_eq!(previewed(&typed), [Some("Rose Pine Dawn".to_owned())]);
    let submitted = on_panel(
        &mut host,
        &mut plugin,
        FIELD,
        UiEventKind::Submit {
            text: "rose".to_owned(),
            shift: false,
        },
    );
    assert!(submitted.contains(&Request::SetTheme {
        variant: ThemeVariant::Light,
        name: "Rose Pine Dawn".to_owned()
    }));
    assert!(host.panels().get(&PanelId::from(PANEL)).is_none());
    // Its closing's event, which it asked for, previews nothing more.
    host.take_done();
    host.deliver(&mut plugin).unwrap();
    assert!(previewed(&host.take_done()).is_empty());
    // Escape ends a preview, the config's theme shown again; the recent
    // commands keep none of the themes.
    run(&mut host, &mut plugin, THEME, Args::new());
    run(&mut host, &mut plugin, MOVE, delta(1));
    host.take_done();
    host.close_panel(&PanelId::from(PANEL));
    host.deliver(&mut plugin).unwrap();
    assert_eq!(previewed(&host.take_done()), [None]);
    assert!(plugin.recent.is_empty());
}
