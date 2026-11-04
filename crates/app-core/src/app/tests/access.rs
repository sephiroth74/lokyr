//! The windows' accessibility: nothing before a client asks, then the
//! window's tree whole, then again as its panes' screens change, at its
//! rate, and of the windows that changed alone.

use accesskit::{Role, TreeId, TreeUpdate};
use lokyr_ui_core::access::{WINDOW, pane_id};

use super::*;

/// The texts of the runs of the terminal of `update`, a whole tree's.
fn runs(update: &TreeUpdate) -> Vec<String> {
    let node = |id| {
        update
            .nodes
            .iter()
            .find(|(at, _)| *at == id)
            .map(|(_, node)| node)
    };
    let Some((_, terminal)) = update
        .nodes
        .iter()
        .find(|(_, node)| node.role() == Role::Terminal)
    else {
        return Vec::new();
    };
    terminal
        .children()
        .iter()
        .filter_map(|&id| node(id).and_then(|run| run.value()).map(str::to_owned))
        .collect()
}

/// Runs the app until `window`'s screen shows `text`, as [`run_until`]
/// does, but for its timers, which wait.
fn show_until(fake: &mut Fake, app: &mut App, window: WindowId, text: &str) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !(0..app.pane(window).snapshot().rows()).any(|y| row(app, window, y).contains(text)) {
        assert!(Instant::now() < deadline, "not shown in time");
        fake.deliver_wake(app);
        if fake.window(window).unwrap().frame_requested {
            fake.send(app, window, WindowEvent::Frame);
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// Whether one of the trees given to `window` has a run that starts with
/// `text`.
fn given(fake: &Fake, window: WindowId, text: &str) -> bool {
    fake.window(window)
        .unwrap()
        .accessibility
        .iter()
        .any(|tree| runs(tree).iter().any(|run| run.starts_with(text)))
}

#[test]
fn a_window_s_tree_goes_once_a_client_asks_then_as_its_screen_changes() {
    let (mut fake, mut app, window) = started_running(shell(
        "echo ready; read line; echo second; read more; echo third; read last; echo fourth; \
         read end",
    ));
    // Without a client, no tree, and no timer for one.
    fake.send(&mut app, window, WindowEvent::Frame);
    assert!(fake.window(window).unwrap().accessibility.is_empty());
    assert!(app.access_timer.is_none());
    // A client asks: the tree whole, of the window, its terminal of its
    // screen, the keyboard's pane focused.
    assert!(fake.ask_accessibility(&mut app, window));
    let trees = &fake.window(window).unwrap().accessibility;
    assert_eq!(trees.len(), 1);
    let first = &trees[0];
    assert_eq!(first.tree.as_ref().map(|tree| tree.root), Some(WINDOW));
    assert_eq!(first.tree_id, TreeId::ROOT);
    assert!(
        runs(first).iter().any(|run| run.starts_with("ready")),
        "{:?}",
        runs(first)
    );
    let pane = app.focused(window).unwrap();
    assert_eq!(first.focus, pane_id(u32::try_from(pane.get()).unwrap()));
    // The program writes, twice within the rate: one timer, then the tree
    // again, of its text.
    app.pane(window).text("x\r");
    show_until(&mut fake, &mut app, window, "second");
    let timer = app.access_timer;
    assert!(timer.is_some());
    app.pane(window).text("y\r");
    show_until(&mut fake, &mut app, window, "third");
    assert_eq!(app.access_timer, timer);
    assert!(!given(&fake, window, "second"));
    fake.fire_timers(&mut app, Instant::now() + Duration::from_secs(1));
    assert!(given(&fake, window, "third"));
    // And again, past the rate.
    app.pane(window).text("z\r");
    run_until(&mut fake, &mut app, |fake, _| given(fake, window, "fourth"));
    // And no more while it shows nothing new.
    let given = fake.window(window).unwrap().accessibility.len();
    for _ in 0..3 {
        fake.send(&mut app, window, WindowEvent::Frame);
        fake.fire_timers(&mut app, Instant::now() + Duration::from_secs(1));
    }
    assert_eq!(fake.window(window).unwrap().accessibility.len(), given);
}

#[test]
fn of_two_windows_only_the_one_whose_screen_changed_takes_its_tree_again() {
    let (mut fake, mut app, first) =
        started_running(shell("echo ready; read line; echo second; read more"));
    fake.send_app(&mut app, AppEvent::Command("window.new".to_owned()));
    let other = fake.windows().find(|&window| window != first).unwrap();
    run_until(&mut fake, &mut app, |_, app| {
        (0..app.pane(other).snapshot().rows()).any(|y| row(app, other, y).contains("ready"))
    });
    assert!(fake.ask_accessibility(&mut app, first));
    assert!(fake.ask_accessibility(&mut app, other));
    // The other window's screen changes, then the first's alone.
    app.pane(other).text("x\r");
    run_until(&mut fake, &mut app, |fake, _| given(fake, other, "second"));
    fake.fire_timers(&mut app, Instant::now() + Duration::from_secs(1));
    let others = fake.window(other).unwrap().accessibility.len();
    app.pane(first).text("x\r");
    run_until(&mut fake, &mut app, |fake, _| given(fake, first, "second"));
    fake.fire_timers(&mut app, Instant::now() + Duration::from_secs(1));
    assert_eq!(fake.window(other).unwrap().accessibility.len(), others);
}
