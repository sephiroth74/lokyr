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
    assert_eq!(commands, [OPEN, MOVE]);
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
    // and its own.
    assert_eq!(
        shown(&mut host, &mut plugin),
        ["New Window", "Copy", "Paste", "New Tab"]
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
    assert_eq!(plugin.open.as_ref().unwrap().selected, 3);
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
    // A double click on an item.
    run(&mut host, &mut plugin, OPEN, Args::new());
    let activated = on_panel(
        &mut host,
        &mut plugin,
        LIST,
        UiEventKind::Activate { item: 3 },
    );
    assert_eq!(ran(&activated), ["tabs.new"]);
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
        ["Copy", "Paste", "New Window", "New Tab"]
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
