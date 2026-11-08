//! The plugin against the SDK's fake host, whose pane holds three commands
//! and tells the views that the test gives.

use lokyr_plugin_sdk::fake::FakeHost;
use lokyr_plugin_sdk::host::{LineText, TabId};
use lokyr_plugin_sdk::message::Request;
use lokyr_plugin_sdk::view::{Key, Kind};

use super::*;

/// The plugin's name, as its calls give it.
const NAME: &str = "lokyr/command-header";

/// The pane of the tests.
const PANE: PaneId = PaneId(1);

/// Three commands, `ls -l`, `make` and `seq 100`, of their prompts on the
/// rows 0, 30 and 60, their outputs on the rows below, the last running.
fn commands() -> Vec<ShellCommand> {
    let command = |id: u64, row: i64, end: Option<i64>, status: Option<i32>| ShellCommand {
        id,
        prompt: Some((0, row)),
        input: Some((2, row)),
        output: Some((0, row + 1)),
        end: end.map(|end| (0, end)),
        status,
    };
    vec![
        command(1, 0, Some(30), Some(0)),
        command(2, 30, Some(60), Some(1)),
        command(3, 60, None, None),
    ]
}

/// The logical line of `row`, of `text`, a cell a character.
fn line(row: i64, text: &str) -> LineText {
    LineText {
        text: text.to_owned(),
        cells: (0..text.chars().count())
            .map(|column| (u16::try_from(column).unwrap(), row))
            .collect(),
    }
}

/// A host of a pane of 100 rows and the three commands, and the plugin,
/// activated.
fn host() -> (FakeHost, Header) {
    let mut host = FakeHost::new(NAME);
    let pane = host.pane(PANE);
    pane.tab = TabId(1);
    pane.held = 0..100;
    pane.commands = commands();
    for (row, text) in [(0, "$ ls -l"), (30, "$ make  "), (60, "$ seq 100")] {
        pane.lines.insert(row, line(row, text));
    }
    let mut plugin = Header::default();
    host.activate(&mut plugin).unwrap();
    (host, plugin)
}

/// A view of 24 rows from `top`, of the primary screen or not.
fn viewport(top: i64, alternate: bool) -> Viewport {
    Viewport {
        top,
        rows: 24,
        screen: 76,
        oldest: 0,
        epoch: 1,
        alternate,
    }
}

/// Tells the plugin of the view from `top`, and returns the header that
/// the pane shows then, its pill's line, if one, of a panel across the
/// pane's top that takes no keyboard.
fn scrolled(host: &mut FakeHost, plugin: &mut Header, top: i64) -> Option<String> {
    told(host, plugin, viewport(top, false))
}

fn told(host: &mut FakeHost, plugin: &mut Header, viewport: Viewport) -> Option<String> {
    host.post(Event::ViewportChanged {
        pane: PANE,
        viewport,
    });
    host.deliver(plugin).unwrap();
    shown(host, plugin)
}

/// The header that the pane shows, its pill's line.
fn shown(host: &mut FakeHost, plugin: &mut Header) -> Option<String> {
    let id = panel(PANE);
    let options = host.panels().get(&id)?.clone();
    assert_eq!(
        (options.anchor, options.role),
        (Anchor::PaneTop(PANE), Role::Overlay)
    );
    let tree = host.render(plugin, &ViewId::Panel(id))?;
    let [lead, pill, rest] = &tree.children[..] else {
        panic!("{tree:?}");
    };
    assert_eq!(
        (lead.layout.height, rest.layout.height, rest.layout.grow),
        (Length::Points(1.0), Length::Points(1.0), 1.0)
    );
    assert_eq!(pill.key, Some(Key::from(PILL)));
    let [text] = &pill.children[..] else {
        panic!("{pill:?}");
    };
    let Kind::Text(text) = &text.kind else {
        panic!("{text:?}");
    };
    Some(text.clone())
}

