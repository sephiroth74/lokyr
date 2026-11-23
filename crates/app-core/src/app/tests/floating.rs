//! A tab's floating panes from the menus and the mouse: a new one, shown
//! and hidden; moved by its header and resized by its corner; floated from
//! the splits and moved back into them from the context menu.

use std::cell::RefCell;
use std::rc::Rc;

use lokyr_kernel::workspace::FloatingFrame;

use super::*;

fn command(fake: &mut Fake, app: &mut App, name: &str) {
    fake.send_app(app, AppEvent::Command(name.to_owned()));
}

/// The frame of the floating pane `pane` of `app`.
fn frame_of(app: &App, pane: PaneId) -> FloatingFrame {
    let tab = app.workspace.tab_of(pane).unwrap();
    app.workspace
        .tab(tab)
        .unwrap()
        .floating()
        .iter()
        .find(|floating| floating.pane == pane)
        .unwrap()
        .frame
}

/// A press at `from`, a move to `to`, and a release there, in points.
fn drag(fake: &mut Fake, app: &mut App, window: WindowId, from: Point, to: Point) {
    fake.send(app, window, click(true, from, Modifiers::NONE, 1));
    fake.send(app, window, moved(to, Modifiers::NONE));
    fake.send(app, window, click(false, to, Modifiers::NONE, 1));
}

#[test]
fn a_floating_pane_opens_shows_and_hides_moves_resizes_and_goes_into_the_splits() {
    let (mut fake, mut app, window) = started_running(idle());
    fake.send(&mut app, window, WindowEvent::Focused(true));
    let shown = app.windows[&window].window;
    let tab = app.workspace.window(shown).unwrap().active_tab();
    let first = app.focused(window).unwrap();
    // A new one, over the tab, with the keyboard.
    command(&mut fake, &mut app, NEW_FLOATING);
    let floating = app.focused(window).unwrap();
    assert_ne!(floating, first);
    assert!(app.workspace.tab(tab).unwrap().is_floating(floating));
    fake.send(&mut app, window, WindowEvent::Frame);
    // Hidden, the layout's pane takes the keyboard back; shown, the
    // floating one again.
    command(&mut fake, &mut app, TOGGLE_FLOATING);
    assert!(!app.workspace.tab(tab).unwrap().floating_shown());
    assert_eq!(app.focused(window), Some(first));
    command(&mut fake, &mut app, TOGGLE_FLOATING);
    assert_eq!(app.focused(window), Some(floating));
    fake.send(&mut app, window, WindowEvent::Frame);
    // Its header dragged moves it by the pointer's way, a share of the
    // tab's room.
    let scale = fake.window(window).unwrap().size.scale;
    let (_, room) = app.windows[&window].floating_rooms[0];
    let content = app.windows[&window].panes_room.unwrap();
    let points = |pixels: i32| f64::from(pixels) / scale;
    let header = Point {
        x: points(room.header.x) + f64::from(room.header.width) / scale / 2.0,
        y: points(room.header.y) + f64::from(room.header.height) / scale / 2.0,
    };
    let before = frame_of(&app, floating);
    let by = 20.0;
    drag(
        &mut fake,
        &mut app,
        window,
        header,
        Point {
            x: header.x + by,
            y: header.y + by,
        },
    );
    let after = frame_of(&app, floating);
    let across = by / (f64::from(content.width) / scale);
    assert!(
        (after.x() - before.x() - across).abs() < 0.002,
        "{before:?} {after:?}"
    );
    assert!((after.width() - before.width()).abs() < 0.001);
    fake.send(&mut app, window, WindowEvent::Frame);
    // Its bottom right corner dragged resizes it, its corner staying.
    let (_, room) = app.windows[&window].floating_rooms[0];
    let corner = Point {
        x: points(room.outer.x) + f64::from(room.outer.width) / scale - 1.0,
        y: points(room.outer.y) + f64::from(room.outer.height) / scale - 1.0,
    };
    drag(
        &mut fake,
        &mut app,
        window,
        corner,
        Point {
            x: corner.x - by,
            y: corner.y - by,
        },
    );
    let resized = frame_of(&app, floating);
    assert!((resized.x() - after.x()).abs() < 0.001);
    assert!(resized.width() < after.width() && resized.height() < after.height());
    // From its context menu, into the splits; and back out of them.
    command(&mut fake, &mut app, DOCK_PANE);
    let held = app.workspace.tab(tab).unwrap();
    assert!(held.floating().is_empty());
    assert_eq!(held.layout().panes(), [first, floating]);
    command(&mut fake, &mut app, FLOAT_PANE);
    let held = app.workspace.tab(tab).unwrap();
    assert!(held.is_floating(floating));
    assert_eq!(held.layout().panes(), [first]);
}

