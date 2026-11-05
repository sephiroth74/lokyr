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
    let mut host = FakeHost::new(NAME);
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
            (GOTO, vec![("index", Kind::Integer, false)]),
            (MOVE, vec![("delta", Kind::Integer, false)]),
            (RENAME, vec![("title", Kind::String, true)]),
            (SET_COLOR, vec![("color", Kind::String, false)]),
            (DETACH, vec![]),
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
}

#[test]
fn a_new_tab_opens_after_the_others_and_shows() {
    let (mut host, mut plugin) = host(&[1], 1);
    let done = run(&mut host, &mut plugin, NEW, Args::new());
    let [
        Request::NewTab {
            window: WINDOW,
            index: None,
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
            ask: true
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
        [Request::TabToNewWindow(TabId(2))]
    );
    let (mut host, mut plugin) = self::host(&[1], 1);
    assert_eq!(run(&mut host, &mut plugin, DETACH, Args::new()), []);
}
