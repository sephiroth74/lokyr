//! The plugin against the SDK's fake host, which hands it the automation's
//! requests as the app's host does and keeps its answers.

use lokyr_plugin_sdk::fake::FakeHost;
use lokyr_plugin_sdk::message::Request;

use super::*;

/// The plugin's name, as its calls give it.
const NAME: &str = "lokyr/automation";

/// The client that asks, in the tests: an app of a script's, which waits
/// for no question.
fn client() -> AutomationClient {
    AutomationClient {
        id: "app:TEAM:com.acme.script".to_owned(),
        name: "Acme Script".to_owned(),
        terminal: false,
        waits: false,
    }
}

/// A host of a window of pane 1, which has the focus and shows `hello`,
/// and the plugin, activated, whose state allows [`client`].
fn host() -> (FakeHost, Automation) {
    let (mut host, _) = asking_host();
    host.set_state(Data::Map(BTreeMap::from([(
        CLIENTS.to_owned(),
        Data::Map(BTreeMap::from([(client().id, Data::Boolean(true))])),
    )])));
    let mut plugin = Automation::default();
    host.activate(&mut plugin).unwrap();
    host.take_done();
    (host, plugin)
}

/// A host as [`host`]'s, whose plugin's state holds no client's answer.
fn asking_host() -> (FakeHost, Automation) {
    let mut host = FakeHost::new(NAME);
    host.pane(PaneId(1))
        .rows
        .insert(0, ("hello  ".to_owned(), false));
    host.focus(Some(PaneId(1)));
    let mut plugin = Automation::default();
    host.activate(&mut plugin).unwrap();
    host.take_done();
    (host, plugin)
}

/// Hands the plugin the request `request` of `id`, of [`client`].
fn ask(host: &mut FakeHost, plugin: &mut Automation, id: u64, request: AutomationRequest) {
    ask_of(host, plugin, id, &client(), request);
}

/// Hands the plugin the request `request` of `id`, of `client`.
fn ask_of(
    host: &mut FakeHost,
    plugin: &mut Automation,
    id: u64,
    client: &AutomationClient,
    request: AutomationRequest,
) {
    host.post(Event::Automation {
        front: Default::default(),
        id,
        client: client.clone(),
        request,
    });
    host.deliver(plugin).unwrap();
}

/// The user's answer `button` to the question that shows.
fn choose(host: &mut FakeHost, plugin: &mut Automation, button: u32) {
    let question = host.question().expect("a question");
    host.post(Event::Chosen {
        id: question,
        button: Some(button),
    });
    host.deliver(plugin).unwrap();
}

/// The plugin's answer to `id`, if it gave one.
fn answer(host: &FakeHost, id: u64) -> Option<AutomationAnswer> {
    host.automation_answers()
        .iter()
        .find(|(answered, _)| *answered == id)
        .map(|(_, answer)| answer.clone())
}

#[test]
fn a_request_opens_a_tab_in_a_folder_and_its_shell_runs_its_command() {
    let (mut host, mut plugin) = host();
    let request = AutomationRequest::NewTab {
        environment: Default::default(),
        window: None,
        folder: Some("~".to_owned()),
        command: Some("ls -l".to_owned()),
    };
    ask(&mut host, &mut plugin, 1, request);
    let Some(AutomationAnswer::Opened { window, tab, pane }) = answer(&host, 1) else {
        panic!("{:?}", host.automation_answers());
    };
    assert_eq!(window, WindowId(1));
    let done = host.take_done();
    assert!(done.contains(&Request::NewTab {
        environment: Default::default(),
        window,
        index: None,
        directory: Some("~".to_owned()),
    }));
    assert!(done.contains(&Request::SendText {
        pane,
        text: "ls -l\r".to_owned(),
    }));
    assert!(done.contains(&Request::ActivateTab(tab)));
}

