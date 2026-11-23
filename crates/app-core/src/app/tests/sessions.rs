//! The tests of the sessions in the app: a switch hides the windows of the
//! session left, of no frame, and shows the other's, the last of them to
//! have had the keyboard taking it again; the Window menu's Sessions; a
//! session's name beside the window's number; a session closed with its
//! windows; and the windows of no session.

use lokyr_kernel::commands::{Capability, Category, Command, Uses, Value};
use lokyr_kernel::plugins::{Cx, Plugin};

use super::*;

/// Whether the app hid `window`, as a session that is not active hides it.
fn hidden(fake: &Fake, window: WindowId) -> bool {
    fake.window(window).unwrap().hidden
}

/// The windows open, past `known`.
fn opened(fake: &Fake, known: &[WindowId]) -> Vec<WindowId> {
    fake.windows()
        .filter(|window| !known.contains(window))
        .collect()
}

fn command(fake: &mut Fake, app: &mut App, name: &str) {
    fake.send_app(app, AppEvent::Command(name.to_owned()));
}

#[test]
fn a_switch_hides_the_windows_of_the_session_left_and_shows_the_other_s() {
    let (mut fake, mut app) = started();
    let first = fake.windows().next().unwrap();
    command(&mut fake, &mut app, NEW_WINDOW);
    let second = opened(&fake, &[first])[0];
    fake.send(&mut app, first, WindowEvent::Focused(true));
    assert!(!fake.command_enabled(LAST_SESSION));
    // A new session: its window shows, the others hide.
    command(&mut fake, &mut app, NEW_SESSION);
    let [third] = opened(&fake, &[first, second])[..] else {
        panic!("one window more");
    };
    assert!(hidden(&fake, first) && hidden(&fake, second));
    assert!(!hidden(&fake, third));
    assert!(fake.command_enabled(LAST_SESSION));
    // Its number is the first of its session's.
    let number = app
        .workspace
        .window(app.windows[&third].window)
        .and_then(lokyr_kernel::workspace::Window::number);
    assert_eq!(number, Some(1));
    // Hidden, a window takes no keys, as it keeps the keyboard while it
    // animates out of full screen.
    fake.send(
        &mut app,
        first,
        key_event(KeyCode::KeyQ, "q", Modifiers::NONE, true),
    );
    let pause = Instant::now() + Duration::from_millis(200);
    run_until(&mut fake, &mut app, |_, _| Instant::now() > pause);
    let typed = {
        let shown = app.windows[&first].window;
        let terminal = app.workspace.focused(shown).unwrap().terminal().unwrap();
        (0..terminal.rows()).any(|y| terminal.row_text(y).contains('q'))
    };
    assert!(!typed);
    // Hidden, a window draws nothing: the frame that it asks for waits.
    command(&mut fake, &mut app, CURSOR_GUIDE);
    let state = fake.window(first).unwrap();
    assert!(state.occluded && state.frame_held && !state.frame_requested);
    // The last two toggle, the window that had the keyboard in front.
    fake.send(&mut app, third, WindowEvent::Focused(true));
    command(&mut fake, &mut app, LAST_SESSION);
    assert!(!hidden(&fake, first) && !hidden(&fake, second));
    assert!(hidden(&fake, third));
    assert_eq!(fake.focus_requests().last(), Some(&first));
    // Shown, it draws the frame that it asked for.
    assert!(fake.window(first).unwrap().frame_requested);
    command(&mut fake, &mut app, LAST_SESSION);
    assert!(hidden(&fake, first) && !hidden(&fake, third));
    assert_eq!(fake.focus_requests().last(), Some(&third));
    // A window of the hidden session brought forward, as a notification's
    // click does, shows its session.
    fake.focus_window(second);
    fake.send(&mut app, second, WindowEvent::Focused(true));
    run_until(&mut fake, &mut app, |fake, _| !hidden(fake, first));
    assert!(hidden(&fake, third));
    // The windows' cycle and numbers keep to the session that shows.
    let digit = key_event(
        KeyCode::Digit1,
        "1",
        Modifiers {
            left_alt: true,
            ..CMD
        },
        true,
    );
    fake.send(&mut app, second, digit);
    assert_eq!(fake.focus_requests().last(), Some(&first));
}

