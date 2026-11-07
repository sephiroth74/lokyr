//! The tabs' bar of the build's `tabs` plugin, in a window: the pointer's
//! drags of its tabs, in each look and at each place.

use lokyr_kernel::plugins::ui::Slot;
use lokyr_platform_core::ScreenPoint;
use lokyr_text_core::FontConfig;

use super::*;
use crate::fakes;

/// The app of a window of three tabs, the last one active, its bar of the
/// `style` at `position`, drawn.
fn three_tabs(style: &str, position: &str) -> (Fake, App, WindowId) {
    tabs_of(style, position, 3)
}

/// The app of a window of `count` tabs, the last one active, its bar of
/// the `style` at `position`, drawn.
fn tabs_of(style: &str, position: &str, count: usize) -> (Fake, App, WindowId) {
    tabs_running(style, position, count, idle(), "")
}

/// The app of a window of `count` tabs running `program`, the last one
/// active, its bar of the `style` at `position`, drawn, of the config's
/// `more` sections.
fn tabs_running(
    style: &str,
    position: &str,
    count: usize,
    program: lokyr_pty::Program,
    more: &str,
) -> (Fake, App, WindowId) {
    let config = Config::parse(
        &format!("[plugins.tabs]\nstyle = \"{style}\"\nposition = \"{position}\"\n{more}"),
        std::path::Path::new("config.toml"),
    )
    .unwrap();
    let drawing = Drawing::new(FontConfig::default()).unwrap();
    let mut fake = Fake::default();
    let mut app = App::new(Some(drawing), config, None).with_program(program);
    fake.start(&mut app);
    let window = fake.windows().next().unwrap();
    app.windows.get_mut(&window).unwrap().surface = Some(fakes::Surface::default());
    fake.send(&mut app, window, WindowEvent::Focused(true));
    for _ in 1..count {
        app.run(&mut fake, "tabs.new", Args::new());
    }
    run_until(&mut fake, &mut app, |_, app| {
        let drawn = bar(app, window).is_some_and(|view| view.frame.is_some());
        // A single tab's title, which holds no tab.
        let title =
            bar(app, window).is_some_and(|view| view.surface.tree.by_key(&TITLE.into()).is_some());
        if count == 1 {
            title
        } else {
            shown_order(app, window).len() == count && drawn
        }
    });
    (fake, app, window)
}

/// The view of `window`'s tab bar.
fn bar(app: &App, window: WindowId) -> Option<&crate::app::chrome::SlotView> {
    app.windows[&window]
        .chrome
        .iter()
        .find(|view| view.view.slot() == Some(Slot::TabBar))
}

/// The key of the bar's node of the tab `id`.
fn tab_key(id: u64) -> String {
    format!("tab-{id}")
}

/// The ids of the tabs of `window`'s bar, in its order.
fn shown_order(app: &App, window: WindowId) -> Vec<u64> {
    let (Some(view), Some(&first)) = (bar(app, window), order(app, window).first()) else {
        return Vec::new();
    };
    let tree = &view.surface.tree;
    let tab = tree.by_key(&tab_key(first).as_str().into());
    let Some(parent) = tab.and_then(|tab| tree.parent(tab)) else {
        return Vec::new();
    };
    tree.children(parent)
        .iter()
        .filter_map(|&child| tree.key(child)?.0.strip_prefix("tab-")?.parse().ok())
        .collect()
}

/// The ids of `window`'s tabs, in the workspace's order.
fn order(app: &App, window: WindowId) -> Vec<u64> {
    let shown = app.workspace.window(app.windows[&window].window).unwrap();
    shown.tabs().iter().map(|tab| tab.get()).collect()
}

/// The id of `window`'s active tab.
fn active(app: &App, window: WindowId) -> u64 {
    let shown = app.workspace.window(app.windows[&window].window).unwrap();
    shown.active_tab().get()
}

/// The middle of the bar's node of `key`, in points from the window's
/// corner.
fn middle_of(fake: &Fake, app: &App, window: WindowId, key: &str) -> Point {
    let scale = fake.window(window).unwrap().size.scale;
    let view = bar(app, window).expect("the bar");
    let frame = view.frame.expect("the bar's frame");
    let node = view.surface.tree.by_key(&key.into()).expect("the node");
    let at = view.surface.layout.frame(&view.surface.tree, node).unwrap();
    Point {
        x: f64::from(frame.x + at.x + at.width / 2.0) / scale,
        y: f64::from(frame.y + at.y + at.height / 2.0) / scale,
    }
}

#[test]
fn a_tab_dragged_past_the_next_one_takes_its_place_in_each_look_and_at_each_place() {
    let none = Modifiers::NONE;
    for style in ["standard", "unified", "seamless"] {
        for position in ["top", "bottom", "left", "right"] {
            let (mut fake, mut app, window) = three_tabs(style, position);
            let ids = order(&app, window);
            let place = format!("{style} at the {position}");
            let (first, second) = (
                middle_of(&fake, &app, window, &tab_key(ids[0])),
                middle_of(&fake, &app, window, &tab_key(ids[1])),
            );
            // Past the second's middle, along the bar: across a row, down a
            // column.
            let past = if matches!(position, "left" | "right") {
                Point {
                    y: second.y + 2.0,
                    ..first
                }
            } else {
                Point {
                    x: second.x + 2.0,
                    ..first
                }
            };
            fake.send(&mut app, window, click(true, first, none, 1));
            fake.send(&mut app, window, moved(past, none));
            fake.send(&mut app, window, click(false, past, none, 1));
            let wanted = vec![ids[1], ids[0], ids[2]];
            run_until(&mut fake, &mut app, |_, app| {
                order(app, window) == wanted && shown_order(app, window) == wanted
            });
            // The drag clicked nothing: the last tab is still the active one.
            assert_eq!(active(&app, window), ids[2], "{place}");
        }
    }
}

