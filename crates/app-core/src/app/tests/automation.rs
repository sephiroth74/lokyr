//! The automation's requests of scripts and shortcuts, as the platform's
//! bridges hand them, through the automation's plugin, and their answers
//! back to the platform.

use std::time::Duration;

use lokyr_platform_core::automation as platform;

use super::*;

/// The app that asks, in the tests.
const CLIENT: &str = "com.acme.script";

/// Hands the app the request `request` of `id`, of [`CLIENT`], and waits
/// for its answer.
fn ask(fake: &mut Fake, app: &mut App, id: u64, request: platform::Request) -> platform::Answer {
    send(fake, app, id, request);
    answered(fake, app, id)
}

/// Hands the app the request `request` of `id`, of [`CLIENT`].
fn send(fake: &mut Fake, app: &mut App, id: u64, request: platform::Request) {
    fake.send_app(
        app,
        AppEvent::Automation {
            id,
            client: CLIENT.to_owned(),
            request,
        },
    );
}

/// The answer of `id`, once it comes.
fn answered(fake: &mut Fake, app: &mut App, id: u64) -> platform::Answer {
    let answer = |fake: &Fake| {
        fake.automation_answers()
            .iter()
            .find(|(answered, _)| *answered == id)
            .map(|(_, answer)| answer.clone())
    };
    run_until(fake, app, |fake, _| answer(fake).is_some());
    answer(fake).unwrap()
}

#[test]
fn a_script_s_request_opens_a_tab_whose_ids_its_answer_tells() {
    let (mut fake, mut app) = started();
    let window = fake.windows().next().unwrap();
    let request = platform::Request::NewTab {
        environment: Default::default(),
        window: None,
        folder: None,
        command: None,
    };
    let platform::Answer::Opened { tab, pane, .. } = ask(&mut fake, &mut app, 1, request) else {
        panic!("{:?}", fake.automation_answers());
    };
    let shown = app.workspace.window(app.windows[&window].window).unwrap();
    assert_eq!(shown.tabs().len(), 2);
    assert!(shown.tabs().iter().any(|id| id.get() == tab));
    assert!(app.workspace.pane(PaneId::of(pane)).is_some());
}

#[test]
fn a_request_of_a_command_that_is_not_there_is_refused() {
    let (mut fake, mut app) = started();
    let request = platform::Request::Run {
        command: "acme.none".to_owned(),
        args: Vec::new(),
    };
    assert!(
        matches!(
            ask(&mut fake, &mut app, 2, request),
            platform::Answer::Error { code, .. } if code == Code::NotFound.number()
        ),
        "{:?}",
        fake.automation_answers()
    );
}

#[test]
fn the_ids_stay_the_same_from_a_request_to_the_next() {
    let (mut fake, mut app, window) = focused();
    let first = ask(&mut fake, &mut app, 3, platform::Request::Workspace);
    let second = ask(&mut fake, &mut app, 4, platform::Request::Workspace);
    assert_eq!(first, second);
    let platform::Answer::Workspace(windows) = first else {
        panic!("{first:?}");
    };
    let id = app.windows[&window].window.get();
    assert_eq!(
        windows.iter().map(|shown| shown.id).collect::<Vec<_>>(),
        [id]
    );
    assert!(windows[0].focused);
}

#[test]
fn a_session_s_switch_makes_the_session_of_a_name_that_is_not_there() {
    let (mut fake, mut app) = started();
    let home = app.workspace.active_session();
    assert!(app.workspace.rename_session(home, "home"));
    let switch = |name: &str| platform::Request::SwitchSession {
        name: name.to_owned(),
    };
    assert_eq!(
        ask(&mut fake, &mut app, 5, switch("work")),
        platform::Answer::Done
    );
    let work = app.workspace.active_session();
    assert_ne!(work, home);
    let names: Vec<String> = app
        .workspace
        .sessions()
        .into_iter()
        .map(|session| session.name().to_owned())
        .collect();
    assert_eq!(names, ["work", "home"]);
    // A session of that name switches, and no other is made.
    assert_eq!(
        ask(&mut fake, &mut app, 6, switch("work")),
        platform::Answer::Done
    );
    assert_eq!(app.workspace.active_session(), work);
    assert_eq!(
        ask(&mut fake, &mut app, 7, switch("home")),
        platform::Answer::Done
    );
    assert_eq!(app.workspace.active_session(), home);
    assert_eq!(app.workspace.sessions().len(), 2);
}