#[test]
fn the_window_menu_s_sessions_rename_and_close_a_session() {
    let (mut fake, mut app) = started();
    let first = fake.windows().next().unwrap();
    // In the Window menu, of a menu of their own, and in no other.
    let window_menu = menu_items(&fake, 4);
    assert!(
        window_menu.contains(&"Sessions".to_owned()),
        "{window_menu:?}"
    );
    let sessions = fake.main_menu().unwrap().menus[4]
        .items
        .iter()
        .find_map(|item| match item {
            MenuItem::Submenu(menu) => Some(menu.clone()),
            _ => None,
        })
        .unwrap();
    let titles: Vec<String> = sessions
        .items
        .iter()
        .filter_map(|item| match item {
            MenuItem::Command {
                title, shortcut, ..
            } => Some(format!(
                "{title}{}",
                if shortcut.is_some() { " ⌨" } else { "" }
            )),
            _ => None,
        })
        .collect();
    assert_eq!(
        titles,
        [
            "Switch Session… ⌨",
            "New Session",
            "Switch to Last Session ⌨",
            "Rename Session…",
            "Save Session as Layout…",
            "Close Session"
        ]
    );
    assert!(!menu_items(&fake, 1).contains(&"New Session".to_owned()));
    // Renamed, its name shows before its windows' keys.
    fake.send(&mut app, first, WindowEvent::Focused(true));
    command(&mut fake, &mut app, RENAME_SESSION);
    let open = fake.alerts().last().expect("the question").clone();
    assert_eq!(open.alert.input.as_deref(), Some(""));
    fake.answer_alert_typed(&mut app, open.id, (0, false), Some("  work  "));
    let accessory = |fake: &Fake, window| fake.window(window).unwrap().title_accessory.clone();
    assert_eq!(accessory(&fake, first).as_deref(), Some("work · ⌥⌘1"));
    // A new session, of no name, shows its keys alone; closed, as its
    // program runs, once the user confirms it, it goes, the one before it
    // showing again.
    command(&mut fake, &mut app, NEW_SESSION);
    let [other] = opened(&fake, &[first])[..] else {
        panic!("one window more");
    };
    assert_eq!(accessory(&fake, other).as_deref(), Some("⌥⌘1"));
    assert_eq!(app.workspace.sessions().len(), 2);
    command(&mut fake, &mut app, CLOSE_SESSION);
    let open = fake.alerts().last().unwrap().clone();
    fake.answer_alert(&mut app, open.id, 0);
    assert!(fake.window(other).is_none());
    assert_eq!(app.workspace.sessions().len(), 1);
    assert!(!hidden(&fake, first));
    assert_eq!(fake.focus_requests().last(), Some(&first));
}

#[test]
fn closing_a_session_of_a_program_running_asks_first() {
    let mut fake = Fake::default();
    let mut app = app(Config::default(), shell("exec sleep 30"));
    fake.start(&mut app);
    let first = fake.windows().next().unwrap();
    command(&mut fake, &mut app, NEW_SESSION);
    let [other] = opened(&fake, &[first])[..] else {
        panic!("one window more");
    };
    fake.send(&mut app, other, WindowEvent::Focused(true));
    run_until(&mut fake, &mut app, |_, app| {
        app.workspace
            .panes_of(app.windows[&other].window)
            .into_iter()
            .any(|pane| app.workspace.pane(pane).and_then(Pane::running).is_some())
    });
    command(&mut fake, &mut app, CLOSE_SESSION);
    let open = fake.alerts().last().expect("the question").clone();
    assert_eq!(open.alert.message, "Close this session?");
    // Cancelled, it stays; confirmed, its windows close.
    fake.answer_alert(&mut app, open.id, 1);
    assert!(fake.window(other).is_some());
    command(&mut fake, &mut app, CLOSE_SESSION);
    let open = fake.alerts().last().unwrap().clone();
    fake.answer_alert(&mut app, open.id, 0);
    assert!(fake.window(other).is_none());
    assert_eq!(app.workspace.sessions().len(), 1);
    assert!(!hidden(&fake, first));
}