#[test]
fn escape_puts_a_tab_dragged_back_and_its_release_selects_nothing() {
    let none = Modifiers::NONE;
    let (mut fake, mut app, window) = three_tabs("unified", "top");
    let ids = order(&app, window);
    let first = middle_of(&fake, &app, window, &tab_key(ids[0]));
    let third = middle_of(&fake, &app, window, &tab_key(ids[2]));
    let past = Point {
        x: third.x + 2.0,
        ..first
    };
    fake.send(&mut app, window, click(true, first, none, 1));
    fake.send(&mut app, window, moved(past, none));
    fake.send(
        &mut app,
        window,
        key_event(KeyCode::Escape, "\u{1b}", none, true),
    );
    fake.send(
        &mut app,
        window,
        key_event(KeyCode::Escape, "\u{1b}", none, false),
    );
    fake.send(&mut app, window, click(false, past, none, 1));
    for _ in 0..5 {
        fake.deliver_wake(&mut app);
        fake.send(&mut app, window, WindowEvent::Frame);
    }
    assert_eq!(order(&app, window), ids);
    assert_eq!(shown_order(&app, window), ids);
    assert_eq!(active(&app, window), ids[2]);
    // The pane's program had no Escape.
    assert!(app.windows[&window].pane_has_focus());
}

#[test]
fn a_click_selects_the_tab_under_it_of_a_bar_scrolled_across() {
    let none = Modifiers::NONE;
    let (mut fake, mut app, window) = tabs_of("unified", "top", 14);
    let ids = order(&app, window);
    // The last tab, active, revealed at the bar's end, past the room of
    // the others.
    let view = bar(&app, window).expect("the bar");
    let track = view
        .surface
        .tree
        .by_key(&"track".into())
        .expect("the track");
    assert!(view.surface.layout.scrolled(track) > 0.0);
    let shown = ids[ids.len() - 3];
    let at = middle_of(&fake, &app, window, &tab_key(shown));
    fake.send(&mut app, window, click(true, at, none, 1));
    fake.send(&mut app, window, click(false, at, none, 1));
    run_until(&mut fake, &mut app, |_, app| active(app, window) == shown);
}

/// `at`, of `window`'s content, on the fake's screen.
fn on_screen(fake: &Fake, window: WindowId, at: Point) -> ScreenPoint {
    let frame = fake.window_frame(window).expect("the window's frame");
    ScreenPoint {
        x: frame.x + at.x,
        y: frame.y + at.y,
    }
}

/// Lets the app's turns run, its frames and its plugins' renders, a few
/// times.
fn settle(fake: &mut Fake, app: &mut App) {
    for _ in 0..5 {
        fake.deliver_wake(app);
        let asked: Vec<WindowId> = fake.windows().collect();
        for window in asked {
            fake.send(app, window, WindowEvent::Frame);
        }
    }
}

#[test]
fn a_tab_dragged_out_of_its_bar_shows_its_image_and_the_next_tab_until_it_comes_back() {
    let none = Modifiers::NONE;
    for (style, position) in [
        ("unified", "top"),
        ("standard", "bottom"),
        ("seamless", "left"),
    ] {
        let (mut fake, mut app, window) = three_tabs(style, position);
        let ids = order(&app, window);
        let place = format!("{style} at the {position}");
        let last = middle_of(&fake, &app, window, &tab_key(ids[2]));
        // Into the panes, across the bar, well past its margin.
        let out = if position == "left" {
            Point {
                x: last.x + 300.0,
                ..last
            }
        } else {
            Point {
                y: if position == "top" { 300.0 } else { 100.0 },
                ..last
            }
        };
        fake.send(&mut app, window, click(true, last, none, 1));
        fake.send(&mut app, window, moved(out, none));
        // Its image under the pointer, its window showing the tab before
        // it, the last, its node nowhere in its bar.
        let image = fake
            .drag_image()
            .unwrap_or_else(|| panic!("an image: {place}"));
        let pointer = on_screen(&fake, window, out);
        assert!(image.contains(pointer), "{place}: {image:?} {pointer:?}");
        let view = bar(&app, window).unwrap();
        assert!(view.surface.carried_out(), "{place}");
        run_until(&mut fake, &mut app, |_, app| active(app, window) == ids[1]);
        // The pointer moves it.
        let further = Point {
            x: out.x + 10.0,
            y: out.y + 20.0,
        };
        fake.send(&mut app, window, moved(further, none));
        let moved_image = fake.drag_image().unwrap();
        assert_eq!(
            (moved_image.x - image.x, moved_image.y - image.y),
            (10.0, 20.0),
            "{place}"
        );
        // Back in its bar, at its place: no image, the tab shown again.
        fake.send(&mut app, window, moved(last, none));
        assert_eq!(fake.drag_image(), None, "{place}");
        assert_eq!(fake.drag_image_hides(), [None], "{place}");
        assert!(!bar(&app, window).unwrap().surface.carried_out());
        run_until(&mut fake, &mut app, |_, app| active(app, window) == ids[2]);
        fake.send(&mut app, window, click(false, last, none, 1));
        settle(&mut fake, &mut app);
        assert_eq!(order(&app, window), ids, "{place}");
        assert_eq!(fake.windows().count(), 1, "{place}");
    }
}