#[test]
fn a_floating_pane_s_context_menu_moves_it_into_the_splits_and_a_split_one_floats() {
    let (mut fake, mut app, window) = started_with(reader(""));
    fake.send(&mut app, window, WindowEvent::Focused(true));
    let open = |fake: &mut Fake, app: &mut App, at: Point| {
        let menus = fake.context_menus().len();
        right_click(fake, app, window, at, Modifiers::NONE);
        run_until(fake, app, |fake, _| fake.context_menus().len() > menus);
        context_menu(fake, window).1
    };
    // A tab of one pane: neither.
    let items = open(&mut fake, &mut app, Point { x: 10.0, y: 10.0 });
    assert!(
        !items
            .iter()
            .any(|item| item.contains("Float") || item.contains("Splits")),
        "{items:?}"
    );
    // Over a floating pane: Move Pane into Splits.
    command(&mut fake, &mut app, NEW_FLOATING);
    fake.send(&mut app, window, WindowEvent::Frame);
    let scale = fake.window(window).unwrap().size.scale;
    let (_, room) = app.windows[&window].floating_rooms[0];
    let inside = Point {
        x: (f64::from(room.grid.x) + f64::from(room.grid.width) / 2.0) / scale,
        y: (f64::from(room.grid.y) + f64::from(room.grid.height) / 2.0) / scale,
    };
    let items = open(&mut fake, &mut app, inside);
    assert!(
        items.contains(&"Move Pane into Splits".to_owned()),
        "{items:?}"
    );
    // Moved, the split pane floats.
    command(&mut fake, &mut app, DOCK_PANE);
    fake.send(&mut app, window, WindowEvent::Frame);
    let items = open(&mut fake, &mut app, Point { x: 10.0, y: 10.0 });
    assert!(items.contains(&"Float Pane".to_owned()), "{items:?}");
    // Over a popup, which stays over its tab: neither.
    app.popup_shell = Some(std::path::PathBuf::from("/bin/sh"));
    app.run(&mut fake, POPUP, popup_of("sleep 30", &[]));
    fake.send(&mut app, window, WindowEvent::Frame);
    let (_, room) = *app.windows[&window].floating_rooms.last().unwrap();
    let inside = Point {
        x: (f64::from(room.grid.x) + f64::from(room.grid.width) / 2.0) / scale,
        y: (f64::from(room.grid.y) + f64::from(room.grid.height) / 2.0) / scale,
    };
    let items = open(&mut fake, &mut app, inside);
    assert!(
        !items
            .iter()
            .any(|item| item.contains("Float") || item.contains("Splits")),
        "{items:?}"
    );
}

/// The arguments of a popup of `command`, and of `more`.
fn popup_of(command: &str, more: &[(&str, Value)]) -> Args {
    let mut args = Args::new();
    args.insert("command".to_owned(), Value::String(command.to_owned()));
    for (name, value) in more {
        args.insert((*name).to_owned(), value.clone());
    }
    args
}

/// The text of the rows of `pane`'s screen that show some.
fn pane_rows(app: &App, pane: PaneId) -> Vec<String> {
    let pane = app.workspace.pane(pane).unwrap();
    (0..8)
        .filter_map(|row| {
            pane.line_text(row, 0)
                .map(|(text, _)| text.trim_end().to_owned())
        })
        .filter(|text| !text.is_empty())
        .collect()
}

