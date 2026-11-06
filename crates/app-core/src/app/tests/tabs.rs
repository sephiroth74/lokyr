//! The tabs' bar of the build's `tabs` plugin, in a window: the pointer's
//! drags of its tabs, in each look and at each place.

use lokyr_kernel::plugins::ui::Slot;
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