#[test]
fn escape_slides_a_tab_dragged_out_of_its_bar_back_to_its_place() {
    let none = Modifiers::NONE;
    let (mut fake, mut app, window) = three_tabs("unified", "top");
    let ids = order(&app, window);
    let last = middle_of(&fake, &app, window, &tab_key(ids[2]));
    let out = Point { y: 300.0, ..last };
    fake.send(&mut app, window, click(true, last, none, 1));
    fake.send(&mut app, window, moved(out, none));
    assert!(fake.drag_image().is_some());
    fake.send(
        &mut app,
        window,
        key_event(KeyCode::Escape, "\u{1b}", none, true),
    );
    // The image slides back to the tab's place in its bar, on the screen.
    let [Some(back)] = fake.drag_image_hides()[..] else {
        panic!("a slide back: {:?}", fake.drag_image_hides());
    };
    assert!(back.contains(on_screen(&fake, window, last)), "{back:?}");
    fake.send(
        &mut app,
        window,
        key_event(KeyCode::Escape, "\u{1b}", none, false),
    );
    fake.send(&mut app, window, click(false, out, none, 1));
    run_until(&mut fake, &mut app, |_, app| active(app, window) == ids[2]);
    settle(&mut fake, &mut app);
    assert_eq!(order(&app, window), ids);
    assert_eq!(fake.windows().count(), 1);
    assert!(!bar(&app, window).unwrap().surface.carried_out());
}

#[test]
fn a_tab_dropped_out_of_every_window_opens_one_of_its_own_under_the_pointer() {
    let none = Modifiers::NONE;
    let (mut fake, mut app, window) = three_tabs("unified", "top");
    let ids = order(&app, window);
    let size = fake.window(window).unwrap().size;
    let second = middle_of(&fake, &app, window, &tab_key(ids[1]));
    // Far below and to the right, where no window is.
    let away = Point {
        x: second.x + 1500.0,
        y: 1200.0,
    };
    fake.send(&mut app, window, click(true, second, none, 1));
    fake.send(&mut app, window, moved(away, none));
    let dropped = on_screen(&fake, window, away);
    assert_eq!(fake.window_at(dropped), None);
    fake.send(&mut app, window, click(false, away, none, 1));
    assert_eq!(fake.drag_image(), None);
    run_until(&mut fake, &mut app, |fake, _| fake.windows().count() == 2);
    let new = fake.windows().find(|&open| open != window).unwrap();
    run_until(&mut fake, &mut app, |_, app| app.windows.contains_key(&new));
    assert_eq!(order(&app, new), [ids[1]]);
    assert_eq!(order(&app, window), [ids[0], ids[2]]);
    // Of its window's size, the pointer over its first tab's place, where
    // the press grabbed the tab, its middle.
    let frame = fake.window_frame(new).unwrap();
    assert_eq!(frame.width, size.points.width);
    assert_eq!(frame.height, size.points.height);
    let first = middle_of(&fake, &app, window, &tab_key(ids[0]));
    assert!(
        (frame.x + first.x - dropped.x).abs() < 1.0 && (frame.y + first.y - dropped.y).abs() < 1.0,
        "{frame:?} {first:?} {dropped:?}"
    );
}

#[test]
fn a_tab_dragged_among_the_others_goes_over_their_progresses_which_the_frame_draws() {
    let none = Modifiers::NONE;
    // Each tab's program tells a progress of 40 percent, then, after a
    // line, one of no known end.
    let telling = shell(
        "printf '\\033]9;4;1;40\\a'; echo ready; read line; printf '\\033]9;4;3\\a'; read line",
    );
    let (mut fake, mut app, window) = tabs_running("unified", "top", 3, telling, "");
    fake.set_draws_fills(true);
    fake.send(&mut app, window, WindowEvent::Focused(true));
    let ids = order(&app, window);
    let platform = |fake: &Fake| {
        let shown = fake.window(window).unwrap();
        (shown.fills.len(), shown.sweeps.len())
    };
    let covered = |app: &App| bar(app, window).map_or(0, |view| view.surface.covered.len());
    let drag = |fake: &mut Fake, app: &mut App| {
        let first = middle_of(fake, app, window, &tab_key(ids[0]));
        let along = Point {
            x: first.x + 40.0,
            ..first
        };
        fake.send(app, window, click(true, first, none, 1));
        fake.send(app, window, moved(along, none));
        settle(fake, app);
        along
    };
    let drop = |fake: &mut Fake, app: &mut App, at: Point| {
        fake.send(
            app,
            window,
            key_event(KeyCode::Escape, "\u{1b}", none, true),
        );
        fake.send(app, window, click(false, at, none, 1));
        settle(fake, app);
    };
    // The outlines' fills, the window server's; then, as the first goes
    // over the others, theirs the frame's, its own still the window
    // server's.
    run_until(&mut fake, &mut app, |fake, _| platform(fake) == (3, 0));
    let at = drag(&mut fake, &mut app);
    assert_eq!((platform(&fake), covered(&app)), ((1, 0), 2));
    drop(&mut fake, &mut app, at);
    assert_eq!((platform(&fake), covered(&app)), ((3, 0), 0));
    // The outlines' sweeps likewise.
    let shown = app.workspace.window(app.windows[&window].window).unwrap();
    for tab in shown.tabs().to_vec() {
        let pane = app.workspace.tab(tab).unwrap().active_pane();
        app.workspace.pane(pane).unwrap().text("\n");
    }
    run_until(&mut fake, &mut app, |fake, _| platform(fake) == (0, 3));
    let at = drag(&mut fake, &mut app);
    assert_eq!((platform(&fake), covered(&app)), ((0, 1), 2));
    drop(&mut fake, &mut app, at);
    assert_eq!((platform(&fake), covered(&app)), ((0, 3), 0));
}