#[test]
fn a_split_opens_at_its_edge_and_tells_its_tab() {
    let (mut host, mut plugin) = host();
    let request = AutomationRequest::Split {
        environment: Default::default(),
        pane: None,
        edge: AutomationEdge::Left,
        folder: None,
        command: None,
    };
    ask(&mut host, &mut plugin, 2, request);
    let Some(AutomationAnswer::Opened { window, tab, pane }) = answer(&host, 2) else {
        panic!("{:?}", host.automation_answers());
    };
    assert_eq!((window, tab), (WindowId(1), TabId(1)));
    assert!(host.take_done().contains(&Request::Split {
        environment: Default::default(),
        pane: PaneId(1),
        direction: Direction::Horizontal,
        side: Side::First,
        directory: None,
    }));
    assert_ne!(pane, PaneId(1));
}

#[test]
fn a_command_that_is_not_there_is_refused() {
    let (mut host, mut plugin) = host();
    let request = AutomationRequest::Run {
        command: "acme.none".to_owned(),
        args: Args::new(),
    };
    ask(&mut host, &mut plugin, 3, request);
    assert!(
        matches!(answer(&host, 3), Some(AutomationAnswer::Error { code, .. }) if code == Code::NotFound.number()),
        "{:?}",
        host.automation_answers()
    );
}

#[test]
fn the_workspace_tells_the_same_ids_from_a_request_to_the_next() {
    let (mut host, mut plugin) = host();
    let (tab, pane) = host.add_tab(WindowId(1));
    host.deliver(&mut plugin).unwrap();
    ask(&mut host, &mut plugin, 4, AutomationRequest::Workspace);
    ask(&mut host, &mut plugin, 5, AutomationRequest::Workspace);
    let Some(AutomationAnswer::Workspace(first)) = answer(&host, 4) else {
        panic!("{:?}", host.automation_answers());
    };
    assert_eq!(
        answer(&host, 5),
        Some(AutomationAnswer::Workspace(first.clone()))
    );
    let tabs: Vec<(TabId, Vec<PaneId>)> = first[0]
        .tabs
        .iter()
        .map(|shown| (shown.id, shown.panes.iter().map(|pane| pane.id).collect()))
        .collect();
    assert_eq!(tabs, [(TabId(1), vec![PaneId(1)]), (tab, vec![pane])]);
}

#[test]
fn a_new_window_answers_once_it_opens_and_its_shell_runs_its_command() {
    let (mut host, mut plugin) = host();
    let request = AutomationRequest::NewWindow {
        environment: Default::default(),
        folder: None,
        command: Some("top".to_owned()),
    };
    ask(&mut host, &mut plugin, 6, request);
    assert_eq!(answer(&host, 6), None);
    assert!(host.take_done().contains(&Request::OpenWindow {
        environment: Default::default(),
        directory: None
    }));
    host.post(Event::Workspace(WorkspaceEvent::WindowOpened {
        window: WindowId(1),
    }));
    host.deliver(&mut plugin).unwrap();
    assert_eq!(
        answer(&host, 6),
        Some(AutomationAnswer::Opened {
            window: WindowId(1),
            tab: TabId(1),
            pane: PaneId(1),
        })
    );
    assert!(host.take_done().contains(&Request::SendText {
        pane: PaneId(1),
        text: "top\r".to_owned(),
    }));
}

