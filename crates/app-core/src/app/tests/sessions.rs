//! The tests of the sessions in the app: a switch hides the windows of the
//! session left, of no frame, and shows the other's, the last of them to
//! have had the keyboard taking it again; the Window menu's Sessions; a
//! session's name beside the window's number; a session closed with its
//! windows; and the windows of no session.

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
