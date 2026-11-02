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