#[test]
fn a_waiting_client_s_first_request_waits_for_the_answer_which_the_plugin_keeps() {
    let (mut host, mut plugin) = asking_host();
    let shortcuts = AutomationClient {
        waits: true,
        ..client()
    };
    // Any request asks, a read of the text as a listing of the windows.
    ask_of(
        &mut host,
        &mut plugin,
        7,
        &shortcuts,
        AutomationRequest::Workspace,
    );
    assert_eq!(answer(&host, 7), None);
    choose(&mut host, &mut plugin, ALLOW);
    assert!(matches!(
        answer(&host, 7),
        Some(AutomationAnswer::Workspace(_))
    ));
    let read = AutomationRequest::PaneText {
        pane: Some(PaneId(1)),
        selection: false,
    };
    ask_of(&mut host, &mut plugin, 8, &shortcuts, read.clone());
    assert_eq!(
        answer(&host, 8),
        Some(AutomationAnswer::Text("hello".to_owned()))
    );
    // The answer is kept: a plugin that starts again of its state asks
    // nothing.
    let Data::Map(state) = host.state() else {
        panic!("{:?}", host.state());
    };
    assert_eq!(
        state.get(CLIENTS),
        Some(&Data::Map(BTreeMap::from([(
            shortcuts.id.clone(),
            Data::Boolean(true)
        )])))
    );
    let mut again = Automation::default();
    host.activate(&mut again).unwrap();
    ask_of(&mut host, &mut again, 9, &shortcuts, read);
    assert_eq!(
        answer(&host, 9),
        Some(AutomationAnswer::Text("hello".to_owned()))
    );
}

#[test]
fn a_script_s_request_fails_while_the_question_waits_and_runs_once_allowed() {
    let (mut host, mut plugin) = asking_host();
    let typed = || AutomationRequest::SendText {
        pane: None,
        text: "ls\r".to_owned(),
    };
    ask(&mut host, &mut plugin, 10, typed());
    assert!(
        matches!(answer(&host, 10), Some(AutomationAnswer::Error { code, .. }) if code == Code::Unavailable.number()),
        "{:?}",
        host.automation_answers()
    );
    // Nothing reached the terminal, and the question shows once.
    assert!(
        host.take_done()
            .iter()
            .all(|done| !matches!(done, Request::SendText { .. }))
    );
    ask(&mut host, &mut plugin, 11, typed());
    choose(&mut host, &mut plugin, ALLOW);
    assert_eq!(host.question(), None);
    ask(&mut host, &mut plugin, 12, typed());
    assert_eq!(answer(&host, 12), Some(AutomationAnswer::Done));
}

#[test]
fn a_refused_client_s_requests_fail_and_ask_nothing_more() {
    let (mut host, mut plugin) = asking_host();
    let shortcuts = AutomationClient {
        waits: true,
        ..client()
    };
    ask_of(
        &mut host,
        &mut plugin,
        13,
        &shortcuts,
        AutomationRequest::Workspace,
    );
    choose(&mut host, &mut plugin, DENY);
    let refused = |host: &FakeHost, id: u64| matches!(answer(host, id), Some(AutomationAnswer::Error { code, .. }) if code == Code::Failed.number());
    assert!(refused(&host, 13), "{:?}", host.automation_answers());
    // ⌘A and ⌘C, which would copy the text, are refused too.
    let key = AutomationRequest::SendKey {
        pane: None,
        keys: "cmd+a".to_owned(),
    };
    ask_of(&mut host, &mut plugin, 14, &shortcuts, key);
    assert_eq!(host.question(), None);
    assert!(refused(&host, 14));
    assert!(
        host.take_done()
            .iter()
            .all(|done| !matches!(done, Request::PressKey { .. }))
    );
}

/// A program in a Lokyr terminal, of its pane's shell `pane` and its chain.
fn in_pane(pane: u32, chain: &str) -> AutomationClient {
    AutomationClient {
        id: format!("terminal:{pane}:1>{chain}"),
        name: "osascript, in a Lokyr terminal".to_owned(),
        terminal: true,
        waits: false,
    }
}

