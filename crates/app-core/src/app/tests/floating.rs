//! A tab's floating panes from the menus and the mouse: a new one, shown
//! and hidden; moved by its header and resized by its corner; floated from
//! the splits and moved back into them from the context menu.

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
}
