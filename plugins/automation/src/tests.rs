//! The plugin against the SDK's fake host, which hands it the automation's
//! requests as the app's host does and keeps its answers.

use lokyr_plugin_sdk::fake::FakeHost;
use lokyr_plugin_sdk::message::Request;

use super::*;

/// The plugin's name, as its calls give it.
const NAME: &str = "lokyr/automation";

/// The app that asks, in the tests.
const CLIENT: &str = "com.acme.script";

/// A host of a window of pane 1, which has the focus and shows `hello`,
/// and the plugin, activated.
fn host() -> (FakeHost, Automation) {
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

/// Hands the plugin the request `request` of `id`, of [`CLIENT`].
fn ask(host: &mut FakeHost, plugin: &mut Automation, id: u64, request: AutomationRequest) {
    host.post(Event::Automation {
        id,
        client: CLIENT.to_owned(),
        request,
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
fn a_pane_s_text_waits_for_its_app_s_answer_which_the_plugin_keeps() {
    let (mut host, mut plugin) = host();
    let read = || AutomationRequest::PaneText {
        pane: None,
        selection: false,
    };
    ask(&mut host, &mut plugin, 7, read());
    assert_eq!(answer(&host, 7), None);
    let question = host.question().expect("a question about the app");
    host.post(Event::Chosen {
        id: question,
        button: Some(ALLOW),
    });
    host.deliver(&mut plugin).unwrap();
    assert_eq!(
        answer(&host, 7),
        Some(AutomationAnswer::Text("hello".to_owned()))
    );
    let Data::Map(state) = host.state() else {
        panic!("{:?}", host.state());
    };
    assert_eq!(
        state.get(READERS),
        Some(&Data::Map(BTreeMap::from([(
            CLIENT.to_owned(),
            Data::Boolean(true)
        )])))
    );
    // The next read, of a plugin that starts again of its state, asks
    // nothing.
    let mut again = Automation::default();
    host.activate(&mut again).unwrap();
    ask(&mut host, &mut again, 8, read());
    assert_eq!(host.question(), None);
    assert_eq!(
        answer(&host, 8),
        Some(AutomationAnswer::Text("hello".to_owned()))
    );
}

#[test]
fn an_app_that_the_user_refused_reads_no_text() {
    let (mut host, mut plugin) = host();
    let read = || AutomationRequest::PaneText {
        pane: Some(PaneId(1)),
        selection: true,
    };
    ask(&mut host, &mut plugin, 9, read());
    let question = host.question().unwrap();
    host.post(Event::Chosen {
        id: question,
        button: Some(DENY),
    });
    host.deliver(&mut plugin).unwrap();
    let refused = |host: &FakeHost, id: u64| matches!(answer(host, id), Some(AutomationAnswer::Error { code, .. }) if code == Code::Failed.number());
    assert!(refused(&host, 9), "{:?}", host.automation_answers());
    ask(&mut host, &mut plugin, 10, read());
    assert_eq!(host.question(), None);
    assert!(refused(&host, 10));
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
fn a_key_goes_to_the_focused_pane_and_a_click_is_refused() {
    let (mut host, mut plugin) = host();
    let key = |keys: &str| AutomationRequest::SendKey {
        pane: None,
        keys: keys.to_owned(),
    };
    ask(&mut host, &mut plugin, 17, key("ctrl+c"));
    assert_eq!(answer(&host, 17), Some(AutomationAnswer::Done));
    assert!(host.take_done().contains(&Request::SendKey {
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