#[test]
fn a_program_in_a_terminal_is_answered_in_its_pane_alone() {
    let (mut host, mut plugin) = asking_host();
    let typed = in_pane(100, "/bin/zsh>/usr/bin/osascript");
    ask_of(
        &mut host,
        &mut plugin,
        15,
        &typed,
        AutomationRequest::Workspace,
    );
    // A script fails at once while its question waits.
    assert!(matches!(
        answer(&host, 15),
        Some(AutomationAnswer::Error { .. })
    ));
    choose(&mut host, &mut plugin, ALLOW);
    // Run again, a new process of the same place takes the answer.
    ask_of(
        &mut host,
        &mut plugin,
        16,
        &typed,
        AutomationRequest::Workspace,
    );
    assert_eq!(host.question(), None);
    assert!(matches!(
        answer(&host, 16),
        Some(AutomationAnswer::Workspace(_))
    ));
    // No state keeps it; another pane, or a program that another runs,
    // asks again.
    assert!(!matches!(host.state(), Data::Map(map) if map.contains_key(CLIENTS)));
    ask_of(
        &mut host,
        &mut plugin,
        17,
        &in_pane(101, "/bin/zsh>/usr/bin/osascript"),
        AutomationRequest::Workspace,
    );
    assert!(host.question().is_some());
    choose(&mut host, &mut plugin, DENY);
    ask_of(
        &mut host,
        &mut plugin,
        18,
        &in_pane(100, "/bin/zsh>/bin/sh>/usr/bin/osascript"),
        AutomationRequest::Workspace,
    );
    assert!(host.question().is_some());
}

#[test]
fn the_terminals_answers_keep_the_newest() {
    let (mut host, mut plugin) = asking_host();
    let mut id = 1000;
    for pane in 0..=MOST_TERMINAL_ANSWERS {
        let pane = u32::try_from(pane).unwrap();
        ask_of(
            &mut host,
            &mut plugin,
            id,
            &in_pane(pane, "/usr/bin/osascript"),
            AutomationRequest::Workspace,
        );
        choose(&mut host, &mut plugin, ALLOW);
        id += 1;
    }
    // The newest is kept, the oldest forgotten and asked again.
    let newest = u32::try_from(MOST_TERMINAL_ANSWERS).unwrap();
    ask_of(
        &mut host,
        &mut plugin,
        id,
        &in_pane(newest, "/usr/bin/osascript"),
        AutomationRequest::Workspace,
    );
    assert_eq!(host.question(), None);
    ask_of(
        &mut host,
        &mut plugin,
        id + 1,
        &in_pane(0, "/usr/bin/osascript"),
        AutomationRequest::Workspace,
    );
    assert!(host.question().is_some());
}

#[test]
fn a_question_that_cannot_show_fails_its_client_s_requests() {
    // No window, which the question would show on.
    let mut host = FakeHost::new(NAME);
    let mut plugin = Automation::default();
    host.activate(&mut plugin).unwrap();
    let shortcuts = AutomationClient {
        waits: true,
        ..client()
    };
    ask_of(
        &mut host,
        &mut plugin,
        18,
        &shortcuts,
        AutomationRequest::Workspace,
    );
    assert_eq!(host.question(), None);
    assert!(
        matches!(answer(&host, 18), Some(AutomationAnswer::Error { .. })),
        "{:?}",
        host.automation_answers()
    );
}

#[test]
fn a_session_s_switch_makes_the_session_of_a_name_that_is_not_there() {
    let (mut host, mut plugin) = host();
    let switch = |name: &str| AutomationRequest::SwitchSession {
        name: name.to_owned(),
    };
    ask(&mut host, &mut plugin, 11, switch("work"));
    assert_eq!(answer(&host, 11), Some(AutomationAnswer::Done));
    assert!(host.take_done().contains(&Request::NewSession {
        name: "work".to_owned(),
        directory: None,
    }));
    // The active session of that name stays as it is.
    ask(&mut host, &mut plugin, 12, switch("work"));
    assert_eq!(answer(&host, 12), Some(AutomationAnswer::Done));
    assert!(
        host.take_done()
            .iter()
            .all(|done| !matches!(done, Request::NewSession { .. } | Request::SwitchSession(_)))
    );
    // A switch of no name is refused.
    ask(&mut host, &mut plugin, 13, switch(" "));
    assert!(
        matches!(answer(&host, 13), Some(AutomationAnswer::Error { code, .. }) if code == Code::MissingArgument.number()),
        "{:?}",
        host.automation_answers()
    );
}

