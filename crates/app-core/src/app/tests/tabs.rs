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
    let config = Config::parse(
        &format!("[plugins.tabs]\nstyle = \"{style}\"\nposition = \"{position}\"\n"),
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
    for _ in 1..count {
        app.run(&mut fake, "tabs.new", Args::new());
    }
    run_until(&mut fake, &mut app, |_, app| {
        shown_order(app, window).len() == count
            && bar(app, window).is_some_and(|view| view.frame.is_some())
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