#[test]
fn the_commands_are_told_as_the_palette_lists_them() {
    let (mut fake, mut app) = started();
    let platform::Answer::Commands(commands) =
        ask(&mut fake, &mut app, 8, platform::Request::Commands)
    else {
        panic!("{:?}", fake.automation_answers());
    };
    assert!(commands.contains(&platform::Command {
        name: "window.show-quick-terminal".to_owned(),
        title: "Show Quick Terminal".to_owned(),
        args: Vec::new(),
    }));
    // The menu bar's items of their arguments, as the palette lists them.
    assert!(
        commands.iter().any(|command| command.name == "splits.split"
            && command.args
                == [(
                    "direction".to_owned(),
                    platform::Arg::Text("right".to_owned())
                )]),
        "{commands:?}"
    );
}

#[test]
fn a_new_tab_s_program_takes_the_variables_that_the_request_gives() {
    let program = Program::command(
        "/bin/sh",
        ["-c", "printf '%s' \"$LOKYR_TEST_VARIABLE\"; exec /bin/cat"],
    );
    let mut fake = Fake::default();
    let mut app = app(Config::default(), program);
    fake.start(&mut app);
    let request = platform::Request::NewTab {
        window: None,
        folder: None,
        command: None,
        environment: vec![(
            "LOKYR_TEST_VARIABLE".to_owned(),
            "from-a-shortcut".to_owned(),
        )],
    };
    let platform::Answer::Opened { pane, .. } = ask(&mut fake, &mut app, 9, request) else {
        panic!("{:?}", fake.automation_answers());
    };
    let first_row = |app: &App| {
        app.workspace
            .pane(PaneId::of(pane))
            .and_then(lokyr_kernel::workspace::Pane::terminal)
            .map(|terminal| terminal.row_text(0))
            .unwrap_or_default()
    };
    run_until(&mut fake, &mut app, |_, app| {
        first_row(app).starts_with("from-a-shortcut")
    });
    // A name of `=` is refused, as the request opens nothing.
    let request = platform::Request::NewTab {
        window: None,
        folder: None,
        command: None,
        environment: vec![("A=B".to_owned(), "x".to_owned())],
    };
    assert!(
        matches!(
            ask(&mut fake, &mut app, 10, request),
            platform::Answer::Error { .. }
        ),
        "{:?}",
        fake.automation_answers()
    );
}

#[test]
fn a_key_runs_its_binding_s_command_or_goes_to_the_program() {
    let program = Program::command("/bin/sh", ["-c", "printf hello; exec /bin/cat"]);
    let mut fake = Fake::default();
    let mut app = app(Config::default(), program);
    fake.start(&mut app);
    let window = fake.windows().next().unwrap();
    assert!(fake.send(&mut app, window, WindowEvent::Focused(true)));
    let pane = app
        .workspace
        .focused(app.windows[&window].window)
        .unwrap()
        .id();
    let text = |app: &App| {
        app.workspace
            .pane(pane)
            .and_then(lokyr_kernel::workspace::Pane::terminal)
            .map(|terminal| terminal.row_text(0))
            .unwrap_or_default()
    };
    run_until(&mut fake, &mut app, |_, app| text(app).starts_with("hello"));
    let key = |keys: &str| platform::Request::SendKey {
        pane: None,
        keys: keys.to_owned(),
    };
    // Command-A, which the keymap binds, selects all.
    assert_eq!(
        ask(&mut fake, &mut app, 11, key("cmd+a")),
        platform::Answer::Done
    );
    run_until(&mut fake, &mut app, |_, app| {
        app.workspace
            .pane(pane)
            .and_then(lokyr_kernel::workspace::Pane::selection_text)
            .is_some_and(|selected| selected.contains("hello"))
    });
    // A key of no binding goes to the program, whose terminal echoes it.
    assert_eq!(
        ask(&mut fake, &mut app, 12, key("x")),
        platform::Answer::Done
    );
    run_until(&mut fake, &mut app, |_, app| {
        text(app).starts_with("hellox")
    });
}