/// A host of the command `tabs.goto` of an integer's `index` in its
/// registry, beside the plugin's, and the plugin, activated.
fn host_of_goto() -> (FakeHost, Automation) {
    use lokyr_plugin_sdk::command::{Category, Param};
    use lokyr_plugin_sdk::host::CommandInfo;
    let mut host = FakeHost::new(NAME);
    host.set_registry(vec![CommandInfo {
        name: "tabs.goto".to_owned(),
        title: "Select Tab".to_owned(),
        category: Category::Tabs,
        params: vec![Param {
            name: "index".to_owned(),
            kind: Kind::Integer,
            default: None,
        }],
        plugin: Some("lokyr/tabs".to_owned()),
        keys: None,
        when: None,
        enabled: Some(true),
    }]);
    host.set_state(Data::Map(BTreeMap::from([(
        CLIENTS.to_owned(),
        Data::Map(BTreeMap::from([(client().id, Data::Boolean(true))])),
    )])));
    host.focus(Some(PaneId(1)));
    let mut plugin = Automation::default();
    host.activate(&mut plugin).unwrap();
    host.take_done();
    (host, plugin)
}

#[test]
fn the_commands_are_told_as_the_palette_lists_them_of_their_entries_arguments() {
    use lokyr_plugin_sdk::command::Category;
    use lokyr_plugin_sdk::host::MenuBarItem;
    let (mut host, mut plugin) = host_of_goto();
    let item = |index: i64, title: &str, enabled: bool| MenuBarItem {
        command: "tabs.goto".to_owned(),
        args: Args::from([("index".to_owned(), Value::Integer(index))]),
        title: title.to_owned(),
        submenu: None,
        menu: Category::Tabs,
        keys: None,
        enabled,
        ticked: false,
        plugin: "lokyr/tabs".to_owned(),
    };
    // An item that the menus disable now is listed all the same, and an
    // item twice once.
    host.set_menu_bar(vec![
        item(1, "Select First Tab", true),
        item(2, "Select Second Tab", false),
        item(1, "Select First Tab", true),
    ]);
    ask(&mut host, &mut plugin, 14, AutomationRequest::Commands);
    let Some(AutomationAnswer::Commands(commands)) = answer(&host, 14) else {
        panic!("{:?}", host.automation_answers());
    };
    let goto: Vec<(&str, &Args)> = commands
        .iter()
        .filter(|command| command.name == "tabs.goto")
        .map(|command| (command.title.as_str(), &command.args))
        .collect();
    // The command, of an argument that it needs, is listed of its items
    // alone.
    assert_eq!(
        goto,
        [
            (
                "Select First Tab",
                &Args::from([("index".to_owned(), Value::Integer(1))])
            ),
            (
                "Select Second Tab",
                &Args::from([("index".to_owned(), Value::Integer(2))])
            ),
        ]
    );
}

#[test]
fn a_run_s_text_argument_takes_its_parameter_s_kind() {
    let (mut host, mut plugin) = host_of_goto();
    let request = AutomationRequest::Run {
        command: "tabs.goto".to_owned(),
        args: Args::from([("index".to_owned(), Value::String(" 2 ".to_owned()))]),
    };
    ask(&mut host, &mut plugin, 15, request);
    assert_eq!(answer(&host, 15), Some(AutomationAnswer::Done));
    assert!(host.take_done().contains(&Request::RunCommand {
        name: "tabs.goto".to_owned(),
        args: Args::from([("index".to_owned(), Value::Integer(2))]),
    }));
    // A text that reads as no number stays a text, which the run refuses.
    let request = AutomationRequest::Run {
        command: "tabs.goto".to_owned(),
        args: Args::from([("index".to_owned(), Value::String("two".to_owned()))]),
    };
    ask(&mut host, &mut plugin, 16, request);
    assert!(
        matches!(answer(&host, 16), Some(AutomationAnswer::Error { .. })),
        "{:?}",
        host.automation_answers()
    );
}