#[test]
fn another_button_during_a_tab_s_drag_leaves_the_drag_under_way() {
    let none = Modifiers::NONE;
    let (mut fake, mut app, window) = three_tabs("unified", "top");
    let ids = order(&app, window);
    fake.send(&mut app, window, WindowEvent::Focused(true));
    let first = middle_of(&fake, &app, window, &tab_key(ids[0]));
    let out = Point { y: 300.0, ..first };
    fake.send(&mut app, window, click(true, first, none, 1));
    fake.send(&mut app, window, moved(out, none));
    assert!(app.tab_dragged_of(window));
    // A tap of two fingers, the secondary button's press and release.
    fake.send(&mut app, window, press(MouseButton::Right, true, out, none));
    fake.send(
        &mut app,
        window,
        press(MouseButton::Right, false, out, none),
    );
    assert!(app.tab_dragged_of(window));
    assert_eq!(order(&app, window), ids);
    // Its own button's release ends it, back over its bar, the tab there.
    fake.send(&mut app, window, moved(first, none));
    fake.send(&mut app, window, click(false, first, none, 1));
    assert!(!app.tab_dragged_of(window));
    settle(&mut fake, &mut app);
    assert_eq!(order(&app, window), ids);
    assert_eq!(shown_order(&app, window), ids);
}

#[test]
fn a_tab_s_context_menu_leaves_the_window_s_next_clicks_where_they_fall() {
    let none = Modifiers::NONE;
    let (mut fake, mut app, window) = three_tabs("unified", "top");
    let ids = order(&app, window);
    fake.send(&mut app, window, WindowEvent::Focused(true));
    let first = middle_of(&fake, &app, window, &tab_key(ids[0]));
    // The secondary button's press opens the tab's menu, the platform's,
    // whose own loop takes the release.
    fake.send(
        &mut app,
        window,
        press(MouseButton::Right, true, first, none),
    );
    run_until(&mut fake, &mut app, |fake, _| {
        !fake.context_menus().is_empty()
    });
    assert_eq!(app.windows[&window].captured, None);
    fake.send(&mut app, window, WindowEvent::ContextMenuClosed);
    settle(&mut fake, &mut app);
    // A click on another tab shows it.
    let third = middle_of(&fake, &app, window, &tab_key(ids[2]));
    fake.send(&mut app, window, click(true, third, none, 1));
    fake.send(&mut app, window, click(false, third, none, 1));
    settle(&mut fake, &mut app);
    assert_eq!(active(&app, window), ids[2]);
}

#[test]
fn the_room_that_a_tab_opened_in_another_bar_closes_as_its_window_closes() {
    let none = Modifiers::NONE;
    let (mut fake, mut app, window) = three_tabs("unified", "top");
    let ids = order(&app, window);
    app.run(&mut fake, "window.new", Args::new());
    run_until(&mut fake, &mut app, |fake, _| fake.windows().count() == 2);
    let other = fake.windows().find(|&open| open != window).unwrap();
    fake.set_window_origin(
        other,
        ScreenPoint {
            x: 1000.0,
            y: 400.0,
        },
    );
    fake.raise(other);
    fake.send(&mut app, other, WindowEvent::Focused(true));
    app.run(&mut fake, "tabs.new", Args::new());
    run_until(&mut fake, &mut app, |_, app| {
        app.windows.contains_key(&other)
            && shown_order(app, other).len() == 2
            && bar(app, other).is_some_and(|view| view.frame.is_some())
    });
    let theirs = order(&app, other);
    fake.send(&mut app, other, WindowEvent::Focused(false));
    fake.send(&mut app, window, WindowEvent::Focused(true));
    let first = middle_of(&fake, &app, window, &tab_key(ids[0]));
    let their_first = middle_of(&fake, &app, other, &tab_key(theirs[0]));
    let target = on_screen(
        &fake,
        other,
        Point {
            x: their_first.x - 20.0,
            ..their_first
        },
    );
    let corner = fake.window_frame(window).unwrap();
    let over = Point {
        x: target.x - corner.x,
        y: target.y - corner.y,
    };
    fake.send(&mut app, window, click(true, first, none, 1));
    fake.send(&mut app, window, moved(Point { y: 300.0, ..first }, none));
    fake.send(&mut app, window, moved(over, none));
    assert_eq!(bar(&app, other).unwrap().surface.room(), Some(0));
    // The window that the tab comes from closes under the drag.
    app.close(&mut fake, window);
    assert!(!app.tab_dragged_of(window));
    assert_eq!(bar(&app, other).unwrap().surface.room(), None);
    assert_eq!(order(&app, other), theirs);
}