/// A mouse event of `action` over the cell at `column` and `row`, from 0.
fn mouse(action: platform::MouseAction, column: u16, row: u16) -> platform::Mouse {
    platform::Mouse {
        action,
        button: None,
        column,
        row,
        shift: false,
        control: false,
        alt: false,
        steps: None,
    }
}

#[test]
fn a_mouse_event_goes_to_a_program_of_the_mouse_over_its_cell() {
    // The program asks for the mouse, of SGR's reports, which its
    // terminal's echo shows.
    let program = Program::command(
        "/bin/sh",
        [
            "-c",
            "printf '\\033[?1000h\\033[?1006hready'; exec /bin/cat",
        ],
    );
    let mut fake = Fake::default();
    let mut app = app(Config::default(), program);
    fake.start(&mut app);
    let window = fake.windows().next().unwrap();
    let pane = app
        .workspace
        .focused(app.windows[&window].window)
        .unwrap()
        .id();
    let text = |app: &App| {
        app.workspace
            .pane(pane)
            .and_then(lokyr_kernel::workspace::Pane::terminal)
            .map(|terminal| terminal.row_text(0))
            .unwrap_or_default()
    };
    run_until(&mut fake, &mut app, |_, app| text(app).starts_with("ready"));
    let press = platform::Request::SendMouse {
        pane: None,
        mouse: mouse(platform::MouseAction::Press, 2, 1),
    };
    assert_eq!(ask(&mut fake, &mut app, 13, press), platform::Answer::Done);
    run_until(&mut fake, &mut app, |_, app| text(app).contains("[<0;3;2M"));
    // A cell off the screen is refused.
    let off = platform::Request::SendMouse {
        pane: None,
        mouse: mouse(platform::MouseAction::Press, 9999, 0),
    };
    assert!(
        matches!(
            ask(&mut fake, &mut app, 14, off),
            platform::Answer::Error { .. }
        ),
        "{:?}",
        fake.automation_answers()
    );
}

#[test]
fn a_scroll_of_a_program_of_no_mouse_scrolls_the_pane_s_view() {
    let program = Program::command("/bin/sh", ["-c", "seq 200; exec /bin/cat"]);
    let mut fake = Fake::default();
    let mut app = app(Config::default(), program);
    fake.start(&mut app);
    let window = fake.windows().next().unwrap();
    let pane = app
        .workspace
        .focused(app.windows[&window].window)
        .unwrap()
        .id();
    run_until(&mut fake, &mut app, |_, app| {
        app.workspace
            .pane(pane)
            .and_then(lokyr_kernel::workspace::Pane::terminal)
            .is_some_and(|terminal| terminal.scrollback_rows() > 100)
    });
    let mut scroll = mouse(platform::MouseAction::ScrollUp, 0, 0);
    scroll.steps = Some(5);
    let request = platform::Request::SendMouse {
        pane: None,
        mouse: scroll,
    };
    assert_eq!(
        ask(&mut fake, &mut app, 15, request),
        platform::Answer::Done
    );
    run_until(&mut fake, &mut app, |_, app| {
        app.workspace
            .pane(pane)
            .is_some_and(|shown| shown.view_offset() == 5)
    });
}