#[test]
fn the_quick_terminal_and_a_man_page_s_window_show_in_every_session() {
    let (mut fake, mut app) = started();
    let first = fake.windows().next().unwrap();
    toggle(&mut fake, &mut app);
    let quick = quick_window(&fake).unwrap();
    let program = man_program(None, &["ls".to_owned()]);
    let page = app
        .open_window_running(&mut fake, &program, Some("man ls".to_owned()), None)
        .unwrap();
    command(&mut fake, &mut app, NEW_SESSION);
    assert!(hidden(&fake, first));
    for window in [quick, page] {
        assert!(!hidden(&fake, window));
        assert_eq!(
            app.workspace
                .window(app.windows[&window].window)
                .unwrap()
                .session(),
            None
        );
    }
    // The man page's window numbers beside every session's windows, as it
    // shows in each: the new session's first window takes the number after
    // it, and a man page's opened then the one after them all.
    let number = |app: &App, window: WindowId| {
        app.workspace
            .window(app.windows[&window].window)
            .and_then(workspace::Window::number)
    };
    let second = opened(&fake, &[first, quick, page])[0];
    assert_eq!(number(&app, first), Some(1));
    assert_eq!(number(&app, page), Some(2));
    assert_eq!(number(&app, second), Some(1));
    assert_eq!(number(&app, quick), None);
    let later = app
        .open_window_running(&mut fake, &program, Some("man ls".to_owned()), None)
        .unwrap();
    assert_eq!(number(&app, later), Some(3));
}

#[test]
fn a_binding_of_a_session_s_name_switches_to_it() {
    let (mut fake, mut app) = started();
    let first = fake.windows().next().unwrap();
    let home = app.workspace.active_session();
    command(&mut fake, &mut app, NEW_SESSION);
    let work = app.workspace.active_session();
    assert!(app.workspace.rename_session(work, "work"));
    assert!(app.workspace.rename_session(home, "home"));
    let named = |name: &str| {
        lokyr_kernel::commands::Args::from([(
            "name".to_owned(),
            lokyr_kernel::commands::Value::String(name.to_owned()),
        )])
    };
    app.run(&mut fake, SWITCH_SESSION, named("home"));
    assert_eq!(app.workspace.active_session(), home);
    assert!(!hidden(&fake, first));
    // A name of no session switches nothing.
    app.run(&mut fake, SWITCH_SESSION, named("gone"));
    assert_eq!(app.workspace.active_session(), home);
    app.run(&mut fake, SWITCH_SESSION, named("work"));
    assert_eq!(app.workspace.active_session(), work);
    assert!(hidden(&fake, first));
}

#[test]
fn the_recent_folders_are_kept_for_the_next_start() {
    let base = real_directory("recent-folders");
    let state = base.join("state");
    let mut fake = Fake::default();
    let mut app = app(Config::default(), quiet()).with_state(Some(state.clone()));
    fake.start(&mut app);
    app.workspace
        .set_recent_folders(vec!["/tmp/a".to_owned(), "/tmp/b".to_owned()]);
    app.quit(&mut fake);
    // The next start lists them again.
    let mut fake = Fake::default();
    let mut app = app_of_state(state.clone());
    fake.start(&mut app);
    assert_eq!(app.workspace.recent_folders(), ["/tmp/a", "/tmp/b"]);
    let _ = std::fs::remove_dir_all(base);
}

/// An app of the default config of the state's directory `state`.
fn app_of_state(state: std::path::PathBuf) -> App {
    app(Config::default(), quiet()).with_state(Some(state))
}