#[test]
fn a_tab_dropped_on_another_window_s_bar_goes_there_at_the_room_that_it_opened() {
    let none = Modifiers::NONE;
    let (mut fake, mut app, window) = three_tabs("unified", "top");
    let ids = order(&app, window);
    // Another window, of two tabs, below and to the right, in front.
    app.run(&mut fake, "window.new", Args::new());
    run_until(&mut fake, &mut app, |fake, _| fake.windows().count() == 2);
    let other = fake.windows().find(|&open| open != window).unwrap();
    fake.set_window_origin(
        other,
        ScreenPoint {
            x: 1000.0,
            y: 400.0,
        },
    );
    fake.raise(other);
    fake.send(&mut app, other, WindowEvent::Focused(true));
    app.run(&mut fake, "tabs.new", Args::new());
    run_until(&mut fake, &mut app, |_, app| {
        app.windows.contains_key(&other)
            && shown_order(app, other).len() == 2
            && bar(app, other).is_some_and(|view| view.frame.is_some())
    });
    let theirs = order(&app, other);
    fake.send(&mut app, other, WindowEvent::Focused(false));
    fake.send(&mut app, window, WindowEvent::Focused(true));
    // The first tab of the window, over the other's bar, before the middle
    // of its first tab.
    let first = middle_of(&fake, &app, window, &tab_key(ids[0]));
    let their_first = middle_of(&fake, &app, other, &tab_key(theirs[0]));
    let target = on_screen(
        &fake,
        other,
        Point {
            x: their_first.x - 20.0,
            ..their_first
        },
    );
    let corner = fake.window_frame(window).unwrap();
    let over = Point {
        x: target.x - corner.x,
        y: target.y - corner.y,
    };
    fake.send(&mut app, window, click(true, first, none, 1));
    fake.send(&mut app, window, moved(Point { y: 300.0, ..first }, none));
    fake.send(&mut app, window, moved(over, none));
    assert_eq!(fake.window_at(target), Some(other));
    // Its tabs slide aside from the first.
    assert_eq!(bar(&app, other).unwrap().surface.room(), Some(0));
    fake.send(&mut app, window, click(false, over, none, 1));
    let wanted = vec![ids[0], theirs[0], theirs[1]];
    run_until(&mut fake, &mut app, |_, app| {
        order(app, other) == wanted && shown_order(app, other) == wanted
    });
    assert_eq!(order(&app, window), [ids[1], ids[2]]);
    assert_eq!(active(&app, other), ids[0]);
    assert_eq!(fake.focus_requests().last(), Some(&other));
}

#[test]
fn a_window_of_a_tab_alone_dropped_out_of_every_window_moves_there() {
    let none = Modifiers::NONE;
    let config = Config::parse(
        "[plugins.tabs]\nhide-when-single = false\n",
        std::path::Path::new("config.toml"),
    )
    .unwrap();
    let drawing = Drawing::new(FontConfig::default()).unwrap();
    let mut fake = Fake::default();
    let mut app = App::new(Some(drawing), config, None).with_program(idle());
    fake.start(&mut app);
    let window = fake.windows().next().unwrap();
    app.windows.get_mut(&window).unwrap().surface = Some(fakes::Surface::default());
    fake.send(&mut app, window, WindowEvent::Focused(true));
    run_until(&mut fake, &mut app, |_, app| {
        shown_order(app, window).len() == 1
            && bar(app, window).is_some_and(|view| view.frame.is_some())
    });
    let ids = order(&app, window);
    let tab = middle_of(&fake, &app, window, &tab_key(ids[0]));
    let away = Point {
        x: tab.x + 1500.0,
        y: 1200.0,
    };
    let dropped = on_screen(&fake, window, away);
    fake.send(&mut app, window, click(true, tab, none, 1));
    fake.send(&mut app, window, moved(away, none));
    fake.send(&mut app, window, click(false, away, none, 1));
    settle(&mut fake, &mut app);
    // The same window, its tab under the pointer.
    assert_eq!(fake.windows().collect::<Vec<_>>(), [window]);
    assert_eq!(order(&app, window), ids);
    let frame = fake.window_frame(window).unwrap();
    assert!(
        (frame.x + tab.x - dropped.x).abs() < 1.0 && (frame.y + tab.y - dropped.y).abs() < 1.0,
        "{frame:?} {tab:?} {dropped:?}"
    );
}

#[test]
fn the_protected_input_leaves_the_title_for_its_tab_where_a_tab_bar_shows() {
    let (mut fake, mut app, window) = three_tabs("unified", "top");
    fake.send_app(
        &mut app,
        AppEvent::Command("app.secure-keyboard-entry".to_owned()),
    );
    settle(&mut fake, &mut app);
    assert!(fake.secure_input().0);
    let title = fake.window(window).unwrap().title.clone();
    assert!(!title.starts_with(SECURE_MARK), "{title}");
    let shown = app.workspace.window(app.windows[&window].window).unwrap();
    let marked: Vec<bool> = shown
        .tabs()
        .iter()
        .map(|&tab| app.workspace.tab(tab).unwrap().marks().secure)
        .collect();
    assert_eq!(marked, [false, false, true]);
    // The lock goes with the tab that shows.
    app.run(&mut fake, "tabs.goto", {
        let mut args = Args::new();
        args.insert(
            "index".to_owned(),
            lokyr_kernel::commands::Value::Integer(1),
        );
        args
    });
    settle(&mut fake, &mut app);
    let shown = app.workspace.window(app.windows[&window].window).unwrap();
    let marked: Vec<bool> = shown
        .tabs()
        .iter()
        .map(|&tab| app.workspace.tab(tab).unwrap().marks().secure)
        .collect();
    assert_eq!(marked, [true, false, false]);
    assert!(fake.secure_input().0);
}