#[test]
fn a_request_of_no_pane_goes_to_the_window_used_last_while_lokyr_is_behind() {
    let (mut fake, mut app) = started();
    let first = fake.windows().next().unwrap();
    app.run(&mut fake, NEW_WINDOW, Args::new());
    let second = fake.windows().find(|&window| window != first).unwrap();
    // The second window takes the focus, then Lokyr goes behind another
    // app, of no window of the focus.
    assert!(fake.send(&mut app, second, WindowEvent::Focused(true)));
    assert!(fake.send(&mut app, second, WindowEvent::Focused(false)));
    let pane_of = |app: &App, window: WindowId| {
        app.workspace
            .focused(app.windows[&window].window)
            .unwrap()
            .id()
    };
    let (in_first, in_second) = (pane_of(&app, first), pane_of(&app, second));
    let row = |app: &App, pane: PaneId| {
        app.workspace
            .pane(pane)
            .and_then(lokyr_kernel::workspace::Pane::terminal)
            .map(|terminal| terminal.row_text(0))
            .unwrap_or_default()
    };
    let request = platform::Request::SendText {
        pane: None,
        text: "behind".to_owned(),
    };
    assert_eq!(
        ask(&mut fake, &mut app, 16, request),
        platform::Answer::Done
    );
    run_until(&mut fake, &mut app, |_, app| {
        row(app, in_second).starts_with("behind")
    });
    assert!(!row(&app, in_first).contains("behind"));
}

#[test]
fn a_key_s_command_runs_in_its_pane_s_window_whatever_window_has_the_focus() {
    let (mut fake, mut app) = started();
    let first = fake.windows().next().unwrap();
    app.run(&mut fake, NEW_WINDOW, Args::new());
    let second = fake.windows().find(|&window| window != first).unwrap();
    let tabs = |app: &App, window: WindowId| {
        app.workspace
            .window(app.windows[&window].window)
            .unwrap()
            .tabs()
            .len()
    };
    let pane_of = |app: &App, window: WindowId| {
        app.workspace
            .focused(app.windows[&window].window)
            .unwrap()
            .id()
            .get()
    };
    let key = |pane: u64| platform::Request::SendKey {
        pane: Some(pane),
        keys: "cmd+t".to_owned(),
    };
    // The first window has the focus: Command-T to the second's pane opens
    // the tab there.
    assert!(fake.send(&mut app, first, WindowEvent::Focused(true)));
    let in_second = pane_of(&app, second);
    assert_eq!(
        ask(&mut fake, &mut app, 17, key(in_second)),
        platform::Answer::Done
    );
    run_until(&mut fake, &mut app, |_, app| tabs(app, second) == 2);
    assert_eq!(tabs(&app, first), 1);
    // No window has the focus, as Lokyr is behind: it opens all the same.
    assert!(fake.send(&mut app, first, WindowEvent::Focused(false)));
    let in_first = pane_of(&app, first);
    assert_eq!(
        ask(&mut fake, &mut app, 18, key(in_first)),
        platform::Answer::Done
    );
    run_until(&mut fake, &mut app, |_, app| tabs(app, first) == 2);
}

/// A plugin whose command, `presser.go`, bound to Control-Option-P, presses
/// its own key in the focused pane, as `runs` counts.
struct Presser {
    runs: std::rc::Rc<std::cell::Cell<usize>>,
}

impl lokyr_kernel::plugins::Plugin for Presser {
    fn activate(&mut self, cx: &mut lokyr_kernel::plugins::Cx<'_>) -> Result<(), Error> {
        cx.register_command(lokyr_kernel::commands::Command {
            name: "presser.go".to_owned(),
            title: "Press".to_owned(),
            category: lokyr_kernel::commands::Category::View,
            capabilities: Vec::new(),
            params: Vec::new(),
            when: None,
        })?;
        cx.bind(lokyr_kernel::plugins::host::Binding {
            keys: "ctrl+alt+p".to_owned(),
            command: "presser.go".to_owned(),
            args: Args::new(),
            when: None,
        })
    }