#[test]
fn a_session_saved_as_a_layout_opens_again_as_a_new_session() {
    let (mut fake, mut app, path) = with_file("layouts", "");
    let first = fake.windows().next().unwrap();
    fake.send(&mut app, first, WindowEvent::Focused(true));
    let session = app.workspace.active_session();
    app.workspace.rename_session(session, "api");
    // Asked for its name, of the session's at first, it writes the file.
    command(&mut fake, &mut app, SAVE_LAYOUT);
    let open = fake.alerts().last().expect("the question").clone();
    assert_eq!(open.alert.input.as_deref(), Some("api"));
    fake.answer_alert_typed(&mut app, open.id, (0, false), Some(" work "));
    let folder = path.parent().unwrap().join("layouts");
    let file = folder.join("work.toml");
    let layout = lokyr_kernel::layouts::Layout::read(&file).unwrap();
    assert_eq!(layout.windows.len(), 1);
    // Saved again of that name, it asks before it replaces it.
    command(&mut fake, &mut app, SAVE_LAYOUT);
    let open = fake.alerts().last().unwrap().clone();
    fake.answer_alert_typed(&mut app, open.id, (0, false), Some("work"));
    let replace = fake.alerts().last().unwrap().clone();
    assert_eq!(replace.alert.message, "Replace the layout “work”?");
    fake.answer_alert(&mut app, replace.id, 1);
    // A pane's command, added by hand, runs in its shell as it starts.
    std::fs::write(
        &file,
        concat!(
            "[[window]]\n[[window.tab]]\ntitle = \"server\"\n[window.tab.split]\n",
            "direction = \"right\"\n[window.tab.split.first.pane]\n",
            "command = \"printf 'from the layout'\"\n[window.tab.split.second.pane]\n",
        ),
    )
    .unwrap();
    let named = |name: &str| {
        lokyr_kernel::commands::Args::from([(
            "name".to_owned(),
            lokyr_kernel::commands::Value::String(name.to_owned()),
        )])
    };
    app.run(&mut fake, OPEN_LAYOUT, named("work"));
    let opened = app.workspace.active_session();
    assert_ne!(opened, session);
    assert_eq!(app.workspace.session(opened).unwrap().name(), "work");
    let [window] = app.workspace.session_windows(opened)[..] else {
        panic!("one window");
    };
    assert!(hidden(&fake, first));
    let tab = app.workspace.window(window).unwrap().active_tab();
    let tab = app.workspace.tab(tab).unwrap();
    assert_eq!(tab.title(), "server");
    assert_eq!(tab.layout().panes().len(), 2);
    // The shell of the test is `cat`, which echoes what its pane typed.
    let pane = tab.layout().panes()[0];
    run_until(&mut fake, &mut app, |_, app| {
        let terminal = app.workspace.pane(pane).unwrap().terminal().unwrap();
        (0..terminal.rows()).any(|y| terminal.row_text(y).contains("printf 'from the layout'"))
    });
    // A command of a folder that is not there runs nowhere else, as a
    // notice tells.
    std::fs::write(
        folder.join("gone.toml"),
        "[[window]]\n[[window.tab]]\n[window.tab.pane]\nfolder = \"/no/such/folder\"\ncommand = \"echo not here\"\n",
    )
    .unwrap();
    app.run(&mut fake, OPEN_LAYOUT, named("gone"));
    let shown = app.diagnostics.shown_diagnostic().expect("a notice");
    assert!(
        shown.text.contains("`echo not here` in /no/such/folder"),
        "{shown:?}"
    );
    let gone = app.workspace.active_session();
    let [window] = app.workspace.session_windows(gone)[..] else {
        panic!("one window");
    };
    let pane = app.workspace.focused(window).unwrap().id();
    let pause = Instant::now() + Duration::from_millis(300);
    run_until(&mut fake, &mut app, |_, _| Instant::now() > pause);
    let terminal = app.workspace.pane(pane).unwrap().terminal().unwrap();
    assert!((0..terminal.rows()).all(|y| !terminal.row_text(y).contains("not here")));
    drop(terminal);
    // A layout that does not read is told, of its file and line.
    std::fs::write(folder.join("broken.toml"), "[[window]]\nshape = 1\n").unwrap();
    app.run(&mut fake, OPEN_LAYOUT, named("broken"));
    let shown = app.diagnostics.shown_diagnostic().expect("a notice");
    assert!(shown.text.contains("broken.toml: line 2"), "{shown:?}");
    assert_eq!(app.workspace.active_session(), gone);
    let _ = std::fs::remove_dir_all(path.parent().unwrap());
}

