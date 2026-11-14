//! The workspace's model, with real programs in its panes, and no window.

use std::time::{Duration, Instant};

use lokyr_pty::Program;
use lokyr_vt_core::terminal::Colors;

use super::*;
use crate::l10n::Texts;
use lokyr_plugin_sdk::event::WorkspaceEvent as Change;

fn grid() -> Grid {
    Grid {
        columns: 80,
        rows: 24,
        cell_width: 8,
        cell_height: 16,
    }
}

/// A pane that runs `script` with `sh`.
fn pane(script: &str) -> Pane {
    let program = Program::command("/bin/sh", ["-c", script]);
    Pane::spawn(
        &program,
        &Environment::default(),
        &Settings::default(),
        grid(),
        &Colors::default(),
        &Texts::new("en", &crate::l10n::KERNEL),
        || {},
    )
}

/// Collects the sessions' events until `done` holds of those collected so
/// far, for ten seconds at most, and returns them.
fn collect_until(workspace: &mut Workspace, done: impl Fn(&[Event]) -> bool) -> Vec<Event> {
    let start = Instant::now();
    let mut events = Vec::new();
    loop {
        let ids = workspace.pane_ids();
        workspace.collect(&ids, Instant::now());
        events.extend(workspace.take_events());
        if done(&events) {
            return events;
        }
        assert!(start.elapsed() < Duration::from_secs(10), "{events:?}");
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn the_second_bell_of_a_pane_that_is_not_drawn_arrives() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    let wakes = Arc::new(AtomicUsize::new(0));
    let woken = Arc::clone(&wakes);
    let program = Program::command(
        "/bin/sh",
        ["-c", "printf '\\a'; read line; printf 'more\\a'; read line"],
    );
    let pane = Pane::spawn(
        &program,
        &Environment::default(),
        &Settings::default(),
        grid(),
        &Colors::default(),
        &Texts::new("en", &crate::l10n::KERNEL),
        move || {
            woken.fetch_add(1, Ordering::SeqCst);
        },
    );
    let id = pane.id();
    let mut workspace = Workspace::new();
    workspace.open_window(pane);
    // The main thread's loop: it collects as it is woken, or as the pane
    // that waits for its turn has it, as the app's timer does, and draws
    // nothing.
    let mut seen = 0;
    let mut bells = 0;
    let mut due = None;
    let start = Instant::now();
    while bells < 2 {
        assert!(start.elapsed() < Duration::from_secs(10), "{bells} bells");
        let now = wakes.load(Ordering::SeqCst);
        let turn = due.is_some_and(|due| Instant::now() >= due);
        if now == seen && !turn {
            std::thread::sleep(Duration::from_millis(10));
            continue;
        }
        seen = now;
        let ids = workspace.pane_ids();
        due = workspace.collect(&ids, Instant::now());
        let rang = workspace
            .take_events()
            .iter()
            .filter(|event| **event == Event::Bell(id))
            .count();
        if rang > 0 && bells == 0 {
            workspace.pane(id).unwrap().text("\n");
        }
        bells += rang;
    }
}

/// The changes of the workspace that wait, oldest first.
fn changes(workspace: &mut Workspace) -> Vec<Change> {
    workspace
        .take_events()
        .into_iter()
        .filter_map(|event| match event {
            Event::Changed(change) => Some(change),
            _ => None,
        })
        .collect()
}

#[test]
fn a_window_opens_with_one_tab_whose_layout_is_its_pane() {
    let mut workspace = Workspace::new();
    let pane = pane("sleep 10");
    let id = pane.id();
    let window = workspace.open_window(pane);
    let tabs = workspace.window(window).unwrap().tabs().to_vec();
    assert_eq!(
        changes(&mut workspace),
        [
            Change::WindowOpened {
                window: window.into()
            },
            Change::TabCreated {
                tab: tabs[0].into(),
                window: window.into(),
                index: 0
            },
            Change::PaneCreated {
                pane: id.into(),
                tab: tabs[0].into()
            },
        ]
    );
    assert_eq!(tabs.len(), 1);
    let tab = workspace.tab(tabs[0]).unwrap();
    assert_eq!(tab.window(), window);
    assert_eq!(tab.layout(), &Layout::Leaf(id));
    assert_eq!(tab.layout().panes(), [id]);
    assert_eq!(workspace.window(window).unwrap().active_tab(), tab.id());
    assert_eq!(workspace.active_pane(window), Some(id));
    assert!(workspace.pane(id).is_some());
}

#[test]
fn tabs_open_show_move_and_close_with_their_windows() {
    let mut workspace = Workspace::new();
    let first = workspace.open_window(pane("sleep 10"));
    let one = workspace.window(first).unwrap().active_tab();
    changes(&mut workspace);
    let two = workspace.open_tab(first, pane("sleep 10"), 99).unwrap();
    let two_pane = workspace.tab(two).unwrap().active_pane();
    let tabs = |workspace: &Workspace, window| workspace.window(window).unwrap().tabs().to_vec();
    // A new tab goes last, and the window shows the one that it showed.
    assert_eq!(
        changes(&mut workspace),
        [
            Change::TabCreated {
                tab: two.into(),
                window: first.into(),
                index: 1
            },
            Change::PaneCreated {
                pane: two_pane.into(),
                tab: two.into()
            },
        ]
    );
    assert_eq!(tabs(&workspace, first), [one, two]);
    assert_eq!(workspace.window(first).unwrap().active_tab(), one);
    assert!(workspace.activate_tab(two));
    assert_eq!(workspace.window(first).unwrap().active_tab(), two);
    assert!(workspace.activate_tab(two));
    assert_eq!(
        changes(&mut workspace),
        [Change::TabActivated {
            tab: two.into(),
            window: first.into()
        }],
        "the second time, nothing changes"
    );
    // Reordered, then out to a window of its own, which shows it.
    assert!(workspace.move_tab(two, first, 0));
    assert!(workspace.move_tab(two, first, 0));
    assert_eq!(
        changes(&mut workspace),
        [Change::TabMoved {
            tab: two.into(),
            from: first.into(),
            window: first.into(),
            index: 0
        }]
    );
    assert_eq!(tabs(&workspace, first), [two, one]);
    let second = workspace.tab_to_new_window(one).unwrap();
    assert_eq!(
        changes(&mut workspace),
        [
            Change::WindowOpened {
                window: second.into()
            },
            Change::TabMoved {
                tab: one.into(),
                from: first.into(),
                window: second.into(),
                index: 0
            },
        ]
    );
    assert_eq!(tabs(&workspace, second), [one]);
    assert_eq!(workspace.tab(one).unwrap().window(), second);
    assert_eq!(workspace.window(second).unwrap().active_tab(), one);
    // A tab alone in its window stays there.
    assert_eq!(workspace.tab_to_new_window(one), Some(second));
    assert_eq!(changes(&mut workspace), []);
    // Back, and the window that it leaves without tabs closes.
    assert!(workspace.move_tab(one, first, 99));
    assert_eq!(
        changes(&mut workspace),
        [
            Change::TabMoved {
                tab: one.into(),
                from: second.into(),
                window: first.into(),
                index: 1
            },
            Change::WindowClosed {
                window: second.into()
            },
        ]
    );
    assert_eq!(tabs(&workspace, first), [two, one]);
    assert_eq!(workspace.window(first).unwrap().active_tab(), two);
    // Closed, with their panes: the shown one gives way to the one after
    // it, or before it, and the last one closes the window.
    let one_pane = workspace.tab(one).unwrap().active_pane();
    assert!(workspace.close_tab(two));
    assert!(workspace.pane(two_pane).is_none());
    assert_eq!(workspace.window(first).unwrap().active_tab(), one);
    assert_eq!(
        changes(&mut workspace),
        [
            Change::PaneClosed {
                pane: two_pane.into(),
                tab: two.into()
            },
            Change::TabClosed {
                tab: two.into(),
                window: first.into()
            },
            Change::TabActivated {
                tab: one.into(),
                window: first.into()
            },
        ]
    );
    assert!(workspace.close_tab(one));
    assert_eq!(
        changes(&mut workspace),
        [
            Change::PaneClosed {
                pane: one_pane.into(),
                tab: one.into()
            },
            Change::TabClosed {
                tab: one.into(),
                window: first.into()
            },
            Change::WindowClosed {
                window: first.into()
            },
        ]
    );
    assert!(workspace.window(first).is_none());
    assert!(!workspace.close_tab(one));
}

#[test]
fn the_settings_tab_of_no_pane_opens_once_moves_and_closes_as_a_tab() {
    let mut workspace = Workspace::new();
    let first = workspace.open_window(pane("sleep 10"));
    let one = workspace.window(first).unwrap().active_tab();
    let two = workspace.open_tab(first, pane("sleep 10"), 99).unwrap();
    changes(&mut workspace);
    let tabs = |workspace: &Workspace, window| workspace.window(window).unwrap().tabs().to_vec();
    // After the tab that the window shows, which it shows, of no pane.
    let settings = workspace.open_settings(first, "Settings").unwrap();
    assert_eq!(tabs(&workspace, first), [one, settings, two]);
    assert_eq!(workspace.window(first).unwrap().active_tab(), settings);
    assert_eq!(workspace.settings_tab(), Some((settings, first)));
    assert!(workspace.is_settings(settings) && !workspace.is_settings(one));
    assert!(workspace.tab(settings).is_none());
    assert_eq!(workspace.settings_title(), "Settings");
    assert_eq!(workspace.active_pane(first), None);
    assert!(workspace.focused(first).is_none());
    assert_eq!(
        changes(&mut workspace),
        [
            Change::TabCreated {
                tab: settings.into(),
                window: first.into(),
                index: 1
            },
            Change::TabActivated {
                tab: settings.into(),
                window: first.into()
            },
        ]
    );
    // Once: opened again, the one there shows.
    assert!(workspace.activate_tab(two));
    changes(&mut workspace);
    assert_eq!(workspace.open_settings(first, "Settings"), Some(settings));
    assert_eq!(tabs(&workspace, first), [one, settings, two]);
    assert_eq!(workspace.window(first).unwrap().active_tab(), settings);
    // Reordered, then to another window, then out to a window of its own.
    assert!(workspace.move_tab(settings, first, 99));
    assert_eq!(tabs(&workspace, first), [one, two, settings]);
    let second = workspace.open_window(pane("sleep 10"));
    assert!(workspace.move_tab(settings, second, 0));
    assert_eq!(workspace.settings_tab(), Some((settings, second)));
    assert_eq!(tabs(&workspace, first), [one, two]);
    let own = workspace.tab_to_new_window(settings).unwrap();
    assert_eq!(tabs(&workspace, own), [settings]);
    assert_eq!(workspace.settings_tab(), Some((settings, own)));
    changes(&mut workspace);
    // Closed as a tab: its window, of no tab left, closes.
    assert!(workspace.close_tab(settings));
    assert_eq!(workspace.settings_tab(), None);
    assert!(workspace.window(own).is_none());
    assert_eq!(
        changes(&mut workspace),
        [
            Change::TabClosed {
                tab: settings.into(),
                window: own.into()
            },
            Change::WindowClosed { window: own.into() },
        ]
    );
    assert!(!workspace.close_tab(settings));
    // Of its window too, which then opens a new one.
    let again = workspace.open_settings(first, "Settings").unwrap();
    assert_ne!(again, settings);
    assert!(workspace.close_window(first));
    assert_eq!(workspace.settings_tab(), None);
    assert!(workspace.open_settings(first, "Settings").is_none());
}

#[test]
fn the_settings_open_in_a_window_of_their_own_which_takes_no_tab_and_closes_with_them() {
    let mut workspace = Workspace::new();
    let first = workspace.open_window(pane("sleep 10"));
    let one = workspace.window(first).unwrap().active_tab();
    changes(&mut workspace);
    // A window of the settings tab alone, which shows it.
    let own = workspace.open_settings_window("Settings");
    assert_ne!(own, first);
    let (settings, of) = workspace.settings_tab().unwrap();
    assert_eq!(of, own);
    assert_eq!(workspace.settings_window(), Some(own));
    assert_eq!(workspace.window(own).unwrap().tabs(), [settings]);
    assert_eq!(workspace.window(own).unwrap().active_tab(), settings);
    assert_eq!(
        changes(&mut workspace),
        [
            Change::WindowOpened { window: own.into() },
            Change::TabCreated {
                tab: settings.into(),
                window: own.into(),
                index: 0
            },
            Change::TabActivated {
                tab: settings.into(),
                window: own.into()
            },
        ]
    );
    // Opened again: the same.
    assert_eq!(workspace.open_settings_window("Settings"), own);
    assert_eq!(workspace.settings_tab(), Some((settings, own)));
    // No other tab goes into it, new or moved, nor the settings tab out of
    // it.
    assert!(workspace.open_tab(own, pane("sleep 10"), 1).is_none());
    assert!(!workspace.move_tab(one, own, 0));
    assert!(!workspace.move_tab(settings, first, 0));
    assert_eq!(workspace.window(first).unwrap().tabs(), [one]);
    // Its tab closed, it closes.
    changes(&mut workspace);
    assert!(workspace.close_tab(settings));
    assert!(workspace.window(own).is_none());
    assert_eq!(workspace.settings_tab(), None);
    assert_eq!(workspace.settings_window(), None);
    // Closed as a window, the settings go too.
    let own = workspace.open_settings_window("Settings");
    assert!(workspace.close_window(own));
    assert_eq!(workspace.settings_tab(), None);
}

#[test]
fn panes_split_zoom_close_and_move_with_their_tabs() {
    let mut workspace = Workspace::new();
    let a = pane("sleep 10");
    let a_id = a.id();
    let window = workspace.open_window(a);
    let tab = workspace.window(window).unwrap().active_tab();
    changes(&mut workspace);
    let b = workspace
        .split(a_id, Direction::Horizontal, pane("sleep 10"), Side::Second)
        .unwrap();
    let laid_out = Change::LayoutChanged { tab: tab.into() };
    assert_eq!(
        changes(&mut workspace),
        [
            Change::PaneCreated {
                pane: b.into(),
                tab: tab.into()
            },
            laid_out.clone(),
        ]
    );
    let layout = |workspace: &Workspace, tab| workspace.tab(tab).unwrap().layout().clone();
    assert_eq!(layout(&workspace, tab).panes(), [a_id, b]);
    assert_eq!(workspace.tab_of(b), Some(tab));
    // The keyboard stays where it was; a zoom ends with a split.
    assert_eq!(workspace.active_pane(window), Some(a_id));
    assert!(workspace.zoom(tab, Some(b)));
    assert!(workspace.zoom(tab, Some(b)));
    assert_eq!(changes(&mut workspace), std::slice::from_ref(&laid_out));
    assert_eq!(workspace.tab(tab).unwrap().zoomed(), Some(b));
    let c = workspace
        .split(b, Direction::Vertical, pane("sleep 10"), Side::Second)
        .unwrap();
    assert_eq!(workspace.tab(tab).unwrap().zoomed(), None);
    // A closed pane's room to its sibling, and the keyboard to the
    // sibling's nearest pane.
    assert!(workspace.focus_pane(c));
    assert!(workspace.focus_pane(c));
    assert!(workspace.zoom(tab, Some(c)));
    assert_eq!(
        changes(&mut workspace),
        [
            Change::PaneCreated {
                pane: c.into(),
                tab: tab.into()
            },
            laid_out.clone(),
            Change::PaneFocused {
                pane: c.into(),
                tab: tab.into()
            },
            laid_out.clone(),
        ]
    );
    assert!(workspace.close_pane(c));
    assert_eq!(
        changes(&mut workspace),
        [
            Change::PaneClosed {
                pane: c.into(),
                tab: tab.into()
            },
            laid_out.clone(),
            Change::PaneFocused {
                pane: b.into(),
                tab: tab.into()
            },
        ]
    );
    assert!(workspace.pane(c).is_none());
    assert_eq!(workspace.active_pane(window), Some(b));
    assert_eq!(workspace.tab(tab).unwrap().zoomed(), None);
    assert_eq!(
        layout(&workspace, tab),
        Layout::Split {
            direction: Direction::Horizontal,
            ratio: None,
            first: Box::new(Layout::Leaf(a_id)),
            second: Box::new(Layout::Leaf(b)),
        }
    );
    // Resized, then equalized; equal already, it does not change.
    assert!(workspace.resize_pane(a_id, Edge::Right, 0.2));
    assert!((layout(&workspace, tab).share().unwrap() - 0.7).abs() < 1e-9);
    assert!(workspace.equalize(tab));
    assert_eq!(layout(&workspace, tab).share(), Some(0.5));
    assert!(workspace.equalize(tab));
    assert!(
        !workspace.resize_pane(a_id, Edge::Left, 0.2),
        "no edge there"
    );
    assert_eq!(
        changes(&mut workspace),
        [laid_out.clone(), laid_out.clone()]
    );
    // Out to a tab of its own, then a pane alone to a window of its own,
    // with its tab.
    let other = workspace.pane_to_new_tab(b, window, 99).unwrap();
    assert_eq!(layout(&workspace, tab), Layout::Leaf(a_id));
    assert_eq!(layout(&workspace, other), Layout::Leaf(b));
    assert_eq!(
        changes(&mut workspace),
        [
            laid_out.clone(),
            Change::PaneFocused {
                pane: a_id.into(),
                tab: tab.into()
            },
            Change::TabCreated {
                tab: other.into(),
                window: window.into(),
                index: 1
            },
            Change::PaneMoved {
                pane: b.into(),
                from: tab.into(),
                tab: other.into()
            },
        ]
    );
    let moved = workspace.pane_to_new_window(a_id).unwrap();
    assert_eq!(
        changes(&mut workspace),
        [
            Change::WindowOpened {
                window: moved.into()
            },
            Change::TabMoved {
                tab: tab.into(),
                from: window.into(),
                window: moved.into(),
                index: 0
            },
            Change::TabActivated {
                tab: other.into(),
                window: window.into()
            },
        ]
    );
    assert_eq!(workspace.tab_of(a_id), Some(tab));
    assert_eq!(workspace.window(moved).unwrap().tabs(), [tab]);
    // A tab's last pane closes it, and a window's last tab the window.
    assert!(workspace.close_pane(b));
    assert_eq!(
        changes(&mut workspace),
        [
            Change::PaneClosed {
                pane: b.into(),
                tab: other.into()
            },
            Change::TabClosed {
                tab: other.into(),
                window: window.into()
            },
            Change::WindowClosed {
                window: window.into()
            },
        ]
    );
    assert!(workspace.tab(other).is_none());
    assert_eq!(workspace.tab_of(b), None);
}

#[test]
fn ids_are_never_reused() {
    let mut workspace = Workspace::new();
    let first = workspace.open_window(pane("sleep 10"));
    let first_pane = workspace.active_pane(first).unwrap();
    assert!(workspace.close_window(first));
    assert!(!workspace.close_window(first), "closed already");
    let second = workspace.open_window(pane("sleep 10"));
    let second_pane = workspace.active_pane(second).unwrap();
    assert!(second.get() > first.get());
    assert_ne!(second_pane, first_pane);
    // Another workspace of the process takes others too.
    let mut other = Workspace::new();
    let third = other.open_window(pane("sleep 10"));
    assert!(third.get() > second.get());
}

#[test]
fn closing_a_window_closes_its_tab_and_its_pane() {
    let mut workspace = Workspace::new();
    let kept = workspace.open_window(pane("sleep 10"));
    let window = workspace.open_window(pane("sleep 10"));
    let tab = workspace.window(window).unwrap().active_tab();
    let id = workspace.active_pane(window).unwrap();
    changes(&mut workspace);
    assert!(workspace.close_window(window));
    assert_eq!(
        changes(&mut workspace),
        [
            Change::PaneClosed {
                pane: id.into(),
                tab: tab.into()
            },
            Change::TabClosed {
                tab: tab.into(),
                window: window.into()
            },
            Change::WindowClosed {
                window: window.into()
            },
        ]
    );
    assert!(workspace.window(window).is_none());
    assert!(workspace.tab(tab).is_none());
    assert!(workspace.pane(id).is_none());
    assert_eq!(workspace.active_pane(window), None);
    let open: Vec<WindowId> = workspace.windows().map(Window::id).collect();
    assert_eq!(open, [kept]);
    assert_eq!(workspace.panes().count(), 1);
}

#[test]
fn the_sessions_events_come_in_as_they_are_collected() {
    let mut workspace = Workspace::new();
    // A title, the bell, a write to the clipboard of `hi`, and the end.
    let window = workspace.open_window(pane(
        "printf '\\033]2;hello\\007\\007\\033]52;c;aGk=\\007'; sleep 0.2",
    ));
    let id = workspace.active_pane(window).unwrap();
    workspace.take_events();
    let events = collect_until(&mut workspace, |events| events.contains(&Event::Ended(id)));
    assert!(events.contains(&Event::TitleChanged(id)), "{events:?}");
    assert!(events.contains(&Event::Bell(id)), "{events:?}");
    let write = Event::ClipboardWrite {
        pane: id,
        target: ClipboardTarget {
            clipboard: true,
            primary: false,
        },
        data: b"hi".to_vec(),
    };
    assert!(events.contains(&write), "{events:?}");
    // The end, once, and after what the program wrote.
    let ended = events.iter().position(|event| *event == Event::Ended(id));
    assert!(ended > events.iter().position(|event| *event == Event::Bell(id)));
    let ids = workspace.pane_ids();
    workspace.collect(&ids, Instant::now());
    assert!(!workspace.take_events().contains(&Event::Ended(id)));
    // The tab takes its active pane's title.
    let tab = workspace.window(window).unwrap().active_tab();
    assert_eq!(workspace.tab(tab).unwrap().title(), "hello");
}

#[test]
fn a_command_is_timed_by_its_marks_from_the_look_at_its_start_to_the_one_at_its_end() {
    let mut workspace = Workspace::new();
    // A command of 0.3 s, of the status 3, then one whose start and end
    // come at once, which one look may find.
    let window = workspace.open_window(pane(concat!(
        "printf '\\033]133;A\\007$ \\033]133;B\\007sleep\\n\\033]133;C\\007'; sleep 0.3; ",
        "printf '\\033]133;D;3\\007\\033]133;A\\007$ '; sleep 0.1; ",
        "printf '\\033]133;B\\007x\\n\\033]133;C\\007\\033]133;D;0\\007'; sleep 5",
    )));
    let id = workspace.active_pane(window).unwrap();
    workspace.take_events();
    let ended = |events: &[Event]| -> Vec<(u64, Option<i32>, Option<Duration>)> {
        events
            .iter()
            .filter_map(|event| match *event {
                Event::CommandEnded {
                    pane,
                    command,
                    status,
                    took,
                } if pane == id => Some((command, status, took)),
                _ => None,
            })
            .collect()
    };
    let events = collect_until(&mut workspace, |events| ended(events).len() == 2);
    let ended = ended(&events);
    let (command, status, took) = ended[0];
    assert_eq!((command, status), (1, Some(3)));
    // Of the looks' times: the look that finds its start may come a pace
    // and the loop's pause after it, under a load of other tests longer.
    let took = took.expect("its start was seen");
    assert!(
        took >= Duration::from_millis(200) && took < Duration::from_secs(3),
        "{took:?}"
    );
    // A start and an end that one look found, of no time between.
    let (command, status, took) = ended[1];
    assert_eq!((command, status), (2, Some(0)));
    assert!(
        took.is_some_and(|took| took < Duration::from_millis(300)),
        "{took:?}"
    );
    // The start goes to the plugins as the terminal told it.
    assert!(events.contains(&Event::Terminal {
        pane: id,
        event: terminal::Event::CommandStarted(1),
    }));
}

#[test]
fn a_tab_takes_the_title_and_the_color_of_the_pane_that_takes_its_keyboard() {
    let mut workspace = Workspace::new();
    let window = workspace.open_window(pane(
        "printf '\\033]2;first\\007\\033]6;1;bg;red;brightness;255\\007'; sleep 5",
    ));
    let first = workspace.active_pane(window).unwrap();
    let second = workspace
        .split(
            first,
            Direction::Horizontal,
            pane("printf '\\033]2;second\\007'; sleep 5"),
            Side::Second,
        )
        .unwrap();
    let titled = |events: &[Event], id| events.contains(&Event::TitleChanged(id));
    collect_until(&mut workspace, |events| {
        titled(events, first) && titled(events, second)
    });
    let tab = workspace.window(window).unwrap().active_tab();
    let look = |workspace: &Workspace| {
        let tab = workspace.tab(tab).unwrap();
        (tab.title().to_owned(), tab.color())
    };
    let red = Some(Rgb::new(255, 0, 0));
    // The second's title, set while the first had the keyboard, is kept.
    assert_eq!(look(&workspace), ("first".to_owned(), red));
    assert!(workspace.focus_pane(second));
    assert_eq!(look(&workspace), ("second".to_owned(), None));
    assert!(workspace.focus_pane(first));
    assert_eq!(look(&workspace), ("first".to_owned(), red));
    // A pane out to a tab of its own takes its title there; the tab that
    // it leaves, its other pane's.
    let moved = workspace.pane_to_new_tab(first, window, 1).unwrap();
    assert_eq!(workspace.tab(moved).unwrap().title(), "first");
    assert_eq!(look(&workspace), ("second".to_owned(), None));
}

/// Collects the sessions' events until `done` holds of the workspace, for
/// ten seconds at most, and returns the events collected.
fn collect_while_not(workspace: &mut Workspace, done: impl Fn(&Workspace) -> bool) -> Vec<Event> {
    let start = Instant::now();
    let mut events = Vec::new();
    while !done(workspace) {
        assert!(start.elapsed() < Duration::from_secs(10), "{events:?}");
        let ids = workspace.pane_ids();
        workspace.collect(&ids, Instant::now());
        events.extend(workspace.take_events());
        std::thread::sleep(Duration::from_millis(10));
    }
    events
}

/// A window of a shell's `script`, which reads a line before each of its
/// steps, its pane and its tab's title.
fn stepped(script: &str) -> (Workspace, PaneId, impl Fn(&Workspace) -> String) {
    let mut workspace = Workspace::new();
    let window = workspace.open_window(pane(script));
    let id = workspace.active_pane(window).unwrap();
    let tab = workspace.window(window).unwrap().active_tab();
    let title = move |workspace: &Workspace| workspace.tab(tab).unwrap().title().to_owned();
    (workspace, id, title)
}

#[test]
fn a_program_s_title_goes_as_the_shell_comes_back() {
    let (mut workspace, id, title) = stepped(
        "printf '\\033]2;local\\007'; read a; printf '\\033]2;remote\\007'; read b; printf 'back\\n'; sleep 30",
    );
    collect_while_not(&mut workspace, |workspace| title(workspace) == "local");
    // A program, as `ssh`, comes to the foreground and titles the tab.
    assert!(workspace.set_pane_running(id, true, false));
    workspace.pane(id).unwrap().send_text("\n");
    collect_while_not(&mut workspace, |workspace| title(workspace) == "remote");
    // It leaves: the shell's output, of no title, gives the title back,
    // told, and the terminal's own too.
    assert!(workspace.set_pane_running(id, false, false));
    workspace.pane(id).unwrap().send_text("\n");
    let events = collect_while_not(&mut workspace, |workspace| title(workspace) == "local");
    assert!(events.contains(&Event::TitleChanged(id)), "{events:?}");
    assert_eq!(
        workspace.pane(id).unwrap().terminal().unwrap().title(),
        "local"
    );
}

#[test]
fn a_shell_s_own_titles_stay() {
    // A title while no program other than a shell runs is the shell's.
    let (mut workspace, id, title) =
        stepped("printf '\\033]2;local\\007'; read a; printf '\\033]2;mine\\007'; sleep 30");
    collect_while_not(&mut workspace, |workspace| title(workspace) == "local");
    workspace.pane(id).unwrap().send_text("\n");
    collect_while_not(&mut workspace, |workspace| title(workspace) == "mine");
    // A program that sets none leaves the title as it was.
    assert!(workspace.set_pane_running(id, true, false));
    assert!(workspace.set_pane_running(id, false, false));
    std::thread::sleep(Duration::from_millis(100));
    let ids = workspace.pane_ids();
    workspace.collect(&ids, Instant::now());
    assert_eq!(title(&workspace), "mine");
    // The title that the shell sets as it comes back, as its prompt does,
    // stays over the program's.
    let (mut workspace, id, title) = stepped(
        "read a; printf '\\033]2;remote\\007'; read b; printf '\\033]2;prompt\\007back'; sleep 30",
    );
    assert!(workspace.set_pane_running(id, true, false));
    workspace.pane(id).unwrap().send_text("\n");
    collect_while_not(&mut workspace, |workspace| title(workspace) == "remote");
    assert!(workspace.set_pane_running(id, false, false));
    workspace.pane(id).unwrap().send_text("\n");
    collect_while_not(&mut workspace, |workspace| title(workspace) == "prompt");
    std::thread::sleep(Duration::from_millis(100));
    let ids = workspace.pane_ids();
    workspace.collect(&ids, Instant::now());
    assert_eq!(title(&workspace), "prompt");
}

#[test]
fn a_tab_s_label_is_its_title_or_its_place_and_its_changes_are_told() {
    let mut workspace = Workspace::new();
    let window = workspace.open_window(pane("sleep 5"));
    let tab = workspace.window(window).unwrap().active_tab();
    workspace.take_events();
    let told = |workspace: &mut Workspace| {
        workspace
            .take_events()
            .iter()
            .filter(|event| **event == Event::Changed(Change::TabChanged { tab: tab.into() }))
            .count()
    };
    let label = |workspace: &Workspace| workspace.tab(tab).unwrap().label().to_owned();
    // Its place, without a title, told once.
    assert!(workspace.set_tab_place(tab, Some("src — vim".to_owned())));
    assert_eq!(
        (label(&workspace), told(&mut workspace)),
        ("src — vim".to_owned(), 1)
    );
    workspace.set_tab_place(tab, Some("src — vim".to_owned()));
    assert_eq!(told(&mut workspace), 0);
    // A title of its own before it, and back; a color.
    workspace.set_tab_title(tab, Some("work".to_owned()));
    assert_eq!(
        (label(&workspace), told(&mut workspace)),
        ("work".to_owned(), 1)
    );
    workspace.set_tab_color(tab, Some(Rgb::new(1, 2, 3)));
    assert_eq!(told(&mut workspace), 1);
    workspace.set_tab_title(tab, None);
    assert_eq!(
        (label(&workspace), told(&mut workspace)),
        ("src — vim".to_owned(), 1)
    );
    // Another pane's keyboard: its place goes, which the app gives anew.
    let first = workspace.active_pane(window).unwrap();
    let second = workspace
        .split(first, Direction::Horizontal, pane("sleep 5"), Side::Second)
        .unwrap();
    workspace.take_events();
    assert!(workspace.focus_pane(second));
    assert_eq!(
        (label(&workspace), told(&mut workspace)),
        (String::new(), 1)
    );
    assert!(!workspace.set_tab_place(TabId::of(999_999), None));
}

#[test]
fn a_kept_screen_tells_how_its_program_ended_in_the_language_shown() {
    let program = Program::command("/bin/sh", ["-c", "exit 3"]);
    let texts = Texts::new("it", &crate::l10n::KERNEL);
    let mut pane = Pane::spawn(
        &program,
        &Environment::default(),
        &Settings::default(),
        grid(),
        &Colors::default(),
        &texts,
        || {},
    );
    let start = Instant::now();
    while pane
        .exited(crate::config::CloseOnExit::Clean, Instant::now(), &texts)
        .is_none()
    {
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "the program ends"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    pane.update();
    let snapshot = pane.snapshot();
    let shown: String = (0..snapshot.rows())
        .map(|row| {
            snapshot
                .line(row)
                .cells()
                .iter()
                .map(|cell| match cell.content() {
                    lokyr_vt_core::page::Content::Char(c) if c != '\0' => c,
                    _ => ' ',
                })
                .collect::<String>()
        })
        .collect();
    assert!(
        shown.contains("[il programma è uscito con codice 3; un tasto chiude la finestra]"),
        "{shown:?}"
    );
}

#[test]
fn a_pane_that_cannot_start_tells_nothing() {
    let program = Program::command("/nonexistent/program", [""; 0]);
    let pane = Pane::spawn(
        &program,
        &Environment::default(),
        &Settings::default(),
        grid(),
        &Colors::default(),
        &Texts::new("en", &crate::l10n::KERNEL),
        || {},
    );
    assert!(pane.is_kept());
    let mut workspace = Workspace::new();
    workspace.open_window(pane);
    workspace.take_events();
    let ids = workspace.pane_ids();
    workspace.collect(&ids, Instant::now());
    assert_eq!(workspace.take_events(), []);
}

#[test]
fn a_pane_tells_the_program_that_runs_but_a_shell() {
    // A program that the shell became.
    let busy = pane("exec sleep 30");
    let start = Instant::now();
    while busy.running().as_deref() != Some("sleep") {
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "{:?}",
            busy.running()
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    // A shell at its prompt, here waiting for a line, runs nothing.
    let idle = pane("printf ready; read line");
    let start = Instant::now();
    loop {
        idle.select_all();
        if idle
            .selection_text()
            .is_some_and(|text| text.contains("ready"))
        {
            break;
        }
        assert!(start.elapsed() < Duration::from_secs(10));
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(idle.running(), None);
}

/// Waits until `pane` runs `wanted`, for ten seconds at most.
fn running_until(pane: &Pane, wanted: Option<&str>) {
    let start = Instant::now();
    while pane.running().as_deref() != wanted {
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "{:?}",
            pane.running()
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn a_pane_tells_the_jobs_behind_the_shell() {
    // A job in the background, and one stopped, at a shell that reads.
    running_until(&pane("sleep 30 & read line"), Some("sleep"));
    // Stopped once it is `sleep`, rather than the shell's copy that
    // becomes it.
    running_until(
        &pane("sleep 30 & sleep 0.3; kill -STOP $!; read line"),
        Some("sleep"),
    );
    // A program that started a shell, which reads.
    running_until(
        &pane("awk 'BEGIN { system(\"sh -c \\\"read line\\\"\") }'"),
        Some("awk"),
    );
}

#[test]
fn a_pane_tells_its_contexts() {
    // As its frames show them, but for the app's own changes.
    let mut pane = pane("printf 'ready'; sleep 10");
    assert!(pane.contexts().is_empty());
    // Once the program shows what it shows, so that no output moves the
    // selection's rows away.
    let start = Instant::now();
    loop {
        pane.select_all();
        if pane
            .selection_text()
            .is_some_and(|text| text.contains("ready"))
        {
            break;
        }
        assert!(start.elapsed() < Duration::from_secs(10));
        std::thread::sleep(Duration::from_millis(10));
    }
    // The app's own change, as the terminal has it, before the frame;
    // then as the frame shows it.
    assert_eq!(pane.contexts(), ["selection"]);
    pane.update();
    assert_eq!(pane.contexts(), ["selection"]);
    pane.clear_selection();
    assert!(pane.contexts().is_empty());
    pane.update();
    assert!(pane.contexts().is_empty());
    let mut alternate = self::pane("printf '\\033[?1049h'; sleep 10");
    let start = Instant::now();
    while alternate.contexts().is_empty() {
        assert!(start.elapsed() < Duration::from_secs(10));
        std::thread::sleep(Duration::from_millis(10));
        alternate.update();
    }
    assert_eq!(alternate.contexts(), ["alternate-screen"]);
}

#[test]
fn a_marked_cell_s_line_and_link_keep_its_row_s_number_as_output_scrolls_the_view() {
    let pane = pane(concat!(
        "stty -echo; printf '\\033]8;;https://lokyr.dev/osc\\033\\\\see\\033]8;;\\033\\\\ ",
        "https://lokyr.dev/x\\n'; read line; seq 1 60; echo done; read line"
    ));
    let text = |row| {
        pane.line_text(row, 4)
            .map(|(text, _)| text.trim_end().to_owned())
    };
    let wait = |done: &dyn Fn() -> bool| {
        let start = Instant::now();
        while !done() {
            assert!(start.elapsed() < Duration::from_secs(10));
            std::thread::sleep(Duration::from_millis(10));
        }
    };
    let first = "see https://lokyr.dev/x".to_owned();
    wait(&|| text(0).as_deref() == Some(first.as_str()));
    // A menu's cell, on the OSC 8 link.
    pane.mark_cell((1, 0));
    assert_eq!(pane.marked_cell(), Some((1, 0)));
    assert_eq!(pane.origin().map(|origin| origin.top), Some(0));
    // Output scrolls the line into the scrollback: the view's first row
    // shows another, and the row numbered 0 is still the line.
    pane.send_text("\r");
    wait(&|| {
        pane.origin().is_some_and(|origin| origin.top > 0) && text(61).as_deref() == Some("done")
    });
    assert_eq!(pane.marked_cell(), Some((1, 0)));
    assert_eq!(text(0), Some(first));
    assert_eq!(pane.link_at(1, 0).as_deref(), Some("https://lokyr.dev/osc"));
    let (_, cells) = pane.line_text(0, 4).unwrap();
    assert_eq!(cells[0], (0, 0));
    assert_eq!(text(1).as_deref(), Some("1"));
    pane.unmark_cell();
    assert_eq!(pane.marked_cell(), None);
}

/// The workspace as its changes tell it, from the empty one.
mod told {
    use std::collections::{BTreeMap, BTreeSet};

    use lokyr_plugin_sdk::event::WorkspaceEvent as Change;
    use lokyr_plugin_sdk::host as sdk;
    use proptest::prelude::*;

    use super::{changes, pane};
    use crate::workspace::{
        Direction, Edge, Layout, PaneId, Ratio, Side, TabId, Window as Shown, Workspace,
    };

    /// A window's tabs, in their order, and the one that it shows.
    #[derive(Debug, Default, PartialEq, Eq)]
    struct Window {
        tabs: Vec<sdk::TabId>,
        shown: Option<sdk::TabId>,
    }

    /// A tab's window, its panes and the one that has its keyboard.
    #[derive(Debug, PartialEq, Eq)]
    struct Tab {
        window: sdk::WindowId,
        panes: BTreeSet<sdk::PaneId>,
        active: Option<sdk::PaneId>,
    }

    #[derive(Debug, Default, PartialEq, Eq)]
    struct Told {
        windows: BTreeMap<sdk::WindowId, Window>,
        tabs: BTreeMap<sdk::TabId, Tab>,
    }

    impl Told {
        /// What `workspace` holds, as its changes would tell it.
        fn of(workspace: &Workspace) -> Self {
            let mut told = Self::default();
            for window in workspace.windows() {
                let shown = Window {
                    tabs: window.tabs().iter().map(|&tab| tab.into()).collect(),
                    shown: Some(window.active_tab().into()),
                };
                told.windows.insert(window.id().into(), shown);
                for &tab in window.tabs() {
                    let tab = workspace.tab(tab).unwrap();
                    let panes = tab.layout().panes().into_iter().map(Into::into).collect();
                    let shown = Tab {
                        window: tab.window().into(),
                        panes,
                        active: Some(tab.active_pane().into()),
                    };
                    told.tabs.insert(tab.id().into(), shown);
                }
            }
            told
        }

        /// Takes `change`, which changes what it tells, and the tabs whose
        /// layouts it changed into `laid_out`.
        fn take(&mut self, change: &Change, laid_out: &mut BTreeSet<sdk::TabId>) {
            match *change {
                // A tab's look and a pane's title, which the model does not
                // keep.
                Change::TabChanged { .. } | Change::PaneChanged { .. } => {}
                Change::WindowOpened { window } => {
                    assert!(self.windows.insert(window, Window::default()).is_none());
                }
                Change::WindowClosed { window } => {
                    let closed = self.windows.remove(&window).unwrap();
                    assert!(closed.tabs.is_empty(), "its tabs before it");
                }
                Change::TabCreated { tab, window, index } => {
                    let into = self.window(window);
                    into.tabs.insert(index, tab);
                    into.shown.get_or_insert(tab);
                    let created = Tab {
                        window,
                        panes: BTreeSet::new(),
                        active: None,
                    };
                    assert!(self.tabs.insert(tab, created).is_none());
                }
                Change::TabClosed {
                    tab: id,
                    window: from,
                } => {
                    let closed = self.tabs.remove(&id).unwrap();
                    assert_eq!(closed.window, from);
                    assert!(closed.panes.is_empty(), "its panes before it");
                    let from = self.window(from);
                    from.tabs.retain(|&tab| tab != id);
                    if from.shown == Some(id) {
                        from.shown = None;
                    }
                }
                Change::TabActivated {
                    tab: id,
                    window: of,
                } => {
                    let of = self.window(of);
                    assert!(of.tabs.contains(&id));
                    assert_ne!(of.shown, Some(id));
                    of.shown = Some(id);
                }
                Change::TabMoved {
                    tab: id,
                    from,
                    window: into,
                    index,
                } => {
                    let left = self.window(from);
                    let at = left.tabs.iter().position(|&tab| tab == id).unwrap();
                    left.tabs.remove(at);
                    if from != into && left.shown == Some(id) {
                        left.shown = None;
                    }
                    assert!(from != into || at != index, "a move in place");
                    let taken = self.window(into);
                    taken.tabs.insert(index, id);
                    taken.shown.get_or_insert(id);
                    self.tab(id).window = into;
                }
                Change::PaneCreated { pane, tab: id } => {
                    let into = self.tab(id);
                    assert!(into.panes.insert(pane));
                    into.active.get_or_insert(pane);
                }
                Change::PaneClosed { pane, tab: id } => {
                    let from = self.tab(id);
                    assert!(from.panes.remove(&pane));
                    if from.active == Some(pane) {
                        from.active = None;
                    }
                }
                Change::PaneFocused { pane, tab: id } => {
                    let of = self.tab(id);
                    assert!(of.panes.contains(&pane));
                    assert_ne!(of.active, Some(pane));
                    of.active = Some(pane);
                }
                Change::PaneMoved {
                    pane,
                    from,
                    tab: id,
                } => {
                    let left = self.tab(from);
                    assert!(left.panes.remove(&pane));
                    if left.active == Some(pane) {
                        left.active = None;
                    }
                    let taken = self.tab(id);
                    assert!(taken.panes.insert(pane));
                    taken.active.get_or_insert(pane);
                }
                Change::LayoutChanged { tab } => {
                    laid_out.insert(tab);
                }
            }
        }
    }

    impl Told {
        fn window(&mut self, id: sdk::WindowId) -> &mut Window {
            self.windows.get_mut(&id).unwrap()
        }

        fn tab(&mut self, id: sdk::TabId) -> &mut Tab {
            self.tabs.get_mut(&id).unwrap()
        }
    }

    /// The item of `of` at `index`, round its length.
    fn pick<T: Copy>(of: &[T], index: usize) -> Option<T> {
        of.get(index % of.len().max(1)).copied()
    }

    #[derive(Clone, Debug)]
    enum Operation {
        OpenWindow,
        CloseWindow(usize),
        OpenTab(usize, usize),
        CloseTab(usize),
        ActivateTab(usize),
        MoveTab(usize, usize, usize),
        TabToNewWindow(usize),
        Split(usize, bool, bool),
        ClosePane(usize),
        FocusPane(usize),
        SetRatio(usize, Vec<bool>, Option<f64>),
        ResizePane(usize, u8, f64),
        Equalize(usize),
        Zoom(usize, Option<usize>),
        PaneToNewTab(usize, usize, usize),
        PaneToNewWindow(usize),
    }

    fn operation() -> impl Strategy<Value = Operation> {
        let index = || 0..4usize;
        prop_oneof![
            2 => Just(Operation::OpenWindow),
            1 => any::<usize>().prop_map(Operation::CloseWindow),
            3 => (any::<usize>(), index()).prop_map(|(window, at)| Operation::OpenTab(window, at)),
            1 => any::<usize>().prop_map(Operation::CloseTab),
            2 => any::<usize>().prop_map(Operation::ActivateTab),
            2 => (any::<usize>(), any::<usize>(), index())
                .prop_map(|(tab, window, at)| Operation::MoveTab(tab, window, at)),
            1 => any::<usize>().prop_map(Operation::TabToNewWindow),
            4 => (any::<usize>(), any::<bool>(), any::<bool>())
                .prop_map(|(pane, vertical, first)| Operation::Split(pane, vertical, first)),
            2 => any::<usize>().prop_map(Operation::ClosePane),
            2 => any::<usize>().prop_map(Operation::FocusPane),
            1 => (
                any::<usize>(),
                proptest::collection::vec(any::<bool>(), 0..3),
                proptest::option::of(0.0..1.0f64),
            )
                .prop_map(|(tab, path, share)| Operation::SetRatio(tab, path, share)),
            2 => (any::<usize>(), 0..4u8, -1.0..1.0f64)
                .prop_map(|(pane, edge, by)| Operation::ResizePane(pane, edge, by)),
            1 => any::<usize>().prop_map(Operation::Equalize),
            2 => (any::<usize>(), proptest::option::of(any::<usize>()))
                .prop_map(|(tab, pane)| Operation::Zoom(tab, pane)),
            1 => (any::<usize>(), any::<usize>(), index())
                .prop_map(|(pane, window, at)| Operation::PaneToNewTab(pane, window, at)),
            1 => any::<usize>().prop_map(Operation::PaneToNewWindow),
        ]
    }

    /// Makes `operation` on `workspace`, its targets picked among those
    /// that are open; whether it had them.
    fn make(workspace: &mut Workspace, operation: Operation) -> bool {
        let windows: Vec<_> = workspace.windows().map(Shown::id).collect();
        let tabs: Vec<TabId> = windows
            .iter()
            .flat_map(|&window| workspace.window(window).unwrap().tabs().to_vec())
            .collect();
        let panes: Vec<PaneId> = workspace.pane_ids();
        let window = |index| pick(&windows, index);
        let tab = |index| pick(&tabs, index);
        let pane_at = |index| pick(&panes, index);
        match operation {
            Operation::OpenWindow => {
                workspace.open_window(pane("exec sleep 30"));
            }
            Operation::CloseWindow(at) => {
                let Some(window) = window(at) else {
                    return false;
                };
                assert!(workspace.close_window(window));
            }
            Operation::OpenTab(at, index) => {
                let Some(window) = window(at) else {
                    return false;
                };
                assert!(
                    workspace
                        .open_tab(window, pane("exec sleep 30"), index)
                        .is_some()
                );
            }
            Operation::CloseTab(at) => {
                let Some(tab) = tab(at) else { return false };
                assert!(workspace.close_tab(tab));
            }
            Operation::ActivateTab(at) => {
                let Some(tab) = tab(at) else { return false };
                assert!(workspace.activate_tab(tab));
            }
            Operation::MoveTab(at, to, index) => {
                let (Some(tab), Some(window)) = (tab(at), window(to)) else {
                    return false;
                };
                assert!(workspace.move_tab(tab, window, index));
            }
            Operation::TabToNewWindow(at) => {
                let Some(tab) = tab(at) else { return false };
                assert!(workspace.tab_to_new_window(tab).is_some());
            }
            Operation::Split(at, vertical, first) => {
                let Some(target) = pane_at(at) else {
                    return false;
                };
                let direction = if vertical {
                    Direction::Vertical
                } else {
                    Direction::Horizontal
                };
                let side = if first { Side::First } else { Side::Second };
                let split = workspace.split(target, direction, pane("exec sleep 30"), side);
                assert!(split.is_some());
            }
            Operation::ClosePane(at) => {
                let Some(target) = pane_at(at) else {
                    return false;
                };
                assert!(workspace.close_pane(target));
            }
            Operation::FocusPane(at) => {
                let Some(target) = pane_at(at) else {
                    return false;
                };
                assert!(workspace.focus_pane(target));
            }
            Operation::SetRatio(at, path, share) => {
                let Some(tab) = tab(at) else { return false };
                let path: Vec<Side> = path
                    .into_iter()
                    .map(|first| if first { Side::First } else { Side::Second })
                    .collect();
                workspace.set_ratio(tab, &path, share.map(Ratio::new));
            }
            Operation::ResizePane(at, edge, by) => {
                let Some(target) = pane_at(at) else {
                    return false;
                };
                let edge = [Edge::Left, Edge::Right, Edge::Top, Edge::Bottom][usize::from(edge)];
                workspace.resize_pane(target, edge, by);
            }
            Operation::Equalize(at) => {
                let Some(tab) = tab(at) else { return false };
                assert!(workspace.equalize(tab));
            }
            Operation::Zoom(at, zoomed) => {
                let Some(tab) = tab(at) else { return false };
                // Any pane, one of another tab's too, which it refuses.
                workspace.zoom(tab, zoomed.and_then(pane_at));
            }
            Operation::PaneToNewTab(at, to, index) => {
                let (Some(target), Some(window)) = (pane_at(at), window(to)) else {
                    return false;
                };
                assert!(workspace.pane_to_new_tab(target, window, index).is_some());
            }
            Operation::PaneToNewWindow(at) => {
                let Some(target) = pane_at(at) else {
                    return false;
                };
                assert!(workspace.pane_to_new_window(target).is_some());
            }
        }
        true
    }

    /// Each tab's layout and zoom.
    fn layouts(workspace: &Workspace) -> BTreeMap<sdk::TabId, (Layout, Option<PaneId>)> {
        workspace
            .windows()
            .flat_map(|window| window.tabs().to_vec())
            .map(|id| {
                let tab = workspace.tab(id).unwrap();
                (id.into(), (tab.layout().clone(), tab.zoomed()))
            })
            .collect()
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(24))]

        #[test]
        fn the_changes_tell_every_change_and_nothing_else(
            operations in proptest::collection::vec(operation(), 1..24),
        ) {
            let mut workspace = Workspace::new();
            let mut told = Told::default();
            for operation in operations {
                let before = layouts(&workspace);
                let made = format!("{operation:?}");
                make(&mut workspace, operation);
                let mut laid_out = BTreeSet::new();
                for change in changes(&mut workspace) {
                    told.take(&change, &mut laid_out);
                }
                prop_assert_eq!(&told, &Told::of(&workspace), "after {}", made);
                let panes: BTreeSet<sdk::PaneId> =
                    workspace.pane_ids().into_iter().map(Into::into).collect();
                let told_panes: BTreeSet<sdk::PaneId> =
                    told.tabs.values().flat_map(|tab| tab.panes.iter().copied()).collect();
                prop_assert_eq!(panes, told_panes, "after {}", made);
                // A layout changed if and only if its change is told.
                for (tab, now) in layouts(&workspace) {
                    if let Some(was) = before.get(&tab) {
                        prop_assert_eq!(
                            was != &now,
                            laid_out.contains(&tab),
                            "{:?} after {}",
                            tab,
                            made
                        );
                    }
                }
            }
        }
    }
}

/// Collects until the marks of `tab` of `workspace` hold `done`, for ten
/// seconds at most, and returns the events collected.
fn collect_marks(
    workspace: &mut Workspace,
    tab: TabId,
    done: impl Fn(&Marks) -> bool,
) -> Vec<Event> {
    let start = Instant::now();
    let mut events = Vec::new();
    loop {
        let ids = workspace.pane_ids();
        workspace.collect(&ids, Instant::now());
        events.extend(workspace.take_events());
        let marks = workspace.tab(tab).unwrap().marks();
        if done(&marks) {
            return events;
        }
        assert!(start.elapsed() < Duration::from_secs(10), "{marks:?}");
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn a_hidden_tab_s_output_and_bell_mark_it_until_it_shows() {
    let mut workspace = Workspace::new();
    let window = workspace.open_window(pane("read line"));
    let shown = workspace.window(window).unwrap().active_tab();
    // A tab behind the first, which prints, then rings.
    let hidden = workspace
        .open_tab(
            window,
            pane("sleep 0.2; echo out; read line; printf '\\a'; read line"),
            1,
        )
        .unwrap();
    let hidden_pane = workspace.tab(hidden).unwrap().active_pane();
    let events = collect_marks(&mut workspace, hidden, |marks| marks.activity);
    assert!(!workspace.tab(hidden).unwrap().marks().bell);
    let told = |events: &[Event], tab: TabId| {
        events
            .iter()
            .any(|event| *event == Event::Changed(Change::TabChanged { tab: tab.into() }))
    };
    assert!(told(&events, hidden), "{events:?}");
    workspace.pane(hidden_pane).unwrap().text("\n");
    collect_marks(&mut workspace, hidden, |marks| marks.bell);
    // Shown, they are seen; the tab that showed marks nothing of its own.
    assert!(workspace.activate_tab(hidden));
    let marks = workspace.tab(hidden).unwrap().marks();
    assert!(!marks.activity && !marks.bell, "{marks:?}");
    assert!(told(&workspace.take_events(), hidden));
    assert_eq!(workspace.tab(shown).unwrap().marks(), Marks::default());
}

#[test]
fn a_shown_tab_s_output_marks_nothing_and_its_progress_marks_it() {
    let mut workspace = Workspace::new();
    let window = workspace.open_window(pane(
        "printf 'out\\033]9;4;1;40\\a'; read line; printf '\\033]9;4;2\\a'; read line; printf '\\033]9;4;0\\a'; read line",
    ));
    let tab = workspace.window(window).unwrap().active_tab();
    let id = workspace.tab(tab).unwrap().active_pane();
    collect_marks(&mut workspace, tab, |marks| {
        marks.progress == Some(terminal::Progress::Set(40))
    });
    assert!(!workspace.tab(tab).unwrap().marks().activity);
    workspace.pane(id).unwrap().text("\n");
    collect_marks(&mut workspace, tab, |marks| {
        marks.progress == Some(terminal::Progress::Error(None))
    });
    workspace.pane(id).unwrap().text("\n");
    collect_marks(&mut workspace, tab, |marks| marks.progress.is_none());
}

#[test]
fn a_tab_runs_a_program_while_any_of_its_panes_does_whichever_has_the_keyboard() {
    let mut workspace = Workspace::new();
    let window = workspace.open_window(pane("read line"));
    let tab = workspace.window(window).unwrap().active_tab();
    let a = workspace.tab(tab).unwrap().active_pane();
    let b = workspace
        .split(a, Direction::Horizontal, pane("read line"), Side::Second)
        .unwrap();
    let marks = |workspace: &Workspace| workspace.tab(tab).unwrap().marks();
    // A build in the first pane, full screen; the keyboard in the second's
    // shell: the tab runs it, of the second's screen.
    assert!(workspace.focus_pane(a));
    assert!(workspace.set_pane_running(a, true, true));
    assert!(marks(&workspace).running && marks(&workspace).alternate);
    assert!(workspace.focus_pane(b));
    assert!(workspace.set_pane_running(b, false, false));
    assert!(marks(&workspace).running);
    assert!(!marks(&workspace).alternate);
    // Back to the first, its screen again; its build ends, the tab runs
    // nothing.
    assert!(workspace.focus_pane(a));
    assert!(marks(&workspace).alternate);
    assert!(workspace.set_pane_running(a, false, false));
    assert!(!marks(&workspace).running);
    // A pane of a program that goes takes it with it.
    assert!(workspace.set_pane_running(b, true, false));
    assert!(marks(&workspace).running);
    assert!(workspace.close_pane(b));
    assert!(!marks(&workspace).running);
    assert!(!workspace.set_pane_running(b, true, false));
}

#[test]
fn a_tab_is_marked_of_what_runs_its_protected_input_and_its_agent_each_told_once() {
    let mut workspace = Workspace::new();
    let window = workspace.open_window(pane("read line"));
    let tab = workspace.window(window).unwrap().active_tab();
    workspace.take_events();
    let told = |workspace: &mut Workspace| {
        changes(workspace)
            .iter()
            .filter(|change| **change == Change::TabChanged { tab: tab.into() })
            .count()
    };
    assert!(workspace.set_tab_running(tab, true, false));
    assert!(workspace.set_tab_running(tab, true, false));
    assert_eq!(told(&mut workspace), 1);
    // A full-screen program's, one change more.
    assert!(workspace.set_tab_running(tab, true, true));
    assert_eq!(told(&mut workspace), 1);
    assert!(workspace.set_tab_secure(tab, true));
    assert!(workspace.set_tab_agent(tab, Some(sdk::AgentState::Waiting)));
    assert_eq!(told(&mut workspace), 2);
    let marks = workspace.tab(tab).unwrap().marks();
    assert!(marks.running && marks.alternate && marks.secure);
    assert_eq!(marks.agent, Some(sdk::AgentState::Waiting));
    assert!(!workspace.set_tab_secure(TabId::of(u64::MAX), true));
    assert!(is_shell("-zsh") && is_shell("tmux") && !is_shell("vim"));
}

#[test]
fn a_pane_s_header_is_told_as_its_title_or_its_place_changes_and_not_of_the_same_title() {
    let mut workspace = Workspace::new();
    let window = workspace.open_window(pane(
        r#"printf '\033]0;same\007'; read line; printf '\033]0;same\007\a'; read line; printf '\033]0;\007'; read line; cd /tmp; exec sleep 30"#,
    ));
    let tab = workspace.window(window).unwrap().active_tab();
    let id = workspace.tab(tab).unwrap().active_pane();
    let told = Change::PaneChanged {
        pane: id.into(),
        tab: tab.into(),
    };
    let header = |workspace: &Workspace| workspace.pane(id).unwrap().header_title(true);
    // Its program's title.
    let start = Instant::now();
    while !changes(&mut workspace).contains(&told) {
        assert!(start.elapsed() < Duration::from_secs(10), "no title");
        workspace.collect(&workspace.pane_ids(), Instant::now());
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(header(&workspace).as_deref(), Some("same"));
    // The same again, with a bell, tells nothing.
    workspace.pane(id).unwrap().text("\n");
    let events = collect_until(&mut workspace, |events| events.contains(&Event::Bell(id)));
    assert!(
        !events.contains(&Event::Changed(told.clone())),
        "{events:?}"
    );
    // No title: its folder and its program, which follow as the app looks.
    workspace.pane(id).unwrap().text("\n");
    let start = Instant::now();
    let mut seen = Vec::new();
    while header(&workspace).is_none_or(|header| !header.contains("sleep")) {
        assert!(start.elapsed() < Duration::from_secs(10), "{seen:?}");
        workspace.collect(&workspace.pane_ids(), Instant::now());
        workspace.look_at_headers(&[id]);
        seen.extend(changes(&mut workspace));
        if seen.iter().filter(|change| **change == told).count() == 1 {
            // Past its title's end, to the folder of `cd`.
            workspace.pane(id).unwrap().text("\n");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    workspace.look_at_headers(&[id]);
    seen.extend(changes(&mut workspace));
    assert_eq!(header(&workspace).as_deref(), Some("tmp — sleep"));
    assert!(
        seen.iter().filter(|change| **change == told).count() >= 2,
        "{seen:?}"
    );
    // Looked at again, the same, it tells nothing.
    workspace.look_at_headers(&[id]);
    assert!(!changes(&mut workspace).contains(&told));
}

#[test]
fn a_tab_s_progress_goes_with_the_pane_that_told_it_and_comes_with_a_pane_moved_in() {
    let mut workspace = Workspace::new();
    let quiet = pane("read line");
    let quiet_id = quiet.id();
    let window = workspace.open_window(quiet);
    let tab = workspace.window(window).unwrap().active_tab();
    let telling = || pane("printf '\\033]9;4;1;40\\a'; read line");
    let told = Some(terminal::Progress::Set(40));
    let first = workspace
        .split(quiet_id, Direction::Horizontal, telling(), Side::Second)
        .unwrap();
    collect_marks(&mut workspace, tab, |marks| marks.progress == told);
    // Closed, its tab's outline goes.
    workspace.take_events();
    assert!(workspace.close_pane(first));
    assert_eq!(workspace.tab(tab).unwrap().marks().progress, None);
    assert!(changes(&mut workspace).contains(&Change::TabChanged { tab: tab.into() }));
    // Out to a tab of its own, its progress goes with it.
    let second = workspace
        .split(quiet_id, Direction::Horizontal, telling(), Side::Second)
        .unwrap();
    collect_marks(&mut workspace, tab, |marks| marks.progress == told);
    let moved = workspace
        .pane_to_new_tab(second, window, 1)
        .expect("a tab of its own");
    assert_eq!(workspace.tab(tab).unwrap().marks().progress, None);
    assert_eq!(workspace.tab(moved).unwrap().marks().progress, told);
}

#[test]
fn a_tab_is_marked_while_its_program_prints_of_its_own_and_not_of_its_input_s_echo() {
    let mut workspace = Workspace::new();
    let window = workspace.open_window(pane(
        r#"read line; printf 'got %s\a' "$line"; sleep 0.8; echo late; read line"#,
    ));
    let tab = workspace.window(window).unwrap().active_tab();
    let id = workspace.tab(tab).unwrap().active_pane();
    // As the app finds a program other than a shell.
    assert!(workspace.set_tab_running(tab, true, false));
    // The echo of a line typed, and the program's answer to it, are not
    // its own.
    workspace.pane(id).unwrap().text("x\n");
    collect_until(&mut workspace, |events| events.contains(&Event::Bell(id)));
    assert!(!workspace.tab(tab).unwrap().marks().printing);
    assert!(!workspace.pane(id).unwrap().printing());
    // What it prints a while after, as nothing asked for it, is, which
    // marks the tab for a while.
    let events = collect_marks(&mut workspace, tab, |marks| marks.printing);
    assert!(
        events
            .iter()
            .any(|event| *event == Event::Changed(Change::TabChanged { tab: tab.into() })),
        "{events:?}"
    );
    let now = Instant::now();
    let due = workspace.collect(&[], now).expect("when it may be quiet");
    assert!(due > now && due <= now + PRINTING, "{due:?}");
    assert!(workspace.tab(tab).unwrap().marks().printing);
    // A shell's output marks nothing, as its prompt.
    assert!(workspace.set_tab_running(tab, false, false));
    assert!(!workspace.tab(tab).unwrap().marks().printing);
    assert!(workspace.set_tab_running(tab, true, false));
    assert!(workspace.tab(tab).unwrap().marks().printing);
    // Quiet once its time is up, as told.
    workspace.take_events();
    let later = Instant::now() + PRINTING;
    assert_eq!(workspace.collect(&[], later), None);
    assert!(!workspace.tab(tab).unwrap().marks().printing);
    assert_eq!(
        changes(&mut workspace),
        [Change::TabChanged { tab: tab.into() }]
    );
}

#[test]
fn an_agent_works_is_done_and_waits_as_its_program_and_the_user_tell() {
    use sdk::AgentState::{Done, Waiting, Working};
    let mut workspace = Workspace::new();
    // An agent's start, a request's work a while after its echo, then a
    // notification that asks for the user, and the title's spinner.
    let window = workspace.open_window(pane(concat!(
        "read line; sleep 0.8; echo working; read line; ",
        r"printf '\033]9;Claude needs your permission\007'; read line; ",
        r"sleep 0.8; printf '\033]2;⠋ Fixing\007'; sleep 0.3; ",
        r"printf '\033]2;⠙ Fixing\007'; read line",
    )));
    let tab = workspace.window(window).unwrap().active_tab();
    let id = workspace.tab(tab).unwrap().active_pane();
    // As the app finds `claude` in its foreground: of no state yet.
    assert!(workspace.set_pane_agent(id, Some("claude")));
    assert_eq!(
        workspace.pane(id).unwrap().agent().unwrap().name(),
        "claude"
    );
    assert_eq!(workspace.tab(tab).unwrap().marks().agent, None);
    // Asked, it works.
    workspace.typed(id);
    workspace.pane(id).unwrap().text("fix it\n");
    let events = collect_marks(&mut workspace, tab, |marks| marks.agent == Some(Working));
    assert!(
        events.iter().any(|event| matches!(
            event,
            Event::Agent { pane, name, before: None, now: Some(Working), told: false } if *pane == id && name == "claude"
        )),
        "{events:?}"
    );
    // Done once quiet for a while.
    std::thread::sleep(Duration::from_millis(300));
    let later = Instant::now() + AgentSettings::default().done_after;
    workspace.collect(&[], later);
    assert_eq!(workspace.tab(tab).unwrap().marks().agent, Some(Done));
    assert_eq!(workspace.agents_waiting(), 0);
    // Seen as the user types, then a notification asks for the user.
    workspace.typed(id);
    assert_eq!(workspace.tab(tab).unwrap().marks().agent, None);
    workspace.pane(id).unwrap().text("go on\n");
    let events = collect_marks(&mut workspace, tab, |marks| marks.agent == Some(Waiting));
    assert!(
        events.iter().any(|event| matches!(
            event,
            Event::Agent {
                now: Some(Waiting),
                told: true,
                ..
            }
        )),
        "{events:?}"
    );
    assert_eq!(workspace.agents_waiting(), 1);
    // Its tab tells its message.
    assert_eq!(
        workspace.tab(tab).unwrap().marks().agent_message.as_deref(),
        Some("Claude needs your permission")
    );
    // Waiting holds, past any time, until the user answers.
    workspace.collect(&[], Instant::now() + Duration::from_secs(60));
    assert_eq!(workspace.tab(tab).unwrap().marks().agent, Some(Waiting));
    workspace.typed(id);
    let marks = workspace.tab(tab).unwrap().marks();
    assert_eq!((marks.agent, marks.agent_message), (None, None));
    // Its title's spinner tells its work too.
    workspace.pane(id).unwrap().text("yes\n");
    collect_marks(&mut workspace, tab, |marks| marks.agent == Some(Working));
    // Gone from the foreground: no agent.
    workspace.take_events();
    assert!(workspace.set_pane_agent(id, None));
    assert_eq!(workspace.tab(tab).unwrap().marks().agent, None);
    assert!(workspace.take_events().iter().any(|event| matches!(
        event,
        Event::Agent {
            before: Some(Working),
            now: None,
            ..
        }
    )));
}

#[test]
fn a_tab_shows_its_most_urgent_agent_and_its_done_agents_are_seen_as_it_shows() {
    use sdk::AgentState::{Done, Waiting};
    let mut workspace = Workspace::new();
    let first = pane(r"read line; printf '\a'; read line");
    let first_id = first.id();
    let window = workspace.open_window(first);
    let tab = workspace.window(window).unwrap().active_tab();
    let second = workspace
        .split(
            first_id,
            Direction::Horizontal,
            pane("read line; sleep 0.8; echo done; read line"),
            Side::Second,
        )
        .unwrap();
    for (id, name) in [(first_id, "codex"), (second, "gemini")] {
        assert!(workspace.set_pane_agent(id, Some(name)));
        workspace.typed(id);
        workspace.pane(id).unwrap().text("x\n");
    }
    // One waits, of its bell, which it does not tell of its own, and one
    // is done: the tab shows the first.
    let events = collect_marks(&mut workspace, tab, |marks| marks.agent == Some(Waiting));
    assert!(
        events.iter().any(|event| matches!(
            event,
            Event::Agent { pane, now: Some(Waiting), told: false, .. } if *pane == first_id
        )),
        "{events:?}"
    );
    std::thread::sleep(Duration::from_millis(1200));
    workspace.collect(&workspace.pane_ids(), Instant::now());
    workspace.collect(&[], Instant::now() + Duration::from_secs(4));
    let state = |workspace: &Workspace, id| workspace.pane(id).unwrap().agent().unwrap().state();
    assert_eq!(state(&workspace, second), Some(Done));
    assert_eq!(workspace.tab(tab).unwrap().marks().agent, Some(Waiting));
    // Another tab, then this one again: done is seen, waiting is not.
    let other = workspace.open_tab(window, pane("sleep 10"), 1).unwrap();
    workspace.activate_tab(other);
    workspace.activate_tab(tab);
    assert_eq!(state(&workspace, second), None);
    assert_eq!(state(&workspace, first_id), Some(Waiting));
    // Out of the tab, its agent goes with its pane, told gone, as the
    // Dock's badge counts it.
    workspace.take_events();
    assert!(workspace.close_pane(first_id));
    assert_eq!(workspace.tab(tab).unwrap().marks().agent, None);
    assert!(workspace.take_events().iter().any(|event| matches!(
        event,
        Event::Agent { pane, before: Some(Waiting), now: None, .. } if *pane == first_id
    )));
    assert_eq!(workspace.agents_waiting(), 0);
}

#[test]
fn a_hooked_agent_asks_by_its_hooks_alone_and_an_unknown_hook_shows_nothing() {
    use sdk::AgentState::{Done, Working};
    let mut workspace = Workspace::new();
    let window = workspace.open_window(pane(concat!(
        r"read line; printf '\033]777;notify;lokyr-agent:working;\007'; read line; ",
        r"printf '\033]777;notify;lokyr-agent:done;\007'; read line; ",
        r"printf '\a\033]9;Claude is waiting for your input\007\033]2;asked\007'; read line; ",
        r"printf '\033]777;notify;lokyr-agent:other;x\007\033]2;told\007'; read line",
    )));
    let tab = workspace.window(window).unwrap().active_tab();
    let id = workspace.tab(tab).unwrap().active_pane();
    assert!(workspace.set_pane_agent(id, Some("claude")));
    workspace.typed(id);
    workspace.pane(id).unwrap().text("go\n");
    collect_marks(&mut workspace, tab, |marks| marks.agent == Some(Working));
    workspace.pane(id).unwrap().text("x\n");
    collect_marks(&mut workspace, tab, |marks| marks.agent == Some(Done));
    // The events up to the next title's change, of the pane's next line.
    let until_titled = |workspace: &mut Workspace, line: &str| {
        workspace.take_events();
        workspace.pane(id).unwrap().text(line);
        let start = Instant::now();
        let mut events = Vec::new();
        while !events
            .iter()
            .any(|event| matches!(event, Event::TitleChanged(pane) if *pane == id))
        {
            assert!(start.elapsed() < Duration::from_secs(10), "{events:?}");
            workspace.collect(&workspace.pane_ids(), Instant::now());
            events.extend(workspace.take_events());
            std::thread::sleep(Duration::from_millis(10));
        }
        events
    };
    // Its idle prompt's bell and notification, later: done still, as its
    // hooks alone tell its asking.
    until_titled(&mut workspace, "y\n");
    assert_eq!(workspace.tab(tab).unwrap().marks().agent, Some(Done));
    assert_eq!(workspace.agents_waiting(), 0);
    // A hook of a state that this version does not know: no notification.
    let events = until_titled(&mut workspace, "z\n");
    assert!(
        !events.iter().any(|event| matches!(
            event,
            Event::Terminal {
                event: terminal::Event::Notification { .. },
                ..
            }
        )),
        "{events:?}"
    );
}

#[test]
fn a_tab_is_marked_of_the_printing_of_its_active_pane_alone() {
    let mut workspace = Workspace::new();
    let quiet = pane("read line");
    let quiet_id = quiet.id();
    let window = workspace.open_window(quiet);
    let tab = workspace.window(window).unwrap().active_tab();
    assert!(workspace.set_tab_running(tab, true, false));
    let printing = workspace
        .split(
            quiet_id,
            Direction::Horizontal,
            pane("echo one; read line"),
            Side::Second,
        )
        .unwrap();
    // Its program, as the app finds it, runs too.
    assert!(workspace.set_pane_running(printing, true, false));
    let start = Instant::now();
    while !workspace.pane(printing).unwrap().printing() {
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "it never printed"
        );
        workspace.collect(&workspace.pane_ids(), Instant::now());
        std::thread::sleep(Duration::from_millis(10));
    }
    // The keyboard stays where it was, in the quiet pane.
    assert!(!workspace.tab(tab).unwrap().marks().printing);
    workspace.take_events();
    assert!(workspace.focus_pane(printing));
    assert!(workspace.tab(tab).unwrap().marks().printing);
    assert!(changes(&mut workspace).contains(&Change::TabChanged { tab: tab.into() }));
    assert!(workspace.focus_pane(quiet_id));
    assert!(!workspace.tab(tab).unwrap().marks().printing);
}

#[test]
fn two_panes_of_a_tab_swap_their_rooms_and_its_keyboard_stays_with_its_pane() {
    let mut workspace = Workspace::new();
    let window = workspace.open_window(pane("read line"));
    let tab = workspace.window(window).unwrap().active_tab();
    let first = workspace.tab(tab).unwrap().active_pane();
    let second = workspace
        .split(
            first,
            Direction::Horizontal,
            pane("read line"),
            Side::Second,
        )
        .unwrap();
    let third = workspace
        .split(second, Direction::Vertical, pane("read line"), Side::Second)
        .unwrap();
    workspace.focus_pane(third);
    workspace.take_events();
    assert!(workspace.swap_panes(first, third));
    let shown = workspace.tab(tab).unwrap();
    assert_eq!(shown.layout().panes(), [third, second, first]);
    assert_eq!(shown.active_pane(), third);
    assert_eq!(
        changes(&mut workspace),
        [Change::LayoutChanged { tab: tab.into() }]
    );
    // A pane with itself, or of another tab, does not swap.
    assert!(!workspace.swap_panes(first, first));
    let other = workspace.open_tab(window, pane("read line"), 1).unwrap();
    let alone = workspace.tab(other).unwrap().active_pane();
    assert!(!workspace.swap_panes(first, alone));
    assert!(
        changes(&mut workspace)
            .iter()
            .all(|change| !matches!(change, Change::LayoutChanged { .. }))
    );
}

#[test]
fn a_tab_goes_into_a_pane_of_another_whole_in_the_half_at_its_edge() {
    let mut workspace = Workspace::new();
    // A first window of a tab of one pane, and of a tab of two, one over
    // the other; a second window of a tab of one pane.
    let first = workspace.open_window(pane("sleep 10"));
    let alone = workspace.window(first).unwrap().active_tab();
    let a = workspace.tab(alone).unwrap().active_pane();
    let pair = workspace.open_tab(first, pane("sleep 10"), 99).unwrap();
    let top = workspace.tab(pair).unwrap().active_pane();
    let bottom = workspace
        .split(top, Direction::Vertical, pane("sleep 10"), Side::Second)
        .unwrap();
    let second = workspace.open_window(pane("sleep 10"));
    let target = workspace.window(second).unwrap().active_tab();
    let c = workspace.tab(target).unwrap().active_pane();
    workspace.take_events();
    let layout = |workspace: &Workspace| workspace.tab(target).unwrap().layout().clone();
    // The pair on the left of the second's pane, whole, half and half: the
    // first window shows its other tab, the keyboard goes to the pair's.
    assert!(workspace.tab_into_pane(pair, c, Edge::Left));
    let pair_layout = Layout::Split {
        direction: Direction::Vertical,
        ratio: None,
        first: Box::new(Layout::Leaf(top)),
        second: Box::new(Layout::Leaf(bottom)),
    };
    assert_eq!(
        layout(&workspace),
        Layout::Split {
            direction: Direction::Horizontal,
            ratio: Some(Ratio::HALF),
            first: Box::new(pair_layout.clone()),
            second: Box::new(Layout::Leaf(c)),
        }
    );
    assert!(workspace.tab(pair).is_none());
    assert_eq!(workspace.tab_of(top), Some(target));
    assert_eq!(workspace.tab_of(bottom), Some(target));
    assert_eq!(workspace.tab(target).unwrap().active_pane(), top);
    assert_eq!(workspace.window(first).unwrap().tabs(), [alone]);
    assert_eq!(workspace.window(first).unwrap().active_tab(), alone);
    let events = workspace.take_events();
    let moved = |pane: PaneId| {
        Event::Changed(Change::PaneMoved {
            pane: pane.into(),
            from: pair.into(),
            tab: target.into(),
        })
    };
    assert!(events.contains(&moved(top)) && events.contains(&moved(bottom)));
    assert!(events.contains(&Event::Changed(Change::TabClosed {
        tab: pair.into(),
        window: first.into(),
    })));
    // The first window's last tab below the second's pane: the window
    // closes.
    assert!(workspace.tab_into_pane(alone, c, Edge::Bottom));
    assert_eq!(
        layout(&workspace),
        Layout::Split {
            direction: Direction::Horizontal,
            ratio: Some(Ratio::HALF),
            first: Box::new(pair_layout),
            second: Box::new(Layout::Split {
                direction: Direction::Vertical,
                ratio: Some(Ratio::HALF),
                first: Box::new(Layout::Leaf(c)),
                second: Box::new(Layout::Leaf(a)),
            }),
        }
    );
    assert!(workspace.window(first).is_none());
    assert!(
        workspace
            .take_events()
            .contains(&Event::Changed(Change::WindowClosed {
                window: first.into()
            }))
    );
    // Not into a pane of its own, nor of a tab not there.
    assert!(!workspace.tab_into_pane(target, c, Edge::Right));
    assert!(!workspace.tab_into_pane(pair, c, Edge::Right));
    assert_eq!(workspace.panes_of(second).len(), 4);
}