#[test]
fn the_header_of_each_command_shows_in_turn_as_the_view_scrolls_through_their_outputs() {
    let (mut host, mut plugin) = host();
    // Down from the first prompt: its line shows, then its output alone.
    assert_eq!(scrolled(&mut host, &mut plugin, 0), None);
    assert_eq!(
        scrolled(&mut host, &mut plugin, 10).as_deref(),
        Some("ls -l")
    );
    assert_eq!(
        scrolled(&mut host, &mut plugin, 29).as_deref(),
        Some("ls -l")
    );
    // The next prompt at the top takes it away, and its output shows the
    // next one's; a command's line of spaces after it shows without them.
    assert_eq!(scrolled(&mut host, &mut plugin, 30), None);
    assert_eq!(
        scrolled(&mut host, &mut plugin, 31).as_deref(),
        Some("make")
    );
    assert_eq!(scrolled(&mut host, &mut plugin, 60), None);
    // The running one's, then back up to the second's, of a render alone.
    assert_eq!(
        scrolled(&mut host, &mut plugin, 70).as_deref(),
        Some("seq 100")
    );
    host.take_renders();
    assert_eq!(
        scrolled(&mut host, &mut plugin, 45).as_deref(),
        Some("make")
    );
    assert_eq!(host.take_renders(), [ViewId::Panel(panel(PANE))]);
    // The same command asks no render again.
    assert_eq!(
        scrolled(&mut host, &mut plugin, 46).as_deref(),
        Some("make")
    );
    assert!(host.take_renders().is_empty());
    // The alternate screen shows none.
    assert_eq!(told(&mut host, &mut plugin, viewport(46, true)), None);
}

#[test]
fn a_click_on_the_header_scrolls_to_its_command_s_prompt() {
    let (mut host, mut plugin) = host();
    assert_eq!(
        scrolled(&mut host, &mut plugin, 52).as_deref(),
        Some("make")
    );
    host.take_done();
    let click = |key: &str| UiEvent {
        view: ViewId::Panel(panel(PANE)),
        key: Key::from(key),
        kind: UiEventKind::Click,
    };
    host.ui_event(&mut plugin, &click(PILL)).unwrap();
    assert_eq!(
        host.take_done(),
        [Request::Scroll {
            pane: PANE,
            scroll: Scroll::Row(30),
        }]
    );
    // A click elsewhere on the panel does nothing.
    host.ui_event(&mut plugin, &click("other")).unwrap();
    assert!(host.take_done().is_empty());
}

#[test]
fn a_command_that_did_not_run_or_whose_line_left_shows_no_header() {
    let (mut host, mut plugin) = host();
    let pane = host.pane(PANE);
    // A prompt of no output's mark, as a line of no command; one whose
    // input's mark left the scrollback.
    pane.commands = vec![
        ShellCommand {
            id: 1,
            prompt: Some((0, 0)),
            input: Some((2, 0)),
            ..ShellCommand::default()
        },
        ShellCommand {
            id: 2,
            prompt: Some((0, 30)),
            output: Some((0, 31)),
            ..ShellCommand::default()
        },
    ];
    assert_eq!(scrolled(&mut host, &mut plugin, 10), None);
    assert_eq!(scrolled(&mut host, &mut plugin, 40), None);
}

#[test]
fn a_closed_pane_s_header_goes_and_a_moved_one_s_opens_anew() {
    let (mut host, mut plugin) = host();
    assert!(scrolled(&mut host, &mut plugin, 10).is_some());
    host.take_done();
    // Moved to another tab, it opens anew there.
    host.post(Event::Workspace(WorkspaceEvent::PaneMoved {
        pane: PANE,
        from: TabId(1),
        tab: TabId(2),
    }));
    host.deliver(&mut plugin).unwrap();
    let done = host.take_done();
    assert!(
        matches!(
            &done[..],
            [Request::ClosePanel(_), Request::OpenPanel { .. }]
        ),
        "{done:?}"
    );
    // Closed, it goes, and the plugin forgets the pane.
    host.post(Event::Workspace(WorkspaceEvent::PaneClosed {
        pane: PANE,
        tab: TabId(2),
    }));
    host.deliver(&mut plugin).unwrap();
    assert!(!host.panels().contains_key(&panel(PANE)));
    assert!(plugin.panes.is_empty());
}

#[test]
fn the_texts_are_in_both_languages() {
    for (_, catalog) in CATALOGS {
        assert!(catalog.contains("header-label = ") && catalog.contains("header-tooltip = "));
    }
}