/// The labels of the nodes of `window`'s bar under the tab `id`.
fn labels_of(app: &App, window: WindowId, id: u64) -> Vec<String> {
    let Some(view) = bar(app, window) else {
        return Vec::new();
    };
    let tree = &view.surface.tree;
    let Some(tab) = tree.by_key(&tab_key(id).as_str().into()) else {
        return Vec::new();
    };
    let mut labels = Vec::new();
    let mut left = vec![tab];
    while let Some(node) = left.pop() {
        labels.extend(tree.label(node).map(str::to_owned));
        left.extend(tree.children(node).iter().rev().copied());
    }
    labels
}

/// The key of a single tab's title in the titlebar, as the tabs plugin
/// draws it.
const TITLE: &str = "title";

/// The labels of every node of `window`'s bar, or of its single tab's
/// title.
fn bar_labels(app: &App, window: WindowId) -> Vec<String> {
    let Some(view) = bar(app, window) else {
        return Vec::new();
    };
    let tree = &view.surface.tree;
    let mut labels = Vec::new();
    let mut left: Vec<_> = tree.root().into_iter().collect();
    while let Some(node) = left.pop() {
        labels.extend(tree.label(node).map(str::to_owned));
        left.extend(tree.children(node).iter().rev().copied());
    }
    labels
}

#[test]
fn a_single_tab_s_title_shows_its_window_s_bell_until_the_window_takes_the_focus() {
    let ringing = shell("echo ready; read line; printf '\\a'; read line");
    let (mut fake, mut app, window) = tabs_running("unified", "top", 1, ringing, "");
    let rang = |app: &App| bar_labels(app, window).contains(&"Bell".to_owned());
    assert!(!rang(&app));
    fake.send(&mut app, window, WindowEvent::Focused(false));
    let shown = app.workspace.window(app.windows[&window].window).unwrap();
    let pane = app.workspace.tab(shown.active_tab()).unwrap().active_pane();
    // Its program rings after a line.
    app.workspace.pane(pane).unwrap().text("\n");
    run_until(&mut fake, &mut app, |_, app| rang(app));
    fake.send(&mut app, window, WindowEvent::Focused(true));
    run_until(&mut fake, &mut app, |_, app| !rang(app));
}

#[test]
fn a_bar_in_the_titlebar_shows_its_window_s_bell_on_the_shown_tab() {
    let ringing = shell("echo ready; read line; printf '\\a'; read line");
    let (mut fake, mut app, window) = tabs_running("unified", "top", 2, ringing, "");
    let shown = app.workspace.window(app.windows[&window].window).unwrap();
    let active_tab = shown.active_tab();
    let rang = |app: &App| labels_of(app, window, active_tab.get()).contains(&"Bell".to_owned());
    assert!(!rang(&app));
    fake.send(&mut app, window, WindowEvent::Focused(false));
    let pane = app.workspace.tab(active_tab).unwrap().active_pane();
    app.workspace.pane(pane).unwrap().text("\n");
    run_until(&mut fake, &mut app, |_, app| rang(app));
    fake.send(&mut app, window, WindowEvent::Focused(true));
    run_until(&mut fake, &mut app, |_, app| !rang(app));
}

#[test]
fn a_hidden_tab_s_output_marks_it_on_the_bar_until_it_shows() {
    let none = Modifiers::NONE;
    let (mut fake, mut app, window) = three_tabs("unified", "top");
    let ids = order(&app, window);
    let shown = app.workspace.window(app.windows[&window].window).unwrap();
    let first = shown.tabs()[0];
    let pane = app.workspace.tab(first).unwrap().active_pane();
    // Its program echoes a key while the third tab shows.
    app.workspace.pane(pane).unwrap().text("x");
    run_until(&mut fake, &mut app, |_, app| {
        labels_of(app, window, ids[0]).contains(&"New Output".to_owned())
    });
    // Shown, it is seen.
    let at = middle_of(&fake, &app, window, &tab_key(ids[0]));
    fake.send(&mut app, window, click(true, at, none, 1));
    fake.send(&mut app, window, click(false, at, none, 1));
    run_until(&mut fake, &mut app, |_, app| {
        active(app, window) == ids[0]
            && !labels_of(app, window, ids[0]).contains(&"New Output".to_owned())
    });
}

#[test]
fn the_pointer_keeps_its_shape_while_a_tab_is_dragged() {
    use lokyr_platform_core::PointerShape;

    let none = Modifiers::NONE;
    let (mut fake, mut app, window) = three_tabs("unified", "top");
    let ids = order(&app, window);
    let held = |fake: &Fake| fake.window(window).unwrap().held_pointer;
    let first = middle_of(&fake, &app, window, &tab_key(ids[0]));
    fake.send(&mut app, window, click(true, first, none, 1));
    assert_eq!(held(&fake), None, "a press holds nothing");
    // Along the bar, then over the panes, then out of the window: the
    // tab's arrow, held.
    let along = Point {
        x: first.x + 40.0,
        ..first
    };
    fake.send(&mut app, window, moved(along, none));
    assert_eq!(held(&fake), Some(PointerShape::Arrow));
    for at in [Point { y: 300.0, ..first }, Point { x: -50.0, y: -50.0 }] {
        fake.send(&mut app, window, moved(at, none));
        assert_eq!(held(&fake), Some(PointerShape::Arrow));
    }
    // Back and dropped: let go.
    fake.send(&mut app, window, moved(first, none));
    fake.send(&mut app, window, click(false, first, none, 1));
    assert_eq!(held(&fake), None);
    // Escape lets it go too.
    fake.send(&mut app, window, click(true, first, none, 1));
    fake.send(&mut app, window, moved(along, none));
    assert_eq!(held(&fake), Some(PointerShape::Arrow));
    fake.send(
        &mut app,
        window,
        key_event(KeyCode::Escape, "\u{1b}", none, true),
    );
    assert_eq!(held(&fake), None);
    fake.send(&mut app, window, click(false, along, none, 1));
}