    fn on_command(
        &mut self,
        cx: &mut lokyr_kernel::plugins::Cx<'_>,
        _: &str,
        _: Args,
    ) -> Result<(), Error> {
        self.runs.set(self.runs.get() + 1);
        let pane = cx
            .focused_pane()
            .ok_or_else(|| Error::new(Code::NotFound, "no pane"))?;
        cx.press_key(pane, "ctrl+alt+p")
    }
}

#[test]
fn a_command_that_presses_its_own_key_stops_as_deep_as_commands_go() {
    let runs = std::rc::Rc::new(std::cell::Cell::new(0));
    let mut fake = Fake::default();
    let mut app = app(Config::default(), quiet()).with_plugin(
        "presser",
        Box::new(Presser {
            runs: std::rc::Rc::clone(&runs),
        }),
    );
    fake.start(&mut app);
    let window = fake.windows().next().unwrap();
    assert!(fake.send(&mut app, window, WindowEvent::Focused(true)));
    app.run(&mut fake, "presser.go", Args::new());
    let pause = Instant::now() + Duration::from_millis(200);
    run_until(&mut fake, &mut app, |_, _| Instant::now() > pause);
    assert!(runs.get() > 1, "{}", runs.get());
    assert!(runs.get() <= COMMANDS_DEEP + 1, "{}", runs.get());
}

#[test]
fn a_pane_s_text_comes_once_the_user_allows_its_app() {
    let (mut fake, mut app) = started();
    let request = || platform::Request::PaneText {
        pane: None,
        selection: false,
    };
    send(&mut fake, &mut app, 5, request());
    run_until(&mut fake, &mut app, |fake, _| fake.alerts().len() == 1);
    assert!(fake.alerts()[0].alert.message.contains(CLIENT));
    assert!(fake.automation_answers().is_empty());
    // The second button allows it.
    let question = fake.alerts()[0].id;
    fake.answer_alert(&mut app, question, 1);
    let answer = answered(&mut fake, &mut app, 5);
    assert!(matches!(answer, platform::Answer::Text(_)), "{answer:?}");
    // The next read asks nothing.
    assert!(matches!(
        ask(&mut fake, &mut app, 6, request()),
        platform::Answer::Text(_)
    ));
    assert!(fake.alerts().is_empty());
}

#[test]
fn a_request_that_waits_past_its_deadline_fails() {
    let (mut fake, mut app) = started();
    let request = platform::Request::PaneText {
        pane: None,
        selection: true,
    };
    send(&mut fake, &mut app, 7, request);
    run_until(&mut fake, &mut app, |fake, _| fake.alerts().len() == 1);
    fake.fire_timers(&mut app, Instant::now() + Duration::from_secs(11));
    assert_eq!(
        answered(&mut fake, &mut app, 7),
        platform::Answer::Error {
            code: Code::Failed.number(),
            message: super::super::automation::LATE.to_owned(),
        }
    );
    // The answer that comes after is the plugin's alone: the platform
    // took the request's.
    let question = fake.alerts()[0].id;
    fake.answer_alert(&mut app, question, 1);
    assert_eq!(fake.automation_answers().len(), 1);
}

#[test]
fn the_automation_turned_off_answers_at_once() {
    let config = Config::parse(
        "[plugins.automation]\nenabled = false\n",
        std::path::Path::new("config.toml"),
    )
    .unwrap();
    let mut fake = Fake::default();
    let mut app = app(config, quiet());
    fake.start(&mut app);
    assert_eq!(
        ask(&mut fake, &mut app, 8, platform::Request::Workspace),
        platform::Answer::Error {
            code: Code::Unavailable.number(),
            message: super::super::automation::OFF.to_owned(),
        }
    );
}