#[test]
fn a_plugin_runs_the_sessions_commands_of_their_requests_capabilities() {
    let (_fake, app) = started();
    let uses = Uses::new(["window.*"]);
    let check = |name: &str, args: Args, granted: &[Capability]| {
        app.commands
            .check_for_plugin(name, args, &[], &uses, granted)
            .map(|_| ())
            .map_err(|error| error.code)
    };
    let named = |name: &str| {
        let mut args = Args::new();
        args.insert("name".to_owned(), Value::String(name.to_owned()));
        args
    };
    let write = [Capability::WorkspaceWrite];
    for name in [
        NEW_SESSION,
        LAST_SESSION,
        RENAME_SESSION,
        CLOSE_SESSION,
        SAVE_LAYOUT,
    ] {
        assert_eq!(
            check(name, Args::new(), &[]),
            Err(Code::MissingCapability),
            "{name}"
        );
        assert_eq!(check(name, Args::new(), &write), Ok(()), "{name}");
    }
    assert_eq!(
        check(SWITCH_SESSION, named("work"), &[]),
        Err(Code::MissingCapability)
    );
    assert_eq!(check(SWITCH_SESSION, named("work"), &write), Ok(()));
    // A layout's commands, typed into its shells, take `pty-write` too.
    assert_eq!(
        check(OPEN_LAYOUT, named("api"), &write),
        Err(Code::MissingCapability)
    );
    assert_eq!(
        check(
            OPEN_LAYOUT,
            named("api"),
            &[Capability::WorkspaceWrite, Capability::PtyWrite]
        ),
        Ok(())
    );
}

/// A plugin whose `maker.two` makes two sessions in one call, and whose
/// `maker.back` makes one and switches back to the first session.
struct Maker;

const MAKER: &str = concat!(
    "[plugin]\nname = \"lokyr/maker\"\nkind = \"builtin\"\nlicense = \"MIT\"\n",
    "[capabilities]\nworkspace-write = true\n",
);

impl Plugin for Maker {
    fn activate(&mut self, cx: &mut Cx<'_>) -> Result<(), Error> {
        for name in ["maker.two", "maker.back"] {
            cx.register_command(Command {
                name: name.to_owned(),
                title: name.to_owned(),
                category: Category::View,
                capabilities: Vec::new(),
                params: Vec::new(),
                when: None,
            })?;
        }
        Ok(())
    }

    fn on_command(&mut self, cx: &mut Cx<'_>, name: &str, _: Args) -> Result<(), Error> {
        if name == "maker.two" {
            cx.new_session("one", None)?;
            cx.new_session("two", None)?;
            return Ok(());
        }
        let first = cx.sessions()?.last().map(|session| session.id);
        cx.new_session("three", None)?;
        if let Some(first) = first {
            cx.switch_session(first)?;
        }
        Ok(())
    }
}

#[test]
fn a_plugin_s_new_session_opens_its_first_window_in_it_alone() {
    let mut fake = Fake::default();
    let mut app = App::new(None, unpadded(), None)
        .with_program(quiet())
        .with_builtin(MAKER, Box::new(Maker));
    fake.start(&mut app);
    let first = fake.windows().next().unwrap();
    // Two sessions in one call: the first, of no window, goes as the second
    // takes its place, which opens its window.
    command(&mut fake, &mut app, "maker.two");
    assert_eq!(names(&app), ["two", ""]);
    let opened = opened(&fake, &[first]);
    assert_eq!(opened.len(), 1);
    let two = app.workspace.active_session();
    assert_eq!(
        app.workspace
            .window(app.windows[&opened[0]].window)
            .unwrap()
            .session(),
        Some(two)
    );
    assert!(hidden(&fake, first));
    // A session made and left in one call opens no window: it went.
    let before: Vec<WindowId> = fake.windows().collect();
    command(&mut fake, &mut app, "maker.back");
    assert_eq!(fake.windows().count(), before.len());
    assert_eq!(names(&app), ["", "two"]);
    assert!(!hidden(&fake, first));
}

#[test]
fn a_new_session_from_no_window_leaves_one() {
    let (mut fake, mut app, first) = started_running(idle());
    fake.send(&mut app, first, WindowEvent::CloseRequested);
    assert_eq!(fake.windows().count(), 0);
    command(&mut fake, &mut app, NEW_SESSION);
    assert_eq!(fake.windows().count(), 1);
    assert_eq!(app.workspace.sessions().len(), 1);
}

/// The names of the sessions, in the order of their use.
fn names(app: &App) -> Vec<String> {
    app.workspace
        .sessions()
        .iter()
        .map(|session| session.name().to_owned())
        .collect()
}
