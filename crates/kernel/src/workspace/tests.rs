//! The workspace's model, with real programs in its panes, and no window.

use std::time::{Duration, Instant};

use lokyr_pty::Program;
use lokyr_vt_core::terminal::Colors;

use super::*;
use crate::l10n::Texts;

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
        workspace.collect();
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
    // The main thread's loop: it collects as it is woken, and draws
    // nothing.
    let mut seen = 0;
    let mut bells = 0;
    let start = Instant::now();
    while bells < 2 {
        assert!(start.elapsed() < Duration::from_secs(10), "{bells} bells");
        let now = wakes.load(Ordering::SeqCst);
        if now == seen {
            std::thread::sleep(Duration::from_millis(10));
            continue;
        }
        seen = now;
        workspace.collect();
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

#[test]
fn a_window_opens_with_one_tab_whose_layout_is_its_pane() {
    let mut workspace = Workspace::new();
    let pane = pane("sleep 10");
    let id = pane.id();
    let window = workspace.open_window(pane);
    assert_eq!(workspace.take_events(), [Event::WindowOpened(window)]);
    let tabs = workspace.window(window).unwrap().tabs().to_vec();
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
    workspace.take_events();
    assert!(workspace.close_window(window));
    assert_eq!(workspace.take_events(), [Event::WindowClosed(window)]);
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
    workspace.collect();
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
    workspace.collect();
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
    let pane = pane("printf 'ready'; sleep 10");
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
    assert_eq!(pane.contexts(), ["selection"]);
    pane.clear_selection();
    assert!(pane.contexts().is_empty());
    let alternate = self::pane("printf '\\033[?1049h'; sleep 10");
    let start = Instant::now();
    while alternate.contexts().is_empty() {
        assert!(start.elapsed() < Duration::from_secs(10));
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(alternate.contexts(), ["alternate-screen"]);
}

#[test]
fn a_marked_cell_s_line_and_link_are_its_own_as_output_scrolls_the_view() {
    let pane = pane(concat!(
        "stty -echo; printf '\\033]8;;https://lokyr.dev/osc\\033\\\\see\\033]8;;\\033\\\\ ",
        "https://lokyr.dev/x\\n'; read line; seq 1 60; echo done; read line"
    ));
    let text = |row| {
        pane.line_text(row)
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
    // A menu's cell, on the OSC 8 link, read with its line and its link.
    pane.mark_cell((1, 0));
    assert_eq!(pane.marked_cell(), Some((1, 0)));
    // Output scrolls the view, and its row 0 shows another line.
    pane.send_text("\r");
    wait(&|| (1..24).any(|row| text(row).as_deref() == Some("done")));
    assert_eq!(text(0), Some(first.clone()));
    assert_eq!(pane.link_at(1, 0).as_deref(), Some("https://lokyr.dev/osc"));
    let (_, cells) = pane.line_text(0).unwrap();
    assert_eq!(cells[0], (0, 0));
    // The other rows are the view's own, and the mark gone, the row too.
    assert_ne!(text(1), Some(first.clone()));
    pane.unmark_cell();
    assert_ne!(text(0), Some(first));
    assert_eq!(pane.link_at(1, 0), None);
}
