//! The tests of a window's panes: the keyboard that a click gives, the keys
//! that the pane with it takes, and the ends that close a pane, then the
//! window with its last.

use lokyr_kernel::workspace::{Direction, Environment, Side};

use super::*;

/// The text of every row of `pane`'s screen, a line each.
fn screen_of(app: &App, pane: PaneId) -> String {
    let snapshot = app.workspace.pane(pane).unwrap().snapshot();
    (0..snapshot.rows())
        .map(|row| {
            snapshot
                .line(row)
                .cells()
                .iter()
                .filter(|cell| cell.width() != lokyr_vt_core::page::Width::SpacerTail)
                .map(|cell| match cell.content() {
                    lokyr_vt_core::page::Content::Char(c)
                    | lokyr_vt_core::page::Content::Grapheme(c) => c,
                    _ => ' ',
                })
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// A program that tells it is ready, echoes a line after its `name`, and
/// ends after the next.
fn echoing(name: &str) -> Program {
    shell(&format!(
        "echo ready-{name}; read line; echo \"{name}:$line\"; read line"
    ))
}

/// The middle of `pane`'s room, in points, as `window`'s last frame laid it
/// out.
fn middle_of(fake: &Fake, app: &App, window: WindowId, pane: PaneId) -> Point {
    let scale = fake.window(window).unwrap().size.scale;
    let view = app.windows[&window]
        .views
        .iter()
        .find(|view| view.pane == pane)
        .copied()
        .expect("the pane's room");
    Point {
        x: (f64::from(view.rect.x) + f64::from(view.rect.width) / 2.0) / scale,
        y: (f64::from(view.rect.y) + f64::from(view.rect.height) / 2.0) / scale,
    }
}

/// A click in the middle of `pane`'s room.
fn click_in(fake: &mut Fake, app: &mut App, window: WindowId, pane: PaneId) {
    let at = middle_of(fake, app, window, pane);
    fake.send(app, window, click(true, at, Modifiers::NONE, 1));
    fake.send(app, window, click(false, at, Modifiers::NONE, 1));
}

/// `window` of two panes side by side, the first running `first`, the
/// second `second`, of the keyboard and the focus, once both showed
/// `ready-` and their names, `a` and `b`.
fn two_panes(first: Program, second: &Program) -> (Fake, App, WindowId, PaneId, PaneId) {
    let mut fake = Fake::default();
    let mut app = app(unpadded(), first);
    fake.start(&mut app);
    let window = fake.windows().next().unwrap();
    fake.send(&mut app, window, WindowEvent::Focused(true));
    let a = app.focused(window).unwrap();
    let grid = app.pane(window).grid();
    let pane = app.spawn_pane(
        &fake,
        second,
        &Environment::default(),
        grid,
        &lokyr_vt_core::terminal::Colors::default(),
    );
    let b = app
        .workspace
        .split(a, Direction::Horizontal, pane, Side::Second)
        .unwrap();
    fake.send(&mut app, window, WindowEvent::Frame);
    run_until(&mut fake, &mut app, |_, app| {
        screen_of(app, a).contains("ready-a") && screen_of(app, b).contains("ready-b")
    });
    (fake, app, window, a, b)
}

#[test]
fn each_of_two_panes_takes_the_keyboard_its_keys_and_its_end() {
    let (mut fake, mut app, window, a, b) = two_panes(echoing("a"), &echoing("b"));
    // A click in the second pane gives it the keyboard, whose keys reach
    // its program alone.
    click_in(&mut fake, &mut app, window, b);
    assert_eq!(app.focused(window), Some(b));
    fake.send(&mut app, window, typed(KeyCode::KeyX, "x", false));
    fake.send(&mut app, window, typed(KeyCode::Enter, "\r", false));
    run_until(&mut fake, &mut app, |_, app| {
        screen_of(app, b).contains("b:x")
    });
    assert!(!screen_of(&app, a).contains('x'), "{}", screen_of(&app, a));
    // And back to the first.
    click_in(&mut fake, &mut app, window, a);
    assert_eq!(app.focused(window), Some(a));
    fake.send(&mut app, window, typed(KeyCode::KeyZ, "z", false));
    fake.send(&mut app, window, typed(KeyCode::Enter, "\r", false));
    run_until(&mut fake, &mut app, |_, app| {
        screen_of(app, a).contains("a:z")
    });
    assert!(!screen_of(&app, b).contains('z'));
    // The first program's end closes its pane, not the window, and the
    // keyboard goes to the other.
    fake.send(&mut app, window, typed(KeyCode::Enter, "\r", false));
    run_until(&mut fake, &mut app, |_, app| {
        app.workspace.pane(a).is_none()
    });
    assert_eq!(fake.windows().count(), 1);
    assert_eq!(app.focused(window), Some(b));
    // The last one's closes the window with its tab.
    fake.send(&mut app, window, typed(KeyCode::Enter, "\r", false));
    run_until(&mut fake, &mut app, |fake, _| fake.windows().count() == 0);
    assert!(app.workspace.pane(b).is_none());
}

#[test]
fn the_wheel_scrolls_the_pane_under_it_which_keeps_the_keyboard_where_it_was() {
    let (mut fake, mut app, window, a, b) = two_panes(
        shell("echo ready-a; exec sleep 10"),
        &shell("seq 1 200; echo ready-b; exec sleep 10"),
    );
    let offset = |app: &App, pane| app.workspace.pane(pane).unwrap().snapshot().offset();
    let at = middle_of(&fake, &app, window, b);
    fake.send(
        &mut app,
        window,
        WindowEvent::Scroll {
            delta: ScrollDelta::Lines { x: 0.0, y: 3.0 },
            phase: None,
            momentum: false,
            position: at,
            mods: Modifiers::NONE,
        },
    );
    run_until(&mut fake, &mut app, |_, app| offset(app, b) > 0);
    assert_eq!(offset(&app, a), 0);
    assert_eq!(app.focused(window), Some(a));
}