#[test]
fn the_keyboard_stays_where_it_was_while_a_tab_is_dragged_over_another_pane() {
    use lokyr_kernel::workspace::Layout;
    let none = Modifiers::NONE;
    let follows = "[plugins.splits]\nfocus-follows-mouse = true\n";
    let (mut fake, mut app, window) = tabs_running("unified", "top", 2, idle(), follows);
    let ids = order(&app, window);
    fake.send(&mut app, window, WindowEvent::Focused(true));
    let goto = |index: i64| {
        let mut args = Args::new();
        args.insert(
            "index".to_owned(),
            lokyr_kernel::commands::Value::Integer(index),
        );
        args
    };
    // The first tab split, side by side, its keyboard in its new pane.
    app.run(&mut fake, "tabs.goto", goto(1));
    let mut right = Args::new();
    right.insert(
        "direction".to_owned(),
        lokyr_kernel::commands::Value::String("right".to_owned()),
    );
    app.run(&mut fake, "splits.split", right);
    run_until(&mut fake, &mut app, |_, app| {
        matches!(shown_layout(app, window), Layout::Split { .. })
    });
    app.run(&mut fake, "tabs.goto", goto(2));
    settle(&mut fake, &mut app);
    // The second tab out of its bar, the first showing, then over the
    // pane of the first without its keyboard.
    let second = middle_of(&fake, &app, window, &tab_key(ids[1]));
    fake.send(&mut app, window, click(true, second, none, 1));
    fake.send(&mut app, window, moved(Point { y: 300.0, ..second }, none));
    settle(&mut fake, &mut app);
    assert_eq!(active(&app, window), ids[0]);
    let focused = app.focused(window).unwrap();
    let other = app.windows[&window].views[0].pane;
    assert_ne!(other, focused);
    let (over, _) = in_pane(&fake, &app, window, (0.5, 0.5));
    fake.send(&mut app, window, moved(over, none));
    assert_eq!(app.focused(window), Some(focused));
    fake.send(
        &mut app,
        window,
        key_event(KeyCode::Escape, "\u{1b}", none, true),
    );
    fake.send(&mut app, window, click(false, over, none, 1));
}

/// The layout of `window`'s active tab.
fn shown_layout(app: &App, window: WindowId) -> lokyr_kernel::workspace::Layout {
    let shown = app.workspace.window(app.windows[&window].window).unwrap();
    app.workspace
        .tab(shown.active_tab())
        .unwrap()
        .layout()
        .clone()
}

/// The pane of the tab of `id`, of one pane.
fn pane_of(app: &App, id: u64) -> lokyr_kernel::workspace::PaneId {
    app.workspace
        .tab(lokyr_kernel::workspace::TabId::of(id))
        .unwrap()
        .active_pane()
}

/// A point of `window`'s pane `at` its share across and down, in points
/// from the window's corner, and that pane's room in pixels.
fn in_pane(fake: &Fake, app: &App, window: WindowId, at: (f64, f64)) -> (Point, Rect) {
    let scale = fake.window(window).unwrap().size.scale;
    let rect = app.windows[&window].views[0].rect;
    let point = Point {
        x: (f64::from(rect.x) + f64::from(rect.width) * at.0) / scale,
        y: (f64::from(rect.y) + f64::from(rect.height) * at.1) / scale,
    };
    (point, rect)
}

#[test]
fn a_tab_dropped_on_another_window_s_pane_goes_in_beside_it_at_the_half_that_lit_up() {
    use lokyr_kernel::workspace::{Direction, Layout, Ratio};
    let none = Modifiers::NONE;
    let (mut fake, mut app, window) = three_tabs("unified", "top");
    let ids = order(&app, window);
    let moving = pane_of(&app, ids[0]);
    // Another window, of one tab, below and to the right, in front.
    app.run(&mut fake, "window.new", Args::new());
    run_until(&mut fake, &mut app, |fake, _| fake.windows().count() == 2);
    let other = fake.windows().find(|&open| open != window).unwrap();
    fake.set_window_origin(
        other,
        ScreenPoint {
            x: 1000.0,
            y: 400.0,
        },
    );
    fake.raise(other);
    run_until(&mut fake, &mut app, |_, app| {
        app.windows
            .get(&other)
            .is_some_and(|state| !state.views.is_empty())
    });
    let target = app.windows[&other].views[0].pane;
    fake.send(&mut app, window, WindowEvent::Focused(true));
    // The first tab out of its bar, over the other window's pane near each
    // of its edges: that half lights up.
    let first = middle_of(&fake, &app, window, &tab_key(ids[0]));
    fake.send(&mut app, window, click(true, first, none, 1));
    fake.send(&mut app, window, moved(Point { y: 300.0, ..first }, none));
    let over = |fake: &Fake, at: Point| {
        let target = on_screen(fake, other, at);
        let corner = fake.window_frame(window).unwrap();
        Point {
            x: target.x - corner.x,
            y: target.y - corner.y,
        }
    };
    let (_, rect) = in_pane(&fake, &app, other, (0.5, 0.5));
    let (wide, high) = (rect.width / 2, rect.height / 2);
    let halves = [
        (
            (0.1, 0.5),
            Rect {
                width: wide,
                ..rect
            },
        ),
        (
            (0.9, 0.5),
            Rect {
                x: rect.x + (rect.width - wide) as i32,
                width: wide,
                ..rect
            },
        ),
        (
            (0.5, 0.1),
            Rect {
                height: high,
                ..rect
            },
        ),
        (
            (0.5, 0.9),
            Rect {
                y: rect.y + (rect.height - high) as i32,
                height: high,
                ..rect
            },
        ),
    ];
    for (at, half) in halves {
        let (point, _) = in_pane(&fake, &app, other, at);
        fake.send(&mut app, window, moved(over(&fake, point), none));
        assert_eq!(app.lit_half(other), Some(half), "{at:?}");
        assert_eq!(app.lit_half(window), None);
    }
    // Dropped near its right edge: in there, half and half, the window
    // that took it in front, its own of the other two.
    let (point, _) = in_pane(&fake, &app, other, (0.9, 0.5));
    let at = over(&fake, point);
    fake.send(&mut app, window, moved(at, none));
    fake.send(&mut app, window, click(false, at, none, 1));
    let wanted = Layout::Split {
        direction: Direction::Horizontal,
        ratio: Some(Ratio::HALF),
        first: Box::new(Layout::Leaf(target)),
        second: Box::new(Layout::Leaf(moving)),
    };
    run_until(&mut fake, &mut app, |_, app| {
        shown_layout(app, other) == wanted && order(app, window).len() == 2
    });
    assert_eq!(order(&app, window), [ids[1], ids[2]]);
    assert_eq!(app.lit_half(other), None);
    assert_eq!(fake.focus_requests().last(), Some(&other));
}