#[test]
fn a_key_is_pressed_in_the_focused_pane_and_a_click_is_refused() {
    let (mut host, mut plugin) = host();
    let key = |keys: &str| AutomationRequest::SendKey {
        pane: None,
        keys: keys.to_owned(),
    };
    ask(&mut host, &mut plugin, 17, key("ctrl+c"));
    assert_eq!(answer(&host, 17), Some(AutomationAnswer::Done));
    assert!(host.take_done().contains(&Request::PressKey {
        pane: PaneId(1),
        keys: "ctrl+c".to_owned(),
    }));
    ask(&mut host, &mut plugin, 18, key("click"));
    assert!(
        matches!(answer(&host, 18), Some(AutomationAnswer::Error { code, .. }) if code == Code::WrongKind.number()),
        "{:?}",
        host.automation_answers()
    );
}

#[test]
fn a_command_line_runs_at_the_shell_s_empty_prompt_alone() {
    let (mut host, mut plugin) = host();
    let run = || AutomationRequest::RunLine {
        pane: None,
        line: "ls -la".to_owned(),
    };
    // The shell edits an empty command line at its prompt: the line runs.
    host.pane(PaneId(1)).edited_line = Some(String::new());
    ask(&mut host, &mut plugin, 19, run());
    assert_eq!(answer(&host, 19), Some(AutomationAnswer::Done));
    assert!(host.take_done().contains(&Request::RunLine {
        pane: PaneId(1),
        line: "ls -la".to_owned(),
    }));
    // Of no prompt, as while a program runs, it is refused.
    host.pane(PaneId(1)).edited_line = None;
    ask(&mut host, &mut plugin, 20, run());
    assert!(
        matches!(answer(&host, 20), Some(AutomationAnswer::Error { code, .. }) if code == Code::Unavailable.number()),
        "{:?}",
        host.automation_answers()
    );
}

#[test]
fn a_mouse_event_goes_to_the_focused_pane() {
    use lokyr_plugin_sdk::host::{MouseButton, MouseDoes, MouseInput};
    let (mut host, mut plugin) = host();
    let input = MouseInput {
        action: MouseDoes::Press,
        button: Some(MouseButton::Left),
        column: 4,
        row: 2,
        shift: false,
        control: false,
        alt: false,
        steps: None,
    };
    ask(
        &mut host,
        &mut plugin,
        21,
        AutomationRequest::SendMouse { pane: None, input },
    );
    assert_eq!(answer(&host, 21), Some(AutomationAnswer::Done));
    assert!(host.take_done().contains(&Request::SendMouse {
        pane: PaneId(1),
        input,
    }));
}

#[test]
fn a_request_of_no_pane_takes_the_window_in_front_while_lokyr_is_behind() {
    let (mut host, mut plugin) = host();
    // A second window, of pane 5, and no window of the focus, as while
    // Lokyr is behind the app that asks.
    let second = host.pane(PaneId(5));
    second.window = WindowId(2);
    second.tab = TabId(2);
    host.focus(None);
    host.take_done();
    host.post(Event::Automation {
        id: 22,
        client: client(),
        request: AutomationRequest::SendText {
            pane: None,
            text: "x".to_owned(),
        },
        front: Some(WindowId(2)),
    });
    host.deliver(&mut plugin).unwrap();
    assert_eq!(answer(&host, 22), Some(AutomationAnswer::Done));
    assert!(host.take_done().contains(&Request::SendText {
        pane: PaneId(5),
        text: "x".to_owned(),
    }));
    // Of no window in front, the first window's.
    ask(
        &mut host,
        &mut plugin,
        23,
        AutomationRequest::SendText {
            pane: None,
            text: "y".to_owned(),
        },
    );
    assert!(host.take_done().contains(&Request::SendText {
        pane: PaneId(1),
        text: "y".to_owned(),
    }));
}