#[test]
fn a_popup_closes_as_its_command_ends_or_is_interrupted_and_a_failed_one_waits_for_a_key() {
    let (mut fake, mut app, window) = started_running(idle());
    app.popup_shell = Some(std::path::PathBuf::from("/bin/sh"));
    let shown = app.windows[&window].window;
    let tab = app.workspace.window(shown).unwrap().active_tab();
    let first = app.focused(window).unwrap();
    // In the room's middle, of the shares that a popup takes, with the
    // keyboard; Escape goes to its program, which it leaves open.
    app.run(&mut fake, POPUP, popup_of("read line", &[]));
    let popup = app.focused(window).unwrap();
    assert_ne!(popup, first);
    assert!(app.workspace.is_popup(popup));
    assert_eq!(
        frame_of(&app, popup),
        FloatingFrame::centered(0.8, 0.7),
        "{:?}",
        app.workspace.tab(tab).unwrap().floating()
    );
    fake.send(
        &mut app,
        window,
        key_event(KeyCode::Escape, "\u{1b}", Modifiers::NONE, true),
    );
    assert_eq!(app.focused(window), Some(popup));
    // Its command's end of 0, as Return ends the read, closes it, the
    // layout's pane taking the keyboard back.
    fake.send(
        &mut app,
        window,
        key_event(KeyCode::Enter, "\r", Modifiers::NONE, true),
    );
    run_until(&mut fake, &mut app, |_, app| {
        app.workspace.pane(popup).is_none()
    });
    assert_eq!(app.focused(window), Some(first));
    // An interrupt closes it too, the signal's or the status of a program
    // that takes it, as fzf's of Escape.
    for command in ["kill -INT $$", "exit 130"] {
        app.run(&mut fake, POPUP, popup_of(command, &[]));
        let interrupted = app.focused(window).unwrap();
        assert_ne!(interrupted, first);
        run_until(&mut fake, &mut app, |_, app| {
            app.workspace.pane(interrupted).is_none()
        });
    }
    // A failed one, in its folder and of its shares, keeps its screen and
    // the line of its status until a key closes it, whatever
    // `close-on-exit` says.
    app.config.window.close_on_exit = CloseOnExit::Always;
    let folder = real_directory("popup-folder");
    app.run(
        &mut fake,
        POPUP,
        popup_of(
            "pwd; exit 3",
            &[
                (
                    "folder",
                    Value::String(folder.to_string_lossy().into_owned()),
                ),
                ("width", Value::Number(0.5)),
                ("height", Value::Integer(1)),
            ],
        ),
    );
    let failed = app.focused(window).unwrap();
    assert_eq!(frame_of(&app, failed), FloatingFrame::centered(0.5, 1.0));
    run_until(&mut fake, &mut app, |_, app| {
        app.workspace.pane(failed).is_some_and(Pane::is_kept)
    });
    let rows = pane_rows(&app, failed);
    assert_eq!(rows[0], folder.to_string_lossy(), "{rows:?}");
    assert!(rows.iter().any(|row| row.contains("code 3")), "{rows:?}");
    fake.send(&mut app, window, key());
    assert!(app.workspace.pane(failed).is_none());
    assert_eq!(app.focused(window), Some(first));
    assert_eq!(fake.windows().count(), 1);
    // Of no command, or of a blank one, none opens.
    app.run(&mut fake, POPUP, Args::new());
    app.run(&mut fake, POPUP, popup_of("  ", &[]));
    assert!(app.workspace.tab(tab).unwrap().floating().is_empty());
    assert_eq!(app.focused(window), Some(first));
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn a_popup_shows_of_its_own_and_leaves_the_hidden_floating_panes_hidden() {
    let (mut fake, mut app, window) = started_running(idle());
    app.popup_shell = Some(std::path::PathBuf::from("/bin/sh"));
    let shown = app.windows[&window].window;
    let tab = app.workspace.window(shown).unwrap().active_tab();
    let first = app.focused(window).unwrap();
    let rooms = |app: &App| -> Vec<PaneId> {
        app.windows[&window]
            .floating_rooms
            .iter()
            .map(|&(pane, _)| pane)
            .collect()
    };
    // A floating pane, hidden.
    command(&mut fake, &mut app, NEW_FLOATING);
    let floating = app.focused(window).unwrap();
    command(&mut fake, &mut app, TOGGLE_FLOATING);
    fake.send(&mut app, window, WindowEvent::Frame);
    assert!(rooms(&app).is_empty());
    // A popup shows alone, the floating pane hidden still.
    app.run(&mut fake, POPUP, popup_of("read line", &[]));
    let popup = app.focused(window).unwrap();
    fake.send(&mut app, window, WindowEvent::Frame);
    assert!(!app.workspace.tab(tab).unwrap().floating_shown());
    assert_eq!(rooms(&app), [popup]);
    // The others show and hide under it, which keeps the keyboard.
    command(&mut fake, &mut app, TOGGLE_FLOATING);
    fake.send(&mut app, window, WindowEvent::Frame);
    assert_eq!(rooms(&app), [floating, popup]);
    assert_eq!(app.focused(window), Some(popup));
    command(&mut fake, &mut app, TOGGLE_FLOATING);
    fake.send(&mut app, window, WindowEvent::Frame);
    assert_eq!(rooms(&app), [popup]);
    assert_eq!(app.focused(window), Some(popup));
    // Its end leaves them hidden, the keyboard to the layout's pane.
    fake.send(
        &mut app,
        window,
        key_event(KeyCode::Enter, "\r", Modifiers::NONE, true),
    );
    run_until(&mut fake, &mut app, |_, app| {
        app.workspace.pane(popup).is_none()
    });
    fake.send(&mut app, window, WindowEvent::Frame);
    assert!(rooms(&app).is_empty());
    assert!(!app.workspace.tab(tab).unwrap().floating_shown());
    assert_eq!(app.focused(window), Some(first));
}

#[test]
fn a_floating_pane_s_close_button_shows_its_hover_and_closes_it() {
    let (mut fake, mut app, window) = started_running(idle());
    let first = app.focused(window).unwrap();
    command(&mut fake, &mut app, NEW_FLOATING);
    let floating = app.focused(window).unwrap();
    fake.send(&mut app, window, WindowEvent::Frame);
    let scale = fake.window(window).unwrap().size.scale;
    let (_, room) = app.windows[&window].floating_rooms[0];
    let middle = |rect: lokyr_render_core::Rect| Point {
        x: (f64::from(rect.x) + f64::from(rect.width) / 2.0) / scale,
        y: (f64::from(rect.y) + f64::from(rect.height) / 2.0) / scale,
    };
    let close = middle(room.close);
    // Its hover under the pointer, none past it or out of the window.
    fake.send(&mut app, window, moved(close, Modifiers::NONE));
    assert_eq!(app.windows[&window].floating_close_hover, Some(floating));
    fake.send(&mut app, window, moved(middle(room.grid), Modifiers::NONE));
    assert_eq!(app.windows[&window].floating_close_hover, None);
    fake.send(&mut app, window, moved(close, Modifiers::NONE));
    fake.send(&mut app, window, WindowEvent::MouseExited);
    assert_eq!(app.windows[&window].floating_close_hover, None);
    // A click on it closes the pane, of a shell alone, at once, which
    // starts no move.
    fake.send(&mut app, window, click(true, close, Modifiers::NONE, 1));
    fake.send(&mut app, window, click(false, close, Modifiers::NONE, 1));
    assert!(app.workspace.pane(floating).is_none());
    assert!(app.windows[&window].floating_drag.is_none());
    assert_eq!(app.focused(window), Some(first));
}

/// A plugin whose `popper.open` opens a popup of the command that the test
/// gives, which records the events that it hears.
struct Popper {
    command: Rc<RefCell<String>>,
    heard: Rc<RefCell<Vec<lokyr_kernel::plugins::Event>>>,
}

const POPPER: &str = concat!(
    "[plugin]\nname = \"lokyr/popper\"\nkind = \"builtin\"\nlicense = \"MIT\"\n",
    "[capabilities]\nworkspace-write = true\npty-write = true\n",
);

impl lokyr_kernel::plugins::Plugin for Popper {
    fn activate(&mut self, cx: &mut lokyr_kernel::plugins::Cx<'_>) -> Result<(), Error> {
        cx.register_command(lokyr_kernel::commands::Command {
            name: "popper.open".to_owned(),
            title: "Open".to_owned(),
            category: lokyr_kernel::commands::Category::View,
            capabilities: Vec::new(),
            params: Vec::new(),
            when: None,
        })
    }

    fn on_command(
        &mut self,
        cx: &mut lokyr_kernel::plugins::Cx<'_>,
        _: &str,
        _: Args,
    ) -> Result<(), Error> {
        let command = self.command.borrow().clone();
        cx.open_popup(None, &command, None, None, None).map(|_| ())
    }

    fn on_event(
        &mut self,
        _: &mut lokyr_kernel::plugins::Cx<'_>,
        events: &[lokyr_kernel::plugins::Event],
    ) -> Result<(), Error> {
        self.heard.borrow_mut().extend(events.iter().cloned());
        Ok(())
    }
}

#[test]
fn a_plugin_s_popup_tells_it_its_command_s_status_as_it_closes() {
    let asked = Rc::new(RefCell::new(String::new()));
    let heard = Rc::new(RefCell::new(Vec::new()));
    let mut fake = Fake::default();
    let mut app = App::new(None, unpadded(), None)
        .with_program(idle())
        .with_builtin(
            POPPER,
            Box::new(Popper {
                command: Rc::clone(&asked),
                heard: Rc::clone(&heard),
            }),
        );
    app.popup_shell = Some(std::path::PathBuf::from("/bin/sh"));
    fake.start(&mut app);
    let window = fake.windows().next().unwrap();
    fake.send(&mut app, window, WindowEvent::Focused(true));
    let first = app.focused(window).unwrap();
    // A failed command's popup waits for a key, then tells its status.
    "exit 3".clone_into(&mut asked.borrow_mut());
    command(&mut fake, &mut app, "popper.open");
    let failed = app.focused(window).unwrap();
    assert_ne!(failed, first);
    assert!(app.workspace.is_popup(failed));
    run_until(&mut fake, &mut app, |_, app| {
        app.workspace.pane(failed).is_some_and(Pane::is_kept)
    });
    assert!(heard.borrow().is_empty());
    fake.send(&mut app, window, key());
    run_until(&mut fake, &mut app, |_, _| !heard.borrow().is_empty());
    assert_eq!(
        *heard.borrow(),
        [lokyr_kernel::plugins::Event::PopupClosed {
            pane: failed.into(),
            status: Some(3)
        }]
    );
    // One of 0 closes of its own, and tells it.
    heard.borrow_mut().clear();
    "true".clone_into(&mut asked.borrow_mut());
    command(&mut fake, &mut app, "popper.open");
    let clean = app.focused(window).unwrap();
    run_until(&mut fake, &mut app, |_, _| !heard.borrow().is_empty());
    assert_eq!(
        *heard.borrow(),
        [lokyr_kernel::plugins::Event::PopupClosed {
            pane: clean.into(),
            status: Some(0)
        }]
    );
    assert_eq!(app.focused(window), Some(first));
}

#[test]
fn command_w_of_hidden_floating_panes_closes_the_split_pane_not_the_tab() {
    let (mut fake, mut app, window) = started_running(idle());
    let first = app.focused(window).unwrap();
    command(&mut fake, &mut app, NEW_FLOATING);
    let floating = app.focused(window).unwrap();
    command(&mut fake, &mut app, TOGGLE_FLOATING);
    assert_eq!(app.focused(window), Some(first));
    fake.send(&mut app, window, WindowEvent::Frame);
    // ⌘W of the tab's one split pane: it closes alone, the floating pane
    // taking its place, the window and its tab staying.
    fake.send(&mut app, window, command_key(KeyCode::KeyW, "w"));
    run_until(&mut fake, &mut app, |_, app| {
        app.workspace.pane(first).is_none()
    });
    assert_eq!(fake.windows().count(), 1);
    assert!(app.workspace.pane(floating).is_some());
    assert_eq!(app.focused(window), Some(floating));
}