#[test]
fn a_tab_of_splits_dropped_on_its_own_window_s_pane_goes_in_whole() {
    use lokyr_kernel::workspace::{Direction, Layout, Ratio};
    let none = Modifiers::NONE;
    let (mut fake, mut app, window) = three_tabs("unified", "top");
    let ids = order(&app, window);
    // The first tab split, side by side.
    fake.send(&mut app, window, WindowEvent::Focused(true));
    app.run(&mut fake, "tabs.goto", {
        let mut args = Args::new();
        args.insert(
            "index".to_owned(),
            lokyr_kernel::commands::Value::Integer(1),
        );
        args
    });
    let mut right = Args::new();
    right.insert(
        "direction".to_owned(),
        lokyr_kernel::commands::Value::String("right".to_owned()),
    );
    app.run(&mut fake, "splits.split", right);
    run_until(&mut fake, &mut app, |_, app| {
        matches!(shown_layout(app, window), Layout::Split { .. })
    });
    let split = shown_layout(&app, window);
    let theirs = pane_of(&app, ids[1]);
    // Out of its bar, the window shows the next tab, whose pane lights up
    // near its top; the tab's own panes, which no longer show, never do.
    let first = middle_of(&fake, &app, window, &tab_key(ids[0]));
    fake.send(&mut app, window, click(true, first, none, 1));
    fake.send(&mut app, window, moved(Point { y: 300.0, ..first }, none));
    settle(&mut fake, &mut app);
    assert_eq!(active(&app, window), ids[1]);
    let (point, rect) = in_pane(&fake, &app, window, (0.5, 0.1));
    fake.send(&mut app, window, moved(point, none));
    assert_eq!(
        app.lit_half(window),
        Some(Rect {
            height: rect.height / 2,
            ..rect
        })
    );
    fake.send(&mut app, window, click(false, point, none, 1));
    let wanted = Layout::Split {
        direction: Direction::Vertical,
        ratio: Some(Ratio::HALF),
        first: Box::new(split),
        second: Box::new(Layout::Leaf(theirs)),
    };
    run_until(&mut fake, &mut app, |_, app| {
        shown_layout(app, window) == wanted
    });
    assert_eq!(order(&app, window), [ids[1], ids[2]]);
    assert_eq!(app.lit_half(window), None);
}

#[test]
fn the_window_s_keys_show_at_its_titlebar_s_right_of_the_frame_s_own_titlebar() {
    use lokyr_platform_core::Titlebar;
    let (mut fake, mut app, window) = tabs_of("unified", "top", 1);
    // Each frame's titlebar and keys: the keys of a titlebar of its title,
    // none of a bar of tabs there, which shows its own.
    let check = |fake: &Fake| {
        let shown = fake.window(window).unwrap();
        let keys = shown.title_accessory.as_deref();
        match shown.titlebar {
            Titlebar::Tabs => assert_eq!(keys, None),
            _ => assert_eq!(keys, Some("⌥⌘1")),
        }
        shown.titlebar
    };
    fake.send(&mut app, window, WindowEvent::Frame);
    assert_eq!(check(&fake), Titlebar::Untitled);
    // A second tab, its bar in the titlebar, of each frame.
    app.run(&mut fake, "tabs.new", Args::new());
    let deadline = Instant::now() + Duration::from_secs(10);
    while check(&fake) != Titlebar::Tabs {
        assert!(Instant::now() < deadline, "no bar in the titlebar");
        fake.deliver_wake(&mut app);
        fake.send(&mut app, window, WindowEvent::Frame);
    }
    // Closed back to one, its title again.
    app.run(&mut fake, "tabs.close", Args::new());
    let deadline = Instant::now() + Duration::from_secs(10);
    while check(&fake) != Titlebar::Untitled {
        assert!(Instant::now() < deadline, "no title");
        fake.deliver_wake(&mut app);
        fake.send(&mut app, window, WindowEvent::Frame);
    }
}