#[test]
fn a_window_that_never_opened_lets_another_s_window_go_of_its_command() {
    let (mut host, mut plugin) = host();
    let request = AutomationRequest::NewWindow {
        folder: None,
        command: Some("rm -rf build".to_owned()),
        environment: Default::default(),
    };
    ask(&mut host, &mut plugin, 24, request);
    assert_eq!(answer(&host, 24), None);
    // Its deadline passes, the host failing it, and no window opened.
    host.advance(std::time::Duration::from_millis(ANSWER_WITHIN_MS + 1));
    host.deliver(&mut plugin).unwrap();
    host.take_done();
    // A window that another opened takes no script's command.
    host.post(Event::Workspace(WorkspaceEvent::WindowOpened {
        window: WindowId(1),
    }));
    host.deliver(&mut plugin).unwrap();
    assert!(
        host.take_done()
            .iter()
            .all(|done| !matches!(done, Request::SendText { .. }))
    );
    assert_eq!(answer(&host, 24), None);
}

#[test]
fn a_shortcut_s_request_past_its_deadline_fails_and_a_later_answer_does_it_not() {
    let (mut host, mut plugin) = asking_host();
    let shortcuts = AutomationClient {
        waits: true,
        ..client()
    };
    let typed = || AutomationRequest::SendText {
        pane: None,
        text: "ls\r".to_owned(),
    };
    ask_of(&mut host, &mut plugin, 30, &shortcuts, typed());
    assert_eq!(answer(&host, 30), None);
    host.take_done();
    // Its deadline, a second before the host's, passes: it fails.
    host.advance(std::time::Duration::from_millis(WAITS_FOR_ANSWER_MS + 1));
    host.deliver(&mut plugin).unwrap();
    assert!(
        matches!(answer(&host, 30), Some(AutomationAnswer::Error { ref message, .. }) if message.contains("waited too long")),
        "{:?}",
        answer(&host, 30)
    );
    // The user allows the shortcuts later: the request that failed types
    // nothing, a new one runs.
    choose(&mut host, &mut plugin, ALLOW);
    assert!(
        host.take_done()
            .iter()
            .all(|done| !matches!(done, Request::SendText { .. })),
        "the late request typed"
    );
    ask_of(&mut host, &mut plugin, 31, &shortcuts, typed());
    assert!(
        host.take_done()
            .iter()
            .any(|done| matches!(done, Request::SendText { .. }))
    );
}

#[test]
fn the_requests_that_wait_for_the_answer_are_bounded_and_an_answer_ends_their_timers() {
    let (mut host, mut plugin) = asking_host();
    let shortcuts = AutomationClient {
        waits: true,
        ..client()
    };
    let read = || AutomationRequest::PaneText {
        pane: Some(PaneId(1)),
        selection: false,
    };
    for id in 0..MOST_WAITING as u64 {
        ask_of(&mut host, &mut plugin, id, &shortcuts, read());
        assert_eq!(answer(&host, id), None);
    }
    assert_eq!(host.timers().len(), MOST_WAITING);
    // One more fails at once.
    let past = MOST_WAITING as u64;
    ask_of(&mut host, &mut plugin, past, &shortcuts, read());
    assert!(matches!(
        answer(&host, past),
        Some(AutomationAnswer::Error { .. })
    ));
    // The answer does them all, their timers ended.
    choose(&mut host, &mut plugin, ALLOW);
    for id in 0..past {
        assert_eq!(
            answer(&host, id),
            Some(AutomationAnswer::Text("hello".to_owned()))
        );
    }
    assert!(host.timers().is_empty());
}
